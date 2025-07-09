use crate::{errors::Error, parsers::Mining};
use codec_sv2::binary_sv2;
use mining_sv2::{
    CloseChannel, NewExtendedMiningJob, NewMiningJob, OpenExtendedMiningChannel,
    OpenExtendedMiningChannelSuccess, OpenMiningChannelError, OpenStandardMiningChannel,
    OpenStandardMiningChannelSuccess, SetCustomMiningJob, SetCustomMiningJobError,
    SetCustomMiningJobSuccess, SetExtranoncePrefix, SetGroupChannel, SetNewPrevHash, SetTarget,
    SubmitSharesError, SubmitSharesExtended, SubmitSharesStandard, SubmitSharesSuccess,
    UpdateChannel, UpdateChannelError,
};

use mining_sv2::*;
use std::fmt::Debug as D;
use template_distribution_sv2::MESSAGE_TYPE_SET_NEW_PREV_HASH;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SupportedChannelTypes {
    Standard,
    Extended,
    Group,
    GroupAndExtended,
}

#[derive(PartialEq, Eq)]
pub enum DownstreamAuth {
    NotImplemented,
    Allow,
    Restrict,
}

pub trait MiningChannelConfig {
    fn get_channel_type(&self) -> SupportedChannelTypes {
        SupportedChannelTypes::Extended
    }
    fn is_work_selection_enabled(&self) -> bool {
        false
    }

    fn is_downstream_authorized(
        &self,
        user_identity: &binary_sv2::Str0255,
    ) -> Result<DownstreamAuth, Error> {
        Ok(DownstreamAuth::NotImplemented)
    }
}

pub trait ParseMiningMessagesFromDownstream<Up: D>
where
    Self: Sized + D + MiningChannelConfig,
{
    fn handle_open_standard_mining_channel(
        &mut self,
        msg: OpenStandardMiningChannel,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
        ))
    }

    fn handle_open_extended_mining_channel(
        &mut self,
        msg: OpenExtendedMiningChannel,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
        ))
    }

    fn handle_update_channel(
        &mut self,
        msg: UpdateChannel,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_UPDATE_CHANNEL))
    }

    fn handle_submit_shares_standard(
        &mut self,
        msg: SubmitSharesStandard,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
        ))
    }

    fn handle_submit_shares_extended(
        &mut self,
        msg: SubmitSharesExtended,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
        ))
    }

    fn handle_set_custom_mining_job(
        &mut self,
        msg: SetCustomMiningJob,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_CUSTOM_MINING_JOB))
    }
}

pub trait ParseMiningMessagesFromUpstream<Down: D>
where
    Self: Sized + D + MiningChannelConfig,
{
    fn handle_open_standard_mining_channel_success(
        &mut self,
        msg: OpenStandardMiningChannelSuccess,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
        ))
    }

    fn handle_open_extended_mining_channel_success(
        &mut self,
        msg: OpenExtendedMiningChannelSuccess,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
        ))
    }

    fn handle_open_mining_channel_error(
        &mut self,
        msg: OpenMiningChannelError,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_OPEN_MINING_CHANNEL_ERROR,
        ))
    }

    fn handle_update_channel_error(
        &mut self,
        msg: UpdateChannelError,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_UPDATE_CHANNEL_ERROR))
    }

    fn handle_close_channel(
        &mut self,
        msg: CloseChannel,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_CLOSE_CHANNEL))
    }

    fn handle_set_extranonce_prefix(
        &mut self,
        msg: SetExtranoncePrefix,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_EXTRANONCE_PREFIX))
    }

    fn handle_submit_shares_success(
        &mut self,
        msg: SubmitSharesSuccess,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS))
    }

    fn handle_submit_shares_error(
        &mut self,
        msg: SubmitSharesError,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_SUBMIT_SHARES_ERROR))
    }

    fn handle_new_mining_job(
        &mut self,
        msg: NewMiningJob,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_NEW_MINING_JOB))
    }

    fn handle_new_extended_mining_job(
        &mut self,
        msg: NewExtendedMiningJob,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
        ))
    }

    fn handle_set_new_prev_hash_mining(
        &mut self,
        msg: SetNewPrevHash,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_NEW_PREV_HASH))
    }

    fn handle_set_custom_mining_job_success(
        &mut self,
        msg: SetCustomMiningJobSuccess,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
        ))
    }

    fn handle_set_custom_mining_job_error(
        &mut self,
        msg: SetCustomMiningJobError,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
        ))
    }

    fn handle_set_target(&mut self, msg: SetTarget) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_TARGET))
    }

    fn handle_set_group_channel(
        &mut self,
        msg: SetGroupChannel,
    ) -> Result<Option<Mining<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_GROUP_CHANNEL))
    }
}
