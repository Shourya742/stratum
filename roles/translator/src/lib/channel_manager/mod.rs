#![allow(warnings)]
//! What should be the role of channel manager?
//!
//! 1. It should assign extranonce to new connection.
//! 2. It can coordinate with upstream module to open connection in case of non-aggregation.
//! 3. It should perform share validation and send correct response to corresponding downstream.
//! 4. It should be responsible for difficulty management for each sv1 channel.
//! 5. It should harbour the jobs received from upstream, to perform validation correctly.

/// We gonna be having two flows, one for aggregation and another for non-aggregation
/// In case of aggregation, the whole tproxy flow gonna start from the upstream submodule
/// where the upstream gonna connect to Pool/JDC by itself at the beginning of the setup.
/// In case of non-aggregation, the whole tproxy flow gonna start from downstream, where
/// once a downstream connects we gonna open a extended mining channel with the upstream.
use std::collections::{HashMap, HashSet};

use binary_sv2::u256_from_int;
use roles_logic_sv2::{
    channels::server::{jobs::extended::ExtendedJob, share_accounting::ShareAccounting},
    mining_sv2::{ExtendedExtranonce, Extranonce, NewExtendedMiningJob, SetNewPrevHash, Target},
    utils::{bytes_to_hex, merkle_root_from_path, target_to_difficulty, u256_to_block_hash, Id},
};
use stratum_common::bitcoin::{
    blockdata::block::{Header, Version},
    hashes::sha256d::Hash,
    transaction::TxOut,
    CompactTarget, Target as BitcoinTarget,
};
use tracing::{debug, info};
use v1::utils::HexU32Be;

use crate::{
    config::UpstreamDifficultyConfig, downstream_sv1::SubmitShareWithChannelId,
    utils::proxy_extranonce1_len,
};

#[derive(PartialEq, Hash, Eq, Clone, Debug, Copy)]
pub struct Sv1ChannelId(u32);

/// Sv1 channel representation
pub struct Sv1Channel {
    // Channel id of the connection
    channel_id: Sv1ChannelId,
    // User identity
    user_identity: String,
    // Extranonce prefix allocated for the connection
    extranonce_prefix: Vec<u8>,
    // Rollable extranonce size for the connection
    rollable_extranonce_size: u16,
    /// Version rolling mask bits
    version_rolling_mask: Option<HexU32Be>,
    /// Minimum version rolling mask bits size
    version_rolling_min_bit: Option<HexU32Be>,
}

impl Sv1Channel {
    fn new(
        channel_id: Sv1ChannelId,
        user_identity: String,
        extranonce_prefix: Vec<u8>,
        rollable_extranonce_size: u16,
    ) -> Self {
        Self {
            channel_id,
            user_identity,
            extranonce_prefix,
            rollable_extranonce_size,
            version_rolling_mask: None,
            version_rolling_min_bit: None,
        }
    }
}

#[derive(Debug)]
pub struct UpstreamChannelManager {
    pub channel_ids: HashSet<u32>,
    pub upstream_manager: HashMap<u32, UpstreamChannel>,
    pub aggregate: bool,
    pub min_extranonce_size: u16,
    pub bootstrap_nominal_hashrate: f32,
    pub update_interval: u32,
    pub shares_per_minute: f32,
}

#[derive(Debug)]
pub struct UpstreamChannel {
    pub downstream_manager: ChannelManager,
    pub last_sent_hashrate: f32,
    pub upstream_difficulty: UpstreamDifficultyConfig,
    pub target: Target,
}

impl UpstreamChannelManager {
    pub fn new(
        min_extranonce_size: u16,
        bootstrap_nominal_hashrate: f32,
        update_interval: u32,
        shares_per_minute: f32,
    ) -> Self {
        Self {
            channel_ids: HashSet::new(),
            upstream_manager: HashMap::new(),
            aggregate: true,
            min_extranonce_size,
            bootstrap_nominal_hashrate,
            update_interval,
            shares_per_minute,
        }
    }

    pub fn remove(&mut self, id: u32) {
        self.channel_ids.remove(&id);
        // todo: Improve this later
        self.upstream_manager.remove(&id);
    }

    pub fn downstream_difficulty_hashrate(
        &self,
        channel_id: u32,
        connection_id: Sv1ChannelId,
    ) -> Option<f32> {
        if let Some(upstream_channel) = self.upstream_manager.get(&channel_id) {
            if let Some(difficulty_manager) = upstream_channel
                .downstream_manager
                .difficulty_config
                .get(&connection_id)
            {
                return Some(difficulty_manager.min_individual_miner_hashrate.clone());
            }
        }
        None
    }

