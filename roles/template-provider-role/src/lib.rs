#![allow(warnings)]
use std::time::Duration;

use async_channel::{Receiver, Sender};
use codec_sv2::{Frame, HandshakeRole, StandardEitherFrame, StandardSv2Frame};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use network_helpers_sv2::noise_connection::Connection;
use noise_sv2::Responder;
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
mod message_utils;
mod provider;
mod rpc_client;
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

use provider::Provider;
use rpc_client::error::RpcClientError;
use tracing::{debug, error};

#[derive(Debug, Clone)]
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
            authority_public_key: authority_public_key
                .parse()
                .expect("Invalid authority public key"),
            authority_secret_key: authority_secret_key
                .parse()
                .expect("Invalid authority secret key"),
            cert_validity_sec,
            socket_path,
        }
    }
}

/// Main entry point to start the Template Provider service.
pub async fn start_template_provider(config: Config) {
    info!("Starting Template Provider service...");
    let (prev_hash_sender, prev_hash_receiver) = async_channel::unbounded::<EitherFrame>();
    let (template_sender, template_receiver) = async_channel::unbounded::<EitherFrame>();
    let (tx_data_response_sender, tx_data_response_receiver) =
        async_channel::unbounded::<EitherFrame>();
    let (message_sender, message_receiver) = async_channel::unbounded::<EitherFrame>();
    info!("Communication channels initialized.");
    let network_config = config.clone();
    tokio::spawn(async move {
        listen_for_connections(
            network_config,
            prev_hash_receiver,
            template_receiver,
            tx_data_response_receiver,
            message_sender,
        )
        .await;
        error!("Network listener task finished unexpectedly.");
    });

    let local_set = LocalSet::new();
    info!("Running Provider initialization and loop within LocalSet.");

    local_set
        .run_until(async move {
            info!("Attempting to initialize Provider...");
            match Provider::new(
                prev_hash_sender,
                template_sender,
                tx_data_response_sender,
                message_receiver,
                config.sv2feedelta,
                config.sv2interval,
                config.socket_path,
            )
            .await
            {
                Ok(provider) => {
                    info!("Provider initialized successfully. Starting run loop.");
                    provider.run().await;
                    info!("Provider run loop finished.");
                }
                Err(RpcClientError::Connection(e)) => {
                    error!(
                        "Provider initialization failed: Could not connect to RPC socket: {}",
                        e
                    );
                }
                Err(RpcClientError::Capnp(e)) => {
                    error!(
                        "Provider initialization failed: Cap'n Proto RPC error: {}",
                        e
                    );
                }
                Err(e) => {
                    error!("Provider initialization failed");
                }
            }
            // If initialization fails, the application might effectively stop here.
            // Consider more robust shutdown logic if needed.
            error!("Provider task finished or failed to initialize.");
        })
        .await;

    info!("Template Provider LocalSet finished execution.");
}

async fn listen_for_connections(
    config: Config,
    prev_hash_receiver: Receiver<EitherFrame>,
    template_receiver: Receiver<EitherFrame>,
    tx_data_response_receiver: Receiver<EitherFrame>,
    message_sender: Sender<EitherFrame>,
) {
    let bind_addr = format!("{}:{}", config.sv2bind, config.sv2port);
    let listener = match TcpListener::bind(&bind_addr).await {
        Ok(l) => {
            info!("SV2 Listener started on: {}", bind_addr);
            l
        }
        Err(e) => {
            error!("Failed to bind TCP listener to {}: {}", bind_addr, e);
            return;
        }
    };

    loop {
        match listener.accept().await {
            Ok((stream, address)) => {
                info!("Accepted SV2 connection from: {}", address);
                let conn_prev_hash_receiver = prev_hash_receiver.clone();
                let conn_template_receiver = template_receiver.clone();
                let conn_tx_data_receiver = tx_data_response_receiver.clone();
                let conn_message_sender = message_sender.clone();
                let conn_config = config.clone();

                tokio::spawn(async move {
                    info!("Spawning connection handler for {}", address);
                    if let Err(e) = handle_connection(
                        conn_config,
                        stream,
                        address,
                        conn_prev_hash_receiver,
                        conn_template_receiver,
                        conn_tx_data_receiver,
                        conn_message_sender,
                    )
                    .await
                    {
                        error!("Error handling connection from {}: {}", address, e);
                    } else {
                        info!("Connection handler finished gracefully for {}", address);
                    }
                });
            }
            Err(e) => {
                error!("Error accepting connection: {}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

type ConnectionHandlerError = Box<dyn std::error::Error + Send + Sync>;

async fn handle_connection(
    config: Config,
    stream: tokio::net::TcpStream,
    address: std::net::SocketAddr,
    prev_hash_receiver: Receiver<EitherFrame>,
    template_receiver: Receiver<EitherFrame>,
    tx_data_response_receiver: Receiver<EitherFrame>,
    message_sender: Sender<EitherFrame>,
) -> Result<(), ConnectionHandlerError> {
    let responder = Responder::from_authority_kp(
        &config.authority_public_key.into_bytes(),
        &config.authority_secret_key.into_bytes(),
        std::time::Duration::from_secs(config.cert_validity_sec),
    )
    .unwrap();
    let (mut receiver, mut sender) = Connection::new(stream, HandshakeRole::Responder(responder))
        .await
        .unwrap();

    _ = SetupConnectionHandler::setup(&mut receiver, &mut sender, address)
        .await
        .unwrap();

    info!("SV2 SetupConnection successful with {}", address);
    tokio::spawn(forward_messages(
        sender.clone(),
        prev_hash_receiver,
        "PrevHash",
    ));
    tokio::spawn(forward_messages(
        sender.clone(),
        template_receiver,
        "Template",
    ));
    tokio::spawn(forward_messages(
        sender.clone(),
        tx_data_response_receiver,
        "TxDataResponse",
    ));

    loop {
        match receiver.recv().await {
            Ok(msg) => {
                if message_sender.send(msg).await.is_err() {
                    error!("Failed to forward message to Provider; channel closed. Closing connection handler for {}.", address);
                    break;
                }
            }
            Err(e) => {
                info!("Connection closed by peer {}", address);
                return Err(e.into());
            }
        }
    }
    Ok(())
}

async fn forward_messages(
    sender: Sender<EitherFrame>,
    receiver: Receiver<EitherFrame>,
    message_type_name: &'static str,
) {
    debug!(
        "Starting forwarder task for {} messages.",
        message_type_name
    );
    while let Ok(msg) = receiver.recv().await {
        if sender.send(msg).await.is_err() {
            debug!(
                "Failed to forward {} message; connection likely closed.",
                message_type_name
            );
            break;
        }
    }
    debug!(
        "Stopping forwarder task for {} messages (receiver closed or send failed).",
        message_type_name
    );
}
