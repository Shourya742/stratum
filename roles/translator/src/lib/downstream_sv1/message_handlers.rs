use binary_sv2::{Sv2DataType, U256};
use std::ops::{Div, Mul};
use stratum_common::bitcoin::{
    self,
    block::{Header, Version},
    hashes::{sha256d, Hash},
    hex::DisplayHex,
    BlockHash, CompactTarget,
};

use crate::downstream_sv1;

use super::{Downstream, DownstreamMessages, SubmitShareWithChannelId};

use roles_logic_sv2::{
    common_properties::{IsDownstream, IsMiningDownstream},
    utils::{from_u128_to_u256, u256_to_block_hash},
};

use std::collections::VecDeque;
use tracing::{debug, error, info, warn};
use v1::{
    client_to_server, json_rpc, server_to_client,
    utils::{Extranonce, HexU32Be},
    IsServer,
};

/// Implements `IsServer` for `Downstream` to handle the SV1 messages.
impl IsServer<'static> for Downstream {
    /// Handles the incoming SV1 `mining.configure` message.
    ///
    /// This message is received after `mining.subscribe` and `mining.authorize`.
    /// It allows the miner to negotiate capabilities, particularly regarding
    /// version rolling. This method processes the version rolling mask and
    /// minimum bit count provided by the client.
    ///
    /// Returns a tuple containing:
    /// 1. `Option<server_to_client::VersionRollingParams>`: The version rolling parameters
    ///    negotiated by the server (proxy).
    /// 2. `Option<bool>`: A boolean indicating whether the server (proxy) supports version rolling
    ///    (always `Some(false)` for TProxy according to the SV1 spec when not supporting work
    ///    selection).
    fn handle_configure(
        &mut self,
        request: &client_to_server::Configure,
    ) -> (Option<server_to_client::VersionRollingParams>, Option<bool>) {
        info!("Down: Configuring");
        debug!("Down: Handling mining.configure: {:?}", &request);

        // TODO 0x1FFFE000 should be configured
        // = 11111111111111110000000000000
        // this is a reasonable default as it allows all 16 version bits to be used
        // If the tproxy/pool needs to use some version bits this needs to be configurable
        // so upstreams can negotiate with downstreams. When that happens this should consider
        // the min_bit_count in the mining.configure message
        self.version_rolling_mask = request
            .version_rolling_mask()
            .map(|mask| HexU32Be(mask & 0x1FFFE000));
        self.version_rolling_min_bit = request.version_rolling_min_bit_count();

        debug!(
            "Negotiated version_rolling_mask is {:?}",
            self.version_rolling_mask
        );
        (
            Some(server_to_client::VersionRollingParams::new(
                self.version_rolling_mask.clone().unwrap_or(HexU32Be(0)),
                self.version_rolling_min_bit.clone().unwrap_or(HexU32Be(0)),
            ).expect("Version mask invalid, automatic version mask selection not supported, please change it in carte::downstream_sv1::mod.rs")),
            Some(false),
        )
    }

    /// Handles the incoming SV1 `mining.subscribe` message.
    ///
    /// This is typically the first message received from a new client. In the SV1
    /// protocol, it's used to subscribe to job notifications and receive session
    /// details like extranonce1 and extranonce2 size. This method acknowledges the subscription and
    /// provides the necessary details derived from the upstream SV2 connection (extranonce1 and
    /// extranonce2 size). It also provides subscription IDs for the
    /// `mining.set_difficulty` and `mining.notify` methods.
    fn handle_subscribe(&self, request: &client_to_server::Subscribe) -> Vec<(String, String)> {
        info!("Down: Subscribing");
        debug!("Down: Handling mining.subscribe: {:?}", &request);

        let set_difficulty_sub = (
            "mining.set_difficulty".to_string(),
            downstream_sv1::new_subscription_id(),
        );
        let notify_sub = (
            "mining.notify".to_string(),
            "ae6812eb4cd7735a302a8a9dd95cf71f".to_string(),
        );

        vec![set_difficulty_sub, notify_sub]
    }

    /// Any numbers of workers may be authorized at any time during the session. In this way, a
    /// large number of independent Mining Devices can be handled with a single SV1 connection.
    /// https://bitcoin.stackexchange.com/questions/29416/how-do-pool-servers-handle-multiple-workers-sharing-one-connection-with-stratum
    fn handle_authorize(&self, request: &client_to_server::Authorize) -> bool {
        info!("Down: Authorizing");
        debug!("Down: Handling mining.authorize: {:?}", &request);
        true
    }

    /// Handles the incoming SV1 `mining.submit` message.
    ///
    /// This message is sent by the miner when they find a share that meets
    /// their current difficulty target. It contains the job ID, ntime, nonce,
    /// and extranonce2.
    ///
    /// This method processes the submitted share, potentially validates it
    /// against the downstream target (although this might happen in the Bridge
    /// or difficulty management logic), translates it into a
    /// [`SubmitShareWithChannelId`], and sends it to the Bridge for
    /// translation to SV2 and forwarding upstream if it meets the upstream target.
    fn handle_submit(&self, request: &client_to_server::Submit<'static>) -> bool {
        info!("Downstream: Received mining.submit {:?}", request);

        if !self.first_job_received {
            warn!("Share rejected: No job received yet from upstream");
            return false;
        }

        if self.active_jobs.is_empty() {
            warn!("Share rejected: No recent notify messages available");
            return false;
        }

        let target_result = roles_logic_sv2::utils::hash_rate_to_target(
            self.difficulty_mgmt.min_individual_miner_hashrate.into(),
            self.difficulty_mgmt.shares_per_minute.into(),
        );

        let current_target = match target_result {
            Ok(t) => t.to_vec(),
            Err(e) => {
                error!("Failed to compute target from hash rate: {:?}", e);
                return false;
            }
        };

        let current_difficulty = match Downstream::difficulty_from_target(current_target) {
            Ok(d) => d as f32,
            Err(e) => {
                error!(
                    "Failed to derive difficulty from target, {:?}",
                    e.to_string()
                );
                return false;
            }
        };

        let is_valid = is_valid_share(
            request,
            &self.active_jobs,
            current_difficulty,
            self.extranonce1.clone(),
            self.version_rolling_mask.clone(),
        );

        if is_valid {
            let to_send = SubmitShareWithChannelId {
                channel_id: self.connection_id,
                share: request.clone(),
                extranonce: self.extranonce1.clone(),
                extranonce2_len: self.extranonce2_len,
                version_rolling_mask: self.version_rolling_mask.clone(),
            };

            if let Err(e) = self
                .tx_sv1_bridge
                .try_send(DownstreamMessages::SubmitShares(to_send))
            {
                error!("Failed to send share to SV1 bridge: {:?}", e);
                return false;
            }

            info!("Share accepted and forwarded to bridge");
            true
        } else {
            warn!("Share rejected: Validation failed");
            false
        }
    }

    /// Indicates to the server that the client supports the mining.set_extranonce method.
    fn handle_extranonce_subscribe(&self) {}

    /// Checks if a Downstream role is authorized.
    fn is_authorized(&self, name: &str) -> bool {
        self.authorized_names.contains(&name.to_string())
    }

    /// Authorizes a Downstream role.
    fn authorize(&mut self, name: &str) {
        self.authorized_names.push(name.to_string());
    }

    /// Sets the `extranonce1` field sent in the SV1 `mining.notify` message to the value specified
    /// by the SV2 `OpenExtendedMiningChannelSuccess` message sent from the Upstream role.
    fn set_extranonce1(
        &mut self,
        _extranonce1: Option<Extranonce<'static>>,
    ) -> Extranonce<'static> {
        self.extranonce1.clone().try_into().unwrap()
    }

    /// Returns the `Downstream`'s `extranonce1` value.
    fn extranonce1(&self) -> Extranonce<'static> {
        self.extranonce1.clone().try_into().unwrap()
    }

    /// Sets the `extranonce2_size` field sent in the SV1 `mining.notify` message to the value
    /// specified by the SV2 `OpenExtendedMiningChannelSuccess` message sent from the Upstream role.
    fn set_extranonce2_size(&mut self, _extra_nonce2_size: Option<usize>) -> usize {
        self.extranonce2_len
    }

    /// Returns the `Downstream`'s `extranonce2_size` value.
    fn extranonce2_size(&self) -> usize {
        self.extranonce2_len
    }

    /// Returns the version rolling mask.
    fn version_rolling_mask(&self) -> Option<HexU32Be> {
        self.version_rolling_mask.clone()
    }

    /// Sets the version rolling mask.
    fn set_version_rolling_mask(&mut self, mask: Option<HexU32Be>) {
        self.version_rolling_mask = mask;
    }

    /// Sets the minimum version rolling bit.
    fn set_version_rolling_min_bit(&mut self, mask: Option<HexU32Be>) {
        self.version_rolling_min_bit = mask
    }

    fn notify(&mut self) -> Result<json_rpc::Message, v1::error::Error> {
        unreachable!()
    }
}

