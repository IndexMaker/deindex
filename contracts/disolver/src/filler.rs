use deli::{amount::Amount, log_msg, vector::Vector};

pub struct Filler {
    pub axpxes: Vector, // (in) executed prices for exchange of individual assets
    pub axfees: Vector, // (in) fees paid for exchange of individual assets
    pub axqtys: Vector, // (in) total executed quantity of each asset
    pub coeffs: Vector, // (in) row major matrix of coefficient columns for individual index orders
    pub iaqtys: Vector, // (in) row major matrix of individual asset quantities
    pub quotes: Vector, // (in) fitted quantities for individual index orders
    pub collat: Vector, // (in) collateral for individual index orders
    pub ifills: Vector, // (out) index fill rates
    pub ixqtys: Vector, // (out) index quantities
    pub aafees: Vector, // (out) asset assigned fees to index order
    pub aaqtys: Vector, // (out) asset assigned quantities to index order
    pub ccovrs: Vector, // (out) collateral carry overs
    pub acovrs: Vector, // (out) asset carry overs
    // --
    num_assets: usize, // number of matrix rows
    num_orders: usize, // number of matrix columns
}

impl Filler {
    pub fn new(
        axpxes: Vector,
        axfees: Vector,
        axqtys: Vector,
        coeffs: Vector,
        iaqtys: Vector,
        quotes: Vector,
        collat: Vector,
    ) -> Self {
        let num_assets = axpxes.data.len();
        let num_coeffs = coeffs.data.len();
        if num_coeffs % num_coeffs != 0 {
            panic!("Invalid number of coeffs or assets");
        }
        let num_orders = num_coeffs / num_assets;
        let mut ifills = Vector::new();
        let mut ixqtys = Vector::new();
        let mut aafees = Vector::new();
        let mut aaqtys = Vector::new();
        let mut ccovrs = Vector::new();
        let mut acovrs = Vector::new();

        ixqtys.data.resize(num_orders, Amount::ZERO);
        aafees.data.resize(num_coeffs, Amount::ZERO);
        aaqtys.data.resize(num_coeffs, Amount::ZERO);

        // start with: index_fill_rate[..] = 100%
        ifills.data.resize(num_orders, Amount::ONE);

        // start with: original collateral
        ccovrs.data.resize(num_orders, Amount::ZERO);
        ccovrs.data.copy_from_slice(&collat.data);

        // start with: filled asset quantities
        acovrs.data.resize(num_assets, Amount::ZERO);
        acovrs.data.copy_from_slice(&axqtys.data);

        Self {
            axpxes,
            axfees,
            axqtys,
            coeffs,
            iaqtys,
            quotes,
            collat,
            ifills,
            ixqtys,
            aafees,
            aaqtys,
            ccovrs,
            acovrs,
            num_assets,
            num_orders,
        }
    }

