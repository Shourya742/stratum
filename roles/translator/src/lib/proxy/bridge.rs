//!  ## Proxy Bridge Module
//!
//! This module defines the [`Bridge`] structure, which acts as the central component
//! responsible for translating messages and coordinating communication between
//! the upstream SV2 role and the downstream SV1 mining clients.
//!
//! The Bridge manages message queues, maintains the state required for translation
//! (such as job IDs, previous hashes, and mining jobs), handles share submissions
//! from downstream, and forwards translated jobs received from upstream to downstream miners.
//!
//! This module handles:
//! - Receiving SV1 `mining.submit` messages from [`Downstream`] connections.
//! - Translating SV1 submits into SV2 `SubmitSharesExtended`.
//! - Receiving SV2 `SetNewPrevHash` and `NewExtendedMiningJob` from the upstream.
//! - Translating SV2 job messages into SV1 `mining.notify` messages.
//! - Sending translated SV2 submits to the upstream.
//! - Broadcasting translated SV1 notifications to connected downstream miners.
//! - Managing channel state and difficulty related to job translation.
//! - Handling new downstream SV1 connections.
use crate::{
    channel_manager::{Sv1ChannelId, UpstreamChannelManager},
    proxy::next_mining_notify::create_notify,
};

use super::super::{
    downstream_sv1::{DownstreamMessages, SetDownstreamTarget, SubmitShareWithChannelId},
    error::{Error, ProxyResult},
    status,
};
use async_channel::{Receiver, Sender};
use error_handling::handle_result;
use roles_logic_sv2::{
    mining_sv2::{NewExtendedMiningJob, SetNewPrevHash, SubmitSharesExtended},
    utils::Mutex,
    Error as RolesLogicError,
};
use std::sync::Arc;
use tokio::{sync::broadcast, task::AbortHandle};
use tracing::debug;
use v1::{client_to_server::Submit, server_to_client, utils::HexU32Be};

/// Bridge between the SV2 `Upstream` and SV1 `Downstream` responsible for the following messaging
/// translation:
/// 1. SV1 `mining.submit` -> SV2 `SubmitSharesExtended`
/// 2. SV2 `SetNewPrevHash` + `NewExtendedMiningJob` -> SV1 `mining.notify`
#[derive(Debug)]
pub struct Bridge {
    /// Receives a SV1 `mining.submit` message from the Downstream role.
    rx_sv1_downstream: Receiver<DownstreamMessages>,
    /// Sends SV2 `SubmitSharesExtended` messages translated from SV1 `mining.submit` messages to
    /// the `Upstream`.
    tx_sv2_submit_shares_ext: Sender<SubmitSharesExtended<'static>>,
    /// Receives a SV2 `SetNewPrevHash` message from the `Upstream` to be translated (along with a
    /// SV2 `NewExtendedMiningJob` message) to a SV1 `mining.submit` for the `Downstream`.
    rx_sv2_set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,
    /// Receives a SV2 `NewExtendedMiningJob` message from the `Upstream` to be translated (along
    /// with a SV2 `SetNewPrevHash` message) to a SV1 `mining.submit` to be sent to the
    /// `Downstream`.
    rx_sv2_new_ext_mining_job: Receiver<NewExtendedMiningJob<'static>>,
    /// Sends SV1 `mining.notify` message (translated from the SV2 `SetNewPrevHash` and
    /// `NewExtendedMiningJob` messages stored in the `NextMiningNotify`) to the `Downstream`.
    tx_sv1_notify: broadcast::Sender<server_to_client::Notify<'static>>,
    /// Allows the bridge the ability to communicate back to the main thread any status updates
    /// that would interest the main thread for error handling
    tx_status: status::Sender,
    /// The job ID of the last sent `mining.notify` message.
    last_job_id: u32,
    task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>>,
    upstream_channel_manager: Arc<Mutex<UpstreamChannelManager>>,
}