// Can we remove this?
impl IsMiningDownstream for Downstream {}
// Can we remove this?
impl IsDownstream for Downstream {
    fn get_downstream_mining_data(
        &self,
    ) -> roles_logic_sv2::common_properties::CommonDownstreamData {
        todo!()
    }
}

/// Validates a submitted mining share against recent jobs and target difficulty.
fn is_valid_share(
    submit: &client_to_server::Submit<'static>,
    recent_jobs: &VecDeque<server_to_client::Notify<'static>>,
    difficulty: f32,
    extranonce1: Vec<u8>,
    version_rolling_mask: Option<HexU32Be>,
) -> bool {
    let job = match recent_jobs.iter().find(|job| job.job_id == submit.job_id) {
        Some(job) => job,
        None => {
            error!(
                "Share rejected: No matching job ID ({}) found",
                submit.job_id
            );
            return false;
        }
    };

    let prev_hash_bytes: Vec<u8> = job.prev_hash.clone().into();
    let prev_hash = match U256::from_vec_(prev_hash_bytes) {
        Ok(p) => u256_to_block_hash(p),
        Err(e) => {
            error!("Failed to convert prev_hash to U256: {:?}", e);
            return false;
        }
    };

    let extranonce2 = submit.extra_nonce2.0.to_vec();
    let extranonce: Vec<u8> = [extranonce1, extranonce2].concat();

    let job_version = job.version.0;
    let submit_version = submit
        .version_bits
        .clone()
        .map(|v| v.0)
        .unwrap_or(job_version);
    let mask = version_rolling_mask.unwrap_or(HexU32Be(0x1FFFE000)).0;
    let merged_version = (job_version & !mask) | (submit_version & mask);

    let merkle_branch: Vec<Vec<u8>> = job.merkle_branch.iter().map(|b| b.0.to_vec()).collect();

    let block_hash = compute_block_hash(
        submit.nonce.0,
        merged_version,
        submit.time.0,
        &extranonce,
        job,
        prev_hash,
        merkle_branch,
    );

    let mut hash = block_hash;
    hash.reverse();

    let target = difficulty_to_target(difficulty);
    info!("Computed hash:   {:?}", hash.as_hex());
    info!("Current target:  {:?}", target.as_hex());

    hash <= target
}

