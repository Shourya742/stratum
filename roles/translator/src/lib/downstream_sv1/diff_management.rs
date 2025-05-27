//! ## Downstream SV1 Difficulty Management Module
//!
//! This module contains the logic and helper functions
//! for managing difficulty and hashrate adjustments for downstream mining clients
//! communicating via the SV1 protocol.
//!
//! It handles tasks such as:
//! - Converting SV2 targets received from upstream into SV1 difficulty values.
//! - Calculating and updating individual miner hashrates based on submitted shares.
//! - Preparing SV1 `mining.set_difficulty` messages.
//! - Potentially managing difficulty thresholds and adjustment logic for downstream miners.

use super::{Downstream, DownstreamMessages, SetDownstreamTarget};

use super::super::error::{Error, ProxyResult};
use primitive_types::U256;
use roles_logic_sv2::utils::Mutex;
use std::{ops::Div, sync::Arc};
use v1::json_rpc;

impl Downstream {
    /// Initializes the difficulty management parameters for a downstream connection.
    ///
    /// This function sets the initial timestamp for the last difficulty update and
    /// resets the count of submitted shares. It also adds the miner's configured
    /// minimum hashrate to the aggregated channel nominal hashrate stored in the
    /// upstream difficulty configuration.Finally, it sends a `SetDownstreamTarget` message upstream
    /// to the Bridge to inform it of the initial target for this new connection, derived from
    /// the provided `init_target`.This should typically be called once when a downstream connection
    /// is established.
    pub async fn init_difficulty_management(
        self_: Arc<Mutex<Self>>,
        init_target: &[u8],
    ) -> ProxyResult<'static, ()> {
        let (channel_id, connection_id, upstream_difficulty_config, miner_hashrate) = self_
            .safe_lock(|d| {
                let timestamp_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("time went backwards")
                    .as_secs();
                d.difficulty_mgmt.timestamp_of_last_update = timestamp_secs;
                d.difficulty_mgmt.submits_since_last_update = 0;
                (
                    d.channel_id,
                    d.connection_id.clone(),
                    d.upstream_difficulty_config.clone(),
                    d.difficulty_mgmt.min_individual_miner_hashrate,
                )
            })?;
        // add new connection hashrate to channel hashrate
        upstream_difficulty_config.safe_lock(|u| {
            u.channel_nominal_hashrate += miner_hashrate;
        })?;
        // update downstream target with bridge
        let init_target = binary_sv2::U256::try_from(init_target.to_vec())?;
        Self::send_message_upstream(
            self_,
            DownstreamMessages::SetDownstreamTarget(SetDownstreamTarget {
                connection_id,
                channel_id,
                new_target: init_target.into(),
            }),
        )
        .await?;

