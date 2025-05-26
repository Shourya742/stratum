#![allow(warnings)]
//! What should be the role of channel manager?
//!
//! 1. It should assign extranonce to new connection.
//! 2. It can coordinate with upstream module to open connection in case of non-aggregation.
//! 3. It should perform share validation and send correct response to corresponding downstream.
//! 4. It should be responsible for difficulty management for each sv1 channel.
//! 5. It should harbour the jobs received from upstream, to perform validation correctly.

/// We gonna be having two flows, one for aggregation and another for non-aggregation
/// In case of aggregation, the whole tproxy flow gonna start from the upstream submodule
/// where the upstream gonna connect to Pool/JDC by itself at the beginning of the setup.
/// In case of non-aggregation, the whole tproxy flow gonna start from downstream, where
/// once a downstream connects we gonna open a extended mining channel with the upstream.
use std::collections::{HashMap, HashSet};

use roles_logic_sv2::{
    channels::server::{jobs::extended::ExtendedJob, share_accounting::ShareAccounting},
    mining_sv2::{ExtendedExtranonce, Extranonce, NewExtendedMiningJob, SetNewPrevHash, Target},
    utils::Id,
};
use v1::utils::HexU32Be;

use crate::{
    config::UpstreamDifficultyConfig, downstream_sv1::SubmitShareWithChannelId,
    utils::proxy_extranonce1_len,
};

#[derive(PartialEq, Hash, Eq, Clone)]
pub struct Sv1ChannelId(u32);

/// Sv1 channel representation
pub struct Sv1Channel {
    // Channel id of the connection
    channel_id: Sv1ChannelId,
    // User identity
    user_identity: String,
    // Extranonce prefix allocated for the connection
    extranonce_prefix: Vec<u8>,
    // Rollable extranonce size for the connection
    rollable_extranonce_size: u16,
    /// Version rolling mask bits
    version_rolling_mask: Option<HexU32Be>,
    /// Minimum version rolling mask bits size
    version_rolling_min_bit: Option<HexU32Be>,
}

impl Sv1Channel {
    fn new(
        channel_id: Sv1ChannelId,
        user_identity: String,
        extranonce_prefix: Vec<u8>,
        rollable_extranonce_size: u16,
    ) -> Self {
        Self {
            channel_id,
            user_identity,
            extranonce_prefix,
            rollable_extranonce_size,
            version_rolling_mask: None,
            version_rolling_min_bit: None,
        }
    }
}

pub struct UpstreamChannelManager {
    pub channel_ids: HashSet<u32>,
    pub downstream_managers: HashMap<u32, ChannelManager>,
    pub upstream_difficulty: HashMap<u32, UpstreamDifficultyConfig>,
}

impl UpstreamChannelManager {
    pub fn new() -> Self {
        Self {
            channel_ids: HashSet::new(),
            downstream_managers: HashMap::new(),
            upstream_difficulty: HashMap::new(),
        }
    }
    
    pub fn remove(&mut self, id: u32) {
        self.channel_ids.remove(&id);
        // todo: Improve this later
        self.downstream_managers.remove(&id);
        self.upstream_difficulty.remove(&id);
    }
}

// Just struct this for non-aggregation case first.
pub struct ChannelManager {
    // Channel extranonce distributor.
    pub extended_extranonce_factory: ExtendedExtranonce,
    // Share account.
    pub share_accounting: HashMap<Sv1ChannelId, ShareAccounting>,
    // expected share per minute from config.
    pub expected_share_per_minute: f32,
    // Difficulty config per connection
    pub difficulty_config: HashMap<Sv1ChannelId, DownstreamDifficultyConfig>,
    // ID generator
    pub downstream_id_factory: Id,
    // future jobs are indexed with job_id (u32)
    pub future_jobs: HashMap<u32, NewExtendedMiningJob<'static>>,
    // Currently active job shared by upstream
    pub active_job: Option<NewExtendedMiningJob<'static>>,
    // past jobs are indexed with job_id (u32)
    pub past_jobs: HashMap<u32, NewExtendedMiningJob<'static>>,
    // stale jobs are indexed with job_id (u32)
    pub stale_jobs: HashMap<u32, NewExtendedMiningJob<'static>>,
}

#[derive(Debug, Clone)]
pub struct DownstreamDifficultyConfig {
    pub min_individual_miner_hashrate: f32,
    pub submits_since_last_update: u32,
    pub timestamp_of_last_update: u64,
}

