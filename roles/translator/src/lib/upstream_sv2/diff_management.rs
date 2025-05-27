//! ## Upstream SV2 Difficulty Management
//!
//! This module contains logic for managing difficulty and hashrate updates
//! specifically for the upstream SV2 connection.
//!
//! It defines method for the [`Upstream`] struct
//! related to checking configuration intervals and sending
//! `UpdateChannel` messages to the upstream server
//! based on configured nominal hashrate changes.

use crate::config::UpstreamDifficultyConfig;

use super::Upstream;

use super::super::{
    error::ProxyResult,
    upstream_sv2::{EitherFrame, Message, StdFrame},
};
use binary_sv2::u256_from_int;
use roles_logic_sv2::{
    mining_sv2::UpdateChannel, parsers::Mining, utils::Mutex, Error as RolesLogicError,
};
use std::{sync::Arc, time::Duration};

impl Upstream {
    /// Attempts to update the upstream channel's nominal hashrate if the configured
    /// update interval has elapsed or if the nominal hashrate has changed
    pub(super) async fn try_update_hashrate(self_: Arc<Mutex<Self>>) -> ProxyResult<'static, ()> {
        let tx_frame = self_.safe_lock(|u| u.connection.sender.clone())?;
        let result = self_.safe_lock(|upstream| {
            let result = upstream
                .upstream_channel_manager
                .safe_lock(|upstream_manager| {
                    let result = upstream_manager
                        .channel_ids
                        .iter()
                        .map(|id| {
                            (
                                *id,
                                upstream_manager
                                    .upstream_difficulty
                                    .get(id)
                                    .unwrap()
                                    .clone(),
                                *upstream_manager.last_sent_hashrate.get(id).unwrap(),
                            )
                        })
                        .collect::<Vec<(u32, UpstreamDifficultyConfig, f32)>>();
                    result
                })
                .unwrap();
            result
        })?;

        for (channel_id, diff_mgmt, last_sent_hashrate) in result {
            let new_hashrate = diff_mgmt.channel_nominal_hashrate;

            let has_changed = new_hashrate != last_sent_hashrate;

        if has_changed {
            // Send UpdateChannel only if hashrate actually changed
            let update_channel = UpdateChannel {
                channel_id,
                nominal_hash_rate: new_hashrate,
                maximum_target: u256_from_int(u64::MAX),
            };
            let message = Message::Mining(Mining::UpdateChannel(update_channel));
            let either_frame: StdFrame = message.try_into()?;
            let frame: EitherFrame = either_frame.into();

                tx_frame.send(frame).await?;
                self_.safe_lock(|upstream| {
                    _ = upstream
                        .upstream_channel_manager
                        .safe_lock(|upstream_manager| {
                            upstream_manager
                                .last_sent_hashrate
                                .insert(channel_id, new_hashrate);
                        });
                })?;
            }
        }

        // Always sleep, regardless of update
        tokio::time::sleep(Duration::from_secs(60_u64)).await;
        Ok(())
    }
}
