use crate::{init_capnp, proxy_capnp};
use async_channel::{Receiver, Sender};
use binary_sv2::{B016M, U256};
use codec_sv2::Sv2Frame;
use roles_logic_sv2::{
    parsers::{
        AnyMessage,
        TemplateDistribution::{CoinbaseOutputConstraints, RequestTransactionData, SubmitSolution},
    },
    template_distribution_sv2::{
        NewTemplate, RequestTransactionDataError, RequestTransactionDataSuccess, SetNewPrevHash,
    },
};
use std::{
    cell::Cell,
    collections::HashMap,
    io::Cursor,
    path::Path,
    rc::Rc,
    sync::atomic::{AtomicBool, AtomicU64},
    time::{Duration, Instant},
};
use stratum_common::bitcoin::{
    block::Header,
    consensus,
    consensus::{Decodable, Encodable},
    hashes::Hash,
    Block, BlockHash, Transaction,
};
use tokio::time::interval;
use tracing::{debug, info};

use crate::{into_static, message_from_frame, EitherFrame};
const BLOCK_RESERVED_WEIGHT: u64 = 3990000;
mod setup_connection;

struct chain_tip {
    pub prevhash: BlockHash,
    pub height: i32,
}

struct CoolDownTransactionData {
    pub block: Block,
    pub timeout: Instant,
}

