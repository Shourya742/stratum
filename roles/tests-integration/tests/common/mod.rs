use std::{
    env,
    fs::{create_dir_all, File},
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use async_std::net::TcpStream;
use bitcoind::{bitcoincore_rpc::RpcApi, BitcoinD, Conf};
use flate2::read::GzDecoder;
use jd_server::core::JDServer;
use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use tar::Archive;
use translator_sv2::proxy_config::{TranslatorProxyDownstream, TranslatorProxyUpstream};

const VERSION_TP: &str = "0.1.7";

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

fn download_bitcoind_tarball(download_url: &str) -> Vec<u8> {
    let response = minreq::get(download_url)
        .send()
        .expect(&format!("Cannot reach URL: {}", download_url));
    assert_eq!(
        response.status_code, 200,
        "URL {} didn't return 200",
        download_url
    );
    response.as_bytes().to_vec()
}

fn read_tarball_from_file(path: &str) -> Vec<u8> {
    let file = File::open(path).expect(&format!(
        "Cannot find {:?} specified with env var BITCOIND_TARBALL_FILE",
        path
    ));
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer).unwrap();
    buffer
}

fn unpack_tarball(tarball_bytes: &[u8], destination: &Path) {
    let decoder = GzDecoder::new(tarball_bytes);
    let mut archive = Archive::new(decoder);
    for mut entry in archive.entries().unwrap().flatten() {
        if let Ok(file) = entry.path() {
            if file.ends_with("bitcoind") {
                entry.unpack_in(destination).unwrap();
            }
        }
    }
}

fn get_bitcoind_filename(os: &str, arch: &str) -> String {
    match (os, arch) {
        ("macos", "aarch64") => format!("bitcoin-sv2-tp-{}-arm64-apple-darwin.tar.gz", VERSION_TP),
        ("macos", "x86_64") => format!("bitcoin-sv2-tp-{}-x86_64-apple-darwin.tar.gz", VERSION_TP),
        ("linux", "x86_64") => format!("bitcoin-sv2-tp-{}-x86_64-linux-gnu.tar.gz", VERSION_TP),
        ("linux", "aarch64") => format!("bitcoin-sv2-tp-{}-aarch64-linux-gnu.tar.gz", VERSION_TP),
        _ => format!(
            "bitcoin-sv2-tp-{}-x86_64-apple-darwin-unsigned.zip",
            VERSION_TP
        ),
    }
}

pub struct TemplateProvider {
    bitcoind: BitcoinD,
}

impl TemplateProvider {
    pub fn start() -> Self {
        let temp_dir = PathBuf::from("/tmp/.template-provider/");

        let mut conf = Conf::default();
        conf.args.extend(vec![
            "-txindex=1",
            "-sv2",
            "-sv2port=8442",
            "-debug=sv2",
            "-sv2interval=20",
            "-sv2feedelta=1000",
            "-loglevel=sv2:trace",
        ]);
        conf.staticdir = Some(temp_dir.join(".bitcoin"));

        let os = env::consts::OS;
        let arch = env::consts::ARCH;
        let download_filename = get_bitcoind_filename(os, arch);
        let bitcoin_exe_home = temp_dir
            .join(format!("bitcoin-sv2-tp-{}", VERSION_TP))
            .join("bin");

        if !bitcoin_exe_home.exists() {
            let tarball_bytes = match env::var("BITCOIND_TARBALL_FILE") {
                Ok(path) => read_tarball_from_file(&path),
                Err(_) => {
                    let download_endpoint =
                        env::var("BITCOIND_DOWNLOAD_ENDPOINT").unwrap_or_else(|_| {
                            "https://github.com/Sjors/bitcoin/releases/download".to_owned()
                        });
                    let url = format!(
                        "{}/sv2-tp-{}/{}",
                        download_endpoint, VERSION_TP, download_filename
                    );
                    download_bitcoind_tarball(&url)
                }
            };

            if let Some(parent) = bitcoin_exe_home.parent() {
                create_dir_all(parent).unwrap();
            }

            unpack_tarball(&tarball_bytes, &temp_dir);

            if os == "macos" {
                let bitcoind_binary = bitcoin_exe_home.join("bitcoind");
                std::process::Command::new("codesign")
                    .arg("--sign")
                    .arg("-")
                    .arg(&bitcoind_binary)
                    .output()
                    .expect("Failed to sign bitcoind binary");
            }
        }

        env::set_var("BITCOIND_EXE", bitcoin_exe_home.join("bitcoind"));
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
