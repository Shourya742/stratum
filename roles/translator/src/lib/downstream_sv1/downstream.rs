//! ## Downstream SV1 Module: Downstream Connection Logic
//!
//! Defines the [`Downstream`] structure, which represents and manages an
//! individual connection from a downstream SV1 mining client.
//!
//! This module is responsible for:
//! - Accepting incoming TCP connections from SV1 miners.
//! - Handling the SV1 protocol handshake (`mining.subscribe`, `mining.authorize`,
//!   `mining.configure`).
//! - Receiving SV1 `mining.submit` messages from miners.
//! - Translating SV1 `mining.submit` messages into internal [`DownstreamMessages`] (specifically
//!   [`SubmitShareWithChannelId`]) and sending them to the Bridge.
//! - Receiving translated SV1 `mining.notify` messages from the Bridge and sending them to the
//!   connected miner.
//! - Managing the miner's extranonce1, extranonce2 size, and version rolling parameters.
//! - Implementing downstream-specific difficulty management logic, including tracking submitted
//!   shares and updating the miner's difficulty target.
//! - Implementing the necessary SV1 server traits ([`IsServer`]) and SV2 roles logic traits
//!   ([`IsMiningDownstream`], [`IsDownstream`]).

use crate::{
    config::{DownstreamDifficultyConfig, UpstreamDifficultyConfig},
    error::ProxyResult,
    status,
};
use async_channel::{bounded, Receiver, Sender};
use error_handling::handle_result;
use futures::{FutureExt, StreamExt};

