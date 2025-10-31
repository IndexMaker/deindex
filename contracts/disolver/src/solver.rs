use alloc::vec::Vec;

use deli::{amount::Amount, log_msg, vector::Vector};

pub struct Solver {
    pub prices: Vector, // (in) prices of individual assets
    pub liquid: Vector, // (in) liquidity of individual assets
    pub matrix: Vector, // (in) row major matrix of basket columns for individual index orders
    pub collat: Vector, // (in) collateral for individual index orders
    pub iaqtys: Vector, // (out) row major matrix of individual asset quantities
    pub iavals: Vector, // (out) row major matrix of individual asset values
    pub assets: Vector, // (out) optimised total asset quantities
    pub coeffs: Vector, // (out) row major matrix of fitting coefficient columns for individual index orders
    pub netavs: Vector, // (out) net asset values (index prices)
    pub quotes: Vector, // (out) fitted quantities for individual index orders
    // --
    num_assets: usize, // number of matrix rows
    num_orders: usize, // number of matrix columns
}

impl Solver {
    pub fn new(prices: Vector, liquid: Vector, matrix: Vector, collat: Vector) -> Self {
        if prices.data.len() != liquid.data.len() {
            panic!("Prices and liquidity vectors must have same length");
        }

        if prices.data.len() * collat.data.len() != matrix.data.len() {
            panic!(
                "Matrix must have number of assets x number of orders length: \
                number of assets = {}, number of orders = {}, number of matrix elements = {}",
                prices.data.len(),
                collat.data.len(),
                matrix.data.len()
            );
        }

        let mut iaqtys = Vector::new();
        let mut iavals = Vector::new();
        let mut assets = Vector::new();
        let mut coeffs = Vector::new();
        let mut netavs = Vector::new();
        let mut quotes = Vector::new();

        let num_assets = prices.data.len();
        let num_orders = collat.data.len();

        assets.data.resize(prices.data.len(), Amount::ZERO);
        coeffs.data.resize(matrix.data.len(), Amount::ZERO);
        iaqtys.data.resize(matrix.data.len(), Amount::ZERO);
        iavals.data.resize(matrix.data.len(), Amount::ZERO);
        netavs.data.resize(collat.data.len(), Amount::ZERO);
        quotes.data.resize(collat.data.len(), Amount::ZERO);

        Self {
            prices,
            liquid,
            matrix,
            collat,
            iaqtys,
            iavals,
            assets,
            coeffs,
            netavs,
            quotes,
            num_assets,
            num_orders,
        }
    }

