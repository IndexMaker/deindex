use chrono::Utc;
use clap::Parser;
use deli::{amount::Amount, labels::Labels, log_msg, vector::Vector};
use ethers::{
    abi::Detokenize,
    contract::FunctionCall,
    middleware::SignerMiddleware,
    prelude::abigen,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{Address, Filter, U256},
};
use eyre::Context;
use futures::future::join_all;
use itertools::Itertools;
use std::{borrow::Borrow, env, str::FromStr};
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    rpc_url: Option<String>,

    #[arg(long)]
    disolver_address: String,

    #[arg(long)]
    dior_address: String,
}

fn get_private_key() -> String {
    env::var("AP_PRIVATE_KEY").expect("AP_PRIVATE_KEY not found in environment")
}

fn gen_unique_id() -> i64 {
    let now = Utc::now();
    let value = now.timestamp_micros();
    value
}


struct TxSender {
    client: Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
    signed_txs: Vec<ethers::core::types::Bytes>,
}

impl TxSender {
    pub fn new(client: Arc<SignerMiddleware<Provider<Http>, LocalWallet>>) -> Self {
        Self {
            client,
            signed_txs: Vec::new(),
        }
    }

    pub async fn add<B, M, D>(&mut self, mut call: FunctionCall<B, M, D>) -> eyre::Result<()>
    where
        B: Borrow<M>,
        M: Middleware + 'static,
        D: Detokenize,
    {
        log_msg!("adding transaction...");
        // self.client.fill_transaction(&mut call.tx, call.block).await?;
        // log_msg!("gas {:?}", call.tx.gas());
        // log_msg!("gas price {:?}", call.tx.gas_price());
        call.tx.set_gas(1_000_000u64);
        call.tx.set_gas_price(2_000_000_000);
        call.tx.set_chain_id(self.client.signer().chain_id());
        call.tx.set_from(self.client.signer().address());
        let signature = self
            .client
            .signer()
            .sign_transaction(&call.tx)
            .await
            .context("Failed to sign tx")?;
        let signed_tx: ethers::core::types::Bytes = call.tx.rlp_signed(&signature);
        self.signed_txs.push(signed_tx);
        Ok(())
    }

    pub async fn flush(self) -> eyre::Result<()> {
        log_msg!("sending transactions...");
        let mut pending_txs = Vec::new();

        for signed_tx in self.signed_txs {
            let pending_tx = self
                .client
                .send_raw_transaction(signed_tx)
                .await
                .context("Failed to send tx")?;
            pending_txs.push(pending_tx);
        }

        log_msg!("awaiting receipts...");
        let (tx_receipts, send_errors): (Vec<_>, Vec<_>) =
            join_all(pending_txs).await.into_iter().partition_result();

        if !send_errors.is_empty() {
            Err(eyre::eyre!(
                "Errors while sending transactions: {:?}",
                send_errors
            ))?;
        }

        for _tx_receipt in tx_receipts {
            log_msg!("Receipt: {:?}", _tx_receipt);
        }

        Ok(())
    }
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