fn compute_block_hash(
    nonce: u32,
    version: u32,
    ntime: u32,
    extranonce: &[u8],
    job: &server_to_client::Notify,
    prev_hash: BlockHash,
    merkle_path: Vec<Vec<u8>>,
) -> [u8; 32] {
    let mut coinbase = Vec::new();
    coinbase.extend_from_slice(job.coin_base1.as_ref());
    coinbase.extend_from_slice(extranonce);
    coinbase.extend_from_slice(job.coin_base2.as_ref());

    let coinbase_hash = sha256d::Hash::hash(&coinbase);
    let mut merkle_root = coinbase_hash.to_byte_array();

    for node in merkle_path {
        let mut combined = Vec::with_capacity(64);
        combined.extend_from_slice(&merkle_root);
        combined.extend_from_slice(&node);
        merkle_root = sha256d::Hash::hash(&combined).to_byte_array();
    }

    let header = Header {
        version: Version::from_consensus(version.try_into().unwrap_or_else(|_| {
            warn!("Invalid version cast, defaulting to 0");
            0
        })),
        prev_blockhash: prev_hash,
        merkle_root: bitcoin::TxMerkleNode::from_byte_array(merkle_root),
        time: ntime,
        bits: CompactTarget::from_consensus(job.bits.0),
        nonce,
    };

    header.block_hash().to_raw_hash().to_byte_array()
}

pub fn difficulty_to_target(difficulty: f32) -> [u8; 32] {
    let min_diff = 0.001;
    let difficulty = difficulty.max(min_diff);
    let scale_factor: u128 = 1_000_000;

    let pdiff: [u8; 32] = [
        0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ];
    let max_target = primitive_types::U256::from_big_endian(&pdiff);

    let scaled_diff = difficulty * (scale_factor as f32);
    if scaled_diff > u128::MAX as f32 {
        error!("Scaled difficulty {} exceeds u128 maximum", scaled_diff);
        return [0xff; 32]; // Return max target
    }

    let diff = from_u128_to_u256(scaled_diff as u128);
    let scale = from_u128_to_u256(scale_factor);

    let target = max_target.mul(scale).div(diff);
    target.to_big_endian()
}