pub async fn start_provider(
    prev_hash_sender: Sender<EitherFrame>,
    template_sender: Sender<EitherFrame>,
    tx_data_response_sender: Sender<EitherFrame>,
    message_receiver: Receiver<EitherFrame>,
    sv2feedelta: u64,
    sv2interval: u64,
    socket_path: String,
) {
    info!("Starting provider setup");
    let (init_client, thread_client) = setup_connection::connect(Path::new(socket_path.as_str()))
        .await
        .unwrap();
    let mut template_timer = interval(Duration::from_secs(sv2interval));
    let mut block_checker = interval(Duration::from_secs(1));
    let mut template_eviction_timer = interval(Duration::from_secs(360));
    let mut template_id = Cell::new(0);
    let mut last_tip = chain_tip {
        prevhash: BlockHash::from_byte_array([0; 32]),
        height: 0,
    };
    let mut is_prev_hash_received = AtomicBool::new(false);
    let mut template: HashMap<u64, CoolDownTransactionData> = HashMap::new();
    let mut fee = 0;
    let is_coinbase_constraint_receiver = Cell::new(false);

    loop {
        let message_receiver_clone = message_receiver.clone();
        tokio::select! {
            _ = block_checker.tick() => {
                if !is_coinbase_constraint_receiver.get() {
                    debug!("Coinbase constraints not received can't do anything");
                } else {
                    let mut mining_request = init_client.make_mining_request();
                    mining_request.get().get_context().unwrap().set_thread(thread_client.clone());
                    let mining_client = mining_request.send().promise.await.unwrap().get().unwrap().get_result().unwrap();
                    let mut tip_change_request = mining_client.get_tip_request();
                    tip_change_request.get().get_context().unwrap().set_thread(thread_client.clone());
                    let tip_change_value = tip_change_request.send().promise.await.unwrap();
                    let tip_change = tip_change_value.get().unwrap().get_result().unwrap();
                    let tip_hash = tip_change.get_hash().unwrap().try_into().unwrap();
                    let tip_height = tip_change.get_height();
                    if last_tip.prevhash != BlockHash::from_byte_array(tip_hash) || tip_height != last_tip.height {
                        let (new_template, template_fee, block) = get_new_template(init_client.clone(), thread_client.clone(), &template_id, true).await;
                        fee = template_fee;
                        let prev_hash = SetNewPrevHash {
                            template_id: template_id.get() - 1,
                            prev_hash: block.header.prev_blockhash.to_raw_hash().to_byte_array().try_into().unwrap(),
                            header_timestamp: block.header.time,
                            n_bits: block.header.bits.to_consensus(),
                            target: block.header.target().to_be_bytes().try_into().unwrap()
                        };
                        info!("Tip change occurred");

                        let template_message = into_static(AnyMessage::TemplateDistribution(roles_logic_sv2::parsers::TemplateDistribution::NewTemplate(new_template)));
                        let template_frame = Sv2Frame::from_message(template_message, 0x71, 0x0001, false).expect("Failed to frame the message");
                        let prevhash_message = into_static(AnyMessage::TemplateDistribution(roles_logic_sv2::parsers::TemplateDistribution::SetNewPrevHash(prev_hash)));
                        let prevhash_frame = Sv2Frame::from_message(prevhash_message, 0x72, 0x0001, false).expect("Failed to frame the message");

                        template_sender.send(template_frame.into()).await;
                        prev_hash_sender.send(prevhash_frame.into()).await;

                        is_prev_hash_received.store(true, std::sync::atomic::Ordering::Relaxed);

                        last_tip = chain_tip {
                            prevhash: BlockHash::from_byte_array(tip_hash),
                            height: tip_height
                        };

                        template.insert(template_id.get() - 1,CoolDownTransactionData { block, timeout: Instant::now()});
                    }
                }
            },
            _ = template_timer.tick() => {
                if !is_coinbase_constraint_receiver.get() || !is_prev_hash_received.load(std::sync::atomic::Ordering::Relaxed){
                    debug!("Coinbase constraints not received can't do anything");
                } else {
                    // let mut echo_request = init_client.make_echo_request();
                    // echo_request.get().get_context().unwrap().set_thread(thread_client.clone());
                    // let echo_client = echo_request.send().promise.await.unwrap().get().unwrap().get_result().unwrap();
                    // let mut say_hi = echo_client.echo_request();=                // println!("Hello: {:?}", hi);
                    let (new_template, coinbase_tx_value_remaining, block) = get_new_template(init_client.clone(), thread_client.clone(), &template_id, false).await;

                    if fee < (coinbase_tx_value_remaining + sv2feedelta) || true {
                        template.insert(template_id.get() - 1,CoolDownTransactionData { block, timeout: Instant::now()});
                        fee = coinbase_tx_value_remaining;
                        let message = into_static(AnyMessage::TemplateDistribution(roles_logic_sv2::parsers::TemplateDistribution::NewTemplate(new_template)));
                        let frame = Sv2Frame::from_message(message, 0x71, 0x0001, false)
                                .expect("Failed to frame the message");
                        template_sender.send(frame.into()).await;
                    }
                }
            },
            _ = template_eviction_timer.tick() => {
                let ten_minutes = Duration::from_secs(600);
                template.retain(|_key, value| {
                    let elapsed_since_timeout = value.timeout.elapsed();
                    elapsed_since_timeout <= ten_minutes
                });
            }
            _ = async {
                handle_message_from_upstream(init_client.clone(), thread_client.clone(), message_receiver_clone, &is_coinbase_constraint_receiver, &template, tx_data_response_sender.clone(), &last_tip).await;
            } => {}
        }
    }
}

