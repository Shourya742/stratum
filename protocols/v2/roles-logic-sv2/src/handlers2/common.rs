use crate::{errors::Error, parsers::CommonMessages};
use common_messages_sv2::{
    ChannelEndpointChanged, Reconnect, SetupConnectionError, SetupConnectionSuccess, *,
};
use core::convert::TryInto;

pub trait ParseCommonMessagesFromUpstream {
    fn handle_setup_connection_success(
        &mut self,
        msg: SetupConnectionSuccess,
    ) -> Result<Option<CommonMessages<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
        ))
    }

    fn handle_setup_connection_error(
        &mut self,
        msg: SetupConnectionError,
    ) -> Result<Option<CommonMessages<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_SETUP_CONNECTION_ERROR,
        ))
    }

    fn handle_channel_endpoint_changed(
        &mut self,
        msg: ChannelEndpointChanged,
    ) -> Result<Option<CommonMessages<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_CHANNEL_ENDPOINT_CHANGED,
        ))
    }

    fn handle_reconnect(
        &mut self,
        msg: Reconnect,
    ) -> Result<Option<CommonMessages<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_RECONNECT))
    }
}

pub trait ParseCommonMessagesFromDownstream
where
    Self: Sized,
{
    fn handle_setup_connection(
        &mut self,
        msg: SetupConnection,
    ) -> Result<Option<CommonMessages<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_SETUP_CONNECTION))
    }
}
