use std::convert::TryInto;

use const_sv2::{MESSAGE_TYPE_SUBMIT_SHARES_ERROR, MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS};
use integration_tests_sv2::*;
use sniffer::InterceptMessage;

use crate::sniffer::MessageDirection;
use roles_logic_sv2::{
    mining_sv2::SubmitSharesError,
    parsers::{CommonMessages, Mining, PoolMessages},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
// #[tokio::test]
async fn test_jdc_pool_fallback_after_submit_rejection() {
    console_subscriber::ConsoleLayer::builder()
        .server_addr(([127, 0, 0, 1], 6669))
        .init();
    let message = PoolMessages::Mining(Mining::SubmitSharesError(SubmitSharesError {
        channel_id: 0,
        sequence_number: 0,
        error_code: "invalid-nonce".to_string().into_bytes().try_into().unwrap(),
    }));

    let (_, tp_addr) = start_template_provider(None).await;
    let (_, pool_addr) = start_pool(Some(tp_addr)).await;
    let (_, sniffer_addr) = start_sniffer(
        "0".to_string(),
        pool_addr,
        false,
        Some(vec![InterceptMessage::new(
            MessageDirection::ToDownstream,
            MESSAGE_TYPE_SUBMIT_SHARES_SUCCESS,
            message,
            MESSAGE_TYPE_SUBMIT_SHARES_ERROR,
            false,
        )]),
    )
    .await;

    let (_, pool_addr_2) = start_pool(Some(tp_addr)).await;
    let (sniffer_2, sniffer_addr_2) =
        start_sniffer("1".to_string(), pool_addr_2, false, None).await;
    let (_, jds_addr) = start_jds(tp_addr).await;
    let (_, jdc_addr) = start_jdc(vec![sniffer_addr, sniffer_addr_2], tp_addr, jds_addr).await;
    let (_, sv2_translator_addr) = start_sv2_translator(jdc_addr).await;
    let _ = start_mining_device_sv1(sv2_translator_addr).await;
    dbg!("before");
    // tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    dbg!("after");
    assert_common_message!(&sniffer_2.next_message_from_downstream(), SetupConnection);
    dbg!("after first");
    assert_common_message!(
        &sniffer_2.next_message_from_upstream(),
        SetupConnectionSuccess
    );
}