async fn handle_message_from_upstream(
    init_client: init_capnp::init::Client,
    thread_client: proxy_capnp::thread::Client,
    message_receiver: Receiver<EitherFrame>,
    is_coinbase_constraint_receiver: &Cell<bool>,
    template: &HashMap<u64, CoolDownTransactionData>,
    tx_data_response_sender: Sender<EitherFrame>,
    last_tip: &chain_tip,
) {
    while let Ok(mut msg) = message_receiver.recv().await {
        let msg_from_frame = message_from_frame(&mut msg);
        match msg_from_frame.1 {
            AnyMessage::TemplateDistribution(CoinbaseOutputConstraints(m)) => {
                info!("Received coinbase Output constraint");
                info!("The message is : {:?}", m);
                let mut mining_request = init_client.make_mining_request();
                mining_request
                    .get()
                    .get_context()
                    .unwrap()
                    .set_thread(thread_client.clone());
                let mining_client = mining_request
                    .send()
                    .promise
                    .await
                    .unwrap()
                    .get()
                    .unwrap()
                    .get_result()
                    .unwrap();
                let mut create_block_req = mining_client.create_new_block_request();
                create_block_req
                    .get()
                    .get_options()
                    .unwrap()
                    .set_block_reserved_weight(BLOCK_RESERVED_WEIGHT);
                create_block_req
                    .get()
                    .get_options()
                    .unwrap()
                    .set_coinbase_output_max_additional_sigops(
                        m.coinbase_output_max_additional_sigops as u64,
                    );
                create_block_req
                    .send()
                    .promise
                    .await
                    .unwrap()
                    .get()
                    .unwrap()
                    .get_result()
                    .unwrap();
                is_coinbase_constraint_receiver.set(true);
            }
            AnyMessage::TemplateDistribution(RequestTransactionData(m)) => {
                info!("Received request transaction data");
                info!("The message is : {:?}", m);

                let transaction_object = template.get(&m.template_id);
                let message = match transaction_object {
                    Some(tx_data) => {
                        if tx_data.block.header.prev_blockhash != last_tip.prevhash {
                            let transaction_error = RequestTransactionDataError {
                                template_id: m.template_id,
                                error_code: "stale-template-id"
                                    .as_bytes()
                                    .to_vec()
                                    .try_into()
                                    .unwrap(),
                            };
                            let message = into_static(AnyMessage::TemplateDistribution(roles_logic_sv2::parsers::TemplateDistribution::RequestTransactionDataError(transaction_error)));
                            let frame = Sv2Frame::from_message(message, 0x75, 0x0001, false)
                                .expect("Failed to frame the message");
                            tx_data_response_sender.send(frame.into()).await;
                        } else {
                            let excess_data =
                                tx_data.block.txdata[0].input[0].witness.to_vec().concat();
                            let transaction_list: Vec<B016M> = tx_data.block.txdata[1..]
                                .to_vec()
                                .iter()
                                .map(|tx| {
                                    let mut writer = Vec::new();
                                    tx.consensus_encode(&mut writer);
                                    B016M::Owned(writer)
                                })
                                .collect();
                            let transaction_success = RequestTransactionDataSuccess {
                                template_id: m.template_id,
                                excess_data: excess_data.try_into().unwrap(),
                                transaction_list: transaction_list.try_into().unwrap(),
                            };

                            let message = into_static(AnyMessage::TemplateDistribution(roles_logic_sv2::parsers::TemplateDistribution::RequestTransactionDataSuccess(transaction_success)));
                            let frame = Sv2Frame::from_message(message, 0x75, 0x0001, false)
                                .expect("Failed to frame the message");
                            tx_data_response_sender.send(frame.into()).await;
                        }
                    }
                    None => {
                        let transaction_error = RequestTransactionDataError {
                            template_id: m.template_id,
                            error_code: "template-id-not-found"
                                .as_bytes()
                                .to_vec()
                                .try_into()
                                .unwrap(),
                        };
                        let message = into_static(AnyMessage::TemplateDistribution(roles_logic_sv2::parsers::TemplateDistribution::RequestTransactionDataError(transaction_error)));
                        let frame = Sv2Frame::from_message(message, 0x75, 0x0001, false)
                            .expect("Failed to frame the message");
                        tx_data_response_sender.send(frame.into()).await;
                    }
                };
            }
            AnyMessage::TemplateDistribution(SubmitSolution(m)) => {
                info!("Received submit solution");
                info!("The message is : {:?}", m);
                let mut mining_request = init_client.make_mining_request();
                mining_request
                    .get()
                    .get_context()
                    .unwrap()
                    .set_thread(thread_client.clone());
                let mining_client = mining_request
                    .send()
                    .promise
                    .await
                    .unwrap()
                    .get()
                    .unwrap()
                    .get_result()
                    .unwrap();
                let mut create_block_req = mining_client.create_new_block_request();
                create_block_req
                    .get()
                    .get_options()
                    .unwrap()
                    .set_block_reserved_weight(BLOCK_RESERVED_WEIGHT);
                let create_block = create_block_req
                    .send()
                    .promise
                    .await
                    .unwrap()
                    .get()
                    .unwrap()
                    .get_result()
                    .unwrap();
                let mut submit_solution_context = create_block.submit_solution_request();
                submit_solution_context.get().set_version(m.version);
                submit_solution_context.get().set_nonce(m.header_nonce);
                submit_solution_context
                    .get()
                    .set_timestamp(m.header_timestamp);
                submit_solution_context
                    .get()
                    .set_coinbase(&m.coinbase_tx.to_vec()[..]);
                submit_solution_context
                    .get()
                    .get_context()
                    .unwrap()
                    .set_thread(thread_client.clone());
                let value = submit_solution_context.send().promise.await.unwrap();
                let value = value.get().unwrap().get_result();
                println!("Is solution submitted successfully: {:?}", value);
            }
            _ => {
                info!("Something unfamiliar it seems");
            }
        }
    }
}

