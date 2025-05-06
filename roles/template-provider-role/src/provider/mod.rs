use crate::{
    init_capnp, proxy_capnp,
    rpc_client::{error::RpcClientError, BackendRpcClient, BlockTemplateClient},
};
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
use state::{ChainTip, ProviderState};
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
use tokio::{task, time::interval};
use tracing::{debug, error, info, warn};

use crate::{
    message_utils::{into_static, message_from_frame},
    EitherFrame,
};
const BLOCK_RESERVED_WEIGHT: u64 = 3990000;
const USE_MEMPOOL_DEFAULT: bool = true;
const TEMPLATE_EVICTION_DURATION: Duration = Duration::from_secs(600);
mod state;

pub struct Provider {
    state: ProviderState,
    rpc_client: BackendRpcClient,
    prev_hash_sender: Sender<EitherFrame>,
    template_sender: Sender<EitherFrame>,
    tx_data_response_sender: Sender<EitherFrame>,
    message_receiver: Receiver<EitherFrame>,
    sv2interval: Duration,
    sv2feedelta: u64,
    active_template: Option<BlockTemplateClient>,
}

impl Provider {
    pub async fn new(
        prev_hash_sender: Sender<EitherFrame>,
        template_sender: Sender<EitherFrame>,
        tx_data_response_sender: Sender<EitherFrame>,
        message_receiver: Receiver<EitherFrame>,
        sv2feedelta: u64,
        sv2interval: u64,
        socket_path: String,
    ) -> Result<Self, RpcClientError> {
        info!("Initializing Provider...");
        let rpc_client = BackendRpcClient::connect(std::path::Path::new(&socket_path)).await?;
        info!("RPC client connected successfully.");
        Ok(Provider {
            state: ProviderState::new(),
            rpc_client,
            prev_hash_sender,
            template_sender,
            tx_data_response_sender,
            message_receiver,
            sv2interval: Duration::from_secs(sv2interval),
            sv2feedelta,
            active_template: None,
        })
    }

    /// Runs the main event loop for the provider.
    pub async fn run(mut self) {
        info!("Starting Provider event loop.");
        let mut template_timer = interval(self.sv2interval);
        let mut block_checker = interval(Duration::from_secs(1));
        let mut template_eviction_timer = interval(Duration::from_secs(360));

        loop {
            let message_receiver_clone = self.message_receiver.clone();

            tokio::select! {
                biased;
                maybe_msg = message_receiver_clone.recv() => {
                    match maybe_msg {
                        Ok(mut msg) => self.handle_upstream_message(&mut msg).await,
                        Err(e) => {
                            error!("Message receiver channel closed or error: {}. Shutting down provider loop.", e);
                            break;
                        }
                    }
                },

                _ = block_checker.tick() => {
                    if let Err(e) = self.check_for_tip_change().await {
                        error!("Error during tip change check");
                    }
                },

                _ = template_timer.tick() => {
                     if let Err(e) = self.generate_periodic_template().await {
                        error!("Error during periodic template generation");
                    }
                },

                _ = template_eviction_timer.tick() => {
                    self.evict_old_templates();
                },
            }
        }
        info!("Provider event loop finished.");
    }

