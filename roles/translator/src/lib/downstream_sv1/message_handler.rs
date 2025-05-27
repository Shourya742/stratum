use crate::downstream_sv1;

use super::{Downstream, DownstreamMessages, SubmitShareWithChannelId};

use roles_logic_sv2::common_properties::{IsDownstream, IsMiningDownstream};

use tracing::{debug, info};
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
        info!("Down: Submitting Share {:?}", request);
        debug!("Down: Handling mining.submit: {:?}", &request);

        // TODO: Check if receiving valid shares by adding diff field to Downstream

        let to_send = SubmitShareWithChannelId {
            connection_id: self.connection_id,
            channel_id: self.channel_id,
            share: request.clone(),
            extranonce: self.extranonce1.clone(),
            extranonce2_len: self.extranonce2_len,
            version_rolling_mask: self.version_rolling_mask.clone(),
        };

        self.tx_sv1_bridge
            .try_send(DownstreamMessages::SubmitShares(to_send))
            .unwrap();

        true
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
