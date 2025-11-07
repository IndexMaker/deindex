use chrono::Utc;
use clap::Parser;
use deli::{amount::Amount, labels::Labels, log_msg, vector::Vector};
use ethers::{
    middleware::SignerMiddleware,
    prelude::abigen,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{Address, Filter, U256},
};
use eyre::Context;
use std::sync::Arc;
use std::{env, str::FromStr};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    rpc_url: Option<String>,

    #[arg(long)]
    disolver_address: String,

    #[arg(long)]
    dior_address: String,

    #[arg(long)]
    dimer_address: String,
}

fn get_private_key() -> String {
    env::var("AP_PRIVATE_KEY").expect("AP_PRIVATE_KEY not found in environment")
}

fn gen_unique_id() -> i64 {
    let now = Utc::now();
    let value = now.timestamp_micros();
    value
}

pub const CT_STRATEGY: u8 = 1;
pub const CT_FILL: u8 = 2;

pub const VT_PRICES: u8 = 1;
pub const VT_LIQUID: u8 = 2;
pub const VT_MATRIX: u8 = 3;
pub const VT_COLLAT: u8 = 4;
pub const VT_IAQTYS: u8 = 5;
pub const VT_IAVALS: u8 = 6;
pub const VT_IFILLS: u8 = 7;
pub const VT_ASSETS: u8 = 8;
pub const VT_COEFFS: u8 = 9;
pub const VT_NETAVS: u8 = 10;
pub const VT_QUOTES: u8 = 11;
pub const VT_AXPXES: u8 = 12;
pub const VT_AXFEES: u8 = 13;
pub const VT_AXQTYS: u8 = 14;
pub const VT_IXQTYS: u8 = 15;
pub const VT_AAFEES: u8 = 16;
pub const VT_AAQTYS: u8 = 17;
pub const VT_CCOVRS: u8 = 18;
pub const VT_ACOVRS: u8 = 19;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let cli = Cli::parse();

    let rpc_url = cli.rpc_url.unwrap_or("http://localhost:8547".to_owned());

    let provider = Provider::<Http>::try_from(rpc_url)?;
    let disolver_address: Address = cli.disolver_address.parse()?;
    let dior_address: Address = cli.dior_address.parse()?;
    let dimer_address: Address = cli.dimer_address.parse()?;

    let priv_key = get_private_key();
    let wallet = LocalWallet::from_str(&priv_key)?;
    let chain_id = provider.get_chainid().await?.as_u64();
    let client = Arc::new(SignerMiddleware::new(
        provider,
        wallet.clone().with_chain_id(chain_id),
    ));

    log_msg!("Done.");
    Ok(())
}