    pub fn fill(&mut self) -> Amount {
        let num_rows = self.num_assets;
        let num_cols = self.num_orders;

        log_msg!("\nFILLER INPUTS");
        log_msg!("axpxes = \n\t{:1.9}", self.axpxes);
        log_msg!("axfees = \n\t{:1.9}", self.axfees);
        log_msg!("axqtys = \n\t{:1.9}", self.axqtys);
        log_msg!("coeffs = \n\t{:2.9}", self.coeffs);
        log_msg!("iaqtys = \n\t{:2.9}", self.iaqtys);
        log_msg!("quotes = \n\t{:2.9}", self.quotes);
        log_msg!("collat = \n\t{:2.9}", self.collat);

        log_msg!("\nFILLER STARTS");
        log_msg!("ifills = \n\t{:0.9}", self.ifills);
        log_msg!("ixqtys = \n\t{:0.9}", self.ixqtys);
        log_msg!("aafees = \n\t{:2.9}", self.aafees);
        log_msg!("aaqtys = \n\t{:2.9}", self.aaqtys);
        log_msg!("ccovrs = \n\t{:0.9}", self.ccovrs);
        log_msg!("acovrs = \n\t{:1.9}", self.acovrs);

        log_msg!("\nFILLER OUTPUTS");

        // compute index order fill rates
        let mut row_offset = 0;
        for row in 0..num_rows {
            let asset_quantity = self.axqtys.data[row];
            let row_start = row_offset;
            row_offset += num_cols;
            let coeffs_row = &self.coeffs.data[row_start..row_offset];
            let iaqtys_row = &self.iaqtys.data[row_start..row_offset];

            for col in 0..num_cols {
                let index_fill_rate = &mut self.ifills.data[col];
                // - index_fill_rate[col] = min(index_fill_rate[col], coeffs[..,col] x axqtys[..])
                let fill_qty = coeffs_row[col].checked_mul(asset_quantity).unwrap();
                let max_qty = iaqtys_row[col];
                if fill_qty.is_less_than(&max_qty) {
                    let fill_rate = fill_qty.checked_div(max_qty).unwrap();
                    if fill_rate < *index_fill_rate {
                        *index_fill_rate = fill_rate;
                    }
                }
            }
        }

        log_msg!("ifills = \n\t{:0.9}", self.ifills);

        // compute index quantities
        // distribute asset exchange fees and quantities

        let mut total_value = Amount::ZERO;

        let mut row_offset = 0;
        for row in 0..num_rows {
            let row_start = row_offset;
            row_offset += num_cols;
            let asset_px = &self.axpxes.data[row];
            let asset_fee = &self.axfees.data[row];
            let asset_qty = &self.axqtys.data[row];
            let asset_carry_overs = &mut self.acovrs.data[row];
            let iaqtys_row = &self.iaqtys.data[row_start..row_offset];
            let coeffs_row = &mut self.coeffs.data[row_start..row_offset];
            let aafees_row = &mut self.aafees.data[row_start..row_offset];
            let aaqtys_row = &mut self.aaqtys.data[row_start..row_offset];

            for col in 0..num_cols {
                let index_fill_rate = &self.ifills.data[col];
                let max_qty = iaqtys_row[col];
                let collat_carry_over = &mut self.ccovrs.data[col];
                let quoted_index_quantity = &self.quotes.data[col];
                let filled_index_quantity = &mut self.ixqtys.data[col];
                let asset_assigned_fee = &mut aafees_row[col];
                let asset_assigned_qty = &mut aaqtys_row[col];
                // - filled_index_quantity[col] = index_fill_rate[col] x quoted_index_quantity[col]
                *filled_index_quantity =
                    index_fill_rate.checked_mul(*quoted_index_quantity).unwrap();
                // - asset_assigned_fee[row,col] = index_fill_rate[col] x asset_coeff[row,col] x axfees[row]
                // - asset_assigned_qty[row,col] = index_fill_rate[col] x asset_coeff[row,col] x axqtys[row]
                let asset_coeff = coeffs_row[col].checked_mul(*filled_index_quantity).unwrap();
                let aafee = asset_coeff.checked_mul(*asset_fee).unwrap();
                let aaqty = asset_coeff.checked_mul(*asset_qty).unwrap();
                if aaqty.is_less_than(&max_qty) {
                    *asset_assigned_qty = aaqty;
                    *asset_assigned_fee = aafee;
                } else {
                    // We need to cap to max for that order, and proportionally reduce the fee
                    let aaqqty_frac = max_qty.checked_div(aaqty).unwrap();
                    let aafee_max = aaqqty_frac.checked_mul(aafee).unwrap();
                    *asset_assigned_qty = max_qty;
                    *asset_assigned_fee = aafee_max;
                }
                // - asset_assigned_val[row,col] = asset_assigned_qty[row,col] x axpxes[row]
                let asset_assigned_val = asset_assigned_qty.checked_mul(*asset_px).unwrap();
                let asset_assigned_cost =
                    asset_assigned_val.checked_add(*asset_assigned_fee).unwrap();
                // acovrs[row] = axqtys[row] - sum(aaqtys[row,..])
                if let Some(x) = asset_carry_overs.checked_sub(*asset_assigned_qty) {
                    *asset_carry_overs = x;
                } else {
                    log_msg!(
                        "ERROR: Resulting asset carry over is negative: [{},{}] {} - {}",
                        row,
                        col,
                        *asset_carry_overs,
                        asset_assigned_qty
                    );
                    *asset_carry_overs = Amount::ZERO;
                }
                if let Some(x) = collat_carry_over.checked_sub(asset_assigned_cost) {
                    *collat_carry_over = x;
                } else {
                    log_msg!(
                        "ERROR: Resulting collateral carry over is negative: [{},{}] {} - {}",
                        row,
                        col,
                        *collat_carry_over,
                        asset_assigned_cost
                    );
                    *collat_carry_over = Amount::ZERO;
                }
                // compute total assigned asset value checksum across all orders
                total_value = total_value.checked_add(asset_assigned_val).unwrap();
            }
        }

        log_msg!("ixqtys = \n\t{:0.9}", self.ixqtys);
        log_msg!("aafees = \n\t{:2.9}", self.aafees);
        log_msg!("aaqtys = \n\t{:2.9}", self.aaqtys);
        log_msg!("ccovrs = \n\t{:0.9}", self.ccovrs);
        log_msg!("acovrs = \n\t{:1.9}", self.acovrs);

        total_value
    }
}