impl Bridge {
    #[allow(clippy::too_many_arguments)]
    /// Instantiates a new `Bridge` with the provided communication channels and initial
    /// configurations.
    ///
    /// Sets up the core communication pathways between upstream and downstream handlers
    /// and initializes the internal state, including the channel factory.
    pub fn new(
        rx_sv1_downstream: Receiver<DownstreamMessages>,
        tx_sv2_submit_shares_ext: Sender<SubmitSharesExtended<'static>>,
        rx_sv2_set_new_prev_hash: Receiver<SetNewPrevHash<'static>>,
        rx_sv2_new_ext_mining_job: Receiver<NewExtendedMiningJob<'static>>,
        tx_sv1_notify: broadcast::Sender<server_to_client::Notify<'static>>,
        tx_status: status::Sender,
        task_collector: Arc<Mutex<Vec<(AbortHandle, String)>>>,
        upstream_channel_manager: Arc<Mutex<UpstreamChannelManager>>,
    ) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            rx_sv1_downstream,
            tx_sv2_submit_shares_ext,
            rx_sv2_set_new_prev_hash,
            rx_sv2_new_ext_mining_job,
            tx_sv1_notify,
            tx_status,
            last_job_id: 0,
            task_collector,
            upstream_channel_manager,
        }))
    }

    /// Handles the event of a new SV1 downstream client connecting.
    ///
    /// Creates a new extended channel using the internal `channel_factory` for the
    /// new connection. It assigns a unique channel ID, determines the initial
    /// extranonce and target for the miner, and provides the last known
    /// `mining.notify` message to immediately send to the new client.
    #[allow(clippy::result_large_err)]
    pub fn on_new_sv1_connection(
        &mut self,
        _hash_rate: f32,
    ) -> ProxyResult<'static, Option<OpenSv1Downstream>> {
        let result = self
            .upstream_channel_manager
            .safe_lock(|upstream_channel_manager| {
                if upstream_channel_manager.aggregate {
                    // In this case we already know that, we gonna have a single downstream
                    // channel manager whose job is gonna be to aggregate downstream miners.

                    if let Some(manager) = upstream_channel_manager
                        .downstream_managers
                        .values_mut()
                        .next()
                    {
                        let (channel_id, connection_id, extranonce, extranonce2_len) =
                            manager.on_new_downstream_connection("dummy".into());
                        let active_job = manager.active_job.clone();
                        let prev_hash = manager.prev_block_hash.clone();
                        if let Some(active_job) = active_job {
                            let result = prev_hash.map(|m| {
                                let last_notify = create_notify(m, active_job, true);
                                OpenSv1Downstream {
                                    channel_id,
                                    connection_id,
                                    last_notify: Some(last_notify),
                                    extranonce,
                                    extranonce2_len: extranonce2_len as u16,
                                }
                            });
                            return Ok(result);
                        }
                        return Ok(Some(OpenSv1Downstream {
                            channel_id,
                            connection_id,
                            last_notify: None,
                            extranonce,
                            extranonce2_len: extranonce2_len as u16,
                        }));
                    }
                    Ok(None)
                } else {
                    // For each new connection we gonna open a separate OpenExtendedMiningChannel
                    // with upstream.
                    Ok(None)
                }
            })?;
        result
    }

    /// Starts the tasks responsible for receiving and processing
    /// messages from both upstream SV2 and downstream SV1 connections.
    ///
    /// This function spawns three main tasks:
    /// 1. `handle_new_prev_hash`: Listens for SV2 `SetNewPrevHash` messages.
    /// 2. `handle_new_extended_mining_job`: Listens for SV2 `NewExtendedMiningJob` messages.
    /// 3. `handle_downstream_messages`: Listens for `DownstreamMessages` (e.g., submit shares) from
    ///    downstream clients.
    pub fn start(self_: Arc<Mutex<Self>>) {
        Self::start_upstream_job_handler(self_.clone());
        Self::handle_downstream_messages(self_);
    }

    fn start_upstream_job_handler(self_: Arc<Mutex<Self>>) {
        let task_collector = self_.safe_lock(|b| b.task_collector.clone()).unwrap();
        let (tx_sv1_notify, rx_prev_hash, rx_new_job, tx_status) = self_
            .safe_lock(|s| {
                (
                    s.tx_sv1_notify.clone(),
                    s.rx_sv2_set_new_prev_hash.clone(),
                    s.rx_sv2_new_ext_mining_job.clone(),
                    s.tx_status.clone(),
                )
            })
            .unwrap();

        debug!("Starting upstream job handler task");
        let handle = tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    Ok(prev_hash) = rx_prev_hash.recv() => {
                        debug!("Received SetNewPrevHash (Job ID: {:?})", prev_hash.job_id);
                        handle_result!(
                            tx_status.clone(),
                            Self::handle_new_prev_hash_(self_.clone(), prev_hash, tx_sv1_notify.clone()).await
                        );
                    }
                    Ok(new_job) = rx_new_job.recv() => {
                        debug!("Received NewExtendedMiningJob (Job ID: {:?})", new_job.job_id);
                        handle_result!(
                            tx_status.clone(),
                            Self::handle_new_extended_mining_job_(self_.clone(), new_job, tx_sv1_notify.clone()).await
                        );
                         crate::upstream_sv2::upstream::IS_NEW_JOB_HANDLED
                            .store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    else => {
                        // One or both channels closed, indicating upstream disconnection or shutdown.
                        debug!("Upstream job channel(s) closed. Exiting job handler.");
                        break;
                    }
                }
            }
        });

        task_collector
            .safe_lock(|c| c.push((handle.abort_handle(), "handle_upstream_job_handler".into())))
            .unwrap();
    }

    /// Task handler that receives `DownstreamMessages` and dispatches them.
    ///
    /// This loop continuously receives messages from the `rx_sv1_downstream` channel.
    /// It matches on the `DownstreamMessages` variant and calls the appropriate
    /// handler function (`handle_submit_shares` or `handle_update_downstream_target`).
    fn handle_downstream_messages(self_: Arc<Mutex<Self>>) {
        let task_collector = self_.safe_lock(|b| b.task_collector.clone()).unwrap();
        let (rx_sv1_downstream, tx_status) = self_
            .safe_lock(|s| (s.rx_sv1_downstream.clone(), s.tx_status.clone()))
            .unwrap();

        let handle = tokio::task::spawn(async move {
            loop {
                match rx_sv1_downstream.recv().await {
                    Ok(msg) => {
                        let res = match msg {
                            DownstreamMessages::SubmitShares(share) => {
                                Self::handle_submit_shares(self_.clone(), share).await
                            }
                            DownstreamMessages::SetDownstreamTarget(new_target) => {
                                Self::handle_update_downstream_target(self_.clone(), new_target)
                            }
                        };
                        handle_result!(tx_status.clone(), res);
                    }
                    Err(_) => {
                        debug!("Downstream channel closed. Exiting downstream handler.");
                        break;
                    }
                }
            }
        });

        task_collector
            .safe_lock(|c| c.push((handle.abort_handle(), "handle_downstream_messages".into())))
            .unwrap();
    }

    /// Receives a `SetDownstreamTarget` message and updates the downstream target for a specific
    /// channel.
    ///
    /// This function is called when the downstream logic determines that a miner's
    /// target needs to be updated (e.g., due to difficulty adjustment). It updates
    /// the target within the internal `channel_factory` for the specified channel ID.
    #[allow(clippy::result_large_err)]
    fn handle_update_downstream_target(
        self_: Arc<Mutex<Self>>,
        new_target: SetDownstreamTarget,
    ) -> ProxyResult<'static, ()> {
        self_.safe_lock(|bridge| {
            bridge
                .upstream_channel_manager
                .safe_lock(|upstream_channel_manager| {
                    let downstream_manager = upstream_channel_manager
                        .downstream_managers
                        .get_mut(&new_target.channel_id);
                    if let Some(downstream_manager) = downstream_manager {
                        let difficulty_config = downstream_manager
                            .difficulty_config
                            .get_mut(&new_target.connection_id);
                        if let Some(difficulty_config) = difficulty_config {
                            difficulty_config.target = new_target.new_target;
                        }
                    }
                })
                .unwrap();
        })?;
        Ok(())
    }
    /// Receives a `SubmitShareWithChannelId` message from a downstream miner,
    /// validates the share, and sends it upstream if it meets the upstream target.
    async fn handle_submit_shares(
        self_: Arc<Mutex<Self>>,
        share: SubmitShareWithChannelId,
    ) -> ProxyResult<'static, ()> {
        let verdict = self_.safe_lock(|bridge| {
            let verdict = bridge
                .upstream_channel_manager
                .safe_lock(|upstream_manager| {
                    if let Some(downstream) = upstream_manager
                        .downstream_managers
                        .get_mut(&share.channel_id)
                    {
                        return downstream.on_submit_share(share.clone());
                    }
                    false
                })
                .unwrap();
            verdict
        })?;
        let tx_sv2_submit_shares_ext = self_.safe_lock(|s| s.tx_sv2_submit_shares_ext.clone())?;

        if verdict {
            let sv2_submit = self_.safe_lock(|s| {
                s.translate_submit(share.channel_id, share.share, share.version_rolling_mask)
            })??;

            tx_sv2_submit_shares_ext.send(sv2_submit).await?;
        }
        Ok(())
    }

    /// Translates a SV1 `mining.submit` message into an SV2 `SubmitSharesExtended` message.
    ///
    /// This function performs the necessary transformations to convert the data
    /// format used by SV1 submissions (`job_id`, `nonce`, `time`, `extra_nonce2`,
    /// `version_bits`) into the SV2 `SubmitSharesExtended` structure,
    /// taking into account version rolling if a mask is provided.
    #[allow(clippy::result_large_err)]
    fn translate_submit(
        &self,
        channel_id: u32,
        sv1_submit: Submit,
        version_rolling_mask: Option<HexU32Be>,
    ) -> ProxyResult<'static, SubmitSharesExtended<'static>> {
        let job = self
            .upstream_channel_manager
            .safe_lock(|upstream_manager| {
                let downstream_manager = upstream_manager.downstream_managers.get(&channel_id);
                if let Some(downstream_manager) = downstream_manager {
                    if let Ok(job_id) = sv1_submit.job_id.parse::<u32>() {
                        return downstream_manager.get_job(job_id);
                    }
                }
                None
            })
            .unwrap();
        if let Some(job) = job {
            let last_version = job.version;
            let version = match (sv1_submit.version_bits, version_rolling_mask) {
                // regarding version masking see https://github.com/slushpool/stratumprotocol/blob/master/stratum-extensions.mediawiki#changes-in-request-miningsubmit
                (Some(vb), Some(mask)) => (last_version & !mask.0) | (vb.0 & mask.0),
                (None, None) => last_version,
                _ => return Err(Error::V1Protocol(v1::error::Error::InvalidSubmission)),
            };
            let mining_device_extranonce: Vec<u8> = sv1_submit.extra_nonce2.into();
            let extranonce2 = mining_device_extranonce;
            return Ok(SubmitSharesExtended {
                channel_id,
                // I put 0 below cause sequence_number is not what should be TODO
                sequence_number: 0,
                job_id: sv1_submit.job_id.parse::<u32>()?,
                nonce: sv1_submit.nonce.0,
                ntime: sv1_submit.time.0,
                version,
                extranonce: extranonce2.try_into()?,
            });
        }
        Err(Error::RolesSv2Logic(RolesLogicError::NoValidJob))
    }

    /// Internal helper function to handle a received SV2 `SetNewPrevHash` message.
    ///
    /// This function processes a `SetNewPrevHash` message received from the upstream.
    /// It updates the Bridge's stored last previous hash, informs the `channel_factory`
    /// about the new previous hash, and then checks the `future_jobs` buffer for
    /// a corresponding `NewExtendedMiningJob`. If a matching future job is found, it constructs a
    /// SV1 `mining.notify` message and broadcasts it to all downstream clients. It also updates
    /// the `last_notify` state for new connections.
    async fn handle_new_prev_hash_(
        self_: Arc<Mutex<Self>>,
        sv2_set_new_prev_hash: SetNewPrevHash<'static>,
        tx_sv1_notify: broadcast::Sender<server_to_client::Notify<'static>>,
    ) -> Result<(), Error<'static>> {
        // The handle_new_prev_hash_ by bridge shouldn't be doing
        // any channel management as its job is just to translate
        // and do nothing else.
        //
        // We are fetching the current active job from corresponding
        // upstream channel, channel manager and creating its notification.

        // fetching the active job, for corresponding SetNewPrevHash message, which should already
        // be populated in channel_manager.
        let active_job = self_.safe_lock(|bridge| {
            let value = bridge
                .upstream_channel_manager
                .safe_lock(|manager| {
                    let downstream_channel_manager = manager
                        .downstream_managers
                        .get(&sv2_set_new_prev_hash.channel_id);
                    if let Some(downstream_channel_manager) = downstream_channel_manager {
                        return downstream_channel_manager.active_job.clone();
                    }
                    None
                })
                .unwrap();
            value
        })?;

        if let Some(active_job) = active_job {
            let job_id = active_job.job_id;

            // Sending the notify message to downstream.
            let notify = crate::proxy::next_mining_notify::create_notify(
                sv2_set_new_prev_hash.clone(),
                active_job,
                true,
            );

            // Get the sender to send the mining.notify to the Downstream
            tx_sv1_notify.send(notify.clone())?;
            self_.safe_lock(|s| {
                s.last_job_id = job_id;
            })?;
        }

        Ok(())
    }

    /// Internal helper function to handle a received SV2 `NewExtendedMiningJob` message.
    ///
    /// This function processes a `NewExtendedMiningJob` message received from the upstream.
    /// It first informs the `channel_factory` about the new job. If the job's `is_future` is true,
    /// the job is buffered in `future_jobs`. If `is_future` is false, it expects a
    /// corresponding `SetNewPrevHash` (which should have been received prior according to the
    /// protocol) and immediately constructs and broadcasts a SV1 `mining.notify` message to
    /// downstream clients, updating the `last_notify` state.
    async fn handle_new_extended_mining_job_(
        self_: Arc<Mutex<Self>>,
        sv2_new_extended_mining_job: NewExtendedMiningJob<'static>,
        tx_sv1_notify: broadcast::Sender<server_to_client::Notify<'static>>,
    ) -> Result<(), Error<'static>> {
        // The handle_new_extended_mining_job_ by bridge shouldn't be doing
        // any channel management as its job is just to translate
        // and do nothing else.
        //
        // We are fetching the current previous block hash from corresponding
        // upstream channel, channel manager and creating its notification.

        if sv2_new_extended_mining_job.is_future() {
            return Ok(());
        }

        // fetching the active job, for corresponding SetNewPrevHash message, which should already
        // be populated in channel_manager.
        let prev_block_hash = self_.safe_lock(|bridge| {
            let value = bridge
                .upstream_channel_manager
                .safe_lock(|manager| {
                    let downstream_channel_manager = manager
                        .downstream_managers
                        .get(&sv2_new_extended_mining_job.channel_id);
                    if let Some(downstream_channel_manager) = downstream_channel_manager {
                        return downstream_channel_manager.prev_block_hash.clone();
                    }
                    None
                })
                .unwrap();
            value
        })?;

        if let Some(prev_block_hash) = prev_block_hash {
            let job_id = sv2_new_extended_mining_job.job_id;

            // Sending the notify message to downstream.
            let notify = crate::proxy::next_mining_notify::create_notify(
                prev_block_hash,
                sv2_new_extended_mining_job,
                true,
            );
            // Get the sender to send the mining.notify to the Downstream
            tx_sv1_notify.send(notify.clone())?;
            self_.safe_lock(|s| {
                s.last_job_id = job_id;
            })?;
        }

        Ok(())
    }
}

