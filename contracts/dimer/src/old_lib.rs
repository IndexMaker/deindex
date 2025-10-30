// Allow `cargo stylus export-abi` to generate a main function.
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use alloy_primitives::Address;
use alloy_sol_types::sol;
use stylus_sdk::{
    prelude::*,
    storage::{StorageBool, StorageBytes, StorageMap},
};

sol! {
    event NewInventory(address sender);
}

#[storage]
pub struct Inventory {
    active: StorageBool,     // asset info including: base asset, quote asset, side
    assets: StorageBytes,    // labels identifying assets
    positions: StorageBytes, // quantity of each asset (long/short)
    prices: StorageBytes,    // volume weighted mid-point price
    liquidity: StorageBytes, // liquidity available
    slopes: StorageBytes,    // price liquidity slope
}

impl Inventory {
    fn is_active(&self) -> bool {
        self.active.get()
    }

    fn submit(
        &mut self,
        assets: Vec<u8>,
        positions: Vec<u8>,
        prices: Vec<u8>,
        liquidity: Vec<u8>,
        slopes: Vec<u8>,
    ) {
        self.active.set(true);
        self.assets.set_bytes(assets);
        self.positions.set_bytes(positions);
        self.prices.set_bytes(prices);
        self.liquidity.set_bytes(liquidity);
        self.slopes.set_bytes(slopes);
    }

    fn get_inventory(&self) -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
        (
            self.assets.get_bytes(),
            self.positions.get_bytes(),
            self.prices.get_bytes(),
            self.liquidity.get_bytes(),
            self.slopes.get_bytes(),
        )
    }
}

#[storage]
#[entrypoint]
pub struct Dimer {
    inventory: StorageMap<Address, Inventory>,
}

#[public]
impl Dimer {
    pub fn submit_inventory(
        &mut self,
        assets: Vec<u8>,
        positions: Vec<u8>,
        prices: Vec<u8>,
        liquidity: Vec<u8>,
        slopes: Vec<u8>,
    ) {
        let sender = self.vm().tx_origin();
        let mut inventory = self.inventory.setter(sender);
        inventory.submit(assets, positions, prices, liquidity, slopes);
        log(self.vm(), NewInventory { sender });
    }

    pub fn get_inventory(
        &self,
        supplier: Address,
    ) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>), Vec<u8>> {
        let inventory = self.inventory.getter(supplier);
        if !inventory.is_active() {
            Err(b"No such suppier")?;
        }
        let result = inventory.get_inventory();
        Ok(result)
    }

    pub fn match_inventory(
        &mut self,
        supplier: Address,
        assets: Vec<u8>,
        orders: Vec<u8>,
    ) -> Result<(Vec<u8>, Vec<u8>), Vec<u8>> {
        Ok((vec![], vec![]))
    }
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use alloy_primitives::address;
    use alloy_sol_types::SolEvent;
    use deli::{amount::Amount, labels::Labels, log_msg, vector::Vector};

    use super::*;

    const SUPPLIER: Address = address!("0x90F79bf6EB2c4f870365E785982E1f101E93b906");
    const SOLVER: Address = address!("0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65");

    #[test]
    fn test_dior() {
        use stylus_sdk::testing::*;
        let vm = TestVM::default();
        let mut contract = Dimer::from(&vm);

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

        let inventory_prices = Vector {
            data: vec![
                Amount::from_u128_with_scale(100_00, 2),
                Amount::from_u128_with_scale(1000_00, 2),
                Amount::from_u128_with_scale(10_00, 2),
                Amount::from_u128_with_scale(1_00, 2),
            ],
        };

        let inventory_liquidity = Vector {
            data: vec![
                Amount::from_u128_with_scale(2_00, 2),
                Amount::from_u128_with_scale(5_00, 2),
                Amount::from_u128_with_scale(0_10, 2),
                Amount::from_u128_with_scale(0_75, 2),
            ],
        };

        let inventory_slopes = Vector {
            data: vec![
                Amount::from_u128_with_scale(0_01, 2),
                Amount::from_u128_with_scale(0_02, 2),
                Amount::from_u128_with_scale(0_01, 2),
                Amount::from_u128_with_scale(0_01, 2),
            ],
        };

        log_msg!("\nsupplier submits inventory");
        vm.set_sender(SUPPLIER);
        contract.submit_inventory(
            inventory_assets.to_vec(),
            inventory_positions.to_vec(),
            inventory_prices.to_vec(),
            inventory_liquidity.to_vec(),
            inventory_slopes.to_vec(),
        );

        log_msg!("\nsolver collecting events...");
        vm.set_sender(SOLVER);
        let emitted_logs = vm.get_emitted_logs();

        let suppliers: Vec<_> = emitted_logs
            .iter()
            .filter_map(|(topics, data)| {
                let inventory = NewInventory::decode_raw_log(topics, data, true);
                inventory.ok()
            })
            .map(|inventory| inventory.sender)
            .collect();

        let mut total_positions = BTreeMap::new();
        for supplier in suppliers {
            let (assets, positions, _prices, _liquidity, _slopes) = contract.get_inventory(supplier).unwrap();
            let assets = Labels::from_vec(assets);
            let positions = Vector::from_vec(positions);
            for i in 0..assets.data.len() {
                let asset = assets.data[i];
                let position = positions.data[i];
                let entry = total_positions.entry(asset);
                entry
                    .and_modify(|q: &mut Amount| {
                        *q = q.checked_add(position).unwrap();
                    })
                    .or_insert(position);
            }
        }
        log_msg!("\ninventory:");
        for (asset, position) in total_positions {
            log_msg!("\tposition [{}]: {}", asset, position);
            let _ = (asset, position);
        }
    }
}
