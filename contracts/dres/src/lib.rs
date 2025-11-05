// Allow `cargo stylus export-abi` to generate a main function.
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use alloy_primitives::{Address, U256};
use alloy_sol_types::sol;
use stylus_sdk::{
    prelude::*,
    storage::{StorageBool, StorageBytes, StorageMap},
};

use deli::{amount::Amount, asset::*, labels::Labels, vector::Vector};

sol! {
    // event allows us to know suppliers joining us
    event NewInventory(address supplier);

    // event allows us to know executed orders against suppliers
    event InventoryMatched(address supplier, uint256 order_id);
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
    pub fn init(&mut self) {
        self.active.set(true);
    }

    fn is_active(&self) -> bool {
        self.active.get()
    }

    fn check_assets_sorted(assets: &Labels) -> Result<(), Vec<u8>> {
        if !assets.data.is_sorted_by_key(|x| get_asset_id(*x)) {
            Err(b"Assets must be sorted")?;
        }
        Ok(())
    }

    fn check_assets_aligned(assets: &Labels, vector: &Vector) -> Result<(), Vec<u8>> {
        if assets.data.len() != vector.data.len() {
            Err(b"Assets must be aligned with data")?;
        }
        Ok(())
    }

    fn submit(
        &mut self,
        assets: Labels,
        positions: Vector,
        prices: Vector,
        liquidity: Vector,
        slopes: Vector,
    ) -> Result<(), Vec<u8>> {
        Self::check_assets_sorted(&assets)?;

        if !self.active.get() {
            self.active.set(true);
            self.assets.set_bytes(assets.to_vec());
            self.positions.set_bytes(positions.to_vec());
            self.prices.set_bytes(prices.to_vec());
            self.liquidity.set_bytes(liquidity.to_vec());
            self.slopes.set_bytes(slopes.to_vec());
        } else {
            // merge inventory
            let mut inventory_assets = Labels::from_vec(self.assets.get_bytes());
            let mut inventory_positions = Vector::from_vec(self.positions.get_bytes());
            let mut inventory_prices = Vector::from_vec(self.prices.get_bytes());
            let mut inventory_liquidity = Vector::from_vec(self.liquidity.get_bytes());
            let mut inventory_slopes = Vector::from_vec(self.slopes.get_bytes());

            let mut inventory_index = 0;
            for asset_index in 0..assets.data.len() {
                let asset = assets.data[asset_index]; // asset_id + side
                let asset_id = get_asset_id(asset);

                let mut inventory_updated = false;
                while inventory_index < inventory_assets.data.len() {
                    let inventory_asset = inventory_assets.data[inventory_index];
                    let inventory_asset_id = get_asset_id(inventory_asset);

                    if inventory_asset_id < asset_id {
                        // go to next inventory asset and match with same
                        // incoming asset...
                        inventory_index += 1;
                        continue;
                    } else if inventory_asset_id > asset_id {
                        // if this is new entry, then we insert before
                        // inventory_index, and we keep same side as incoming
                        // asset
                        // NOTE: here in submit() we are adding new assets,
                        // because supplier is telling us they got new assets
                        // either in stock or available on market.
                        inventory_assets.data.insert(inventory_index, asset);
                        inventory_positions
                            .data
                            .insert(inventory_index, positions.data[asset_index]);
                        inventory_prices
                            .data
                            .insert(inventory_index, prices.data[asset_index]);
                        inventory_liquidity
                            .data
                            .insert(inventory_index, liquidity.data[asset_index]);
                        inventory_slopes
                            .data
                            .insert(inventory_index, slopes.data[asset_index]);
                        // go to next incoming asset and match with current
                        // inventory asset...
                        inventory_updated = true;
                        inventory_index += 1; // current inventory asset shifted by one
                        break;
                    } else {
                        // if asset exists in current inventory, then we
                        // overwrite with incoming asset
                        // NOTE: here in submit() we OVERWRITE and NOT UPDATE,
                        // because supplier is telling us new values and not
                        // deltas.
                        inventory_assets.data[inventory_index] = asset;
                        inventory_positions.data[inventory_index] = positions.data[asset_index];
                        inventory_prices.data[inventory_index] = prices.data[asset_index];
                        inventory_liquidity.data[inventory_index] = liquidity.data[asset_index];
                        inventory_slopes.data[inventory_index] = slopes.data[asset_index];
                        // go to next incoming asset and match with next
                        // inventory asset...
                        inventory_updated = true;
                        inventory_index += 1;
                        break;
                    }
                }

                if !inventory_updated {
                    // asset not found in inventory, and sorts after last
                    // inventory asset
                    inventory_assets.data.push(asset);
                    inventory_positions.data.push(positions.data[asset_index]);
                    inventory_prices.data.push(prices.data[asset_index]);
                    inventory_liquidity.data.push(liquidity.data[asset_index]);
                    inventory_slopes.data.push(slopes.data[asset_index]);
                }
            }
        }

        Ok(())
    }

    fn get_inventory(&self, assets: Labels) -> Result<(Vector, Vector, Vector, Vector), Vec<u8>> {
        Self::check_assets_sorted(&assets)?;

        let mut inventory_assets = Labels::from_vec(self.assets.get_bytes());
        let mut inventory_positions = Vector::from_vec(self.positions.get_bytes());
        let mut inventory_prices = Vector::from_vec(self.prices.get_bytes());
        let mut inventory_liquidity = Vector::from_vec(self.liquidity.get_bytes());
        let mut inventory_slopes = Vector::from_vec(self.slopes.get_bytes());

        let mut inventory_index = 0;
        for asset_index in 0..assets.data.len() {
            let asset = assets.data[asset_index]; // asset_id + side
            let asset_id = get_asset_id(asset);

            while inventory_index < inventory_assets.data.len() {
                let inventory_asset = inventory_assets.data[inventory_index];
                let inventory_asset_id = get_asset_id(inventory_asset);

                if inventory_asset_id < asset_id {
                    // inventory asset not included on list of assets
                    inventory_assets.data.remove(inventory_index);
                    inventory_positions.data.remove(inventory_index);
                    inventory_prices.data.remove(inventory_index);
                    inventory_liquidity.data.remove(inventory_index);
                    inventory_slopes.data.remove(inventory_index);
                    // go to next inventory asset..
                    // inventory_index remains the same as we removed item at
                    // that index, so next item is now occupying that index.
                    continue;
                } else if inventory_asset_id > asset_id {
                    // asset on the list, but not in the inventory, so we return
                    // flat position.
                    inventory_assets
                        .data
                        .insert(inventory_index, make_asset(asset_id, SIDE_FLAT));
                    inventory_positions
                        .data
                        .insert(inventory_index, Amount::ZERO);
                    inventory_prices.data.insert(inventory_index, Amount::ZERO);
                    inventory_liquidity
                        .data
                        .insert(inventory_index, Amount::ZERO);
                    inventory_slopes.data.insert(inventory_index, Amount::ZERO);
                    // go to next incoming asset..
                    // inventory_index remains unchanged, we'll match next
                    // incoming asset against that inventory asset.
                    break;
                } else {
                    // asset on the list and in the inventory, we can continue
                    // as positions are already there in the state we want to
                    // return them.
                    inventory_index += 1;
                    break;
                }
            }
        }

        // truncate position to remove any remaining unlisted assets
        inventory_positions
            .data
            .resize(inventory_index, Amount::ZERO);
        inventory_prices.data.resize(inventory_index, Amount::ZERO);
        inventory_liquidity
            .data
            .resize(inventory_index, Amount::ZERO);
        inventory_slopes.data.resize(inventory_index, Amount::ZERO);

        Ok((
            inventory_positions,
            inventory_prices,
            inventory_liquidity,
            inventory_slopes,
        ))
    }

    pub fn match_inventory(
        &mut self,
        order_type: u8,
        order_assets: Labels,
        order_quantities: Vector,
    ) -> Result<(Vector, Vector), Vec<u8>> {
        if order_type != 0 {
            Err(b"Unsupported order type")?;
        }

        Self::check_assets_sorted(&order_assets)?;
        Self::check_assets_aligned(&order_assets, &order_quantities)?;

        let mut inventory_assets = Labels::from_vec(self.assets.get_bytes());
        let mut inventory_positions = Vector::from_vec(self.positions.get_bytes());

        let inventory_prices = Vector::from_vec(self.prices.get_bytes());
        let inventory_slopes = Vector::from_vec(self.slopes.get_bytes());

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
                    inventory_assets.data[inventory_index] =
                        make_asset(asset_id, new_inventory_side);
                }
            }
        }

        self.assets.set_bytes(inventory_assets.to_vec());
        self.positions.set_bytes(inventory_positions.to_vec());

        Ok((executed_prices, executed_quantities))
    }
}

