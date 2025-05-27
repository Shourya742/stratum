use roles_logic_sv2::{
    common_messages_sv2::{Protocol, SetupConnection},
    handlers::common::{ParseCommonMessagesFromUpstream, SendTo as SendToCommon},
    Error as RolesLogicError,
};
use tracing::info;

use crate::error::ProxyResult;

use roles_logic_sv2::common_messages_sv2::Reconnect;

use super::upstream::Upstream;

impl Upstream {
    // Creates the initial `SetupConnection` message for the SV2 handshake.
    //
    // This message contains information about the proxy acting as a mining device,
    // including supported protocol versions, flags, and hardcoded endpoint details.
    //
    // TODO: The Mining Device information is currently hardcoded. It should ideally
    // be configurable or derived from the downstream connections.
    #[allow(clippy::result_large_err)]
    pub fn get_setup_connection_message(
        min_version: u16,
        max_version: u16,
        is_work_selection_enabled: bool,
    ) -> ProxyResult<'static, SetupConnection<'static>> {
        let endpoint_host = "0.0.0.0".to_string().into_bytes().try_into()?;
        let vendor = String::new().try_into()?;
        let hardware_version = String::new().try_into()?;
        let firmware = String::new().try_into()?;
        let device_id = String::new().try_into()?;
        let flags = match is_work_selection_enabled {
            false => 0b0000_0000_0000_0000_0000_0000_0000_0100,
            true => 0b0000_0000_0000_0000_0000_0000_0000_0110,
        };
        Ok(SetupConnection {
            protocol: Protocol::MiningProtocol,
            min_version,
            max_version,
            flags,
            endpoint_host,
            endpoint_port: 50,
            vendor,
            hardware_version,
            firmware,
            device_id,
        })
    }
}

impl ParseCommonMessagesFromUpstream for Upstream {
    // Handles the SV2 `SetupConnectionSuccess` message received from the upstream.
    //
    // Returns `Ok(SendToCommon::None(None))` as this message is handled internally
    // and does not require a direct response or forwarding.
    fn handle_setup_connection_success(
        &mut self,
        m: roles_logic_sv2::common_messages_sv2::SetupConnectionSuccess,
    ) -> Result<SendToCommon, RolesLogicError> {
        info!(
            "Received `SetupConnectionSuccess`: version={}, flags={:b}",
            m.used_version, m.flags
        );
        Ok(SendToCommon::None(None))
    }

    fn handle_setup_connection_error(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::SetupConnectionError,
    ) -> Result<SendToCommon, RolesLogicError> {
        todo!()
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        _: roles_logic_sv2::common_messages_sv2::ChannelEndpointChanged,
    ) -> Result<SendToCommon, RolesLogicError> {
        todo!()
    }

    fn handle_reconnect(&mut self, _m: Reconnect) -> Result<SendToCommon, RolesLogicError> {
        todo!()
    }
}
