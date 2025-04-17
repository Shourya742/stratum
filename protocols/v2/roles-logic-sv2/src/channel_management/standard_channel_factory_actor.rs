/// This actor-based design centralizes all mutable state within a single dedicated task. By doing so, it avoids the need for shared locks
/// 
/// Instead of allowing multiple components to directly mutate shared state, all interactions are funneled through message passing. A separate task
/// listens for incoming messages, processes them serially, and sends back responses via channels. This ensures predictable behavior makes the system easier to reason about under concurrency.

use mining_sv2::{ExtendedExtranonce, ExtendedExtranonceError, OpenStandardMiningChannel, SubmitSharesStandard, Target, UpdateChannel};

use crate::utils::Id;

use super::{group_channel::GroupChannel, standard_channel::StandardChannel, ShareValidationError, ShareValidationResult};
use std::{collections::HashMap, ops::Range, sync::mpsc};


/// Sample usage, currently it is blocking because we dont have any tokio in roles logic.
// fn main() {
//  let standard_factory = spawn_actor();
//  standard_factor.create_group_channel(...);
// }


pub struct StandardChannelFactoryHandle {
    sender: std::sync::mpsc::Sender<StandardChannelFactoryMessage>,
}


impl StandardChannelFactoryHandle {
    pub fn new(sender: std::sync::mpsc::Sender<StandardChannelFactoryMessage>) -> Self {
        Self { sender }
    }

    pub fn create_group_channel(&self, past_jobs_size: usize) -> Result<u32, StandardChannelFactoryError> {
        let (tx, rx) = std::sync::mpsc::channel();
        let msg = StandardChannelFactoryMessage::NewGroupChannel {
            past_jobs_size,
            respond_to: tx,
        };
        self.sender.send(msg).map_err(|_| StandardChannelFactoryError::PoisonError)?;
        rx.recv().map_err(|_| StandardChannelFactoryError::PoisonError)?
    }

    pub fn submit_shares_standard(&self, msg: SubmitSharesStandard) -> Result<ShareValidationResult, StandardChannelFactoryError> {
        let (tx, rx) = std::sync::mpsc::channel();
        let msg = StandardChannelFactoryMessage::SubmitSharesStandard {
            msg,
            respond_to: tx,
        };
        self.sender.send(msg).map_err(|_| StandardChannelFactoryError::PoisonError)?;
        rx.recv().map_err(|_| StandardChannelFactoryError::PoisonError)?
    }
}


/// This will be called by client
fn spawn_actor() -> StandardChannelFactoryHandle {
    let (tx, rx) = std::sync::mpsc::channel();

    let actor = StandardChannelFactoryActor::new(..., rx, ...).unwrap();

    std::thread::spawn(move || actor.run());

    StandardChannelFactoryHandle::new(tx)
}

pub enum StandardChannelFactoryMessage {
    NewGroupChannel {
        past_jobs_size: usize,
        respond_to: mpsc::Sender<Result<u32, StandardChannelFactoryError>>,
    },
    OnOpenStandardMiningChannel {
        m: OpenStandardMiningChannel<'static>,
        group_channel_id: u32,
        respond_to: mpsc::Sender<Result<u32, StandardChannelFactoryError>>,
    },
    UpdateChannel {
        msg: UpdateChannel<'static>,
        respond_to: mpsc::Sender<Result<Target, StandardChannelFactoryError>>,
    },
    SubmitSharesStandard {
        msg: SubmitSharesStandard,
        respond_to: mpsc::Sender<Result<ShareValidationResult, StandardChannelFactoryError>>,
    },
}


#[derive(Debug)]
pub enum StandardChannelFactoryError {
    PoisonError,
    GroupChannelNotFound,
    StandardChannelNotFound,
    ExtendedExtranonceError(ExtendedExtranonceError),
    HashRateToTargetError,
    RequestedMaxTargetTooLow,
    ShareRejected(ShareValidationError),
}

pub struct StandardChannelFactoryActor {
    receiver: mpsc::Receiver<StandardChannelFactoryMessage>,
    channel_id_factory: Id,
    group_channels: HashMap<u32, GroupChannel>,
    standard_channels: HashMap<u32, StandardChannel>,
    extended_extranonce: ExtendedExtranonce,
    share_batch_size: usize,
    expected_share_per_minute_per_channel: f64,
}

impl StandardChannelFactoryActor {
    pub fn run(mut self) {
        while let Ok(msg) = self.receiver.recv() {
            match msg {
                StandardChannelFactoryMessage::NewGroupChannel { past_jobs_size, respond_to } => {
                    let result = self.handle_new_group_channel(past_jobs_size);
                    let _ = respond_to.send(result);
                },
                StandardChannelFactoryMessage::OnOpenStandardMiningChannel { m, group_channel_id, respond_to } => {
                    let result = self.on_open_standard_mining_channel(m, group_channel_id);
                    let _ = respond_to.send(result);
                }
                StandardChannelFactoryMessage::SubmitSharesStandard { msg, respond_to } => {
                    let result = self.on_submit_shares_standard(msg);
                    let _ = respond_to.send(result);
                }
                _ => {}
            }
        }
    }
}


impl StandardChannelFactoryActor {
    pub fn new(
        extended_extranonce_range_0: Range<usize>,
        extended_extranonce_range_1: Range<usize>,
        extended_extranonce_range_2: Range<usize>,
        additional_coinbase_script_data: Option<Vec<u8>>,
        share_batch_size: usize,
        receiver: mpsc::Receiver<StandardChannelFactoryMessage>,
        expected_share_per_minute_per_channel: f64,
    ) -> Result<Self, StandardChannelFactoryError> {
        let extended_extranonce = ExtendedExtranonce::new(
            extended_extranonce_range_0,
            extended_extranonce_range_1,
            extended_extranonce_range_2,
            additional_coinbase_script_data.clone(),
        )
        .map_err(|e: ExtendedExtranonceError| StandardChannelFactoryError::ExtendedExtranonceError(e))?;

        Ok(Self {
            receiver,
            channel_id_factory: Id::new(),
            group_channels: HashMap::new(),
            standard_channels: HashMap::new(),
            extended_extranonce,
            share_batch_size,
            expected_share_per_minute_per_channel,
        })
    }

    pub fn handle_new_group_channel(
        &mut self,
        past_jobs_size: usize,
    ) -> Result<u32, StandardChannelFactoryError> {
        let group_channel_id = self
            .channel_id_factory
            .next();

        let group_channel = GroupChannel::new(group_channel_id, past_jobs_size);

        self.group_channels.insert(group_channel_id, group_channel);

        Ok(group_channel_id)
    }


    pub fn remove_channel(
        &mut self,
        channel_id: u32,
    ) -> Result<(), StandardChannelFactoryError> {
        todo!()
    }


    pub fn on_open_standard_mining_channel(
        &mut self,
        m: OpenStandardMiningChannel<'static>,
        group_channel_id: u32,
    ) -> Result<u32, StandardChannelFactoryError> {
        todo!()
    }


    pub fn on_submit_shares_standard(
        &mut self,
        m: SubmitSharesStandard,
    ) -> Result<ShareValidationResult, StandardChannelFactoryError> {
        todo!()
    }

}