#[storage]
#[entrypoint]
pub struct Dres {
    inventory: StorageMap<Address, Inventory>,
}

#[public]
impl Dres {
    pub fn create_inventory(&mut self) -> Result<(), Vec<u8>> {
        let supplier = self.vm().tx_origin();
        let mut inventory = self.inventory.setter(supplier);
        if inventory.is_active() {
            Err(b"Inventory already exists")?;
        }
        inventory.init();
        Ok(())
    }

    pub fn submit_inventory(
        &mut self,
        assets_bytes: Vec<u8>,
        positions_bytes: Vec<u8>,
        prices_bytes: Vec<u8>,
        liquidity_bytes: Vec<u8>,
        slopes_bytes: Vec<u8>,
    ) -> Result<(), Vec<u8>> {
        let supplier = self.vm().tx_origin();
        let mut inventory = self.inventory.setter(supplier);
        if !inventory.is_active() {
            Err(b"No such supplier")?;
        }
        let assets = Labels::from_vec(assets_bytes);
        let positions = Vector::from_vec(positions_bytes);
        let prices = Vector::from_vec(prices_bytes);
        let liquidity = Vector::from_vec(liquidity_bytes);
        let slopes = Vector::from_vec(slopes_bytes);
        inventory.submit(assets, positions, prices, liquidity, slopes)?;
        log(self.vm(), NewInventory { supplier });
        Ok(())
    }