/// Represents the necessary information to initialize a new SV1 downstream connection
/// after it has been registered with the Bridge's channel factory.
///
/// This structure is returned by `Bridge::on_new_sv1_connection` and contains the
/// channel ID assigned to the connection, the initial job notification to send,
/// and the extranonce and target specific to this channel.
pub struct OpenSv1Downstream {
    /// The unique ID assigned to this upstream channel by the channel factory.
    pub channel_id: u32,
    /// This is use to pin point a single downstream channel.
    pub connection_id: Sv1ChannelId,
    /// The most recent `mining.notify` message to send to the new client immediately
    /// upon connection to provide them with a job.
    pub last_notify: Option<server_to_client::Notify<'static>>,
    /// The extranonce prefix assigned to this channel.
    pub extranonce: Vec<u8>,
    /// The size of the extranonce2 field expected from the miner for this channel.
    pub extranonce2_len: u16,
}

// #[cfg(test)]
// mod test {
//     use super::*;
//     use async_channel::bounded;
//     use stratum_common::bitcoin::{absolute::LockTime, consensus, transaction::Version};

//     pub mod test_utils {
//         use super::*;

//         #[allow(dead_code)]
//         pub struct BridgeInterface {
//             pub tx_sv1_submit: Sender<DownstreamMessages>,
//             pub rx_sv2_submit_shares_ext: Receiver<SubmitSharesExtended<'static>>,
//             pub tx_sv2_set_new_prev_hash: Sender<SetNewPrevHash<'static>>,
//             pub tx_sv2_new_ext_mining_job: Sender<NewExtendedMiningJob<'static>>,
//             pub rx_sv1_notify: broadcast::Receiver<server_to_client::Notify<'static>>,
//         }