    /// Handles messages received from downstream.
    async fn handle_upstream_message(&mut self, frame: &mut EitherFrame) {
        let (msg_type, msg) = message_from_frame(frame);
        debug!("Received message type {}: {:?}", msg_type, msg);

        match msg {
            AnyMessage::TemplateDistribution(CoinbaseOutputConstraints(m)) => {
                info!("Received CoinbaseOutputConstraints: {:?}", m);
                let sigops = m.coinbase_output_max_additional_sigops as u64;
                self.state.set_coinbase_constraints(sigops);
                if let Err(e) = self
                    .rpc_client
                    .update_coinbase_constraints(sigops, BLOCK_RESERVED_WEIGHT)
                    .await
                {
                    error!("Failed to update coinbase constraints via RPC");
                } else {
                    info!("Coinbase constraints options sent successfully via RPC.");
                }
            }
            AnyMessage::TemplateDistribution(RequestTransactionData(m)) => {
                info!(
                    "Received RequestTransactionData for template ID {}: {:?}",
                    m.template_id, m
                );
                self.handle_request_transaction_data(m).await;
            }
            AnyMessage::TemplateDistribution(SubmitSolution(m)) => {
                info!(
                    "Received SubmitSolution for template ID {}: {:?}",
                    m.template_id, m
                );
                if let Some(template_client) = &self.active_template {
                    match template_client
                        .submit_solution(
                            m.version,
                            m.header_timestamp,
                            m.header_nonce,
                            &m.coinbase_tx.to_vec()[..],
                        )
                        .await
                    {
                        Ok(true) => {
                            info!("Solution submitted successfully via RPC for template associated with current capability.");
                            self.active_template = None;
                        }
                        Ok(false) => {
                            warn!("Solution submission via RPC indicated logical failure (e.g., rejected) for template.");
                            self.active_template = None;
                        }
                        Err(e) => {
                            error!("Failed to submit solution via RPC");
                        }
                    }
                } else {
                    warn!("Received SubmitSolution but no active block template client is available (Template ID: {}). Solution ignored.", m.template_id);
                }
            }
            _ => {
                warn!("Received unhandled message type {}: {:?}", msg_type, msg);
            }
        }
    }

    async fn check_for_tip_change(&mut self) -> Result<(), RpcClientError> {
        if !self.state.is_coinbase_constraint_received() {
            debug!("Coinbase constraints not yet received, skipping tip check.");
            return Ok(());
        }
        let maybe_tip = self.rpc_client.get_tip().await?;
        let (current_tip_hash, tip_height) = match maybe_tip {
            Some(tip) => tip,
            None => {
                warn!("Node reported no tip available during check.");
                return Ok(());
            }
        };

        if self.state.last_tip.prevhash != current_tip_hash
            || self.state.last_tip.height as u32 != tip_height
        {
            info!(
                "Tip change detected! New tip: height={}, hash={}",
                tip_height, current_tip_hash
            );

            self.generate_and_process_template(true).await?;
            self.state.last_tip = ChainTip {
                prevhash: current_tip_hash,
                height: tip_height as i32,
            };
            info!("Tip change processing complete and local state updated.");
        }
        Ok(())
    }

    async fn generate_periodic_template(&mut self) -> Result<(), RpcClientError> {
        if !self.state.is_coinbase_constraint_received() || !self.state.is_prev_hash_received() {
            debug!("Skipping periodic template generation: Coinbase constraints received: {}, PrevHash received: {}",
                   self.state.is_coinbase_constraint_received(), self.state.is_prev_hash_received());
            return Ok(());
        }
        self.generate_and_process_template(false).await?;
        debug!("Periodic template generation/processing attempt complete.");
        Ok(())
    }

    /// Fetches new template data, updates state, and sends messages downstream.
    async fn generate_and_process_template(
        &mut self,
        is_future: bool,
    ) -> Result<(), RpcClientError> {
        let template_id = self.state.peek_next_template_id();
        debug!(
            template_id,
            is_future, "Generating template data via RPC..."
        );

        let new_template_client = self
            .rpc_client
            .create_new_block(
                USE_MEMPOOL_DEFAULT,
                BLOCK_RESERVED_WEIGHT,
                self.state.get_coinbase_max_additional_sigops(),
            )
            .await?;

        let block = new_template_client.get_block().await?;
        let merkle_path_bytes = new_template_client.get_coinbase_merkle_path().await?;

        let merkle_path: Vec<U256> = merkle_path_bytes
            .iter()
            .map(|e| U256::Owned(e.to_vec()))
            .collect();
        let fee = block.txdata[0]
            .output
            .iter()
            .fold(0, |acc, x| acc + x.value.to_sat());

        let map_try_into_error = |field: &'static str| {
            RpcClientError::InvalidData(format!("Data too large for {}", field))
        };