    abigen!(
        Dior,
        r#"[
            function createIndex(uint256 index_id, uint8[] memory assets, uint8[] memory weights) external
            function submitOrder(uint256 index_id, uint256 collateral_amount) external
            function submitInventory(uint8[] memory assets, uint8[] memory positions) external
            function getOrders(uint256 index_id, address[] memory users) external view returns (uint8[] memory)
            function getInventory(address supplier) external view returns (uint8[] memory, uint8[] memory)
            event NewIndexOrder(address sender)
            event NewInventory(address sender)
        ]"#
    );

    let mut nonce = client
        .get_transaction_count(client.address(), None)
        .await
        .context("Failed to fetch the current nonce from the Ethereum client")?;

    log_msg!("starting nonce: {}", nonce);

    let mut next_nonce = move || {
        let ret = nonce;
        nonce = nonce.checked_add(U256::one()).unwrap();
        ret
    };

    // ---------------------------------------------------------
    log_msg!("\n[Testing RPC interaction with Dior contract]\n");

    let dior = Dior::new(dior_address, client.clone());

    let assets = Labels {
        data: vec![101, 102, 103, 104],
    };

    let weights = Vector {
        data: vec![
            Amount::from_u128_with_scale(1_00, 2),
            Amount::from_u128_with_scale(2_00, 2),
            Amount::from_u128_with_scale(0_10, 2),
            Amount::from_u128_with_scale(0_50, 2),
        ],
    };

    let index_id = U256::from(gen_unique_id());
    log_msg!(
        "creating index: {}\nassets: \n\t{}\nweights:\n\t{:1.3}",
        index_id,
        assets,
        weights
    );
    let create_index = dior
        .create_index(index_id, assets.to_vec(), weights.to_vec())
        .nonce(next_nonce());

    let index_created_at = client
        .get_block_number()
        .await
        .context("Failed to get block number")?;

    log_msg!("user submits first order");
    let submit_order_1 = dior
        .submit_order(
            index_id,
            Amount::from_u128_with_scale(150_00, 2).to_u256_ethers(),
        )
        .nonce(next_nonce());

    log_msg!("user submits another order");
    let submit_order_2 = dior
        .submit_order(
            index_id,
            Amount::from_u128_with_scale(150_00, 2).to_u256_ethers(),
        )
        .nonce(next_nonce());

    let inventory_assets = Labels {
        data: vec![101, 102, 103, 104],
    };

    let inventory_positions = Vector {
        data: vec![
            Amount::from_u128_with_scale(0_50, 2),
            Amount::from_u128_with_scale(1_00, 2),
            Amount::from_u128_with_scale(0_05, 2),
            Amount::from_u128_with_scale(0_25, 2),
        ],
    };

    log_msg!("supplier submits inventory");
    let submit_inventory = dior
        .submit_inventory(inventory_assets.to_vec(), inventory_positions.to_vec())
        .nonce(next_nonce());

    log_msg!("sending transactions...");
    let mut tx_sender = TxSender::new(client.clone());
    tx_sender.add(create_index).await?;
    tx_sender.add(submit_order_1).await?;
    tx_sender.add(submit_order_2).await?;
    tx_sender.add(submit_inventory).await?;
    tx_sender.flush().await?;

    let new_orders: Vec<NewIndexOrderFilter> = dior
        .event::<NewIndexOrderFilter>()
        .from_block(index_created_at)
        .query()
        .await
        .context("Failed to get new index order events")?;

    for _new_order in new_orders {
        log_msg!("new order from: {}", _new_order.sender);
    }

    let _new_supplies: Vec<NewInventoryFilter> = dior
        .event::<NewInventoryFilter>()
        .from_block(index_created_at)
        .query()
        .await
        .context("Failed to get new index order events")?;

    for _new_supply in _new_supplies {
        log_msg!("new supply from: {}", _new_supply.sender);
    }

    // ---------------------------------------------------------
    log_msg!("\n[Testing RPC interaction with Disolver contract]\n");

    let disolver = Disolver::new(disolver_address, client.clone());

    // create compute context for solver strategy
    let context_id = U256::from(gen_unique_id());

    let prices = Vector {
        data: vec![
            Amount::from_u128_with_scale(50000_00, 2), //< asset_1
            Amount::from_u128_with_scale(5000_00, 2),  //< asset_2
            Amount::from_u128_with_scale(500_00, 2),   //< asset_3
        ],
    };

    let liquid = Vector {
        data: vec![
            Amount::from_u128_with_scale(0_002, 3), //< asset_1
            Amount::from_u128_with_scale(0_020, 3), //< asset_2
            Amount::from_u128_with_scale(0_200, 3), //< asset_3
        ],
    };

    let matrix = Vector {
        data: vec![
            // asset_1
            Amount::from_u128_with_scale(0_001, 3), //< order_1
            Amount::from_u128_with_scale(0_010, 3), //< order_2
            // asset_2
            Amount::from_u128_with_scale(0_010, 3), //< order_1
            Amount::from_u128_with_scale(0_100, 3), //< order_2
            // asset_3
            Amount::from_u128_with_scale(0_100, 3), //< order_1
            Amount::from_u128_with_scale(1_000, 3), //< order_2
        ],
    };

    let collat = Vector {
        data: vec![
            Amount::from_u128_with_scale(150_00, 2), //< order_1
            Amount::from_u128_with_scale(300_00, 2), //< order_2
        ],
    };

    // serialize inputs into binary blobs
    let prices_bytes = prices.to_vec();
    let liquid_bytes = liquid.to_vec();
    let matrix_bytes = matrix.to_vec();
    let collat_bytes = collat.to_vec();

    log_msg!("creating context: {}", context_id);
    let create_context = disolver.create_context(context_id).nonce(next_nonce());

    // submit inputs
    log_msg!("submitting prices: \n\t{:1.3}", prices);
    let submit_prices = disolver
        .submit_vector(context_id, U256::from(VT_PRICES), prices_bytes)
        .nonce(next_nonce());

    log_msg!("submitting liquidity: \n\t{:1.3}", liquid);
    let submit_liquid = disolver
        .submit_vector(context_id, U256::from(VT_LIQUID), liquid_bytes)
        .nonce(next_nonce());

    log_msg!("submitting matrix: \n\t{:2.3}", matrix);
    let submit_matrix = disolver
        .submit_vector(context_id, U256::from(VT_MATRIX), matrix_bytes)
        .nonce(next_nonce());

    log_msg!("submitting collateral: \n\t{:0.3}", collat);
    let submit_collat = disolver
        .submit_vector(context_id, U256::from(VT_COLLAT), collat_bytes)
        .nonce(next_nonce());

    // compute
    log_msg!("submitting compute...");
    let submit_compute = disolver
        .compute(context_id, U256::from(CT_STRATEGY))
        .nonce(next_nonce());

    let mut tx_sender = TxSender::new(client.clone());
    tx_sender.add(create_context).await?;
    for call in [submit_prices, submit_liquid, submit_matrix, submit_collat] {
        tx_sender.add(call).await?;
    }
    tx_sender.add(submit_compute).await?;
    tx_sender.flush().await?;

    log_msg!("fetching results...");
    // collect outputs
    let iaqtys_bytes = disolver
        .get_vector(context_id, U256::from(VT_IAQTYS))
        .call()
        .await
        .context("Failed to fetch index orders asset quantities")?;
    let iavals_bytes = disolver
        .get_vector(context_id, U256::from(VT_IAVALS))
        .call()
        .await
        .context("Failed to fetch asset quantities")?;
    let assets_bytes = disolver
        .get_vector(context_id, U256::from(VT_ASSETS))
        .call()
        .await
        .context("Failed to fetch coefficients")?;
    let coeffs_bytes = disolver
        .get_vector(context_id, U256::from(VT_COEFFS))
        .call()
        .await
        .context("Failed to fetch index net asset values")?;
    let netavs_bytes = disolver
        .get_vector(context_id, U256::from(VT_NETAVS))
        .call()
        .await
        .context("Failed to fetch index orders quotes")?;
    let quotes_bytes = disolver
        .get_vector(context_id, U256::from(VT_QUOTES))
        .call()
        .await
        .context("Failed to fetch")?;

    // deserialize outputs from binary blobs
    let iaqtys = Vector::from_vec(iaqtys_bytes);
    let iavals = Vector::from_vec(iavals_bytes);
    let assets = Vector::from_vec(assets_bytes);
    let coeffs = Vector::from_vec(coeffs_bytes);
    let netavs = Vector::from_vec(netavs_bytes);
    let quotes = Vector::from_vec(quotes_bytes);

    // check assertions
    assert_eq!(iaqtys.data.len(), matrix.data.len());
    assert_eq!(iavals.data.len(), matrix.data.len());
    assert_eq!(coeffs.data.len(), matrix.data.len());
    assert_eq!(assets.data.len(), prices.data.len());
    assert_eq!(netavs.data.len(), collat.data.len());
    assert_eq!(quotes.data.len(), collat.data.len());

    log_msg!("assets: \n\t{:1.3}", assets);
    log_msg!("coeffs: \n\t{:2.3}", coeffs);
    log_msg!("quotes: \n\t{:0.3}", quotes);

    //
    // -- at this point we would send orders to exchange connector
    // -- and then we would receive fills
    // -- so now we simulate fills
    //

    let axpxes = Vector {
        data: vec![
            Amount::from_u128_with_scale(50000_00, 2), //< asset_1
            Amount::from_u128_with_scale(5000_00, 2),  //< asset_2
            Amount::from_u128_with_scale(500_00, 2),   //< asset_3
        ],
    };

    let axfees = Vector {
        data: vec![
            Amount::from_u128_with_scale(50_00, 2), //< asset_1
            Amount::from_u128_with_scale(5_00, 2),  //< asset_2
            Amount::from_u128_with_scale(0_50, 2),  //< asset_3
        ],
    };

    let axqtys = Vector {
        data: vec![
            Amount::from_u128_with_scale(0_005, 3), //< asset_1
            Amount::from_u128_with_scale(0_050, 3), //< asset_2
            Amount::from_u128_with_scale(0_500, 3), //< asset_3
        ],
    };

    // serialize inputs into binary blobs
    let axpxes_bytes = axpxes.to_vec();
    let axfees_bytes = axfees.to_vec();
    let axqtys_bytes = axqtys.to_vec();

    // submit inputs
    log_msg!("submitting asset executed prices: \n\t{:0.3}", axpxes);
    let submit_axpxes = disolver
        .submit_vector(context_id, U256::from(VT_AXPXES), axpxes_bytes)
        .nonce(next_nonce());

    log_msg!("submitting asset executed fees: \n\t{:0.3}", axfees);
    let submit_axfees = disolver
        .submit_vector(context_id, U256::from(VT_AXFEES), axfees_bytes)
        .nonce(next_nonce());

    log_msg!("submitting asset executed quantities: \n\t{:0.3}", axqtys);
    let submit_axqtys = disolver
        .submit_vector(context_id, U256::from(VT_AXQTYS), axqtys_bytes)
        .nonce(next_nonce());

    // compute
    let submit_compute = disolver
        .compute(context_id, U256::from(CT_FILL))
        .nonce(next_nonce());

    let mut tx_sender = TxSender::new(client.clone());
    for call in [submit_axpxes, submit_axfees, submit_axqtys] {
        tx_sender.add(call).await?;
    }
    tx_sender.add(submit_compute).await?;
    tx_sender.flush().await?;

    // collect outputs
    let ifills_bytes = disolver
        .get_vector(context_id, U256::from(VT_IFILLS))
        .call()
        .await
        .context("Failed to fetch index orders fills")?;
    let ixqtys_bytes = disolver
        .get_vector(context_id, U256::from(VT_IXQTYS))
        .call()
        .await
        .context("Failed to fetch index quantities")?;
    let aafees_bytes = disolver
        .get_vector(context_id, U256::from(VT_AAFEES))
        .call()
        .await
        .context("Failed to fetch assigned asset fees")?;
    let aaqtys_bytes = disolver
        .get_vector(context_id, U256::from(VT_AAQTYS))
        .call()
        .await
        .context("Failed to fetch assigned asset quantities")?;
    let ccovrs_bytes = disolver
        .get_vector(context_id, U256::from(VT_CCOVRS))
        .call()
        .await
        .context("Failed to fetch collateral carry-overs")?;
    let acovrs_bytes = disolver
        .get_vector(context_id, U256::from(VT_ACOVRS))
        .call()
        .await
        .context("Failed to fetch assets carry-overs")?;

    // deserialize outputs from binary blobs
    let ifills = Vector::from_vec(ifills_bytes);
    let ixqtys = Vector::from_vec(ixqtys_bytes);
    let aafees = Vector::from_vec(aafees_bytes);
    let aaqtys = Vector::from_vec(aaqtys_bytes);
    let ccovrs = Vector::from_vec(ccovrs_bytes);
    let acovrs = Vector::from_vec(acovrs_bytes);

    // check assertions
    assert_eq!(ifills.data.len(), collat.data.len());
    assert_eq!(ixqtys.data.len(), collat.data.len());
    assert_eq!(aafees.data.len(), coeffs.data.len());
    assert_eq!(aaqtys.data.len(), coeffs.data.len());
    assert_eq!(ccovrs.data.len(), collat.data.len());
    assert_eq!(acovrs.data.len(), prices.data.len());

    log_msg!("ifills: \n\t{:0.3}", ifills);
    log_msg!("ixqtys: \n\t{:0.3}", ixqtys);
    log_msg!("ccovrs: \n\t{:0.3}", ccovrs);

    log_msg!("Done.");
    Ok(())
}
