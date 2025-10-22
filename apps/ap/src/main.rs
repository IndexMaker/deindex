use std::env;

// use alloy_provider::ProviderBuilder;
// use alloy_signer_local::PrivateKeySigner;
use clap::Parser;
use deli::{amount::Amount, log_msg};
// use reqwest::Url;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, value_delimiter = ',')]
    rpc_url: Option<String>,
}

fn get_private_key() -> String {
    env::var("AP_PRIVATE_KEY").expect("AP_PRIVATE_KEY not found in environment")
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let value = Amount::from_u128_with_scale(1_00, 2);
    let other = Amount::from_u128_with_scale(2_50, 2);
    let _result = value.checked_mul(other).unwrap();
    log_msg!("{} x {} = {}", value, other, _result);

    // let signer = get_private_key()
    //     .parse::<PrivateKeySigner>()
    //     .map_err(|err| eyre::eyre!("Failed to parse private key: {:?}", err))
    //     .unwrap();

    // let rpc_url = cli.rpc_url.unwrap_or("http://localhost:8547".to_owned()).parse::<Url>().unwrap();

    // let provider = ProviderBuilder::new()
    //     .wallet(signer.clone())
    //     .on_http(rpc_url);
}