//         pub fn create_bridge(
//             extranonces: ExtendedExtranonce,
//         ) -> (Arc<Mutex<Bridge>>, BridgeInterface) {
//             let (tx_sv1_submit, rx_sv1_submit) = bounded(1);
//             let (tx_sv2_submit_shares_ext, rx_sv2_submit_shares_ext) = bounded(1);
//             let (tx_sv2_set_new_prev_hash, rx_sv2_set_new_prev_hash) = bounded(1);
//             let (tx_sv2_new_ext_mining_job, rx_sv2_new_ext_mining_job) = bounded(1);
//             let (tx_sv1_notify, rx_sv1_notify) = broadcast::channel(1);
//             let (tx_status, _rx_status) = bounded(1);
//             let upstream_target = vec![
//                 0, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
// 0,                 0, 0, 0, 0, 0, 0, 0,
//             ];
//             let interface = BridgeInterface {
//                 tx_sv1_submit,
//                 rx_sv2_submit_shares_ext,
//                 tx_sv2_set_new_prev_hash,
//                 tx_sv2_new_ext_mining_job,
//                 rx_sv1_notify,
//             };

//             let task_collector = Arc::new(Mutex::new(vec![]));
//             let b = Bridge::new(
//                 rx_sv1_submit,
//                 tx_sv2_submit_shares_ext,
//                 rx_sv2_set_new_prev_hash,
//                 rx_sv2_new_ext_mining_job,
//                 tx_sv1_notify,
//                 status::Sender::Bridge(tx_status),
//                 extranonces,
//                 Arc::new(Mutex::new(upstream_target)),
//                 1,
//                 task_collector,
//             );
//             (b, interface)
//         }