        let coinbase_outputs_concat = block.txdata[0]
            .output
            .iter()
            .map(|txout| txout.consensus_encode_to_vec())
            .collect::<Vec<Vec<u8>>>()
            .concat();

        let new_template_msg_data = NewTemplate {
            template_id,
            future_template: is_future,
            version: block.header.version.to_consensus() as u32,
            coinbase_tx_version: block.txdata[0].version.0 as u32,
            coinbase_prefix: block.txdata[0].input[0]
                .script_sig
                .to_bytes()
                .try_into()
                .map_err(|_| map_try_into_error("coinbase_prefix"))?,
            coinbase_tx_input_sequence: block.txdata[0].input[0].sequence.to_consensus_u32(),
            coinbase_tx_value_remaining: fee,
            coinbase_tx_outputs_count: block.txdata[0].output.len() as u32,
            coinbase_tx_outputs: coinbase_outputs_concat
                .try_into()
                .map_err(|_| map_try_into_error("coinbase_tx_outputs"))?,
            coinbase_tx_locktime: block.txdata[0].lock_time.to_consensus_u32(),
            merkle_path: merkle_path
                .try_into()
                .map_err(|_| map_try_into_error("merkle_path"))?,
        };

        let prev_hash_msg_data = SetNewPrevHash {
            template_id,
            prev_hash: block
                .header
                .prev_blockhash
                .to_raw_hash()
                .to_byte_array()
                .try_into()
                .unwrap(),
            header_timestamp: block.header.time,
            n_bits: block.header.bits.to_consensus(),
            target: block.header.target().to_be_bytes().try_into().unwrap(),
        };

        let new_template_msg = into_static(AnyMessage::TemplateDistribution(
            roles_logic_sv2::parsers::TemplateDistribution::NewTemplate(new_template_msg_data),
        ));
        let prev_hash_msg = into_static(AnyMessage::TemplateDistribution(
            roles_logic_sv2::parsers::TemplateDistribution::SetNewPrevHash(prev_hash_msg_data),
        ));

        let should_send_template = is_future
            || (fee > (self.state.current_fee - self.sv2feedelta))
            || self.state.current_fee == 0;

        if is_future || should_send_template {
            let template_frame =
                Sv2Frame::from_message(new_template_msg.clone(), 0x71, 0x0001, false).unwrap();

            if let Err(e) = self.template_sender.send(template_frame.into()).await {
                error!(
                    "(Template {}) Failed to send NewTemplate downstream: {}",
                    template_id, e
                );
                task::spawn_local(async move {
                    if let Err(e) = new_template_client.destroy().await {
                        error!(
                            "(Template {}) Failed to destroy block template after send failure ",
                            template_id
                        );
                    }
                });
            } else {
                info!(
                    "(Template {}) Sent NewTemplate (is_future={})",
                    template_id, is_future
                );
                self.state.insert_template(template_id, block);
                self.state.current_fee = fee;

                if let Some(old_client) = self.active_template.take() {
                    task::spawn_local(async move {
                        if let Err(e) = old_client.destroy().await {
                            error!("Failed to destroy previous block template");
                        } else {
                            debug!("Destroyed previous block template.");
                        }
                    });
                }
                self.active_template = Some(new_template_client);
                self.state.get_next_template_id();
                if is_future {
                    let prevhash_frame =
                        Sv2Frame::from_message(prev_hash_msg.clone(), 0x72, 0x0001, false).unwrap();
                    if let Err(e) = self.prev_hash_sender.send(prevhash_frame.into()).await {
                        error!(
                            "(Template {}) Failed to send SetNewPrevHash downstream: {}",
                            template_id, e
                        );
                    } else {
                        debug!("(Template {}) Sent SetNewPrevHash", template_id);
                        self.state.set_prev_hash_received(true);
                    }
                }
            }
        } else {
            debug!("(Template {}) New template fee ({}) does not meet threshold (current: {}), skipping send.", template_id, fee, self.state.current_fee);
            task::spawn_local(async move {
                if let Err(e) = new_template_client.destroy().await {
                    error!(
                        "(Template {}) Failed to destroy unused block template",
                        template_id
                    );
                } else {
                    debug!(
                        "(Template {}) Destroyed unused block template.",
                        template_id
                    );
                }
            });
        }

