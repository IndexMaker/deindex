#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use alloy_primitives::{Address, U256, U64};
use deli::{amount::Amount, vector::Vector};
use stylus_sdk::{
    prelude::*,
    storage::{StorageAddress, StorageBytes, StorageMap, StorageU64},
};

use crate::{filler::Filler, quoter::Quoter, solver::Solver};

pub mod filler;
pub mod quoter;
pub mod solver;

// CT_QUOTE
// - compute index prices and capacities
pub const CT_QUOTE: u8 = 1;

// CT_STRATEGY can be used to:
// - compute index prices
// - compute individual index asset values for collateral routing
// - compute total asset quantities to send order for
// - compute distribution coefficients for later fills
pub const CT_STRATEGY: u8 = 2;

// CT_FILL can be used to:
// - compute filled assets redistribution between a set of index orders
pub const CT_FILL: u8 = 3;

// Input / Output vectors required for solver computation
pub const VT_PRICES: u8 = 1; // prices of individual assets
pub const VT_LIQUID: u8 = 2; // liquidity of individual assets
pub const VT_MATRIX: u8 = 3; // row major matrix of basket columns for individual index orders
pub const VT_COLLAT: u8 = 4; // collateral for individual index orders
pub const VT_IAQTYS: u8 = 5; // row major matrix of individual asset quantities
pub const VT_IAVALS: u8 = 6; // row major matrix of individual asset values
pub const VT_IFILLS: u8 = 7; // index fill rates
pub const VT_ASSETS: u8 = 8; // optimised total asset quantities
pub const VT_COEFFS: u8 = 9; // row major matrix of fitting coefficient columns for individual index orders
pub const VT_NETAVS: u8 = 10; // net asset values (index prices)
pub const VT_QUOTES: u8 = 11; // max possible index quantity (index capacity or fitted quantities for individual index orders)
pub const VT_AXPXES: u8 = 12; // executed prices for exchange of individual assets
pub const VT_AXFEES: u8 = 13; // fees paid for exchange of individual assets
pub const VT_AXQTYS: u8 = 14; // total executed quantity of each asset
pub const VT_IXQTYS: u8 = 15; // index quantities
pub const VT_AAFEES: u8 = 16; // asset assigned fees to index order
pub const VT_AAQTYS: u8 = 17; // asset assigned quantities to index order
pub const VT_CCOVRS: u8 = 18; // collateral carry overs
pub const VT_ACOVRS: u8 = 19; // asset carry overs
pub const VT_SLOPES: u8 = 20; // price-quantity slopes for individual assets (for linear slippage)
pub const VT_IXSLPS: u8 = 21; // index slope (aggregate linear slippage)

#[storage]
pub struct DisolveContext {
    address: StorageAddress,
    block: StorageU64,
    timestamp: StorageU64,
    vectors: StorageMap<U256, StorageBytes>,
}

impl DisolveContext {
    pub fn init(&mut self, address: Address, block: u64, timestamp: u64) {
        self.address.set(address);
        self.block.set(U64::from(block));
        self.timestamp.set(U64::from(timestamp));
    }

    pub fn set_vector(&mut self, vector_type: U256, data: Vec<u8>) -> Result<(), Vec<u8>> {
        let mut v = self.vectors.setter(vector_type);
        if !v.is_empty() {
            Err(b"Vector already set")?;
        }
        v.set_bytes(data);
        Ok(())
    }

    pub fn get_vector(&self, vector_type: U256) -> Vec<u8> {
        let v = self.vectors.getter(vector_type);
        v.get_bytes()
    }

    fn check_exists(&self) -> Result<(), Vec<u8>> {
        if self.address.get().is_empty() {
            Err(b"Context does not exist")?;
        }
        Ok(())
    }

    fn check_access(
        &mut self,
        address: Address,
        block: u64,
        timestamp: u64,
    ) -> Result<(), Vec<u8>> {
        if self.address.get() != address {
            Err(b"Invalid address")?;
        }
        if block < self.block.get().to::<u64>() {
            Err(b"Invalid block")?;
        }
        if timestamp < self.timestamp.get().to::<u64>() {
            Err("Invalid timestamp")?;
        }
        Ok(())
    }

    fn set_vector_internal(&mut self, vector_type: u8, data: Vec<u8>) -> Result<(), Vec<u8>> {
        self.set_vector(U256::from(vector_type), data)
    }

