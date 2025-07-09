use crate::{errors::Error, parsers::CommonMessages};
use common_messages_sv2::{
    ChannelEndpointChanged, Reconnect, SetupConnectionError,
    SetupConnectionSuccess, *,
};
use core::convert::TryInto;

pub trait ParseCommonMessagesFromUpstream{
    fn handle_common_message(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<Option<CommonMessages<'static>>, Error> {
        let parsed: CommonMessages<'_> = (message_type, payload).try_into()?;
        self.dispatch_common_message(parsed)
    }

    fn dispatch_common_message(
        &mut self,
        message: CommonMessages<'_>,
    ) -> Result<Option<CommonMessages<'static>>, Error> {
        match message {
            CommonMessages::SetupConnectionSuccess(msg) => {
                self.handle_setup_connection_success(msg)
            }
            CommonMessages::SetupConnectionError(msg) => {
                self.handle_setup_connection_error(msg)
            }
            CommonMessages::ChannelEndpointChanged(msg) => {
                self.handle_channel_endpoint_changed(msg)
            }
            CommonMessages::Reconnect(msg) => self.handle_reconnect(msg),

            CommonMessages::SetupConnection(_) => {
                Err(Error::UnexpectedMessage(MESSAGE_TYPE_SETUP_CONNECTION))
            }
        }
    }

    fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
    ) -> Result<Option<CommonMessages<'static>>, Error> {
        let _ = msg;
        Ok(None)
    }

    fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError,
    ) -> Result<Option<CommonMessages<'static>>, Error> {
        let _ = msg;
        Ok(None)
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
    ) -> Result<Option<CommonMessages<'static>>, Error> {
        let _ = msg;
        Ok(None)
    }

    fn handle_reconnect(
        &mut self,
        msg: Reconnect,
    ) -> Result<Option<CommonMessages<'static>>, Error> {
        let _ = msg;
        Ok(None)
    }
}
