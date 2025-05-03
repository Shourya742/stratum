use super::error::{TPError, TPResult};
use super::StdFrame;
use super::EitherFrame;
use async_channel::{Receiver, Sender};
use roles_logic_sv2::common_messages_sv2::SetupConnectionSuccess;
use roles_logic_sv2::handlers::common::ParseCommonMessagesFromDownstream;
use roles_logic_sv2::{
    common_messages_sv2::SetupConnection,
    errors::Error,
    handlers::common::SendTo,
    parsers::{AnyMessage, CommonMessages},
    utils::Mutex,
};
use std::{convert::TryInto, net::SocketAddr, sync::Arc};
use tracing::{debug, error, info};

pub struct SetupConnectionHandler;

impl SetupConnectionHandler {

    pub async fn setup(
        receiver: &mut Receiver<EitherFrame>,
        sender: &mut Sender<EitherFrame>,
        address: SocketAddr,
    ) -> TPResult<()> {

        let mut incoming: StdFrame = match receiver.recv().await {
            Ok(EitherFrame::Sv2(s)) => {
                debug!("Got sv2 message: {:?}", s);
                s
            }
            Ok(EitherFrame::HandShake(s)) => {
                error!(
                    "Got unexpected handshake message from upstream: {:?} at {}",
                    s, address
                );
                panic!()
            }
            Err(e) => {
                error!("Error receiving message: {:?}", e);
                return Err(Error::NoDownstreamsConnected.into());
            }
        };

        let message_type = incoming
            .get_header()
            .ok_or_else(|| TPError::Custom(String::from("No header set")))?
            .msg_type();
        let payload = incoming.payload();
        let response = ParseCommonMessagesFromDownstream::handle_message_common(
            Arc::new(Mutex::new(SetupConnectionHandler)),
            message_type,
            payload,
        )?;

        let message = response.into_message().ok_or(TPError::RolesLogic(
            roles_logic_sv2::Error::NoDownstreamsConnected,
        ))?;

        let sv2_frame: StdFrame = AnyMessage::Common(message.clone()).try_into()?;
        let sv2_frame = sv2_frame.into();
        sender.send(sv2_frame).await?;

        Ok(())
    }
}

impl ParseCommonMessagesFromDownstream for SetupConnectionHandler {
    fn handle_setup_connection(
        &mut self,
        incoming: SetupConnection,
    ) -> Result<roles_logic_sv2::handlers::common::SendTo, Error> {
        info!(
            "Received `SetupConnection`: version={}, flags={:b}",
            incoming.min_version, incoming.flags
        );
        Ok(SendTo::RelayNewMessageToRemote(
            Arc::new(Mutex::new(())),
            CommonMessages::SetupConnectionSuccess(SetupConnectionSuccess {
                flags: incoming.flags,
                used_version: 2,
            }),
        ))
    }
}
