use roles_logic_sv2::{
    common_messages_sv2::Protocol,
    common_properties::{IsMiningUpstream, IsUpstream},
    handlers::mining::{ParseMiningMessagesFromUpstream, SendTo, SupportedChannelTypes},
    mining_sv2::{NewExtendedMiningJob, SetNewPrevHash},
    parsers::Mining,
    Error as RolesLogicError,
};
use tracing::info;

use crate::{
    channel_manager::ChannelManager, downstream_sv1::Downstream,
    upstream_sv2::upstream::IS_NEW_JOB_HANDLED,
};

use tracing::{debug, error, warn};

use roles_logic_sv2::mining_sv2::SetGroupChannel;

use super::upstream::Upstream;

// Can be removed?
impl IsUpstream for Upstream {
    fn get_version(&self) -> u16 {
        todo!()
    }

    fn get_flags(&self) -> u32 {
        todo!()
    }

    fn get_supported_protocols(&self) -> Vec<Protocol> {
        todo!()
    }

    fn get_id(&self) -> u32 {
        todo!()
    }

    fn get_mapper(&mut self) -> Option<&mut roles_logic_sv2::common_properties::RequestIdMapper> {
        todo!()
    }
}

// Can be removed?
impl IsMiningUpstream for Upstream {
    fn total_hash_rate(&self) -> u64 {
        todo!()
    }

    fn add_hash_rate(&mut self, _to_add: u64) {
        todo!()
    }

    fn get_opened_channels(
        &mut self,
    ) -> &mut Vec<roles_logic_sv2::common_properties::UpstreamChannel> {
        todo!()
    }

    fn update_channels(&mut self, _c: roles_logic_sv2::common_properties::UpstreamChannel) {
        todo!()
    }
}

/// Connection-wide SV2 Upstream role messages parser implemented by a downstream ("downstream"
/// here is relative to the SV2 Upstream role and is represented by this `Upstream` struct).
impl ParseMiningMessagesFromUpstream<Downstream> for Upstream {
    /// Returns the type of channel used between this proxy and the SV2 Upstream.
    /// For a Translator Proxy, this is always `Extended`.
    fn get_channel_type(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Extended
    }

    /// Indicates whether work selection is enabled for this upstream connection.
    /// For a Translator Proxy, work selection is handled by the upstream pool,
    /// so this method always returns `false`.
    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    /// The SV2 `OpenStandardMiningChannelSuccess` message is NOT handled because it is NOT used
    /// for the Translator Proxy as only `Extended` channels are used between the SV1/SV2 Translator
    /// Proxy and the SV2 Upstream role.
    fn handle_open_standard_mining_channel_success(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::OpenStandardMiningChannelSuccess,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        panic!("Standard Mining Channels are not used in Translator Proxy")
    }

    /// Handles the SV2 `OpenExtendedMiningChannelSuccess` message.
    ///
    /// This message is received after requesting to open an extended mining channel.
    /// It provides the assigned `channel_id`, the extranonce prefix, the initial
    /// mining `target`, and the expected `extranonce_size`. It stores the `channel_id` and
    /// `extranonce_prefix`, updates the shared `target`, and prepares the extranonce
    /// information (including calculating the size for the TProxy's added extranonce1) to be
    /// sent to the Downstream handler for use with SV1 clients.
    ///
    /// Returns `Ok(SendTo<Downstream>::None(Some(Mining::OpenExtendedMiningChannelSuccess)))`
    /// to indicate that the message has been handled internally and should be
    /// forwarded to the Bridge.
    fn handle_open_extended_mining_channel_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::OpenExtendedMiningChannelSuccess,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!(
            "Received OpenExtendedMiningChannelSuccess with request id: {} and channel id: {}",
            m.request_id, m.channel_id
        );
        debug!("OpenStandardMiningChannelSuccess: {:?}", m);
        let tproxy_e1_len = super::super::utils::proxy_extranonce1_len(
            m.extranonce_size as usize,
            self.min_extranonce_size.into(),
        ) as u16;
        if self.min_extranonce_size + tproxy_e1_len < m.extranonce_size {
            return Err(RolesLogicError::InvalidExtranonceSize(
                self.min_extranonce_size,
                m.extranonce_size,
            ));
        }
        self.target.safe_lock(|t| *t = m.target.to_vec())?;

