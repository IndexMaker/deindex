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

use deli::{amount::Amount, asset::*, labels::Labels, vector::Vector};

sol! {
    event NewInventory(address sender);
}

#[storage]
pub struct Inventory {
    active: StorageBool,
    assets: StorageBytes, // labels identifying assets (Vec<u128> encoded as Vec<u8>)
    positions: StorageBytes, // quantity of each asset (Vec<Amount> encoded as Vec<u8>)
    prices: StorageBytes, // volume weighted mid-point price (Vec<Amount> encoded as Vec<u8>)
    liquidity: StorageBytes, // total liquidity available (Vec<Amount> encoded as Vec<u8>)
    slopes: StorageBytes, // price liquidity slope (Vec<Amount> encoded as Vec<u8>)
}

impl Inventory {
    fn is_active(&self) -> bool {
        self.active.get()
    }

    fn submit(
        &mut self,
        assets_bytes: Vec<u8>,
        positions_bytes: Vec<u8>,
        prices_bytes: Vec<u8>,
        liquidity_bytes: Vec<u8>,
        slopes_bytes: Vec<u8>,
    ) -> Result<(), Vec<u8>> {
        let assets = Labels::from_vec(assets_bytes);
        if !assets.data.is_sorted_by_key(|x| get_asset_id(*x)) {
            Err(b"Assets must be sorted")?;
        }

        self.active.set(true);
        self.assets.set_bytes(assets.to_vec());
        self.positions.set_bytes(positions_bytes);
        self.prices.set_bytes(prices_bytes);
        self.liquidity.set_bytes(liquidity_bytes);
        self.slopes.set_bytes(slopes_bytes);

        Ok(())
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
pub struct Dres {
    inventory: StorageMap<Address, Inventory>,
}

#[public]
impl Dres {
    pub fn submit_inventory(
        &mut self,
        assets: Vec<u8>,
        positions: Vec<u8>,
        prices: Vec<u8>,
        liquidity: Vec<u8>,
        slopes: Vec<u8>,
    ) -> Result<(), Vec<u8>> {
        let sender = self.vm().tx_origin();
        let mut inventory = self.inventory.setter(sender);
        inventory.submit(assets, positions, prices, liquidity, slopes)?;
        log(self.vm(), NewInventory { sender });
        Ok(())
    }

    pub fn get_inventory(
        &self,
        supplier: Address,
    ) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>), Vec<u8>> {
        let inventory = self.inventory.getter(supplier);
        if !inventory.is_active() {
            Err(b"No such supplier")?;
        }
        let result = inventory.get_inventory();
        Ok(result)
    }

    pub fn match_inventory(
        &mut self,
        supplier: Address,
        assets: Vec<u8>,
        orders: Vec<u8>,
        order_type: u8,
    ) -> Result<(Vec<u8>, Vec<u8>), Vec<u8>> {
        let _ = order_type;
        let mut inventory = self.inventory.setter(supplier);
        if !inventory.is_active() {
            Err(b"Supplier not active")?;
        }

        let order_assets = Labels::from_vec(assets);
        let order_quantities = Vector::from_vec(orders);

        if order_assets.data.len() != order_quantities.data.len() {
            Err(b"Order batch length mismatch")?;
        }

        if !order_assets.data.is_sorted_by_key(|x| get_asset_id(*x)) {
            Err(b"Assets must be sorted")?;
        }

        let mut inventory_assets = Labels::from_vec(inventory.assets.get_bytes());
        let mut inventory_positions = Vector::from_vec(inventory.positions.get_bytes());

        let inventory_prices = Vector::from_vec(inventory.prices.get_bytes());
        let inventory_slopes = Vector::from_vec(inventory.slopes.get_bytes());

        let mut executed_prices = Vector::new();
        let mut executed_quantities = Vector::new();

        executed_prices
            .data
            .resize(order_assets.data.len(), Amount::ZERO);

        executed_quantities
            .data
            .resize(order_assets.data.len(), Amount::ZERO);

        let mut next_inventory_index = 0;
        for order_index in 0..order_assets.data.len() {
            let order_asset = order_assets.data[order_index];
            let order_quantity = order_quantities.data[order_index];

            let order_asset_id = get_asset_id(order_asset);
            let order_side = get_side(order_asset);

            // we're performing here merge of two sorted arrays:
            // - asset orders (from the parameter)
            // - inventory positions (from storage)
            // we require that all these are sorted by asset_id.
            // because they are sorted, we can then use O(n+m) scan
            // where we skip inventory positions that we are not matching
            while next_inventory_index < inventory_assets.data.len() {
                let inventory_index = next_inventory_index;
                next_inventory_index += 1;

                let inventory_asset = inventory_assets.data[inventory_index];
                let asset_id = get_asset_id(inventory_asset);

                if asset_id < order_asset_id {
                    // skip this inventory position, because there is no order
                    // for that asset.
                    continue;
                } else if asset_id > order_asset_id {
                    // we should have inventory positions submitted for all
                    // supported assets, even if those positions are zero.
                    Err(b"Missing inventory position for asset")?;
                } else {
                    assert_eq!(asset_id, order_asset_id);
                    // compute excuted price using volume weighted approximation
                    let price = inventory_prices.data[inventory_index];
                    let slope = inventory_slopes.data[inventory_index];
                    let slippage = order_quantity
                        .checked_mul(slope)
                        .unwrap()
                        .checked_mul(price)
                        .unwrap();

                    let executed_price = match order_side {
                        SIDE_LONG => price.checked_add(slippage).unwrap(),
                        SIDE_SHORT => price.checked_sub(slippage).unwrap(),
                        _ => Err(b"Invalid order side in batch")?,
                    };

                    // compute executed quantity and new inventory position
                    let inventory_position = inventory_positions.data[inventory_index];
                    let inventory_side = get_side(inventory_asset);

                    let (new_inventory_position, new_inventory_side) = {
                        if inventory_side == SIDE_FLAT {
                            (order_quantity, order_side)
                        } else if order_side == inventory_side {
                            // we're matching on same side - we're extending position
                            (
                                inventory_position
                                    .checked_add(order_quantity)
                                    .ok_or_else(|| b"Position addition overflow")?,
                                inventory_side,
                            )
                        } else if inventory_position.is_less_than(&order_quantity) {
                            // we're flipping positon
                            (
                                order_quantity
                                    .checked_sub(inventory_position)
                                    .ok_or_else(|| b"Position flip calculation error".to_vec())?,
                                order_side, //< it's opposite by virtue if we're here
                            )
                        } else {
                            // we're reducing position
                            let new_pos = inventory_position
                                .checked_sub(order_quantity)
                                .ok_or_else(|| b"Position subtraction underflow".to_vec())?;

                            if new_pos.is_not() {
                                (Amount::ZERO, SIDE_FLAT)
                            } else {
                                (new_pos, inventory_side)
                            }
                        }
                    };

                    executed_prices.data[order_index] = executed_price;
                    executed_quantities.data[order_index] = order_quantity;

                    inventory_positions.data[inventory_index] = new_inventory_position;
                    inventory_assets.data[inventory_index] = asset_id | new_inventory_side;
                }
            }
        }

        inventory.assets.set_bytes(inventory_assets.to_vec());
        inventory.positions.set_bytes(inventory_positions.to_vec());

        Ok((executed_prices.to_vec(), executed_quantities.to_vec()))
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
        let mut contract = Dres::from(&vm);

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
        contract
            .submit_inventory(
                inventory_assets.to_vec(),
                inventory_positions.to_vec(),
                inventory_prices.to_vec(),
                inventory_liquidity.to_vec(),
                inventory_slopes.to_vec(),
            )
            .unwrap();

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
            let (assets, positions, _prices, _liquidity, _slopes) =
                contract.get_inventory(supplier).unwrap();
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