        Ok(())
    }

    /// Handles a RequestTransactionData message from downstream.
    async fn handle_request_transaction_data(
        &self,
        m: roles_logic_sv2::template_distribution_sv2::RequestTransactionData,
    ) {
        let response_msg = match self.state.templates.get(&m.template_id) {
            Some(tx_data) => {
                if tx_data.block.header.prev_blockhash != self.state.last_tip.prevhash {
                    warn!(
                        "Responding with stale-template-id error for template {}",
                        m.template_id
                    );
                    let error = RequestTransactionDataError {
                        template_id: m.template_id,
                        error_code: b"stale-template-id"[..].to_vec().try_into().unwrap(),
                    };
                    AnyMessage::TemplateDistribution(
                        roles_logic_sv2::parsers::TemplateDistribution::RequestTransactionDataError(
                            error,
                        ),
                    )
                } else {
                    info!("Providing transaction data for template {}", m.template_id);
                    // Ensure witness data exists and is not empty before accessing index 0
                    let excess_data = tx_data
                        .block
                        .txdata
                        .get(0)
                        .and_then(|tx| tx.input.get(0))
                        .map(|input| input.witness.to_vec().concat())
                        .unwrap_or_else(|| {
                            warn!(
                                "Coinbase transaction or input missing/malformed for template {}",
                                m.template_id
                            );
                            Vec::new()
                        });

                    let transaction_list: Vec<B016M> = tx_data.block.txdata[1..]
                        .iter()
                        .map(|tx| B016M::Owned(tx.consensus_encode_to_vec()))
                        .collect();

                    let success = RequestTransactionDataSuccess {
                        template_id: m.template_id,
                        excess_data: excess_data
                            .try_into()
                            .unwrap_or_else(|_| Vec::new().try_into().unwrap()),
                        transaction_list: transaction_list
                            .try_into()
                            .unwrap_or_else(|_| Vec::new().try_into().unwrap()),
                    };
                    AnyMessage::TemplateDistribution(roles_logic_sv2::parsers::TemplateDistribution::RequestTransactionDataSuccess(success))
                }
            }
            None => {
                warn!(
                    "Responding with template-id-not-found error for template {}",
                    m.template_id
                );
                let error = RequestTransactionDataError {
                    template_id: m.template_id,
                    error_code: b"template-id-not-found".to_vec().try_into().unwrap(),
                };
                AnyMessage::TemplateDistribution(
                    roles_logic_sv2::parsers::TemplateDistribution::RequestTransactionDataError(
                        error,
                    ),
                )
            }
        };

        let response_frame =
            Sv2Frame::from_message(into_static(response_msg), 0x75, 0x0001, false).unwrap();

        if self
            .tx_data_response_sender
            .send(response_frame.into())
            .await
            .is_err()
        {
            error!("Failed to send transaction data response for template {} downstream, channel closed?", m.template_id);
        }
    }

    fn evict_old_templates(&mut self) {
        let before_count = self.state.templates.len();
        if before_count > 0 {
            self.state
                .templates
                .retain(|_key, value| value.timeout.elapsed() <= TEMPLATE_EVICTION_DURATION);
            let after_count = self.state.templates.len();
            if before_count != after_count {
                info!(
                    "Evicted {} old templates ({} remaining).",
                    before_count - after_count,
                    after_count
                );
            }
        }
    }
}

trait ConsensusEncodeVec {
    fn consensus_encode_to_vec(&self) -> Vec<u8>;
}
impl<T: Encodable> ConsensusEncodeVec for T {
    fn consensus_encode_to_vec(&self) -> Vec<u8> {
        let mut writer = Vec::new();
        self.consensus_encode(&mut writer)
            .expect("Vec<u8> encode failed");
        writer
    }
}