    pub fn downstream_difficulty_target(
        &self,
        channel_id: u32,
        connection_id: Sv1ChannelId,
    ) -> Option<Target> {
        if let Some(upstream_channel) = self.upstream_manager.get(&channel_id) {
            if let Some(difficulty_manager) = upstream_channel
                .downstream_manager
                .difficulty_config
                .get(&connection_id)
            {
                return Some(difficulty_manager.target.clone());
            }
        }
        None
    }

    pub fn downstream_difficulty_submits_since_last_update(
        &self,
        channel_id: u32,
        connection_id: Sv1ChannelId,
    ) -> Option<u32> {
        if let Some(upstream_channel) = self.upstream_manager.get(&channel_id) {
            if let Some(difficulty_manager) = upstream_channel
                .downstream_manager
                .difficulty_config
                .get(&connection_id)
            {
                return Some(difficulty_manager.submits_since_last_update.clone());
            }
        }
        None
    }

    pub fn downstream_difficulty_timestamp_of_last_update(
        &self,
        channel_id: u32,
        connection_id: Sv1ChannelId,
    ) -> Option<u64> {
        if let Some(upstream_channel) = self.upstream_manager.get(&channel_id) {
            if let Some(difficulty_manager) = upstream_channel
                .downstream_manager
                .difficulty_config
                .get(&connection_id)
            {
                return Some(difficulty_manager.timestamp_of_last_update.clone());
            }
        }
        None
    }

    pub fn set_downstream_difficulty_hashrate(
        &mut self,
        channel_id: u32,
        connection_id: Sv1ChannelId,
        hashrate: f32,
    ) {
        if let Some(upstream_channel) = self.upstream_manager.get_mut(&channel_id) {
            if let Some(difficulty_manager) = upstream_channel
                .downstream_manager
                .difficulty_config
                .get_mut(&connection_id)
            {
                difficulty_manager.min_individual_miner_hashrate = hashrate;
            }
        }
    }

    pub fn set_downstream_difficulty_target(
        &mut self,
        channel_id: u32,
        connection_id: Sv1ChannelId,
        target: Target,
    ) {
        if let Some(upstream_channel) = self.upstream_manager.get_mut(&channel_id) {
            if let Some(difficulty_manager) = upstream_channel
                .downstream_manager
                .difficulty_config
                .get_mut(&connection_id)
            {
                difficulty_manager.target = target;
            }
        }
    }

    pub fn set_downstream_difficulty_submits_since_last_update(
        &mut self,
        channel_id: u32,
        connection_id: Sv1ChannelId,
        last_update: u32,
    ) {
        if let Some(upstream_channel) = self.upstream_manager.get_mut(&channel_id) {
            if let Some(difficulty_manager) = upstream_channel
                .downstream_manager
                .difficulty_config
                .get_mut(&connection_id)
            {
                difficulty_manager.submits_since_last_update = last_update;
            }
        }
    }

    pub fn set_downstream_difficulty_timestamp_of_last_update(
        &mut self,
        channel_id: u32,
        connection_id: Sv1ChannelId,
        timestamp_since_last_update: u64,
    ) {
        if let Some(upstream_channel) = self.upstream_manager.get_mut(&channel_id) {
            if let Some(difficulty_manager) = upstream_channel
                .downstream_manager
                .difficulty_config
                .get_mut(&connection_id)
            {
                difficulty_manager.timestamp_of_last_update = timestamp_since_last_update;
            }
        }
    }
}

// Just struct this for non-aggregation case first.
#[derive(Debug)]
pub struct ChannelManager {
    // Channel extranonce distributor.
    pub extended_extranonce_factory: ExtendedExtranonce,
    // Share account.
    pub share_accounting: HashMap<Sv1ChannelId, ShareAccounting>,
    // expected share per minute from config.
    pub expected_share_per_minute: f32,
    // Difficulty config per connection
    pub difficulty_config: HashMap<Sv1ChannelId, DownstreamDifficultyConfig>,
    // ID generator
    pub downstream_id_factory: Id,
    // Prevhash
    pub prev_block_hash: Option<SetNewPrevHash<'static>>,
    // future jobs are indexed with job_id (u32)
    pub future_jobs: HashMap<u32, NewExtendedMiningJob<'static>>,
    // Currently active job shared by upstream
    pub active_job: Option<NewExtendedMiningJob<'static>>,
    // past jobs are indexed with job_id (u32)
    pub past_jobs: HashMap<u32, NewExtendedMiningJob<'static>>,
    // stale jobs are indexed with job_id (u32)
    pub stale_jobs: HashMap<u32, NewExtendedMiningJob<'static>>,
    // Channel id
    pub channel_id: u32,
}

