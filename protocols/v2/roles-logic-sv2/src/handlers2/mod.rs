#![allow(warnings)]

use std::convert::TryInto;

use mining_sv2::{
    MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB, MESSAGE_TYPE_NEW_MINING_JOB,
    MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL, MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
    MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL, MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB, MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS, MESSAGE_TYPE_SET_GROUP_CHANNEL,
    MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED, MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
};

use crate::{
    handlers2::{
        common::{ParseCommonMessagesFromDownstream, ParseCommonMessagesFromUpstream},
        job_declaration::{
            ParseJobDeclarationMessagesFromDownstream, ParseJobDeclarationMessagesFromUpstream,
        },
        mining::{
            DownstreamAuth, ParseMiningMessagesFromDownstream, ParseMiningMessagesFromUpstream,
            SupportedChannelTypes,
        },
        template_distribution::{
            ParseTemplateDistributionMessagesFromClient,
            ParseTemplateDistributionMessagesFromServer,
        },
    },
    parsers::{AnyMessage, CommonMessages, JobDeclaration, Mining, TemplateDistribution},
    Error,
};
mod common;
mod job_declaration;
mod mining;
mod template_distribution;
pub trait Sv2Router<D: std::fmt::Debug>:
    ParseCommonMessagesFromUpstream
    + ParseCommonMessagesFromDownstream
    + ParseMiningMessagesFromUpstream<D>
    + ParseMiningMessagesFromDownstream<D>
    + ParseJobDeclarationMessagesFromUpstream
    + ParseJobDeclarationMessagesFromDownstream
    + ParseTemplateDistributionMessagesFromServer
    + ParseTemplateDistributionMessagesFromClient
{
    fn handle_message(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<Option<AnyMessage<'static>>, Error> {
        match message_type {
            0x00..=0x05 => self.handle_common(message_type, payload),
            0x10..=0x25 => self.handle_mining(message_type, payload),
            0x50..=0x60 => self.handle_job_declaration(message_type, payload),
            0x70..=0x76 => self.handle_template_distribution(message_type, payload),
            _ => Ok(None),
        }
    }

    fn handle_common(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<Option<AnyMessage<'static>>, Error> {
        let msg: CommonMessages = (message_type, payload).try_into()?;
        let res = match msg {
            CommonMessages::SetupConnectionSuccess(m) => self.handle_setup_connection_success(m),
            CommonMessages::SetupConnectionError(m) => self.handle_setup_connection_error(m),
            CommonMessages::ChannelEndpointChanged(m) => self.handle_channel_endpoint_changed(m),
            CommonMessages::Reconnect(m) => self.handle_reconnect(m),
            CommonMessages::SetupConnection(m) => self.handle_setup_connection(m),
        };
        res.map(|opt| opt.map(AnyMessage::Common))
    }

    fn handle_mining(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<Option<AnyMessage<'static>>, Error> {
        let msg: Mining = (message_type, payload).try_into()?;
        let (chan, work) = (self.get_channel_type(), self.is_work_selection_enabled());

        use SupportedChannelTypes::*;
        let res = match msg {
            Mining::OpenStandardMiningChannel(m) => {
                if self.is_downstream_authorized(&m.user_identity)? == DownstreamAuth::Restrict {
                    return Ok(Some(AnyMessage::Mining(Mining::OpenMiningChannelError(
                        mining_sv2::OpenMiningChannelError::new_unknown_user(
                            m.get_request_id_as_u32(),
                        ),
                    ))));
                }
                match chan {
                    Standard | Group | GroupAndExtended => {
                        self.handle_open_standard_mining_channel(m)
                    }
                    Extended => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
                    )),
                }
            }

            Mining::OpenExtendedMiningChannel(m) => {
                if self.is_downstream_authorized(&m.user_identity)? == DownstreamAuth::Restrict {
                    return Ok(Some(AnyMessage::Mining(Mining::OpenMiningChannelError(
                        mining_sv2::OpenMiningChannelError::new_unknown_user(
                            m.get_request_id_as_u32(),
                        ),
                    ))));
                }
                match chan {
                    Extended | GroupAndExtended => self.handle_open_extended_mining_channel(m),
                    _ => Err(Error::UnexpectedMessage(
                        MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
                    )),
                }
            }

            Mining::UpdateChannel(m) => self.handle_update_channel(m),

            Mining::SubmitSharesStandard(m) => match chan {
                Standard | Group | GroupAndExtended => self.handle_submit_shares_standard(m),
                Extended => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_SUBMIT_SHARES_STANDARD,
                )),
            },

            Mining::SubmitSharesExtended(m) => match chan {
                Extended | GroupAndExtended => self.handle_submit_shares_extended(m),
                _ => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_SUBMIT_SHARES_EXTENDED,
                )),
            },

            Mining::SetCustomMiningJob(m) => match (chan, work) {
                (Extended, true) | (GroupAndExtended, true) => self.handle_set_custom_mining_job(m),
                _ => Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_CUSTOM_MINING_JOB)),
            },

            Mining::OpenStandardMiningChannelSuccess(m) => match chan {
                Standard | Group | GroupAndExtended => {
                    self.handle_open_standard_mining_channel_success(m)
                }
                _ => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
                )),
            },

            Mining::OpenExtendedMiningChannelSuccess(m) => match chan {
                Extended | GroupAndExtended => self.handle_open_extended_mining_channel_success(m),
                _ => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL_SUCCESS,
                )),
            },

            Mining::OpenMiningChannelError(m) => self.handle_open_mining_channel_error(m),
            Mining::UpdateChannelError(m) => self.handle_update_channel_error(m),
            Mining::CloseChannel(m) => self.handle_close_channel(m),
            Mining::SetExtranoncePrefix(m) => self.handle_set_extranonce_prefix(m),
            Mining::SubmitSharesSuccess(m) => self.handle_submit_shares_success(m),
            Mining::SubmitSharesError(m) => self.handle_submit_shares_error(m),

            Mining::NewMiningJob(m) => match chan {
                Standard => self.handle_new_mining_job(m),
                _ => Err(Error::UnexpectedMessage(MESSAGE_TYPE_NEW_MINING_JOB)),
            },

            Mining::NewExtendedMiningJob(m) => match chan {
                Extended | Group | GroupAndExtended => self.handle_new_extended_mining_job(m),
                _ => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_NEW_EXTENDED_MINING_JOB,
                )),
            },

            Mining::SetNewPrevHash(m) => self.handle_set_new_prev_hash_mining(m),

            Mining::SetCustomMiningJobSuccess(m) => match (chan, work) {
                (Extended, true) | (GroupAndExtended, true) => {
                    self.handle_set_custom_mining_job_success(m)
                }
                _ => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_SUCCESS,
                )),
            },

            Mining::SetCustomMiningJobError(m) => match (chan, work) {
                (Extended, true) | (Group, true) | (GroupAndExtended, true) => {
                    self.handle_set_custom_mining_job_error(m)
                }
                _ => Err(Error::UnexpectedMessage(
                    MESSAGE_TYPE_SET_CUSTOM_MINING_JOB_ERROR,
                )),
            },

            Mining::SetTarget(m) => self.handle_set_target(m),

            Mining::SetGroupChannel(m) => match chan {
                Group | GroupAndExtended => self.handle_set_group_channel(m),
                _ => Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_GROUP_CHANNEL)),
            },
        };

        res.map(|opt| opt.map(AnyMessage::Mining))
    }

    fn handle_job_declaration(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<Option<AnyMessage<'static>>, Error> {
        let msg: JobDeclaration = (message_type, payload).try_into()?;
        let res = match msg {
            JobDeclaration::AllocateMiningJobToken(m) => self.handle_allocate_mining_job_token(m),
            JobDeclaration::DeclareMiningJob(m) => self.handle_declare_mining_job(m),
            JobDeclaration::ProvideMissingTransactionsSuccess(m) => {
                self.handle_provide_missing_transactions_success(m)
            }
            JobDeclaration::PushSolution(m) => self.handle_push_solution(m),
            JobDeclaration::AllocateMiningJobTokenSuccess(m) => {
                self.handle_allocate_mining_job_token_success(m)
            }
            JobDeclaration::DeclareMiningJobSuccess(m) => self.handle_declare_mining_job_success(m),
            JobDeclaration::DeclareMiningJobError(m) => self.handle_declare_mining_job_error(m),
            JobDeclaration::ProvideMissingTransactions(m) => {
                self.handle_provide_missing_transactions(m)
            }
        };

        res.map(|opt| opt.map(AnyMessage::JobDeclaration))
    }

    fn handle_template_distribution(
        &mut self,
        message_type: u8,
        payload: &mut [u8],
    ) -> Result<Option<AnyMessage<'static>>, Error> {
        let msg: TemplateDistribution = (message_type, payload).try_into()?;
        let res = match msg {
            TemplateDistribution::CoinbaseOutputConstraints(m) => {
                self.handle_coinbase_out_data_size(m)
            }
            TemplateDistribution::RequestTransactionData(m) => self.handle_request_tx_data(m),
            TemplateDistribution::SubmitSolution(m) => self.handle_request_submit_solution(m),
            TemplateDistribution::NewTemplate(m) => self.handle_new_template(m),
            TemplateDistribution::SetNewPrevHash(m) => {
                self.handle_set_new_prev_hash_template_distribution(m)
            }
            TemplateDistribution::RequestTransactionDataSuccess(m) => {
                self.handle_request_tx_data_success(m)
            }
            TemplateDistribution::RequestTransactionDataError(m) => {
                self.handle_request_tx_data_error(m)
            }
        };

        res.map(|opt| opt.map(AnyMessage::TemplateDistribution))
    }
}
