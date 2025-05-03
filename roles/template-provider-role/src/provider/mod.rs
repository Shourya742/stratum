// use std::path::Path;

use std::path::Path;

use async_channel::{Receiver, Sender};
use setup_connection::connect;
use stratum_common::bitcoin::{block::Header, consensus::Decodable};
use tokio::task::LocalSet;

use super::EitherFrame;
mod setup_connection;

const BLOCK_RESERVED_WEIGHT: u64 = 4_000_000;

macro_rules! send {
    ($expr:expr) => {
        $expr.send().promise.await.unwrap().get().unwrap().get_result()
    };
}

macro_rules! set_thread {
    ($client:expr, $thread:expr) => {
        $client.get().get_context().unwrap().set_thread($thread)
    };
}


pub async fn start_provider(_prev_hash_sender: Sender<EitherFrame>, _template_sender: Sender<EitherFrame>, _tx_data_response_sender: Sender<EitherFrame>, _message_receiver: Receiver<EitherFrame>, _sv2feedelta: u64, _sv2interval: u64, socket_path: String) {
    let local = LocalSet::new();
    local
        .run_until(async move {
            let path = Path::new(&socket_path);
            let (init, thread) = connect(path).await.unwrap();
            let mut mining_req = init.make_mining_request();
            set_thread!(mining_req, thread.clone());
            let mining_client = send!(mining_req).unwrap();
            let mut is_test_mining = mining_client.is_test_chain_request();
            set_thread!(is_test_mining, thread.clone());
            let is_test_chain = send!(is_test_mining);
            println!("Is this a test chain: {is_test_chain}");
            let mut create_block_req = mining_client.create_new_block_request();
            create_block_req
                .get()
                .get_options().unwrap()
                .set_block_reserved_weight(BLOCK_RESERVED_WEIGHT);
            let create_block_client = send!(create_block_req).unwrap();
            let mut header_req = create_block_client.get_block_header_request();
            set_thread!(header_req, thread);
            let header_res = header_req.send().promise.await.unwrap();
            let mut header = header_res.get().unwrap().get_result().unwrap();
            let deserialized_header = Header::consensus_decode(&mut header).unwrap();
            println!("Block header: {:?}", deserialized_header);
        })
        .await;
}