#[derive(Debug, Clone)]
pub struct DownstreamDifficultyConfig {
    pub min_individual_miner_hashrate: f32,
    pub target: Target,
    pub submits_since_last_update: u32,
    pub timestamp_of_last_update: u64,
}

impl DownstreamDifficultyConfig {
    fn new() -> Self {
        Self {
            min_individual_miner_hashrate: 10_000_000_000_000.0,
            submits_since_last_update: 0,
            timestamp_of_last_update: 0,
            target: u256_from_int(u64::MAX).into(),
        }
    }
}

impl ChannelManager {
    pub fn new(
        extranonce_prefix: Extranonce,
        extranonce_prefix_len: usize,
        extranonce_size: usize,
        min_extranonce_size: usize,
        expected_share_per_minute: f32,
        channel_id: u32,
    ) -> Self {
        let tproxy_len = proxy_extranonce1_len(extranonce_size, min_extranonce_size);
        let range_0 = 0..extranonce_prefix_len;
        let range_1 = extranonce_prefix_len..extranonce_prefix_len + tproxy_len;
        let range_2 = extranonce_prefix_len + tproxy_len..extranonce_prefix_len + extranonce_size;
        let extended_extranonce_factory = ExtendedExtranonce::from_upstream_extranonce(
            extranonce_prefix,
            range_0,
            range_1,
            range_2,
        )
        .expect("Something went wrong extranonce factory");

        Self {
            extended_extranonce_factory,
            share_accounting: HashMap::new(),
            expected_share_per_minute,
            difficulty_config: HashMap::new(),
            downstream_id_factory: Id::new(),
            prev_block_hash: None,
            future_jobs: HashMap::new(),
            active_job: None,
            past_jobs: HashMap::new(),
            stale_jobs: HashMap::new(),
            channel_id,
        }
    }

    /// What I need to do:
    /// 1. I should generate an Id to it.
    /// 2. I should assign a extranonce field for new downstream
    /// 3. I should add an entry in share_accounter
    /// 4. I should add an entry in difficulty_config
    pub fn on_new_downstream_connection(
        &mut self,
        user_identity: String,
    ) -> (u32, Sv1ChannelId, Vec<u8>, usize) {
        let new_downstream_id = Sv1ChannelId(self.downstream_id_factory.next());
        let max_extranonce2_len = self.extended_extranonce_factory.get_range2_len() as usize;
        let new_extranonce = self
            .extended_extranonce_factory
            .next_prefix_extended(max_extranonce2_len)
            .expect("Should have generated the extranonce prefix");
        let sv1_object = Sv1Channel::new(
            new_downstream_id.clone(),
            user_identity,
            new_extranonce.clone().to_vec(),
            max_extranonce2_len as u16,
        );
        self.share_accounting
            .insert(new_downstream_id.clone(), ShareAccounting::new(0));
        self.difficulty_config
            .insert(new_downstream_id.clone(), DownstreamDifficultyConfig::new());
        (
            self.channel_id,
            new_downstream_id,
            new_extranonce.to_vec(),
            max_extranonce2_len,
        )
    }

    /// validated whether share is acceptable or not
    /// Then share the result to downstream and upstream (if accepted)
    /// Check against active and past jobs.
    pub fn on_submit_share(&mut self, share: SubmitShareWithChannelId) -> bool {
        info!("Got submit share message in channel manager");
        let job_id = share.share.job_id.parse::<u32>().unwrap();
        match self.active_job.clone() {
            Some(active_job) => {
                if job_id == active_job.job_id {
                    return self.validate_share(share, Some(active_job));
                }

                if self.past_jobs.contains_key(&job_id) {
                    return self.validate_share(share, self.past_jobs.get(&job_id).cloned());
                }

                return false;
            }
            None => return false,
        }
    }

