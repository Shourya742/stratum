use crate::{errors::Error, parsers::TemplateDistribution};
use template_distribution_sv2::{
    CoinbaseOutputConstraints, NewTemplate, RequestTransactionData, RequestTransactionDataError,
    RequestTransactionDataSuccess, SetNewPrevHash, SubmitSolution,
};

use core::convert::TryInto;
use template_distribution_sv2::*;

pub trait ParseTemplateDistributionMessagesFromServer {
    fn handle_new_template(
        &mut self,
        msg: NewTemplate,
    ) -> Result<Option<TemplateDistribution<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_NEW_TEMPLATE))
    }

    fn handle_set_new_prev_hash_template_distribution(
        &mut self,
        msg: SetNewPrevHash,
    ) -> Result<Option<TemplateDistribution<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_SET_NEW_PREV_HASH))
    }

    fn handle_request_tx_data_success(
        &mut self,
        msg: RequestTransactionDataSuccess,
    ) -> Result<Option<TemplateDistribution<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_SUCCESS,
        ))
    }

    fn handle_request_tx_data_error(
        &mut self,
        msg: RequestTransactionDataError,
    ) -> Result<Option<TemplateDistribution<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_REQUEST_TRANSACTION_DATA_ERROR,
        ))
    }
}

pub trait ParseTemplateDistributionMessagesFromClient {
    fn handle_coinbase_out_data_size(
        &mut self,
        msg: CoinbaseOutputConstraints,
    ) -> Result<Option<TemplateDistribution<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS,
        ))
    }

    fn handle_request_tx_data(
        &mut self,
        msg: RequestTransactionData,
    ) -> Result<Option<TemplateDistribution<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(
            MESSAGE_TYPE_REQUEST_TRANSACTION_DATA,
        ))
    }

    fn handle_request_submit_solution(
        &mut self,
        msg: SubmitSolution,
    ) -> Result<Option<TemplateDistribution<'static>>, Error> {
        let _ = msg;
        Err(Error::UnexpectedMessage(MESSAGE_TYPE_SUBMIT_SOLUTION))
    }
}
