use deli::{amount::Amount, log_msg, vector::Vector};

pub struct SingleOrder {
    pub matrix: Vector, // (in) single column of asset weights
    pub quotes: Vector, // (in) fitted quantity for single index order
    pub assets: Vector, // (out) optimised total asset quantities
    // --
    num_assets: usize, // number of matrix rows
}

impl SingleOrder {
    pub fn new(matrix: Vector, quotes: Vector) -> Self {
        if quotes.data.len() != 1 {
            panic!("Quotes must have length of 1");
        }

        if quotes.data.len() != matrix.data.len() {
            panic!(
                "Matrix must have number of assets x 1 length: \
                number of assets = {}, number of orders = 1, number of matrix elements = {}",
                quotes.data.len(),
                matrix.data.len()
            );
        }

        let mut assets = Vector::new();
        let num_assets = matrix.data.len();

        assets.data.resize(assets.data.len(), Amount::ZERO);

        Self {
            matrix,
            quotes,
            assets,
            num_assets,
        }
    }

    pub fn compute(&mut self) -> Amount {
        let num_rows = self.num_assets;
        let quote = self.quotes.data[0];

        log_msg!("\nINPUTS");
        log_msg!("matrix = \n\t{:1.9}", self.matrix);
        log_msg!("quotes = \n\t{:0.9}", self.quotes);

        log_msg!("\nSOLVER STARTS");
        log_msg!("assets = \n\t{:1.9}", self.assets);
        
        log_msg!("\nSOLVER COMPUTES");

        for row in 0..num_rows {
            let asset_weight = self.matrix.data[row];
            if asset_weight.is_not() {
                self.assets.data[row] = Amount::ZERO;
                continue;
            }
            self.assets.data[row] = asset_weight.checked_mul(quote).unwrap();
        }
        
        log_msg!("\nSOLVER OUTPUT");
        log_msg!("assets = \n\t{:1.9}", self.assets);

        quote
    }
}