    pub fn get_inventory(
        &self,
        supplier: Address,
        assets_bytes: Vec<u8>,
    ) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>), Vec<u8>> {
        let inventory = self.inventory.getter(supplier);
        if !inventory.is_active() {
            Err(b"No such supplier")?;
        }
        let assets = Labels::from_vec(assets_bytes);
        let (positions, prices, liquidity, slopes) = inventory.get_inventory(assets)?;
        Ok((
            positions.to_vec(),
            prices.to_vec(),
            liquidity.to_vec(),
            slopes.to_vec(),
        ))
    }

    pub fn match_inventory(
        &mut self,
        supplier: Address,
        order_id: U256,
        order_type: u8,
        assets_bytes: Vec<u8>,
        quantities_bytes: Vec<u8>,
    ) -> Result<(Vec<u8>, Vec<u8>), Vec<u8>> {
        let mut inventory = self.inventory.setter(supplier);
        if !inventory.is_active() {
            Err(b"Supplier not active")?;
        }

        let assets = Labels::from_vec(assets_bytes);
        let quantities = Vector::from_vec(quantities_bytes);

        let (executed_prices, executed_quantities) =
            inventory.match_inventory(order_type, assets, quantities)?;

        log(self.vm(), InventoryMatched { supplier, order_id });
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
            .map(|inventory| inventory.supplier)
            .collect();

        let mut total_positions = BTreeMap::new();
        for supplier in suppliers {
            let (positions, _prices, _liquidity, _slopes) = contract
                .get_inventory(supplier, inventory_assets.to_vec())
                .unwrap();
            let positions = Vector::from_vec(positions);
            for i in 0..inventory_assets.data.len() {
                let asset = inventory_assets.data[i];
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