async fn get_new_template(
    init_client: init_capnp::init::Client,
    thread_client: proxy_capnp::thread::Client,
    template_id: &Cell<u64>,
    is_future: bool,
) -> (
    roles_logic_sv2::template_distribution_sv2::NewTemplate<'static>,
    u64,
    Block,
) {
    let mut mining_request = init_client.make_mining_request();
    mining_request
        .get()
        .get_context()
        .unwrap()
        .set_thread(thread_client.clone());
    let mining_client = mining_request
        .send()
        .promise
        .await
        .unwrap()
        .get()
        .unwrap()
        .get_result()
        .unwrap();
    let mut create_block_req = mining_client.create_new_block_request();
    create_block_req
        .get()
        .get_options()
        .unwrap()
        .set_block_reserved_weight(BLOCK_RESERVED_WEIGHT);
    let create_block = create_block_req
        .send()
        .promise
        .await
        .unwrap()
        .get()
        .unwrap()
        .get_result()
        .unwrap();
    let mut block_request = create_block.get_block_request();
    block_request
        .get()
        .get_context()
        .unwrap()
        .set_thread(thread_client.clone());
    let block = block_request.send().promise.await.unwrap();
    let block_value = block.get().unwrap().get_result().unwrap();
    let mut cursor = Cursor::new(&block_value);
    let block = Block::consensus_decode(&mut cursor).unwrap();
    let coinbase_prefix = block.txdata[0].input[0].script_sig.to_bytes();
    let mut coinbase_merkle_path_request = create_block.get_coinbase_merkle_path_request();
    coinbase_merkle_path_request
        .get()
        .get_context()
        .unwrap()
        .set_thread(thread_client.clone());
    let coinbase_merkle_path_in = coinbase_merkle_path_request.send().promise.await.unwrap();
    let coinbase_merkle_path = coinbase_merkle_path_in.get().unwrap().get_result().unwrap();
    let merkle_path: Vec<U256> = coinbase_merkle_path
        .iter()
        .map(|e: Result<&[u8], capnp::Error>| {
            let value = e.unwrap().to_vec();
            U256::Owned(value)
        })
        .collect();
    let id = template_id.get();
    template_id.set(id + 1);

    let coinbase_tx_value_remaining = block.txdata[0]
        .output
        .iter()
        .fold(0, |acc, x| acc + x.value.to_sat());
    let coinbase_outputs: Vec<Vec<u8>> = block.txdata[0]
        .output
        .iter()
        .map(|txout| {
            let mut writer = Vec::new();
            txout.consensus_encode(&mut writer);
            writer
        })
        .collect();
    let new_template = NewTemplate {
        template_id: id,
        future_template: is_future,
        version: block.header.version.to_consensus() as u32,
        coinbase_tx_version: block.txdata[0].version.0 as u32,
        coinbase_prefix: coinbase_prefix.try_into().unwrap(),
        coinbase_tx_input_sequence: block.txdata[0].input[0].sequence.into(),
        coinbase_tx_value_remaining: block.txdata[0].output[0].value.to_sat(),
        coinbase_tx_outputs_count: block.txdata[0].output.len() as u32,
        // Correct these later.
        coinbase_tx_outputs: coinbase_outputs.concat().try_into().unwrap(),
        coinbase_tx_locktime: block.txdata[0].lock_time.to_consensus_u32(),
        // Correct these later.
        merkle_path: merkle_path.try_into().unwrap(),
    };
    (
        new_template,
        block.txdata[0].output[0].value.to_sat(),
        block,
    )
}
