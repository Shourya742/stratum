#![allow(special_module_name)]
mod args;
mod lib;

use lib::{
    error::{Error, ProxyResult},
    proxy_config::ProxyConfig,
    status, JobDeclaratorClient,
};

use async_channel::unbounded;
use futures::{select, FutureExt};
use roles_logic_sv2::utils::Mutex;
use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};

use args::Args;
use ext_config::{Config, File, FileFormat};
use tracing::{error, info};

/// Process CLI args and load configuration.
#[allow(clippy::result_large_err)]
fn process_cli_args<'a>() -> ProxyResult<'a, ProxyConfig> {
    // Parse CLI arguments
    let args = Args::from_args().map_err(|help| {
        error!("{}", help);
        Error::BadCliArgs
    })?;

    // Build configuration from the provided file path
    let config_path = args.config_path.to_str().ok_or_else(|| {
        error!("Invalid configuration path.");
        Error::BadCliArgs
    })?;

    let settings = Config::builder()
        .add_source(File::new(config_path, FileFormat::Toml))
        .build()?;

    // Deserialize settings into ProxyConfig
    let config = settings.try_deserialize::<ProxyConfig>()?;
    Ok(config)
}

/// TODO on the setup phase JDC must send a random nonce to bitcoind and JDS used for the tx
/// hashlist
///
/// TODO on setupconnection with bitcoind (TP) JDC must signal that want a tx short hash list with
/// the templates
///
/// TODO JDC must handle TxShortHashList message
///
/// This will start:
/// 1. An Upstream, this will connect with the mining Pool
/// 2. A listner that will wait for a mining downstream with ExtendedChannel capabilities (tproxy,
///    mining-proxy)
/// 3. A JobDeclarator, this will connect with the job-declarator-server
/// 4. A TemplateRx, this will connect with bitcoind
///
/// Setup phase
/// 1. Upstream: ->SetupConnection, <-SetupConnectionSuccess
/// 2. Downstream: <-SetupConnection, ->SetupConnectionSuccess, <-OpenExtendedMiningChannel
/// 3. Upstream: ->OpenExtendedMiningChannel, <-OpenExtendedMiningChannelSuccess
/// 4. Downstream: ->OpenExtendedMiningChannelSuccess
///
/// Setup phase
/// 1. JobDeclarator: ->SetupConnection, <-SetupConnectionSuccess, ->AllocateMiningJobToken(x2),
///    <-AllocateMiningJobTokenSuccess (x2)
/// 2. TemplateRx: ->CoinbaseOutputDataSize
///
/// Main loop:
/// 1. TemplateRx: <-NewTemplate, SetNewPrevHash
/// 2. JobDeclarator: -> CommitMiningJob (JobDeclarator::on_new_template), <-CommitMiningJobSuccess
/// 3. Upstream: ->SetCustomMiningJob, Downstream: ->NewExtendedMiningJob, ->SetNewPrevHash
/// 4. Downstream: <-Share
/// 5. Upstream: ->Share
///
/// When we have a NewTemplate we send the NewExtendedMiningJob downstream and the CommitMiningJob
/// to the JDS altoghether.
/// Then we receive CommitMiningJobSuccess and we use the new token to send SetCustomMiningJob to
/// the pool.
/// When we receive SetCustomMiningJobSuccess we set in Upstream job_id equal to the one received
/// in SetCustomMiningJobSuccess so that we still send shares upstream with the right job_id.
///
/// The above procedure, let us send NewExtendedMiningJob downstream right after a NewTemplate has
/// been received this will reduce the time that pass from a NewTemplate and the mining-device
/// starting to mine on the new job.
///
/// In the case a future NewTemplate the SetCustomMiningJob is sent only if the canditate become
/// the actual NewTemplate so that we do not send a lot of useless future Job to the pool. That
/// means that SetCustomMiningJob is sent only when a NewTemplate become "active"
///
/// The JobDeclarator always have 2 avaiable token, that means that whenever a token is used to
/// commit a job with upstream we require a new one. Having always a token when needed means that
/// whenever we want to commit a mining job we can do that without waiting for upstream to provide
/// a new token.
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let proxy_config = match process_cli_args() {
        Ok(p) => p,
        Err(e) => {
            error!("Job Declarator Client Config error: {}", e);
            return;
        }
    };
    let task_collector = Arc::new(Mutex::new(vec![]));
    let mut upstream_index = 0;
    let mut interrupt_signal_future = Box::pin(tokio::signal::ctrl_c().fuse());
    loop {
        let jdc = JobDeclaratorClient::new(proxy_config.clone(), task_collector.clone());
        let rx_status;
        if let Some(upstream) = proxy_config.upstreams.get(upstream_index) {
            let upstream_addr =
                SocketAddr::from_str(upstream.pool_address.as_str()).expect("Invalid pool address");
            let upstream_socket = {
                loop {
                    match tokio::net::TcpStream::connect(upstream_addr).await {
                        Ok(s) => break s,
                        Err(e) => {
                            error!(
                                "Failed to connect to upstream, retrying in 3 seconds: {}",
                                e
                            );
                            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        }
                    }
                }
            };
            rx_status = jdc.initialize_jd(upstream.clone(), upstream_socket).await;
        } else {
            rx_status = jdc.initialize_jd_as_solo_miner().await;
        }
        // Check all tasks if is_finished() is true, if so exit
        loop {
            let task_status = select! {
              task_status = rx_status.recv().fuse() => task_status,
              interrupt_signal = interrupt_signal_future => {
                match interrupt_signal {
                  Ok(()) => {
                    info!("Interrupt received");
                  },
                  Err(err) => {
                    error!("Unable to listen for interrupt signal: {}", err);
                    // we also shut down in case of error
                  },
                }
                std::process::exit(0);
              }
            };
            let task_status: status::Status = task_status.unwrap();

            match task_status.state {
                // Should only be sent by the downstream listener
                status::State::DownstreamShutdown(err) => {
                    error!("SHUTDOWN from: {}", err);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    task_collector
                        .safe_lock(|s| {
                            for handle in s {
                                handle.abort();
                            }
                        })
                        .unwrap();
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    break;
                }
                status::State::UpstreamShutdown(err) => {
                    error!("SHUTDOWN from: {}", err);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    task_collector
                        .safe_lock(|s| {
                            for handle in s {
                                handle.abort();
                            }
                        })
                        .unwrap();
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    break;
                }
                status::State::UpstreamRogue => {
                    error!("Changin Pool");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    task_collector
                        .safe_lock(|s| {
                            for handle in s {
                                handle.abort();
                            }
                        })
                        .unwrap();
                    upstream_index += 1;
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    break;
                }
                status::State::Healthy(msg) => {
                    info!("HEALTHY message: {}", msg);
                }
            }
        }
    }
}
