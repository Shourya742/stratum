use std::{net::SocketAddr, str::FromStr};

mod common;

#[tokio::test]
async fn test_jd_client_translator_sv2_pool_sv2_integration() {
    let template_provider = common::TemplateProvider::start();
    template_provider.generate_blocks(16);
    common::start_sv2_pool().await;
    common::start_job_declarator_server().await;
    let (jd_client, jd_client_config) = common::start_job_declarator_client().await;
    let upstream = jd_client_config.upstreams.clone();
    let upstream = upstream.get(0).cloned().unwrap();
    let address = format!(
        "{}:{}",
        jd_client_config.downstream_address, jd_client_config.downstream_port
    );

    let pool_addr =
        SocketAddr::from_str(upstream.pool_address.as_str()).expect("Invalid pool address");

    let pool_socket = {
        loop {
            match tokio::net::TcpStream::connect(pool_addr).await {
                Ok(s) => break s,
                Err(_e) => {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    };
    jd_client
        .clone()
        .initialize_jd(upstream.clone(), pool_socket)
        .await;
    let tproxy = common::start_sv2_translator(address.clone()).await;
    tokio::task::spawn(async move {
        tproxy.clone().start().await;
        let upstream_address = tproxy.upstream_address();
        dbg!("Here");
        assert_eq!(jd_client.downstream_address(), upstream_address);
    });
    template_provider.stop();
}