use tokio::{
    io::{AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::broadcast,
    task::AbortHandle,
};

use super::{kill, DownstreamMessages, SUBSCRIBE_TIMEOUT_SECS};

use roles_logic_sv2::utils::Mutex;

use crate::error::Error;
use futures::select;
use tokio_util::codec::{FramedRead, LinesCodec};

use std::{collections::VecDeque, net::SocketAddr, sync::Arc};
use tracing::{debug, error, info, warn};
use v1::{client_to_server::Submit, json_rpc, server_to_client, utils::HexU32Be, IsServer};

/// The maximum allowed length for a single line (JSON-RPC message) received from an SV1 client.
const MAX_LINE_LENGTH: usize = 2_usize.pow(16);

/// Handles the sending and receiving of messages to and from an SV2 Upstream role (most typically
/// a SV2 Pool server).
#[derive(Debug)]
pub struct Downstream {
    /// The unique identifier assigned to this downstream connection/channel.
    pub(super) connection_id: u32,
    /// List of authorized Downstream Mining Devices.
    pub(super) authorized_names: Vec<String>,
    /// The extranonce1 value assigned to this downstream miner.
    pub(super) extranonce1: Vec<u8>,
    /// `extranonce1` to be sent to the Downstream in the SV1 `mining.subscribe` message response.
    //extranonce1: Vec<u8>,
    //extranonce2_size: usize,
    /// Version rolling mask bits
    pub(super) version_rolling_mask: Option<HexU32Be>,
    /// Minimum version rolling mask bits size
    pub(super) version_rolling_min_bit: Option<HexU32Be>,
    /// Sends a SV1 `mining.submit` message received from the Downstream role to the `Bridge` for
    /// translation into a SV2 `SubmitSharesExtended`.
    pub(super) tx_sv1_bridge: Sender<DownstreamMessages>,
    /// Sends message to the SV1 Downstream role.
    pub(super) tx_outgoing: Sender<json_rpc::Message>,
    /// True if this is the first job received from `Upstream`.
    pub(super) first_job_received: bool,
    /// The expected size of the extranonce2 field provided by the miner.
    pub(super) extranonce2_len: usize,
    /// Configuration and state for managing difficulty adjustments specific
    /// to this individual downstream miner.
    pub(super) difficulty_mgmt: DownstreamDifficultyConfig,
    /// Configuration settings for the upstream channel's difficulty management.
    pub(super) upstream_difficulty_config: Arc<Mutex<UpstreamDifficultyConfig>>,
    pub(super) active_jobs: VecDeque<server_to_client::Notify<'static>>,
}

impl Downstream {
    // not huge fan of test specific code in codebase.
    #[cfg(test)]
    pub fn new(
        connection_id: u32,
        authorized_names: Vec<String>,
        extranonce1: Vec<u8>,
        version_rolling_mask: Option<HexU32Be>,
        version_rolling_min_bit: Option<HexU32Be>,
        tx_sv1_bridge: Sender<DownstreamMessages>,
        tx_outgoing: Sender<json_rpc::Message>,
        first_job_received: bool,
        extranonce2_len: usize,
        difficulty_mgmt: DownstreamDifficultyConfig,
        upstream_difficulty_config: Arc<Mutex<UpstreamDifficultyConfig>>,
        last_job_id: String,
    ) -> Self {
        Downstream {
            connection_id,
            authorized_names,
            extranonce1,
            version_rolling_mask,
            version_rolling_min_bit,
            tx_sv1_bridge,
            tx_outgoing,
            first_job_received,
            extranonce2_len,
            difficulty_mgmt,
            upstream_difficulty_config,
            active_jobs: VecDeque::with_capacity(2),
        }
    }
    /// Instantiates and manages a new handler for a single downstream SV1 client connection.
    ///
    /// This is the primary function called for each new incoming TCP stream from a miner.
    /// It sets up the communication channels, initializes the `Downstream` struct state,
    /// and spawns the necessary tasks to handle:
    /// 1. Reading incoming messages from the miner's socket.
    /// 2. Writing outgoing messages to the miner's socket.
    /// 3. Sending job notifications to the miner (handling initial job and subsequent updates).
    ///
    /// It uses shutdown channels to coordinate graceful termination of the spawned tasks.
    #[allow(clippy::too_many_arguments)]
    pub async fn new_downstream(
        stream: TcpStream,
        connection_id: u32,
        tx_sv1_bridge: Sender<DownstreamMessages>,
        mut rx_sv1_notify: broadcast::Receiver<server_to_client::Notify<'static>>,
        tx_status: status::Sender,
        extranonce1: Vec<u8>,
        last_notify: Option<server_to_client::Notify<'static>>,
        extranonce2_len: usize,
        host: String,
        difficulty_config: DownstreamDifficultyConfig,
        upstream_difficulty_config: Arc<Mutex<UpstreamDifficultyConfig>>,
        task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>>,
    ) {
        // Reads and writes from Downstream SV1 Mining Device Client
        let (socket_reader, mut socket_writer) = stream.into_split();
        let (tx_outgoing, receiver_outgoing) = bounded(10);

        let mut active_jobs = VecDeque::with_capacity(1);
        if let Some(notify) = last_notify.clone() {
            active_jobs.push_back(notify);
        }

        let downstream = Arc::new(Mutex::new(Downstream {
            connection_id,
            authorized_names: vec![],
            extranonce1,
            version_rolling_mask: None,
            version_rolling_min_bit: None,
            tx_sv1_bridge,
            tx_outgoing,
            first_job_received: false,
            extranonce2_len,
            difficulty_mgmt: difficulty_config,
            upstream_difficulty_config,
            active_jobs: active_jobs.clone(),
        }));
        let self_ = downstream.clone();

        let host_ = host.clone();
        // The shutdown channel is used local to the `Downstream::new_downstream()` function.
        // Each task is set broadcast a shutdown message at the end of their lifecycle with
        // `kill()`, and each task has a receiver to listen for the shutdown message. When a
        // shutdown message is received the task should `break` its loop. For any errors that should
        // shut a task down, we should `break` out of the loop, so that the `kill` function
        // can send the shutdown broadcast. EXTRA: The since all downstream tasks rely on
        // receiving messages with a future (either TCP recv or Receiver<_>) we use the
        // futures::select! macro to merge the receiving end of a task channels into a single loop
        // within the task
        let (tx_shutdown, rx_shutdown): (Sender<bool>, Receiver<bool>) = async_channel::bounded(3);

        let rx_shutdown_clone = rx_shutdown.clone();
        let tx_shutdown_clone = tx_shutdown.clone();
        let tx_status_reader = tx_status.clone();
        let task_collector_mining_device = task_collector.clone();
        // Task to read from SV1 Mining Device Client socket via `socket_reader`. Depending on the
        // SV1 message received, a message response is sent directly back to the SV1 Downstream
        // role, or the message is sent upwards to the Bridge for translation into a SV2 message
        // and then sent to the SV2 Upstream role.
        let socket_reader_task = tokio::task::spawn(async move {
            let reader = BufReader::new(socket_reader);
            let mut messages =
                FramedRead::new(reader, LinesCodec::new_with_max_length(MAX_LINE_LENGTH));
            loop {
                // Read message from SV1 Mining Device Client socket
                // On message receive, parse to `json_rpc:Message` and send to Upstream
                // `Translator.receive_downstream` via `sender_upstream` done in
                // `send_message_upstream`.
                select! {
                    res = messages.next().fuse() => {
                        match res {
                            Some(Ok(incoming)) => {
                                debug!("Receiving from Mining Device {}: {:?}", &host_, &incoming);
                                let incoming: json_rpc::Message = handle_result!(tx_status_reader, serde_json::from_str(&incoming));
                                // Handle what to do with message
                                // if let json_rpc::Message

                                // if message is Submit Shares update difficulty management
                                if let v1::Message::StandardRequest(standard_req) = incoming.clone() {
                                    if let Ok(Submit{..}) = standard_req.try_into() {
                                        handle_result!(tx_status_reader, Self::save_share(self_.clone()));
                                    }
                                }

                                let res = Self::handle_incoming_sv1(self_.clone(), incoming).await;
                                handle_result!(tx_status_reader, res);
                            }
                            Some(Err(_)) => {
                                handle_result!(tx_status_reader, Err(Error::Sv1MessageTooLong));
                            }
                            None => {
                                handle_result!(tx_status_reader, Err(
                                    std::io::Error::new(
                                        std::io::ErrorKind::ConnectionAborted,
                                        "Connection closed by client"
                                    )
                                ));
                            }
                        }
                    },
                    _ = rx_shutdown_clone.recv().fuse() => {
                        break;
                    }
                };
            }
            kill(&tx_shutdown_clone).await;
            warn!("Downstream: Shutting down sv1 downstream reader");
        });
        let _ = task_collector_mining_device.safe_lock(|a| {
            a.push((
                socket_reader_task.abort_handle(),
                "socket_reader_task".to_string(),
            ))
        });

        let rx_shutdown_clone = rx_shutdown.clone();
        let tx_shutdown_clone = tx_shutdown.clone();
        let tx_status_writer = tx_status.clone();
        let host_ = host.clone();

        let task_collector_new_sv1_message_no_transl = task_collector.clone();
        // Task to receive SV1 message responses to SV1 messages that do NOT need translation.
        // These response messages are sent directly to the SV1 Downstream role.
        let socket_writer_task = tokio::task::spawn(async move {
            loop {
                select! {
                    res = receiver_outgoing.recv().fuse() => {
                        let to_send = handle_result!(tx_status_writer, res);
                        let to_send = match serde_json::to_string(&to_send) {
                            Ok(string) => format!("{}\n", string),
                            Err(_e) => {
                                debug!("\nDownstream: Bad SV1 server message\n");
                                break;
                            }
                        };
                        debug!("Sending to Mining Device: {} - {:?}", &host_, &to_send);
                        let res = socket_writer
                                    .write_all(to_send.as_bytes())
                                    .await;
                        handle_result!(tx_status_writer, res);
                    },
                    _ = rx_shutdown_clone.recv().fuse() => {
                            break;
                        }
                };
            }
            kill(&tx_shutdown_clone).await;
            warn!(
                "Downstream: Shutting down sv1 downstream writer: {}",
                &host_
            );
        });
        let _ = task_collector_new_sv1_message_no_transl.safe_lock(|a| {
            a.push((
                socket_writer_task.abort_handle(),
                "socket_writer_task".to_string(),
            ))
        });

        let tx_status_notify = tx_status;
        let self_ = downstream.clone();

        let task_collector_notify_task = task_collector.clone();
        let notify_task = tokio::task::spawn(async move {
            let timeout_timer = std::time::Instant::now();
            let mut first_sent = false;
            loop {
                let mask = self_.safe_lock(|d| d.version_rolling_mask.clone()).unwrap();

                let is_a = match downstream.safe_lock(|d| !d.authorized_names.is_empty()) {
                    Ok(is_a) => is_a,
                    Err(_e) => {
                        debug!("\nDownstream: Poison Lock - authorized_names\n");
                        break;
                    }
                };
                if is_a && !first_sent && !active_jobs.is_empty() {
                    let target = handle_result!(
                        tx_status_notify,
                        Self::hash_rate_to_target(downstream.clone())
                    );
                    // make sure the mining start time is initialized and reset number of shares
                    // submitted
                    handle_result!(
                        tx_status_notify,
                        Self::init_difficulty_management(downstream.clone(), &target).await
                    );
                    let message =
                        handle_result!(tx_status_notify, Self::get_set_difficulty(target));
                    handle_result!(
                        tx_status_notify,
                        Downstream::send_message_downstream(downstream.clone(), message).await
                    );

                    let mut sv1_mining_notify_msg = match active_jobs.back().cloned() {
                        Some(sv1_mining_notify_msg) => sv1_mining_notify_msg,
                        None => {
                            error!("sv1_mining_notify_msg is None");
                            break;
                        }
                    };

                    if let Some(mask) = mask {
                        sv1_mining_notify_msg.version =
                            HexU32Be(sv1_mining_notify_msg.version.0 & !mask.0);
                    }

                    let message: json_rpc::Message = sv1_mining_notify_msg.into();
                    handle_result!(
                        tx_status_notify,
                        Downstream::send_message_downstream(downstream.clone(), message).await
                    );
                    if let Err(_e) = downstream.clone().safe_lock(|s| {
                        s.first_job_received = true;
                    }) {
                        debug!("\nDownstream: Poison Lock - first_job_received\n");
                        break;
                    }
                    first_sent = true;
                } else if is_a && !active_jobs.is_empty() {
                    // if hashrate has changed, update difficulty management, and send new
                    // mining.set_difficulty
                    select! {
                        res = rx_sv1_notify.recv().fuse() => {
                            // if hashrate has changed, update difficulty management, and send new mining.set_difficulty
                            handle_result!(tx_status_notify, Self::try_update_difficulty_settings(downstream.clone()).await);

                            let mut sv1_mining_notify_msg = handle_result!(tx_status_notify, res);

                            if downstream.safe_lock(|d| { d.active_jobs.push_back(sv1_mining_notify_msg.clone());
                                debug!(
                                    "Downstream {}: Added job_id {} to active_jobs. Current jobs: {:?}",
                                    connection_id,
                                    sv1_mining_notify_msg.job_id,
                                    d.active_jobs.iter().map(|n| &n.job_id).collect::<Vec<_>>()
                                );
                                if d.active_jobs.len() > 2 {
                                    if let Some(removed) = d.active_jobs.pop_front() {
                                        debug!("Downstream {}: Removed oldest job_id {}", connection_id, removed.job_id);
                                    }
                                }
                             }).is_err() {
                                error!("Translator Downstream Mutex Poisoned");
                                break;
                             }

                            if let Some(mask) = mask {
                                sv1_mining_notify_msg.version = HexU32Be(sv1_mining_notify_msg.version.0 & !mask.0);
                            }

                            let message: json_rpc::Message = sv1_mining_notify_msg.clone().into();

                            handle_result!(tx_status_notify, Downstream::send_message_downstream(downstream.clone(), message).await);
                        },
                        _ = rx_shutdown.recv().fuse() => {
                                break;
                            }
                    };
                } else {
                    // timeout connection if miner does not send the authorize message after sending
                    // a subscribe
                    if timeout_timer.elapsed().as_secs() > SUBSCRIBE_TIMEOUT_SECS {
                        debug!(
                            "Downstream: miner.subscribe/miner.authorize TIMOUT for {}",
                            &host
                        );
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
            let _ = Self::remove_miner_hashrate_from_channel(self_);
            kill(&tx_shutdown).await;
            warn!(
                "Downstream: Shutting down sv1 downstream job notifier for {}",
                &host
            );
        });

        let _ = task_collector_notify_task
            .safe_lock(|a| a.push((notify_task.abort_handle(), "notify_task".to_string())));
    }

    /// Accepts incoming TCP connections from SV1 mining clients on the configured address.
    ///
    /// For each new connection, it attempts to open a new SV1 downstream channel
    /// via the Bridge (`bridge.on_new_sv1_connection`). If successful, it spawns
    /// a new task using `Downstream::new_downstream` to handle
    /// the communication and logic for that specific miner connection.
    /// This method runs indefinitely, listening for and accepting new connections.
    #[allow(clippy::too_many_arguments)]
    pub fn accept_connections(
        downstream_addr: SocketAddr,
        tx_sv1_submit: Sender<DownstreamMessages>,
        tx_mining_notify: broadcast::Sender<server_to_client::Notify<'static>>,
        tx_status: status::Sender,
        bridge: Arc<Mutex<crate::proxy::Bridge>>,
        downstream_difficulty_config: DownstreamDifficultyConfig,
        upstream_difficulty_config: Arc<Mutex<UpstreamDifficultyConfig>>,
        task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>>,
    ) {
        let accept_connections = tokio::task::spawn({
            let task_collector = task_collector.clone();
            async move {
                let listener = TcpListener::bind(downstream_addr).await.unwrap();

                while let Ok((stream, _)) = listener.accept().await {
                    let expected_hash_rate =
                        downstream_difficulty_config.min_individual_miner_hashrate;
                    let open_sv1_downstream = bridge
                        .safe_lock(|s| s.on_new_sv1_connection(expected_hash_rate))
                        .unwrap();

                    let host = stream.peer_addr().unwrap().to_string();

                    match open_sv1_downstream {
                        Ok(opened) => {
                            info!("PROXY SERVER - ACCEPTING FROM DOWNSTREAM: {}", host);
                            Downstream::new_downstream(
                                stream,
                                opened.channel_id,
                                tx_sv1_submit.clone(),
                                tx_mining_notify.subscribe(),
                                tx_status.listener_to_connection(),
                                opened.extranonce,
                                opened.last_notify,
                                opened.extranonce2_len as usize,
                                host,
                                downstream_difficulty_config.clone(),
                                upstream_difficulty_config.clone(),
                                task_collector.clone(),
                            )
                            .await;
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to create a new downstream connection: {:?}",
                                e
                            );
                        }
                    }
                }
            }
        });
        let _ = task_collector.safe_lock(|a| {
            a.push((
                accept_connections.abort_handle(),
                "accept_connections".to_string(),
            ))
        });
    }

    /// Handles incoming SV1 JSON-RPC messages from a downstream miner.
    ///
    /// This function acts as the entry point for processing messages received
    /// from a miner after framing. It uses the `IsServer` trait implementation
    /// to parse and handle standard SV1 requests (`mining.subscribe`, `mining.authorize`,
    /// `mining.submit`, `mining.configure`). Depending on the message type, it may generate a
    /// direct SV1 response to be sent back to the miner or indicate that the message needs to
    /// be translated and sent upstream (handled elsewhere, typically by the Bridge).
    async fn handle_incoming_sv1(
        self_: Arc<Mutex<Self>>,
        message_sv1: json_rpc::Message,
    ) -> Result<(), super::super::error::Error<'static>> {
        // `handle_message` in `IsServer` trait + calls `handle_request`
        // TODO: Map err from V1Error to Error::V1Error
        let response = self_.safe_lock(|s| s.handle_message(message_sv1)).unwrap();
        match response {
            Ok(res) => {
                if let Some(r) = res {
                    // If some response is received, indicates no messages translation is needed
                    // and response should be sent directly to the SV1 Downstream. Otherwise,
                    // message will be sent to the upstream Translator to be translated to SV2 and
                    // forwarded to the `Upstream`
                    // let sender = self_.safe_lock(|s| s.connection.sender_upstream)
                    if let Err(e) = Self::send_message_downstream(self_, r.into()).await {
                        return Err(e.into());
                    }
                    Ok(())
                } else {
                    // If None response is received, indicates this SV1 message received from the
                    // Downstream MD is passed to the `Translator` for translation into SV2
                    Ok(())
                }
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Sends a SV1 JSON-RPC message to the downstream miner's socket writer task.
    ///
    /// This method is used to send response messages or notifications (like
    /// `mining.notify` or `mining.set_difficulty`) to the connected miner.
    /// The message is sent over the internal `tx_outgoing` channel, which is
    /// read by the socket writer task responsible for serializing and writing
    /// the message to the TCP stream.
    pub(super) async fn send_message_downstream(
        self_: Arc<Mutex<Self>>,
        response: json_rpc::Message,
    ) -> Result<(), async_channel::SendError<v1::Message>> {
        let sender = self_.safe_lock(|s| s.tx_outgoing.clone()).unwrap();
        debug!("To DOWN: {:?}", response);
        sender.send(response).await
    }

    /// Sends a message originating from the downstream handler to the Bridge.
    ///
    /// This function is used to forward messages that require translation or
    /// central processing by the Bridge, such as `SubmitShares` or `SetDownstreamTarget`.
    /// The message is sent over the internal `tx_sv1_bridge` channel.
    pub(super) async fn send_message_upstream(
        self_: Arc<Mutex<Self>>,
        msg: DownstreamMessages,
    ) -> ProxyResult<'static, ()> {
        let sender = self_.safe_lock(|s| s.tx_sv1_bridge.clone()).unwrap();
        debug!("To Bridge: {:?}", msg);
        let _ = sender.send(msg).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gets_difficulty_from_target() {
        let target = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 255, 127,
            0, 0, 0, 0, 0,
        ];
        let actual = Downstream::difficulty_from_target(target).unwrap();
        let expect = 512.0;
        assert_eq!(actual, expect);
    }
}
