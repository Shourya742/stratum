#![allow(warnings)]
use clap::Parser;
use template_provider_sv2::start_template_provider;
use tracing::{debug, info};

/// Handles SV2 communication with configurable network and timing parameters
#[derive(Parser, Debug)]
#[command(name = "sv2", version, about, long_about = None)]
pub struct Sv2CLI {
    /// The IP address to bind for incoming SV2 connections
    #[arg(long, default_value = "127.0.0.1")]
    sv2bind: String,

    /// The port number for incoming SV2 connections
    #[arg(long, default_value_t = 8442)]
    sv2port: u16,

    /// Time interval (in seconds) between SV2 messages
    #[arg(long, default_value_t = 10)]
    sv2interval: u64,

    /// Fee delta to apply to SV2 jobs (in sats)
    #[arg(long, default_value_t = 0)]
    sv2feedelta: u64,

    #[arg(long, default_value = "/home/shourya/.bitcoin/testnet4/node.sock")]
    unix_socket_path: String,
}

impl From<Sv2CLI> for template_provider_sv2::Config {
    fn from(value: Sv2CLI) -> Self {
        template_provider_sv2::Config::new(
            value.sv2bind,
            value.sv2port,
            value.sv2interval,
            value.sv2feedelta,
            value.unix_socket_path,
        )
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    debug!("Parsing the CLI");
    let config = Sv2CLI::parse();
    info!("Starting the template provider instance");
    start_template_provider(config.into()).await;
}