        Ok(())
    }

    /// Removes the disconnecting miner's hashrate from the aggregated channel nominal hashrate.
    ///
    /// This function is called when a downstream miner disconnects to ensure that their
    /// individual hashrate is subtracted from the total nominal hashrate reported for
    /// the channel to the upstream server.
    #[allow(clippy::result_large_err)]
    pub fn remove_miner_hashrate_from_channel(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        self_.safe_lock(|d| {
            d.upstream_difficulty_config
                .safe_lock(|u| {
                    let hashrate_to_subtract = d.difficulty_mgmt.min_individual_miner_hashrate;
                    if u.channel_nominal_hashrate >= hashrate_to_subtract {
                        u.channel_nominal_hashrate -= hashrate_to_subtract;
                    } else {
                        u.channel_nominal_hashrate = 0.0;
                    }
                })
                .map_err(|_e| Error::PoisonLock)
        })??;
        Ok(())
    }

    /// Attempts to update the difficulty settings for a downstream miner based on their
    /// performance.
    ///
    /// This function is triggered periodically or based on share submissions. It calculates
    /// the miner's estimated hashrate based on the number of shares submitted and the elapsed
    /// time since the last update. If the estimated hashrate has changed significantly according to
    /// predefined thresholds, a new target is calculated, a `mining.set_difficulty` message is
    /// sent to the miner, and a `SetDownstreamTarget` message is sent upstream to the Bridge to
    /// notify it of the target change for this channel. The difficulty management parameters
    /// (timestamp and share count) are then reset.
    pub async fn try_update_difficulty_settings(
        self_: Arc<Mutex<Self>>,
    ) -> ProxyResult<'static, ()> {
        let (diff_mgmt, channel_id, connection_id) = self_.clone().safe_lock(|d| {
            (
                d.difficulty_mgmt.clone(),
                d.channel_id,
                d.connection_id.clone(),
            )
        })?;
        tracing::debug!(
            "Time of last diff update: {:?}",
            diff_mgmt.timestamp_of_last_update
        );
        tracing::debug!(
            "Number of shares submitted: {:?}",
            diff_mgmt.submits_since_last_update
        );
        let prev_target = match roles_logic_sv2::utils::hash_rate_to_target(
            diff_mgmt.min_individual_miner_hashrate.into(),
            diff_mgmt.shares_per_minute.into(),
        ) {
            Ok(target) => target.to_vec(),
            Err(v) => return Err(Error::TargetError(v)),
        };
        if let Some(new_hash_rate) =
            Self::update_miner_hashrate(self_.clone(), prev_target.clone())?
        {
            let new_target = match roles_logic_sv2::utils::hash_rate_to_target(
                new_hash_rate.into(),
                diff_mgmt.shares_per_minute.into(),
            ) {
                Ok(target) => target,
                Err(v) => return Err(Error::TargetError(v)),
            };
            tracing::debug!("New target from hashrate: {:?}", new_target.inner_as_ref());
            let message = Self::get_set_difficulty(new_target.to_vec())?;
            // send mining.set_difficulty to miner
            Downstream::send_message_downstream(self_.clone(), message).await?;
            let update_target_msg = SetDownstreamTarget {
                connection_id,
                channel_id,
                new_target: new_target.into(),
            };
            // notify bridge of target update
            Downstream::send_message_upstream(
                self_.clone(),
                DownstreamMessages::SetDownstreamTarget(update_target_msg),
            )
            .await?;
        }
        Ok(())
    }

    /// Calculates the mining target corresponding to the miner's current estimated hashrate.
    ///
    /// This function uses the `min_individual_miner_hashrate` stored in the downstream
    /// difficulty configuration and the configured `shares_per_minute` to calculate
    /// the target required for the miner to achieve the target share rate at their
    /// estimated hashrate.
    #[allow(clippy::result_large_err)]
    pub fn hash_rate_to_target(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, Vec<u8>> {
        self_.safe_lock(|d| {
            match roles_logic_sv2::utils::hash_rate_to_target(
                d.difficulty_mgmt.min_individual_miner_hashrate.into(),
                d.difficulty_mgmt.shares_per_minute.into(),
            ) {
                Ok(target) => Ok(target.to_vec()),
                Err(e) => Err(Error::TargetError(e)),
            }
        })?
    }

    /// Increments the counter for shares submitted by this downstream miner.
    ///
    /// This function is called each time a valid share is received from the miner.
    /// The count is used in the difficulty adjustment logic to estimate the miner's
    /// performance over a period.
    #[allow(clippy::result_large_err)]
    pub(super) fn save_share(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        self_.safe_lock(|d| {
            d.difficulty_mgmt.submits_since_last_update += 1;
        })?;
        Ok(())
    }

    /// Converts an SV2 target received from upstream into an SV1 difficulty value
    /// and formats it as a `mining.set_difficulty` JSON-RPC message.
    #[allow(clippy::result_large_err)]
    pub(super) fn get_set_difficulty(target: Vec<u8>) -> ProxyResult<'static, json_rpc::Message> {
        let value = Downstream::difficulty_from_target(target)?;
        tracing::debug!("Difficulty from target: {:?}", value);
        let set_target = v1::methods::server_to_client::SetDifficulty { value };
        let message: json_rpc::Message = set_target.into();
        Ok(message)
    }

    /// Converts target received by the `SetTarget` SV2 message from the Upstream role into the
    /// difficulty for the Downstream role sent via the SV1 `mining.set_difficulty` message.
    #[allow(clippy::result_large_err)]
    pub(super) fn difficulty_from_target(mut target: Vec<u8>) -> ProxyResult<'static, f64> {
        // reverse because target is LE and this function relies on BE
        target.reverse();
        let target = target.as_slice();
        tracing::debug!("Target: {:?}", target);

        // If received target is 0, return 0
        if Downstream::is_zero(target) {
            return Ok(0.0);
        }
        let target = U256::from_big_endian(target);
        let pdiff: [u8; 32] = [
            0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        ];
        let pdiff = U256::from_big_endian(pdiff.as_ref());

        if pdiff > target {
            let diff = pdiff.div(target);
            Ok(diff.low_u64() as f64)
        } else {
            let diff = target.div(pdiff);
            let diff = diff.low_u64() as f64;
            // TODO still results in a difficulty that is too low
            Ok(1.0 / diff)
        }
    }

    /// Updates the miner's estimated hashrate and adjusts the aggregated channel nominal hashrate.
    ///
    /// This function calculates the miner's realized shares per minute over the period
    /// since the last update and uses it, along with the current target, to estimate
    /// their hashrate. It then compares this new estimate to the previous one and
    /// updates the miner's stored hashrate and the channel's aggregated hashrate
    /// if the change is significant based on time-dependent thresholds.
    #[allow(clippy::result_large_err)]
    pub fn update_miner_hashrate(
        self_: Arc<Mutex<Self>>,
        miner_target: Vec<u8>,
    ) -> ProxyResult<'static, Option<f32>> {
        self_
            .safe_lock(|d| {
                let timestamp_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("time went backwards")
                    .as_secs();

                // reset if timestamp is at 0
                if d.difficulty_mgmt.timestamp_of_last_update == 0 {
                    d.difficulty_mgmt.timestamp_of_last_update = timestamp_secs;
                    d.difficulty_mgmt.submits_since_last_update = 0;
                    return Ok(None);
                }

                let delta_time = timestamp_secs - d.difficulty_mgmt.timestamp_of_last_update;
                #[cfg(test)]
                if delta_time == 0 {
                    return Ok(None);
                }
                #[cfg(not(test))]
                if delta_time <= 15 {
                    return Ok(None);
                }
                tracing::debug!("DELTA TIME: {:?}", delta_time);
                let realized_share_per_min =
                    d.difficulty_mgmt.submits_since_last_update as f64 / (delta_time as f64 / 60.0);
                tracing::debug!("REALIZED SHARES PER MINUTE: {:?}", realized_share_per_min);
                tracing::debug!("CURRENT MINER TARGET: {:?}", miner_target);
                let mut new_miner_hashrate = match roles_logic_sv2::utils::hash_rate_from_target(
                    miner_target.clone().try_into()?,
                    realized_share_per_min,
                ) {
                    Ok(hashrate) => hashrate as f32,
                    Err(e) => {
                        tracing::debug!("{:?} -> Probably min_individual_miner_hashrate parameter was not set properly in config file. New hashrate will be automatically adjusted to match the real one.", e);
                        d.difficulty_mgmt.min_individual_miner_hashrate * realized_share_per_min as f32 / d.difficulty_mgmt.shares_per_minute
                    }
                };

                let mut hashrate_delta =
                    new_miner_hashrate - d.difficulty_mgmt.min_individual_miner_hashrate;
                let hashrate_delta_percentage = (hashrate_delta.abs()
                    / d.difficulty_mgmt.min_individual_miner_hashrate)
                    * 100.0;
                tracing::debug!("\nMINER HASHRATE: {:?}", new_miner_hashrate);

                if (hashrate_delta_percentage >= 100.0)
                    || (hashrate_delta_percentage >= 60.0) && (delta_time >= 60)
                    || (hashrate_delta_percentage >= 50.0) && (delta_time >= 120)
                    || (hashrate_delta_percentage >= 45.0) && (delta_time >= 180)
                    || (hashrate_delta_percentage >= 30.0) && (delta_time >= 240)
                    || (hashrate_delta_percentage >= 15.0) && (delta_time >= 300)
                {
                // realized_share_per_min is 0.0 when d.difficulty_mgmt.submits_since_last_update is 0
                // so it's safe to compare realized_share_per_min with == 0.0
                if realized_share_per_min == 0.0 {
                    new_miner_hashrate = match delta_time {
                        dt if dt <= 30 => d.difficulty_mgmt.min_individual_miner_hashrate / 1.5,
                        dt if dt < 60 => d.difficulty_mgmt.min_individual_miner_hashrate / 2.0,
                        _ => d.difficulty_mgmt.min_individual_miner_hashrate / 3.0,
                    };
                    hashrate_delta =
                        new_miner_hashrate - d.difficulty_mgmt.min_individual_miner_hashrate;
                }
                if (realized_share_per_min > 0.0) && (hashrate_delta_percentage > 1000.0) {
                    new_miner_hashrate = match delta_time {
                        dt if dt <= 30 => d.difficulty_mgmt.min_individual_miner_hashrate * 10.0,
                        dt if dt < 60 => d.difficulty_mgmt.min_individual_miner_hashrate * 5.0,
                        _ => d.difficulty_mgmt.min_individual_miner_hashrate * 3.0,
                    };
                    hashrate_delta =
                        new_miner_hashrate - d.difficulty_mgmt.min_individual_miner_hashrate;
                }
                d.difficulty_mgmt.min_individual_miner_hashrate = new_miner_hashrate;
                d.difficulty_mgmt.timestamp_of_last_update = timestamp_secs;
                d.difficulty_mgmt.submits_since_last_update = 0;
                d.upstream_difficulty_config.super_safe_lock(|c| {
                    if c.channel_nominal_hashrate + hashrate_delta > 0.0 {
                        c.channel_nominal_hashrate += hashrate_delta;
                    } else {
                        c.channel_nominal_hashrate = 0.0;
                    }
                });
                Ok(Some(new_miner_hashrate))
                } else {
                    Ok(None)
                }
            })?
    }

    /// Helper function to check if target is set to zero for some reason (typically happens when
    /// Downstream role first connects).
    /// https://stackoverflow.com/questions/65367552/checking-a-vecu8-to-see-if-its-all-zero
    fn is_zero(buf: &[u8]) -> bool {
        let (prefix, aligned, suffix) = unsafe { buf.align_to::<u128>() };

        prefix.iter().all(|&x| x == 0)
            && suffix.iter().all(|&x| x == 0)
            && aligned.iter().all(|&x| x == 0)
    }
}