    pub fn solve(&mut self) -> Amount {
        let num_rows = self.num_assets;
        let num_cols = self.num_orders;

        log_msg!("\nINPUTS");
        log_msg!("prices = \n\t{:1.9}", self.prices);
        log_msg!("liquid = \n\t{:1.9}", self.liquid);
        log_msg!("matrix = \n\t{:2.9}", self.matrix);
        log_msg!("collat = \n\t{:0.9}", self.collat);

        log_msg!("\nSOLVER STARTS");
        log_msg!("iaqtys = \n\t{:2.9}", self.iaqtys);
        log_msg!("iavals = \n\t{:2.9}", self.iavals);
        log_msg!("assets = \n\t{:1.9}", self.assets);
        log_msg!("coeffs = \n\t{:2.9}", self.coeffs);
        log_msg!("netavs = \n\t{:0.9}", self.netavs);
        log_msg!("quotes = \n\t{:0.9}", self.quotes);

        log_msg!("\nSOLVER COMPUTES");

        // - compute each index price, i.e. index_price[col] = sum(matrix[..,col] x prices[..])
        let mut row_offset = 0;
        for row in 0..num_rows {
            let asset_price = self.prices.data[row];
            let row_start = row_offset;
            row_offset += num_cols;
            let matrix_row = &self.matrix.data[row_start..row_offset];

            for col in 0..num_cols {
                let asset_quantity = matrix_row[col];
                let asset_value = asset_quantity.checked_mul(asset_price).unwrap();
                let index_nav = &mut self.netavs.data[col];
                *index_nav = index_nav.checked_add(asset_value).unwrap();
            }
        }

        log_msg!("netavs = \n\t{:0.9}", self.netavs);

        // - compute index quantity: quote[col] = collat[col] / index_price[col]
        for col in 0..num_cols {
            let collateral = self.collat.data[col];
            let index_nav = self.netavs.data[col];
            self.quotes.data[col] = collateral.checked_div(index_nav).unwrap();
        }

        log_msg!("quotes = \n\t{:0.9}", self.quotes);

        // - compute asset quantities: index_asset[row,col] = quote[col] x matrix[row,col]
        // - compute total asset qty: assets[row] = sum(index_asset[row,..])
        let mut row_offset = 0;
        for row in 0..num_rows {
            let row_start = row_offset;
            row_offset += num_cols;
            let matrix_row = &self.matrix.data[row_start..row_offset];
            let iaqtys_row = &mut self.iaqtys.data[row_start..row_offset];
            let total_asset = &mut self.assets.data[row];

            for col in 0..num_cols {
                let asset_weight = &matrix_row[col];
                if asset_weight.is_not() {
                    continue;
                }
                let order_quantity = self.quotes.data[col];
                let asset_quantity = asset_weight.checked_mul(order_quantity).unwrap();
                iaqtys_row[col] = asset_quantity;
                *total_asset = total_asset.checked_add(asset_quantity).unwrap();
            }
        }

        log_msg!("iaqtys = \n\t{:2.9}", self.iaqtys);
        log_msg!("assets = \n\t{:1.9}", self.assets);

        log_msg!("\nSOLVER FITS LIQUIDITY");

        // - compute index asset coefficients
        // - fit liquidity

        let mut index_fill_rates = Vec::new();
        // start with: index_fill_rate[..] = 100%
        index_fill_rates.resize(num_cols, Amount::ONE);

        let mut row_offset = 0;
        for row in 0..num_rows {
            let asset_liquid = self.liquid.data[row];
            let row_start = row_offset;
            row_offset += num_cols;
            let coeffs_row = &mut self.coeffs.data[row_start..row_offset];
            let iaqtys_row = &self.iaqtys.data[row_start..row_offset];
            let total_asset = &mut self.assets.data[row];

            for col in 0..num_cols {
                let asset_quantity = &iaqtys_row[col];
                let coeff = &mut coeffs_row[col];
                let fill_rate = &mut index_fill_rates[col];
                // - compute asset contribution fraction: coeff[row,col] = index_asset[row,col] / assets[row]
                *coeff = asset_quantity.checked_div(*total_asset).unwrap();
                // - compute: available[row,col] = min(assets[row], liquid[row]) * coeff[row,col]
                let available_asset = asset_liquid.checked_mul(*coeff).unwrap();
                if available_asset.is_less_than(&asset_quantity) {
                    // - fraction_available[row,col] = available[row,col] / index_asset[row,col]
                    let fraction_available = available_asset.checked_div(*asset_quantity).unwrap();
                    // - index_fill_rate[col] = min(index_fill_rate[col], fraction_available[..,col])
                    if fraction_available.is_less_than(&fill_rate) {
                        *fill_rate = fraction_available;
                    }
                } // else we don't touch fill rate, because fraction available >=100% of what is needed
            }

            // - compute total asset qty: assets[row] = min(assets[row], liquid[row])
            if asset_liquid.is_less_than(total_asset) {
                *total_asset = asset_liquid;
            }
        }

        log_msg!("coeffs = \n\t{:2.9}", self.coeffs);
        log_msg!("assets = \n\t{:1.9}", self.assets);

        log_msg!("\nSOLVER OUTPUT");

        // - compute reduced index quantity

        for col in 0..num_cols {
            let fill_rate = &index_fill_rates[col];
            let order_quantity = &mut self.quotes.data[col];
            // - compute reduced index quantity: quote[col] = index_fill_rate[col] x quote[col]
            *order_quantity = order_quantity.checked_mul(*fill_rate).unwrap();
        }

        log_msg!("quotes = \n\t{:0.9}", self.quotes);

        // - compute asset quantities
        // - compute asset values (for collateral routing)

        let mut total_value = Amount::ZERO;

        let mut row_offset = 0;
        for row in 0..num_rows {
            let asset_price = self.prices.data[row];
            let row_start = row_offset;
            row_offset += num_cols;
            let iaqtys_row = &mut self.iaqtys.data[row_start..row_offset];
            let iavals_row = &mut self.iavals.data[row_start..row_offset];

            for col in 0..num_cols {
                let fill_rate = &index_fill_rates[col];
                let asset_quantity = &mut iaqtys_row[col];
                let asset_value = &mut iavals_row[col];
                // - compute asset quantities: index_asset[row,col] = index_fill_rate[col] x index_asset[row,col]
                *asset_quantity = asset_quantity.checked_mul(*fill_rate).unwrap();
                // - compute asset values: index_asset_value[row,col] = index_asset[row,col] x prices[row]
                *asset_value = asset_quantity.checked_mul(asset_price).unwrap();
                // compute total net asset value checksum across all orders
                total_value = total_value.checked_add(*asset_value).unwrap();
            }
        }

        log_msg!("iaqtys = \n\t{:2.9}", self.iaqtys);
        log_msg!("iavals = \n\t{:2.9}", self.iavals);

        // Note: padding orders is off-chain operation, i.e. AP is free to adjust orders so that
        // they meet requirements for sending to trading venues. The fills returned from AP must
        // fill the underlying assets of index orders no more than 100%.

        total_value
    }
}