        info!("Up: Successfully Opened Extended Mining Channel");
        self.channel_id = Some(m.channel_id);
        self.extranonce_prefix = Some(m.extranonce_prefix.to_vec());

        _ = self.upstream_channel_manager.safe_lock(|e| {
            info!("Updating upstream channel manager state with new upstream connection");

            e.channel_ids.insert(m.channel_id);
            let downstream_channel_manager = ChannelManager::new(
                m.extranonce_prefix.clone().into(),
                m.extranonce_prefix.to_vec().len(),
                m.extranonce_size as usize,
                self.min_extranonce_size as usize,
                self.shares_per_minute,
            );
            e.downstream_managers
                .insert(m.channel_id, downstream_channel_manager);
            // Remove this unwrap from here, this can be handled better.
            let upstream_difficulty = self
                .difficulty_config
                .safe_lock(|upstream| upstream.clone())
                .unwrap();
            e.upstream_difficulty
                .insert(m.channel_id, upstream_difficulty)
        })?;

        let m = Mining::OpenExtendedMiningChannelSuccess(m.into_static());
        Ok(SendTo::None(Some(m)))
    }

    /// Handles the SV2 `OpenExtendedMiningChannelError` message (TODO).
    fn handle_open_mining_channel_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::OpenMiningChannelError,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        error!(
            "Received OpenExtendedMiningChannelError with error code {}",
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
        );
        Ok(SendTo::None(Some(Mining::OpenMiningChannelError(
            m.as_static(),
        ))))
    }
    /// Handles the SV2 `UpdateChannelError` message (TODO).
    fn handle_update_channel_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::UpdateChannelError,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        error!(
            "Received UpdateChannelError with error code {}",
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
        );
        Ok(SendTo::None(Some(Mining::UpdateChannelError(
            m.as_static(),
        ))))
    }

    /// Handles the SV2 `CloseChannel` message (TODO).
    fn handle_close_channel(
        &mut self,
        m: roles_logic_sv2::mining_sv2::CloseChannel,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!("Received CloseChannel for channel id: {}", m.channel_id);

        self.upstream_channel_manager.safe_lock(|u| {
            // Todo improve this.
            u.remove(m.channel_id);
        })?;

        Ok(SendTo::None(Some(Mining::CloseChannel(m.as_static()))))
    }

    /// Handles the SV2 `SetExtranoncePrefix` message (TODO).
    fn handle_set_extranonce_prefix(
        &mut self,
        _: roles_logic_sv2::mining_sv2::SetExtranoncePrefix,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        todo!()
    }

    /// Handles the SV2 `SubmitSharesSuccess` message.
    fn handle_submit_shares_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SubmitSharesSuccess,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!("Received SubmitSharesSuccess");
        debug!("SubmitSharesSuccess: {:?}", m);
        Ok(SendTo::None(None))
    }

    /// Handles the SV2 `SubmitSharesError` message.
    fn handle_submit_shares_error(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SubmitSharesError,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        error!(
            "Received SubmitSharesError with error code {}",
            std::str::from_utf8(m.error_code.as_ref()).unwrap_or("unknown error code")
        );
        Ok(SendTo::None(None))
    }

    /// The SV2 `NewMiningJob` message is NOT handled because it is NOT used for the Translator
    /// Proxy as only `Extended` channels are used between the SV1/SV2 Translator Proxy and the SV2
    /// Upstream role.
    fn handle_new_mining_job(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::NewMiningJob,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        panic!("Standard Mining Channels are not used in Translator Proxy")
    }

    /// Handles the SV2 `NewExtendedMiningJob` message which is used (along with the SV2
    /// `SetNewPrevHash` message) to later create a SV1 `mining.notify` for the Downstream
    /// role.
    fn handle_new_extended_mining_job(
        &mut self,
        m: NewExtendedMiningJob,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!(
            "Received new extended mining job for channel id: {} with job id: {} is_future: {}",
            m.channel_id,
            m.job_id,
            m.is_future()
        );
        debug!("NewExtendedMiningJob: {:?}", m);

        self.upstream_channel_manager.safe_lock(|u| {
            let channel_manager = u.downstream_managers.get_mut(&m.channel_id);
            if let Some(channel_manager) = channel_manager {
                channel_manager.on_new_extended_job(m.clone().as_static());
            }
        })?;

        if self.is_work_selection_enabled() {
            Ok(SendTo::None(None))
        } else {
            IS_NEW_JOB_HANDLED.store(false, std::sync::atomic::Ordering::SeqCst);
            if !m.version_rolling_allowed {
                warn!("VERSION ROLLING NOT ALLOWED IS A TODO");
                // todo!()
            }

            let message = Mining::NewExtendedMiningJob(m.into_static());

            Ok(SendTo::None(Some(message)))
        }
    }

    /// Handles the SV2 `SetNewPrevHash` message which is used (along with the SV2
    /// `NewExtendedMiningJob` message) to later create a SV1 `mining.notify` for the Downstream
    /// role.
    fn handle_set_new_prev_hash(
        &mut self,
        m: SetNewPrevHash,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!(
            "Received SetNewPrevHash channel id: {}, job id: {}",
            m.channel_id, m.job_id
        );

        self.upstream_channel_manager.safe_lock(|u| {
            let channel_manager = u.downstream_managers.get_mut(&m.channel_id);
            if let Some(channel_manager) = channel_manager {
                channel_manager.on_new_prev_hash(m.clone().as_static());
            }
        })?;

        debug!("SetNewPrevHash: {:?}", m);
        if self.is_work_selection_enabled() {
            Ok(SendTo::None(None))
        } else {
            let message = Mining::SetNewPrevHash(m.into_static());
            Ok(SendTo::None(Some(message)))
        }
    }

    /// Handles the SV2 `SetCustomMiningJobSuccess` message (TODO).
    fn handle_set_custom_mining_job_success(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetCustomMiningJobSuccess,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!(
            "Received SetCustomMiningJobSuccess for channel id: {} for job id: {}",
            m.channel_id, m.job_id
        );
        debug!("SetCustomMiningJobSuccess: {:?}", m);
        self.last_job_id = Some(m.job_id);
        Ok(SendTo::None(None))
    }

    /// Handles the SV2 `SetCustomMiningJobError` message (TODO).
    fn handle_set_custom_mining_job_error(
        &mut self,
        _m: roles_logic_sv2::mining_sv2::SetCustomMiningJobError,
    ) -> Result<roles_logic_sv2::handlers::mining::SendTo<Downstream>, RolesLogicError> {
        unimplemented!()
    }

    /// Handles the SV2 `SetTarget` message which updates the Downstream role(s) target
    /// difficulty via the SV1 `mining.set_difficulty` message.
    fn handle_set_target(
        &mut self,
        m: roles_logic_sv2::mining_sv2::SetTarget,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        info!("Received SetTarget for channel id: {}", m.channel_id);
        debug!("SetTarget: {:?}", m);
        let m = m.into_static();
        self.target.safe_lock(|t| *t = m.maximum_target.to_vec())?;
        Ok(SendTo::None(None))
    }

    fn handle_set_group_channel(
        &mut self,
        _m: SetGroupChannel,
    ) -> Result<SendTo<Downstream>, RolesLogicError> {
        todo!()
    }
}
