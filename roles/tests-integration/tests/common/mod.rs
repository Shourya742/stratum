use std::{path::PathBuf, sync::Arc};

use async_std::net::TcpStream;
use bitcoind::{
    bitcoincore_rpc::{
        bitcoin::key::rand::{distributions::Alphanumeric, thread_rng, Rng},
        RpcApi,
    },
    BitcoinD,
};
use jd_server::core::JDServer;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use translator_sv2::proxy_config::{TranslatorProxyDownstream, TranslatorProxyUpstream};

pub async fn start_job_declarator_server() {
    let authority_public_key = Secp256k1PublicKey::try_from(
        "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
    )
    .expect("failed");
    let authority_secret_key = Secp256k1SecretKey::try_from(
        "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
    )
    .expect("failed");
    let cert_validity_sec = 3600;
    let listen_jd_address = "0.0.0.0:34264".to_string();
    let core_rpc_url = "http://75.119.150.111".to_string();
    let core_rpc_port = 48332;
    let core_rpc_user = "username".to_string();
    let core_rpc_pass = "password".to_string();
    let coinbase_outputs = vec![jd_server::CoinbaseOutput::new(
        "P2WPKH".to_string(),
        "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075".to_string(),
    )];
    let config = jd_server::Configuration::new(
        listen_jd_address,
        authority_public_key,
        authority_secret_key,
        cert_validity_sec,
        coinbase_outputs,
        core_rpc_url,
        core_rpc_port,
        core_rpc_user,
        core_rpc_pass,
        std::time::Duration::from_secs(5),
    );
    tokio::task::spawn(async move {
        JDServer::new(config).start().await;
    });
}

pub async fn start_job_declarator_client(
) -> (jd_client::JobDeclaratorClient, jd_client::ProxyConfig) {
    let downstream_address = "127.0.0.1".to_string();
    let downstream_port = 34265;
    let max_supported_version = 2;
    let min_supported_version = 2;
    let min_extranonce2_size = 8;
    let withhold = false;
    let authority_pubkey = Secp256k1PublicKey::try_from(
        "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
    )
    .expect("failed");
    let authority_secret_key = Secp256k1SecretKey::try_from(
        "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
    )
    .expect("failed");
    let cert_validity_sec = 3600;
    let retry = 10;
    let tp_address = "127.0.0.1:8442".to_string();
    let pool_address = "127.0.0.1:34254".to_string();
    let jd_address = "127.0.0.1:34264".to_string();
    let pool_signature = "Stratum v2 SRI Pool".to_string();
    use jd_client::proxy_config::CoinbaseOutput;
    let coinbase_outputs: CoinbaseOutput = CoinbaseOutput::new(
        "P2WPKH".to_string(),
        "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075".to_string(),
    );
    let upstreams = vec![jd_client::proxy_config::Upstream {
        authority_pubkey,
        pool_address,
        jd_address,
        pool_signature,
    }];
    let config = jd_client::ProxyConfig::new(
        downstream_address,
        downstream_port,
        max_supported_version,
        min_supported_version,
        min_extranonce2_size,
        withhold,
        authority_pubkey,
        authority_secret_key,
        cert_validity_sec,
        tp_address,
        None,
        retry,
        upstreams,
        std::time::Duration::from_secs(300),
        vec![coinbase_outputs],
    );
    let task_collector = Arc::new(jd_client::Mutex::new(Vec::new()));
    let client = jd_client::JobDeclaratorClient::new(config.clone(), task_collector);
    (client, config)
}

pub async fn start_sv2_pool() {
    use pool_sv2::mining_pool::{CoinbaseOutput, Configuration};
    let authority_public_key = Secp256k1PublicKey::try_from(
        "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
    )
    .expect("failed");
    let authority_secret_key = Secp256k1SecretKey::try_from(
        "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
    )
    .expect("failed");
    let cert_validity_sec = 3600;
    let listen_address = "0.0.0.0:34254".to_string();
    let coinbase_outputs = vec![CoinbaseOutput::new(
        "P2WPKH".to_string(),
        "036adc3bdf21e6f9a0f0fb0066bf517e5b7909ed1563d6958a10993849a7554075".to_string(),
    )];
    let pool_signature = "Stratum v2 SRI Pool".to_string();
    let tp_address = "127.0.0.1:8442".to_string();
    let config = Configuration::new(
        listen_address,
        tp_address,
        None,
        authority_public_key,
        authority_secret_key,
        cert_validity_sec,
        coinbase_outputs,
        pool_signature,
    );
    tokio::spawn(async move {
        pool_sv2::PoolSv2::start(config).await;
    });
}

