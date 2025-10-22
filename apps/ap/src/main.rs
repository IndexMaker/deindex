use chrono::Utc;
use clap::Parser;
use deli::{amount::Amount, log_msg};
use ethers::{
    middleware::SignerMiddleware,
    prelude::abigen,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{Address, U256},
};
use eyre::{Context, OptionExt};
use std::sync::Arc;
use std::{env, str::FromStr};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    rpc_url: Option<String>,

    #[arg(short, long)]
    contract_address: String,
}

fn get_private_key() -> String {
    env::var("AP_PRIVATE_KEY").expect("AP_PRIVATE_KEY not found in environment")
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let cli = Cli::parse();

    // Check "deli" linking and "deli/debug" feature work
    let value = Amount::from_u128_with_scale(1_00, 2);
    let other = Amount::from_u128_with_scale(2_50, 2);
    let _result = value.checked_mul(other).ok_or_eyre("Failed to multiply")?;
    log_msg!("{} x {} = {}", value, other, _result);

    let rpc_url = cli.rpc_url.unwrap_or("http://localhost:8547".to_owned());

    let provider = Provider::<Http>::try_from(rpc_url)?;
    let address: Address = cli.contract_address.parse()?;

    let priv_key = get_private_key();
    let wallet = LocalWallet::from_str(&priv_key)?;
    let chain_id = provider.get_chainid().await?.as_u64();
    let client = Arc::new(SignerMiddleware::new(
        provider,
        wallet.clone().with_chain_id(chain_id),
    ));

    abigen!(
        Disolver,
        r#"[
            function createContext(uint256 context_id) external
            function submitVector(uint256 context_id, uint256 vector_type, uint8[] memory data) external
            function getVector(uint256 context_id, uint256 vector_type) external view returns (uint8[] memory)
            function compute(uint256 context_id, uint256 context_type) external returns (uint8[] memory)
        ]"#
    );

    let disolver = Disolver::new(address, client);

    let now = Utc::now();
    let context_id = U256::from(now.timestamp_micros());
    log_msg!("Creating context: {}", context_id);

    disolver
        .create_context(context_id)
        .call()
        .await
        .context("Failed to crete context")?;

    Ok(())
}
