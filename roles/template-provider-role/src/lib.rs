#![allow(warnings)]
use async_channel::{Receiver, Sender};
use codec_sv2::{Frame, HandshakeRole, StandardEitherFrame, StandardSv2Frame};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use network_helpers_sv2::noise_connection::Connection;
use noise_sv2::Responder;
use provider::start_provider;
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
use setup_connection::SetupConnectionHandler;
use tokio::{net::TcpListener, task::LocalSet};
use tracing::info;

mod error;
mod provider;
mod setup_connection;

pub type Message = AnyMessage<'static>;
pub type StdFrame = StandardSv2Frame<Message>;
pub type EitherFrame = StandardEitherFrame<Message>;

pub mod common_capnp {
    include!(concat!(env!("OUT_DIR"), "/common_capnp.rs"));
}
pub mod echo_capnp {
    include!(concat!(env!("OUT_DIR"), "/echo_capnp.rs"));
}
pub mod init_capnp {
    include!(concat!(env!("OUT_DIR"), "/init_capnp.rs"));
}
pub mod mining_capnp {
    include!(concat!(env!("OUT_DIR"), "/mining_capnp.rs"));
}
pub mod proxy_capnp {
    include!(concat!(env!("OUT_DIR"), "/proxy_capnp.rs"));
}

#[derive(Debug)]
pub struct Config {
    sv2bind: String,
    sv2port: u16,
    sv2interval: u64,
    sv2feedelta: u64,
    socket_path: String,
    authority_public_key: Secp256k1PublicKey,
    authority_secret_key: Secp256k1SecretKey,
    cert_validity_sec: u64,
}

impl Config {
    pub fn new(
        sv2bind: String,
        sv2port: u16,
        sv2interval: u64,
        sv2feedelta: u64,
        socket_path: String,
    ) -> Self {
        let authority_public_key = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72";
        let authority_secret_key = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n";
        let cert_validity_sec = 3600;
        Config {
            sv2bind,
            sv2port,
            sv2interval,
            sv2feedelta,
            authority_public_key: authority_public_key.parse().unwrap(),
            authority_secret_key: authority_secret_key.parse().unwrap(),
            cert_validity_sec,
            socket_path,
        }
    }
}

pub async fn start_template_provider(
    Config {
        sv2bind,
        sv2port,
        sv2feedelta,
        sv2interval,
        authority_public_key,
        authority_secret_key,
        cert_validity_sec,
        socket_path,
    }: Config,
) {
    let (prev_hash_sender, prev_hash_receiver) = async_channel::unbounded::<EitherFrame>();
    let (template_sender, template_receiver) = async_channel::unbounded::<EitherFrame>();
    let (tx_data_response_sender, tx_data_response_receiver) =
        async_channel::unbounded::<EitherFrame>();
    info!("All sender side channels are spawned");

    let (message_sender, message_receiver) = async_channel::unbounded::<EitherFrame>();
    info!("All receiver side channels are spawned");

    tokio::spawn(async move {
        let listener = TcpListener::bind(format!("{sv2bind}:{sv2port}"))
            .await
            .unwrap();
        info!("Starting listening: {}:{}", sv2bind, sv2port);

        while let Ok((stream, address)) = listener.accept().await {
            let prev_hash_receiver_clone = prev_hash_receiver.clone();
            let template_receiver_clone = template_receiver.clone();
            let tx_data_response_receiver_clone = tx_data_response_receiver.clone();
            let message_sender_clone = message_sender.clone();

            tokio::spawn(async move {
                let responder = Responder::from_authority_kp(
                    &authority_public_key.into_bytes(),
                    &authority_secret_key.into_bytes(),
                    std::time::Duration::from_secs(cert_validity_sec),
                )
                .unwrap();
                let (mut receiver, mut sender) =
                    Connection::new(stream, HandshakeRole::Responder(responder))
                        .await
                        .unwrap();
                let _ = SetupConnectionHandler::setup(&mut receiver, &mut sender, address)
                    .await
                    .unwrap();

                tokio::spawn(send_client_prevhash(
                    sender.clone(),
                    prev_hash_receiver_clone,
                ));
                tokio::spawn(send_client_template(
                    sender.clone(),
                    template_receiver_clone,
                ));
                tokio::spawn(send_client_tx_data_response(
                    sender.clone(),
                    tx_data_response_receiver_clone,
                ));

                while let Ok(msg) = receiver.recv().await {
                    println!("message: {:?}", msg);
                    let _ = message_sender_clone.send(msg).await;
                }
            });
        }
    });

    let local_set = LocalSet::new();

    local_set
        .run_until(start_provider(
            prev_hash_sender,
            template_sender,
            tx_data_response_sender,
            message_receiver,
            sv2feedelta,
            sv2interval,
            socket_path,
        ))
        .await;
}

async fn send_client_prevhash(
    sender: Sender<EitherFrame>,
    prev_hash_receiver: Receiver<EitherFrame>,
) {
    info!("Prevhash client setup");
    while let Ok(msg) = prev_hash_receiver.recv().await {
        _ = sender.send(msg).await;
    }
}

async fn send_client_template(
    sender: Sender<EitherFrame>,
    template_receiver: Receiver<EitherFrame>,
) {
    info!("Template client setup");
    while let Ok(msg) = template_receiver.recv().await {
        _ = sender.send(msg).await;
    }
}

async fn send_client_tx_data_response(
    sender: Sender<EitherFrame>,
    tx_data_response_receiver: Receiver<EitherFrame>,
) {
    info!("Transaction data client setup");
    while let Ok(msg) = tx_data_response_receiver.recv().await {
        _ = sender.send(msg).await;
    }
}

fn message_from_frame(frame: &mut EitherFrame) -> (u8, AnyMessage<'static>) {
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

fn into_static(m: AnyMessage<'_>) -> AnyMessage<'static> {
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