async fn connect_to_jd_client(address: String) -> Result<TcpStream, ()> {
    loop {
        match TcpStream::connect(address.clone()).await {
            Ok(stream) => return Ok(stream),
            Err(_e) => {
                continue;
            }
        }
    }
}

pub async fn start_sv2_translator(jd_client_address: String) -> translator_sv2::TranslatorSv2 {
    // let stream = connect_to_jd_client(jd_client_address).await;
    let upstream_address = "127.0.0.1".to_string();
    let upstream_port = 34265;
    let upstream_authority_pubkey = Secp256k1PublicKey::try_from(
        "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
    )
    .expect("failed");
    let downstream_address = "0.0.0.0".to_string();
    let downstream_port = 34255;
    let max_supported_version = 2;
    let min_supported_version = 2;
    let min_extranonce2_size = 8;
    let min_individual_miner_hashrate = 10_000_000_000_000.0;
    let shares_per_minute = 6.0;
    let channel_diff_update_interval = 60;
    let channel_nominal_hashrate = 10_000_000_000_000.0;
    let downstream_difficulty_config =
        translator_sv2::proxy_config::DownstreamDifficultyConfig::new(
            min_individual_miner_hashrate,
            shares_per_minute,
            0,
            0,
        );
    let upstream_difficulty_config = translator_sv2::proxy_config::UpstreamDifficultyConfig::new(
        channel_diff_update_interval,
        channel_nominal_hashrate,
        0,
        false,
    );
    let translator_proxy_upstream = TranslatorProxyUpstream::new(
        upstream_address,
        upstream_port,
        upstream_authority_pubkey,
        upstream_difficulty_config,
    );
    let translator_proxy_downstream = TranslatorProxyDownstream::new(
        downstream_address,
        downstream_port,
        downstream_difficulty_config,
    );
    let config = translator_sv2::proxy_config::ProxyConfig::new(
        translator_proxy_upstream,
        translator_proxy_downstream,
        max_supported_version,
        min_supported_version,
        min_extranonce2_size,
    );

    let translator_v2 = translator_sv2::TranslatorSv2::new(config);
    translator_v2
}

fn get_random_tmp_dir() -> PathBuf {
    let s: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();
    let path = "/tmp/.template-provider/".to_string() + &s;
    PathBuf::from(path)
}

pub struct TemplateProvider {
    bitcoind: BitcoinD,
}

impl TemplateProvider {
    pub fn start() -> Self {
        let temp_dir = get_random_tmp_dir();
        let mut conf = bitcoind::Conf::default();
        conf.args.push("-txindex=1");
        conf.args.push("-sv2");
        conf.args.push("-sv2port=8442");
        conf.args.push("-debug=sv2");
        conf.args.push("-sv2interval=20 -");
        conf.args.push("-sv2feedelta=1000");
        conf.args.push("-loglevel=sv2:trace");
        conf.staticdir = Some(temp_dir.join(".bitcoin"));

        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;
        let key = "BITCOIND_EXE";
        let curr_dir_path = std::env::current_dir().unwrap();
        let bitcoind_path = match (os, arch) {
            ("macos", "aarch64") => curr_dir_path.join("bin").join("bitcoind_mac"),
            ("macos", "x86_64") => curr_dir_path.join("bin").join("bitcoind_mac_x86"),
            _ => curr_dir_path.join("bin").join("bitcoind"),
        };
        std::env::set_var(key, bitcoind_path);
        let exe_path = bitcoind::exe_path().unwrap();

        let bitcoind = BitcoinD::with_conf(exe_path, &conf).unwrap();
        TemplateProvider { bitcoind }
    }

    pub fn stop(&self) {
        let _ = self.bitcoind.client.stop().unwrap();
    }

    pub fn generate_blocks(&self, n: u64) {
        let mining_address = self
            .bitcoind
            .client
            .get_new_address(None, None)
            .unwrap()
            .require_network(bitcoind::bitcoincore_rpc::bitcoin::Network::Regtest)
            .unwrap();
        self.bitcoind
            .client
            .generate_to_address(n, &mining_address)
            .unwrap();
    }

    pub fn get_block_count(&self) -> u64 {
        self.bitcoind.client.get_block_count().unwrap()
    }
}