    fn get_vector_internal(&self, vector_type: u8) -> Vec<u8> {
        self.get_vector(U256::from(vector_type))
    }

    pub fn compute(&mut self, context_type: U256) -> Result<Vec<u8>, Vec<u8>> {
        let ct = context_type.to::<u8>();
        let total_amount = match ct {
            CT_QUOTE => {
                let prices = Vector::from_vec(self.get_vector_internal(VT_PRICES));
                let liquid = Vector::from_vec(self.get_vector_internal(VT_LIQUID));
                let matrix = Vector::from_vec(self.get_vector_internal(VT_MATRIX));
                let slopes = Vector::from_vec(self.get_vector_internal(VT_SLOPES));
                let mut quoter = Quoter::new(prices, liquid, matrix, slopes);
                let total_amount = quoter.quote();
                self.set_vector_internal(VT_NETAVS, quoter.netavs.to_vec())?;
                self.set_vector_internal(VT_IXSLPS, quoter.ixslps.to_vec())?;
                self.set_vector_internal(VT_QUOTES, quoter.quotes.to_vec())?;
                total_amount
            }
            CT_STRATEGY => {
                let prices = Vector::from_vec(self.get_vector_internal(VT_PRICES));
                let liquid = Vector::from_vec(self.get_vector_internal(VT_LIQUID));
                let matrix = Vector::from_vec(self.get_vector_internal(VT_MATRIX));
                let collat = Vector::from_vec(self.get_vector_internal(VT_COLLAT));
                let mut solver = Solver::new(prices, liquid, matrix, collat);
                let total_amount = solver.solve();
                self.set_vector_internal(VT_IAQTYS, solver.iaqtys.to_vec())?;
                self.set_vector_internal(VT_IAVALS, solver.iavals.to_vec())?;
                self.set_vector_internal(VT_ASSETS, solver.assets.to_vec())?;
                self.set_vector_internal(VT_COEFFS, solver.coeffs.to_vec())?;
                self.set_vector_internal(VT_NETAVS, solver.netavs.to_vec())?;
                self.set_vector_internal(VT_QUOTES, solver.quotes.to_vec())?;
                total_amount
            }
            CT_FILL => {
                let prices = Vector::from_vec(self.get_vector_internal(VT_AXPXES));
                let axfees = Vector::from_vec(self.get_vector_internal(VT_AXFEES));
                let assets = Vector::from_vec(self.get_vector_internal(VT_AXQTYS));
                let coeffs = Vector::from_vec(self.get_vector_internal(VT_COEFFS));
                let iaqtys = Vector::from_vec(self.get_vector_internal(VT_IAQTYS));
                let quotes = Vector::from_vec(self.get_vector_internal(VT_QUOTES));
                let collat = Vector::from_vec(self.get_vector_internal(VT_COLLAT));
                let mut filler =
                    Filler::new(prices, axfees, assets, coeffs, iaqtys, quotes, collat);
                let total_amount = filler.fill();
                self.set_vector_internal(VT_IFILLS, filler.ifills.to_vec())?;
                self.set_vector_internal(VT_IXQTYS, filler.ixqtys.to_vec())?;
                self.set_vector_internal(VT_AAFEES, filler.aafees.to_vec())?;
                self.set_vector_internal(VT_AAQTYS, filler.aaqtys.to_vec())?;
                self.set_vector_internal(VT_CCOVRS, filler.ccovrs.to_vec())?;
                self.set_vector_internal(VT_ACOVRS, filler.acovrs.to_vec())?;
                total_amount
            }
            _ => panic!("Invalid context type"),
        };
        let mut output = Vec::new();
        total_amount.to_vec(&mut output);
        Ok(output)
    }
}

#[storage]
#[entrypoint]
pub struct Disolver {
    contexts: StorageMap<U256, DisolveContext>,
}

#[public]
impl Disolver {
    pub fn create_context(&mut self, context_id: U256) -> Result<(), Vec<u8>> {
        let address = self.vm().tx_origin();
        let block = self.vm().block_number();
        let timestamp = self.vm().block_timestamp();
        let mut context = self.contexts.setter(context_id);
        if !context.address.get().is_zero() {
            Err(b"Context already exists")?;
        }
        context.init(address, block, timestamp);
        Ok(())
    }