impl DownstreamDifficultyConfig {
    fn new() -> Self {
        Self {
            min_individual_miner_hashrate: 10_000_000_000_000.0,
            submits_since_last_update: 0,
            timestamp_of_last_update: 0,
        }
    }
}

impl ChannelManager {
    pub fn new(
        extranonce_prefix: Extranonce,
        extranonce_prefix_len: usize,
        extranonce_size: usize,
        min_extranonce_size: usize,
        expected_share_per_minute: f32,
    ) -> Self {
        let tproxy_len = proxy_extranonce1_len(extranonce_size, min_extranonce_size);
        let range_0 = 0..extranonce_prefix_len;
        let range_1 = extranonce_prefix_len..extranonce_prefix_len + tproxy_len;
        let range_2 = extranonce_prefix_len + tproxy_len..extranonce_prefix_len + extranonce_size;
        let extended_extranonce_factory = ExtendedExtranonce::from_upstream_extranonce(
            extranonce_prefix,
            range_0,
            range_1,
            range_2,
        )
        .expect("Something went wrong extranonce factory");

        Self {
            extended_extranonce_factory,
            share_accounting: HashMap::new(),
            expected_share_per_minute,
            difficulty_config: HashMap::new(),
            downstream_id_factory: Id::new(),
            future_jobs: HashMap::new(),
            active_job: None,
            past_jobs: HashMap::new(),
            stale_jobs: HashMap::new(),
        }
    }

    /// What I need to do:
    /// 1. I should generate an Id to it.
    /// 2. I should assign a extranonce field for new downstream
    /// 3. I should add an entry in share_accounter
    /// 4. I should add an entry in difficulty_config
    fn on_new_downstream_connection(
        &mut self,
        user_identity: String,
    ) -> (Sv1ChannelId, Vec<u8>, usize) {
        let new_downstream_id = Sv1ChannelId(self.downstream_id_factory.next());
        let max_extranonce2_len = self.extended_extranonce_factory.get_range2_len() as usize;
        let new_extranonce = self
            .extended_extranonce_factory
            .next_prefix_extended(max_extranonce2_len)
            .expect("Should have generated the extranonce prefix");
        let sv1_object = Sv1Channel::new(
            new_downstream_id.clone(),
            user_identity,
            new_extranonce.clone().to_vec(),
            max_extranonce2_len as u16,
        );
        self.share_accounting
            .insert(new_downstream_id.clone(), ShareAccounting::new(0));
        self.difficulty_config
            .insert(new_downstream_id.clone(), DownstreamDifficultyConfig::new());
        (
            new_downstream_id,
            new_extranonce.to_vec(),
            max_extranonce2_len,
        )
    }

    /// validated whether share is acceptable or not
    /// Then share the result to downstream and upstream (if accepted)
    /// Check against active and past jobs.
    pub fn on_submit_share(&self, share: SubmitShareWithChannelId) -> bool {
        let job_id = share.share.job_id.parse::<u32>().unwrap();
        match self.active_job.as_ref() {
            Some(active_job) => {
                if job_id == active_job.job_id {
                    return self.share_validation(share, Some(active_job));
                }

                if self.past_jobs.contains_key(&job_id) {
                    return self.share_validation(share, self.past_jobs.get(&job_id));
                }

                return false;
            }
            None => return false,
        }
    }

    pub fn share_validation(
        &self,
        share: SubmitShareWithChannelId,
        job: Option<&NewExtendedMiningJob<'static>>,
    ) -> bool {
        todo!()
    }

    pub fn on_new_prev_hash(&mut self, set_new_prevhash: SetNewPrevHash<'static>) {
        let job_id = set_new_prevhash.job_id;
        self.active_job = None;
        if self.future_jobs.contains_key(&job_id) {
            self.active_job = self.future_jobs.get(&job_id).cloned();
        }
        self.future_jobs.clear();
        self.past_jobs.clear();
        self.stale_jobs.clear();
    }

    pub fn on_new_extended_job(&mut self, extended_job: NewExtendedMiningJob<'static>) {
        if extended_job.is_future() {
            self.future_jobs.insert(extended_job.job_id, extended_job);
            return;
        }
        if self.active_job.is_none() {
            self.active_job = Some(extended_job);
            return;
        }

        let past_active_job = self.active_job.take().expect("Active job should be active");
        self.active_job = Some(extended_job);
        self.past_jobs
            .insert(past_active_job.job_id, past_active_job);
    }
}
