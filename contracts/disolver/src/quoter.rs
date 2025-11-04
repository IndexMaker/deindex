use core::cmp::min;

use deli::{amount::Amount, log_msg, vector::Vector};

pub struct Quoter {
    pub prices: Vector, // (input) price of individual assets
    pub liquid: Vector, // (input) available liquidity of individual assets
    pub matrix: Vector, // (input) asset weights for n indices (assets in rows, indices in columns)
    pub slopes: Vector, // (input) price-quantity slopes for individual assets (for linear slippage)

    pub netavs: Vector, // (output) net asset value (index price)
    pub quotes: Vector, // (output) index capacity (maximum purchasable quantity)
    pub ixslps: Vector, // (output) index slope (aggregate linear slippage)

    num_assets: usize, // number of matrix rows
    num_orders: usize, // number of matrix columns
}

impl Quoter {
    pub fn new(prices: Vector, liquid: Vector, matrix: Vector, slopes: Vector) -> Self {
        let num_assets = prices.data.len();

        if num_assets == 0 {
            panic!("Quoter requires a non-empty list of assets");
        }
        if prices.data.len() != liquid.data.len() || prices.data.len() != slopes.data.len() {
            panic!("Prices, liquidity, and slopes vectors must have same length (num_assets)");
        }

        if matrix.data.len() % num_assets != 0 {
            panic!("Matrix length is inconsistent with number of assets: Matrix must be (num_assets x N)");
        }
        let num_orders = matrix.data.len() / num_assets;

        if num_orders == 0 {
            panic!("Quoter requires at least one index order column in the matrix");
        }

        let mut netavs = Vector::new();
        let mut quotes = Vector::new();
        let mut ixslps = Vector::new();

        netavs.data.resize(num_orders, Amount::ZERO);
        quotes.data.resize(num_orders, Amount::MAX);
        ixslps.data.resize(num_orders, Amount::ZERO);

        Self {
            prices,
            liquid,
            matrix,
            slopes,
            netavs,
            quotes,
            ixslps,
            num_assets,
            num_orders,
        }
    }

    pub fn quote(&mut self) -> Amount {
        log_msg!("\nQUOTER INPUTS (Batch of {} Indices)", self.num_orders);
        log_msg!("prices = \n\t{:1.9}", self.prices);
        log_msg!("liquid = \n\t{:1.9}", self.liquid);
        log_msg!("matrix = \n\t{:2.9}", self.matrix);
        log_msg!("slopes = \n\t{:1.9}", self.slopes);

        let num_rows = self.num_assets;
        let num_cols = self.num_orders;

        let mut total_value = Amount::ZERO;

        let mut row_offset = 0;
        for row in 0..num_rows {
            let row_start = row_offset;
            row_offset += num_cols;
            let matrix_row = &self.matrix.data[row_start..row_offset];

            let asset_price = self.prices.data[row];
            let asset_liquidity = self.liquid.data[row];
            let asset_slope = self.slopes.data[row];

            let asset_capacity_limit = asset_liquidity;

            for col in 0..num_cols {
                let asset_weight = matrix_row[col];
                if asset_weight.is_not() {
                    continue;
                }
                let asset_value = asset_price.checked_mul(asset_weight).unwrap();
                let asset_weight_sq = asset_weight.checked_sq().unwrap();
                let asset_slippage = asset_slope.checked_mul(asset_weight_sq).unwrap();
                let current_limit = asset_capacity_limit.checked_div(asset_weight).unwrap();
                // - compute each index price, i.e. index_price[col] = sum(matrix[..,col] x prices[..])
                // - compute index slope: index_slope[col] = sum(matrix[..,col] x slopes[..])
                // - compute index capacity: quotes[col] = min(liquid[..] / matrix[..,col])
                self.netavs.data[col] = self.netavs.data[col].checked_add(asset_value).unwrap();
                self.ixslps.data[col] = self.ixslps.data[col].checked_add(asset_slippage).unwrap();
                self.quotes.data[col] = min(self.quotes.data[col], current_limit);
                // compute total net asset value checksum across all orders
                total_value = total_value.checked_add(asset_value).unwrap();
            }
        }

        log_msg!("\nQUOTER OUTPUT");
        log_msg!("netavs = \n\t{:0.9}", self.netavs);
        log_msg!("ixslps = \n\t{:0.9}", self.ixslps);
        log_msg!("quotes = \n\t{:0.9}", self.quotes);

        total_value
    }
}