    pub fn submit_vector(
        &mut self,
        context_id: U256,
        vector_type: U256,
        data: Vec<u8>,
    ) -> Result<(), Vec<u8>> {
        let address = self.vm().tx_origin();
        let block = self.vm().block_number();
        let timestamp = self.vm().block_timestamp();
        let mut context = self.contexts.setter(context_id);
        context.check_access(address, block, timestamp)?;
        context.set_vector(vector_type, data)
    }

    pub fn get_vector(&self, context_id: U256, vector_type: U256) -> Result<Vec<u8>, Vec<u8>> {
        let context = self.contexts.getter(context_id);
        context.check_exists()?;
        Ok(context.get_vector(vector_type))
    }

    pub fn compute(&mut self, context_id: U256, context_type: U256) -> Result<Vec<u8>, Vec<u8>> {
        let address = self.vm().tx_origin();
        let block = self.vm().block_number();
        let timestamp = self.vm().block_timestamp();
        let mut context = self.contexts.setter(context_id);
        context.check_access(address, block, timestamp)?;
        context.compute(context_type)
    }
}

#[cfg(test)]
mod test {
    use deli::{amount::Amount, vector::Vector};

    use super::*;

    #[test]
    fn test_disolver() {
        use stylus_sdk::testing::*;
        let vm = TestVM::default();
        let mut contract = Disolver::from(&vm);

        //
        // -- we start with fresh computation context
        // -- we supply market prices and liquidity
        // -- we supply quantities of each individual asset for each order as matrix
        // -- we supply collateral amount for each index order
        // -- and then we ask to compute the strategy so we can:
        // -- route collateral to trading venues based on collateral distribution across assets
        // -- send orders for individual assets to exchange
        //

        let context = 1001;

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

        // create compute context for solver strategy
        contract.create_context(U256::from(context)).unwrap();

        // submit inputs
        contract
            .submit_vector(U256::from(context), U256::from(VT_PRICES), prices_bytes)
            .unwrap();
        contract
            .submit_vector(U256::from(context), U256::from(VT_LIQUID), liquid_bytes)
            .unwrap();
        contract
            .submit_vector(U256::from(context), U256::from(VT_MATRIX), matrix_bytes)
            .unwrap();
        contract
            .submit_vector(U256::from(context), U256::from(VT_COLLAT), collat_bytes)
            .unwrap();

        // compute
        contract
            .compute(U256::from(context), U256::from(CT_STRATEGY))
            .unwrap();

        // collect outputs
        let iaqtys_bytes = contract
            .get_vector(U256::from(context), U256::from(VT_IAQTYS))
            .unwrap();
        let iavals_bytes = contract
            .get_vector(U256::from(context), U256::from(VT_IAVALS))
            .unwrap();
        let assets_bytes = contract
            .get_vector(U256::from(context), U256::from(VT_ASSETS))
            .unwrap();
        let coeffs_bytes = contract
            .get_vector(U256::from(context), U256::from(VT_COEFFS))
            .unwrap();
        let netavs_bytes = contract
            .get_vector(U256::from(context), U256::from(VT_NETAVS))
            .unwrap();
        let quotes_bytes = contract
            .get_vector(U256::from(context), U256::from(VT_QUOTES))
            .unwrap();

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
        contract
            .submit_vector(U256::from(context), U256::from(VT_AXPXES), axpxes_bytes)
            .unwrap();
        contract
            .submit_vector(U256::from(context), U256::from(VT_AXFEES), axfees_bytes)
            .unwrap();
        contract
            .submit_vector(U256::from(context), U256::from(VT_AXQTYS), axqtys_bytes)
            .unwrap();

        // compute
        contract
            .compute(U256::from(context), U256::from(CT_FILL))
            .unwrap();

        // collect outputs
        let ifills_bytes = contract
            .get_vector(U256::from(context), U256::from(VT_IFILLS))
            .unwrap();
        let ixqtys_bytes = contract
            .get_vector(U256::from(context), U256::from(VT_IXQTYS))
            .unwrap();
        let aafees_bytes = contract
            .get_vector(U256::from(context), U256::from(VT_AAFEES))
            .unwrap();
        let aaqtys_bytes = contract
            .get_vector(U256::from(context), U256::from(VT_AAQTYS))
            .unwrap();
        let ccovrs_bytes = contract
            .get_vector(U256::from(context), U256::from(VT_CCOVRS))
            .unwrap();
        let acovrs_bytes = contract
            .get_vector(U256::from(context), U256::from(VT_ACOVRS))
            .unwrap();

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
    }
}