    pub fn validate_share(
        &mut self,
        share: SubmitShareWithChannelId,
        job: Option<NewExtendedMiningJob<'static>>,
    ) -> bool {
        info!("Got share {share:?}, for this job {job:?} in channel manager");

        let job_id = share.share.job_id;

        if let Some(active_job) = job {
            let extranonce_prefix = share.extranonce;
            let mut full_extranonce = vec![];
            full_extranonce.extend(extranonce_prefix);
            full_extranonce.extend(share.share.extra_nonce2.as_ref());

            let merkle_root: [u8; 32] = merkle_root_from_path(
                active_job.coinbase_tx_prefix.inner_as_ref(),
                active_job.coinbase_tx_suffix.inner_as_ref(),
                &full_extranonce,
                &active_job.merkle_path.inner_as_ref(),
            )
            .unwrap()
            .try_into()
            .expect("merkle root must be 32 bytes");

            if let Some(prev_hash) = self.prev_block_hash.as_ref() {
                let prev_block_hash = prev_hash.prev_hash.clone();
                let nbits = CompactTarget::from_consensus(prev_hash.nbits);

                let request_version = share
                    .share
                    .version_bits
                    .clone()
                    .map(|vb| vb.0)
                    .unwrap_or(active_job.version);

                let mask = share
                    .version_rolling_mask
                    .unwrap_or(HexU32Be(0x1FFFE000_u32))
                    .0;

                let version = (active_job.version & !mask) | (request_version & mask);

                // create the header for validation
                let header = Header {
                    version: Version::from_consensus(version as i32),
                    prev_blockhash: u256_to_block_hash(prev_block_hash.clone()),
                    merkle_root: (*Hash::from_bytes_ref(&merkle_root)).into(),
                    time: share.share.time.0,
                    bits: nbits,
                    nonce: share.share.nonce.0,
                };

                // convert the header hash to a target type for easy comparison
                let hash = header.block_hash();
                let raw_hash: [u8; 32] = *hash.to_raw_hash().as_ref();
                let hash_as_target: Target = raw_hash.into();
                let hash_as_diff = target_to_difficulty(hash_as_target.clone());

                let network_target = BitcoinTarget::from_compact(nbits);

                // print hash_as_target and self.target as human readable hex
                let hash_as_u256: binary_sv2::U256 = hash_as_target.clone().into();
                let mut hash_bytes = hash_as_u256.to_vec();
                hash_bytes.reverse(); // Convert to big-endian for display

                let difficulty_config = self
                    .difficulty_config
                    .get(&Sv1ChannelId(active_job.channel_id));

                if let Some(difficulty) = difficulty_config {
                    let target = difficulty.target.clone();
                    let target_u256: binary_sv2::U256 = target.clone().into();
                    let mut target_bytes = target_u256.to_vec();
                    target_bytes.reverse();

                    debug!(
                        "share validation \nshare:\t\t{}\nchannel target:\t{}\nnetwork target:\t{}",
                        bytes_to_hex(&hash_bytes),
                        bytes_to_hex(&target_bytes),
                        format!("{:x}", network_target)
                    );

                    if hash_as_target <= target {
                        let share_accounting = self
                            .share_accounting
                            .get_mut(&Sv1ChannelId(active_job.channel_id));
                        if let Some(share_accounting) = share_accounting {
                            if share_accounting.is_share_seen(hash.to_raw_hash()) {
                                return false;
                            }
                            share_accounting.update_share_accounting(
                                target_to_difficulty(target.clone()) as u64,
                                share.share.time.0,
                                hash.to_raw_hash(),
                            );
                            share_accounting.update_best_diff(hash_as_diff);
                            let last_sequence_number =
                                share_accounting.get_last_share_sequence_number();
                            let new_submits_accepted_count = share_accounting.get_shares_accepted();
                            let new_shares_sum = share_accounting.get_share_work_sum();

                            return true;
                        }
                    }
                }
            }
        }

        return false;
    }

    pub fn on_new_prev_hash(&mut self, set_new_prevhash: SetNewPrevHash<'static>) {
        info!("Received new previous block hash in channel manager: {set_new_prevhash:?}");
        let job_id = set_new_prevhash.job_id;
        self.active_job = None;
        if self.future_jobs.contains_key(&job_id) {
            self.active_job = self.future_jobs.get(&job_id).cloned();
        }
        self.prev_block_hash = Some(set_new_prevhash);
        self.future_jobs.clear();
        self.past_jobs.clear();
        self.stale_jobs.clear();
    }

    pub fn on_new_extended_job(&mut self, extended_job: NewExtendedMiningJob<'static>) {
        info!("Received extended mining job in channel manager: {extended_job:?}");
        if extended_job.is_future() {
            self.future_jobs.insert(extended_job.job_id, extended_job);
            return;
        }
        if self.active_job.is_none() {
            self.active_job = Some(extended_job);
            return;
        }

        let past_active_job = self.active_job.take().expect("Active job should be active");
        self.active_job = Some(extended_job);
        self.past_jobs
            .insert(past_active_job.job_id, past_active_job);
    }

    pub fn active_job(&self) -> Option<NewExtendedMiningJob<'static>> {
        self.active_job.clone()
    }

    pub fn current_prev_block_hash(&self) -> Option<SetNewPrevHash<'static>> {
        self.prev_block_hash.clone()
    }

    pub fn get_job(&self, job_id: u32) -> Option<NewExtendedMiningJob<'static>> {
        if let Some(active_job) = self.active_job.clone() {
            if active_job.job_id == job_id {
                return Some(active_job);
            }
        }

        let job = self.past_jobs.get(&job_id);
        if let Some(job) = job {
            if job.job_id == job_id {
                return Some(job.to_owned());
            }
        }

        None
    }
}
