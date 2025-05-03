use std::thread;

use async_channel::{Receiver, Sender};
use codec_sv2::{HandshakeRole, StandardEitherFrame, StandardSv2Frame};
use network_helpers_sv2::noise_connection::Connection;
use noise_sv2::Responder;
use provider::start_provider;
use roles_logic_sv2::parsers::AnyMessage;
use setup_connection::SetupConnectionHandler;
use tokio::{net::TcpListener, task::LocalSet};
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};

mod setup_connection;
mod error;
mod provider;

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
    pub fn new(sv2bind: String, sv2port: u16, sv2interval: u64, sv2feedelta: u64, socket_path: String) -> Self {
        let authority_public_key = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72";
        let authority_secret_key = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n";
        let cert_validity_sec = 3600;
        Config { sv2bind, sv2port, sv2interval, sv2feedelta, authority_public_key: authority_public_key.parse().unwrap(), authority_secret_key: authority_secret_key.parse().unwrap(), cert_validity_sec, socket_path }
    }
}


pub async fn start_template_provider(Config {sv2bind, sv2port, sv2feedelta, sv2interval, authority_public_key, authority_secret_key, cert_validity_sec, socket_path}: Config) {

    let listener = TcpListener::bind(format!("{sv2bind}:{sv2port}")).await.unwrap();

    let (prev_hash_sender, prev_hash_receiver) = async_channel::unbounded::<EitherFrame>();
    let (template_sender, template_receiver) = async_channel::unbounded::<EitherFrame>();
    let (tx_data_response_sender, tx_data_response_receiver)  = async_channel::unbounded::<EitherFrame>();

    let (message_sender, message_receiver) = async_channel::unbounded::<EitherFrame>();

    tokio::spawn_blocking(async {
        let local = LocalSet::new();
        local.run_until(start_provider(prev_hash_sender, template_sender, tx_data_response_sender, message_receiver, sv2feedelta, sv2interval, socket_path)).await;
    });
    
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
            ).unwrap();
            let (mut receiver, mut sender) = Connection::new(stream, HandshakeRole::Responder(responder)).await.unwrap();
            let _ = SetupConnectionHandler::setup(&mut receiver, &mut sender, address).await.unwrap();

            tokio::spawn(send_client_prevhash(sender.clone(), prev_hash_receiver_clone));
            tokio::spawn(send_client_template(sender.clone(), template_receiver_clone));
            tokio::spawn(send_client_tx_data_response(sender.clone(), tx_data_response_receiver_clone));

            while let Ok(msg) = receiver.recv().await {
                let _ = message_sender_clone.send(msg).await;
            }
        });
    }
    
}


async fn send_client_prevhash(sender: Sender<EitherFrame>, prev_hash_receiver: Receiver<EitherFrame>) {
    while let Ok(msg) = prev_hash_receiver.recv().await {
        _ = sender.send(msg).await;
    }
}

async fn send_client_template(sender: Sender<EitherFrame>,template_receiver: Receiver<EitherFrame>) {
    while let Ok(msg) = template_receiver.recv().await {
        _ = sender.send(msg).await;
    }
}


async fn send_client_tx_data_response(sender: Sender<EitherFrame>, tx_data_response_receiver: Receiver<EitherFrame>) {
    while let Ok(msg) = tx_data_response_receiver.recv().await {
        _ = sender.send(msg).await;
    }
}
