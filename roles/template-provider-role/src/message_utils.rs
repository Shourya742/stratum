use codec_sv2::Frame;
use roles_logic_sv2::parsers::{
    message_type_to_name, AnyMessage, CommonMessages, IsSv2Message,
    JobDeclaration::{
        AllocateMiningJobToken, AllocateMiningJobTokenSuccess, DeclareMiningJob,
        DeclareMiningJobError, DeclareMiningJobSuccess, IdentifyTransactions,
        IdentifyTransactionsSuccess, ProvideMissingTransactions, ProvideMissingTransactionsSuccess,
        PushSolution,
    },
    TemplateDistribution::{self, CoinbaseOutputConstraints},
};

use crate::EitherFrame;

pub fn message_from_frame(frame: &mut EitherFrame) -> (u8, AnyMessage<'static>) {
    match frame {
        Frame::Sv2(frame) => {
            if let Some(header) = frame.get_header() {
                let message_type = header.msg_type();
                let mut payload = frame.payload().to_vec();
                let message: Result<AnyMessage<'_>, _> =
                    (message_type, payload.as_mut_slice()).try_into();
                match message {
                    Ok(message) => {
                        let message = into_static(message);
                        (message_type, message)
                    }
                    _ => {
                        println!("Received frame with invalid payload or message type: {frame:?}");
                        panic!();
                    }
                }
            } else {
                println!("Received frame with invalid header: {frame:?}");
                panic!();
            }
        }
        Frame::HandShake(f) => {
            println!("Received unexpected handshake frame: {f:?}");
            panic!();
        }
    }
}

pub fn into_static(m: AnyMessage<'_>) -> AnyMessage<'static> {
    match m {
        AnyMessage::Mining(m) => AnyMessage::Mining(m.into_static()),
        AnyMessage::Common(m) => match m {
            CommonMessages::ChannelEndpointChanged(m) => {
                AnyMessage::Common(CommonMessages::ChannelEndpointChanged(m.into_static()))
            }
            CommonMessages::SetupConnection(m) => {
                AnyMessage::Common(CommonMessages::SetupConnection(m.into_static()))
            }
            CommonMessages::SetupConnectionError(m) => {
                AnyMessage::Common(CommonMessages::SetupConnectionError(m.into_static()))
            }
            CommonMessages::SetupConnectionSuccess(m) => {
                AnyMessage::Common(CommonMessages::SetupConnectionSuccess(m.into_static()))
            }
            CommonMessages::Reconnect(m) => {
                AnyMessage::Common(CommonMessages::Reconnect(m.into_static()))
            }
        },
        AnyMessage::JobDeclaration(m) => match m {
            AllocateMiningJobToken(m) => {
                AnyMessage::JobDeclaration(AllocateMiningJobToken(m.into_static()))
            }
            AllocateMiningJobTokenSuccess(m) => {
                AnyMessage::JobDeclaration(AllocateMiningJobTokenSuccess(m.into_static()))
            }
            DeclareMiningJob(m) => AnyMessage::JobDeclaration(DeclareMiningJob(m.into_static())),
            DeclareMiningJobError(m) => {
                AnyMessage::JobDeclaration(DeclareMiningJobError(m.into_static()))
            }
            DeclareMiningJobSuccess(m) => {
                AnyMessage::JobDeclaration(DeclareMiningJobSuccess(m.into_static()))
            }
            IdentifyTransactions(m) => {
                AnyMessage::JobDeclaration(IdentifyTransactions(m.into_static()))
            }
            IdentifyTransactionsSuccess(m) => {
                AnyMessage::JobDeclaration(IdentifyTransactionsSuccess(m.into_static()))
            }
            ProvideMissingTransactions(m) => {
                AnyMessage::JobDeclaration(ProvideMissingTransactions(m.into_static()))
            }
            ProvideMissingTransactionsSuccess(m) => {
                AnyMessage::JobDeclaration(ProvideMissingTransactionsSuccess(m.into_static()))
            }
            PushSolution(m) => AnyMessage::JobDeclaration(PushSolution(m.into_static())),
        },
        AnyMessage::TemplateDistribution(m) => match m {
            CoinbaseOutputConstraints(m) => {
                AnyMessage::TemplateDistribution(CoinbaseOutputConstraints(m.into_static()))
            }
            TemplateDistribution::NewTemplate(m) => {
                AnyMessage::TemplateDistribution(TemplateDistribution::NewTemplate(m.into_static()))
            }
            TemplateDistribution::RequestTransactionData(m) => AnyMessage::TemplateDistribution(
                TemplateDistribution::RequestTransactionData(m.into_static()),
            ),
            TemplateDistribution::RequestTransactionDataError(m) => {
                AnyMessage::TemplateDistribution(TemplateDistribution::RequestTransactionDataError(
                    m.into_static(),
                ))
            }
            TemplateDistribution::RequestTransactionDataSuccess(m) => {
                AnyMessage::TemplateDistribution(
                    TemplateDistribution::RequestTransactionDataSuccess(m.into_static()),
                )
            }
            TemplateDistribution::SetNewPrevHash(m) => AnyMessage::TemplateDistribution(
                TemplateDistribution::SetNewPrevHash(m.into_static()),
            ),
            TemplateDistribution::SubmitSolution(m) => AnyMessage::TemplateDistribution(
                TemplateDistribution::SubmitSolution(m.into_static()),
            ),
        },
    }
}