//         pub fn create_sv1_submit(job_id: u32) -> Submit<'static> {
//             Submit {
//                 user_name: "test_user".to_string(),
//                 job_id: job_id.to_string(),
//                 extra_nonce2: v1::utils::Extranonce::try_from([0; 32].to_vec()).unwrap(),
//                 time: v1::utils::HexU32Be(1),
//                 nonce: v1::utils::HexU32Be(1),
//                 version_bits: None,
//                 id: 0,
//             }
//         }
//     }

//     #[test]
//     fn test_version_bits_insert() {
//         use stratum_common::{
//             bitcoin,
//             bitcoin::{blockdata::witness::Witness, hashes::Hash},
//         };

//         let extranonces = ExtendedExtranonce::new(0..6, 6..8, 8..16, None)
//             .expect("Failed to create ExtendedExtranonce with valid ranges");
//         let (bridge, _) = test_utils::create_bridge(extranonces);
//         bridge
//             .safe_lock(|bridge| {
//                 let channel_id = 1;
//                 let out_id = bitcoin::hashes::sha256d::Hash::from_slice(&[
//                     0_u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//                     0, 0, 0, 0, 0, 0, 0,
//                 ])
//                 .unwrap();
//                 let p_out = bitcoin::OutPoint {
//                     txid: bitcoin::Txid::from_raw_hash(out_id),
//                     vout: 0xffff_ffff,
//                 };
//                 let in_ = bitcoin::TxIn {
//                     previous_output: p_out,
//                     script_sig: vec![89_u8; 16].into(),
//                     sequence: bitcoin::Sequence(0),
//                     witness: Witness::from(vec![] as Vec<Vec<u8>>),
//                 };
//                 let tx = bitcoin::Transaction {
//                     version: Version::ONE,
//                     lock_time: LockTime::from_consensus(0),
//                     input: vec![in_],
//                     output: vec![],
//                 };
//                 let tx = consensus::serialize(&tx);
//                 let _down = bridge
//                     .channel_factory
//                     .add_standard_channel(0, 10_000_000_000.0, true, 1)
//                     .unwrap();
//                 let prev_hash = SetNewPrevHash {
//                     channel_id,
//                     job_id: 0,
//                     prev_hash: [
//                         3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
// 3,                         3, 3, 3, 3, 3, 3, 3,
//                     ]
//                     .into(),
//                     min_ntime: 989898,
//                     nbits: 9,
//                 };
//                 bridge.channel_factory.on_new_prev_hash(prev_hash).unwrap();
//                 let now = std::time::SystemTime::now()
//                     .duration_since(std::time::UNIX_EPOCH)
//                     .unwrap()
//                     .as_secs() as u32;
//                 let new_mining_job = NewExtendedMiningJob {
//                     channel_id,
//                     job_id: 0,
//                     min_ntime: binary_sv2::Sv2Option::new(Some(now)),
//                     version: 0b0000_0000_0000_0000,
//                     version_rolling_allowed: false,
//                     merkle_path: vec![].into(),
//                     coinbase_tx_prefix: tx[0..42].to_vec().try_into().unwrap(),
//                     coinbase_tx_suffix: tx[58..].to_vec().try_into().unwrap(),
//                 };
//                 bridge
//                     .channel_factory
//                     .on_new_extended_mining_job(new_mining_job.clone())
//                     .unwrap();

//                 // pass sv1_submit into Bridge::translate_submit
//                 let sv1_submit = test_utils::create_sv1_submit(0);
//                 let sv2_message = bridge
//                     .translate_submit(channel_id, sv1_submit, None)
//                     .unwrap();
//                 // assert sv2 message equals sv1 with version bits added
//                 assert_eq!(
//                     new_mining_job.version, sv2_message.version,
//                     "Version bits were not inserted for non version rolling sv1 message"
//                 );
//             })
//             .unwrap();
//     }
// }
