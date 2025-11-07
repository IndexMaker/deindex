// Allow `cargo stylus export-abi` to generate a main function.
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use alloy_primitives::{U128, U256};
use alloy_sol_types::sol;
use stylus_sdk::{
    prelude::*,
    storage::{StorageBytes, StorageU128},
};

use deli::{amount::Amount, asset::*, labels::Labels, vector::Vector};

sol! {
    // event allows us to know suppliers joining us
    event NewInventory(uint8[] assets);

    // event allows us to know executed orders against suppliers
    event InventoryMatched(uint256 order_id, uint8[] assets);
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

fn compute_effective_price(
    side: u128,
    quantity: Amount,
    price: Amount,
    slope: Amount,
) -> Result<Amount, Vec<u8>> {
    let slippage = quantity
        .checked_mul(slope)
        .unwrap()
        .checked_mul(price)
        .unwrap();

    let effective_price = match side {
        SIDE_LONG => price.checked_add(slippage).unwrap(),
        SIDE_SHORT => price.checked_sub(slippage).unwrap(),
        _ => Err(b"Invalid side")?,
    };
    Ok(effective_price)
}

fn compute_effective_position_and_side(
    order_side: u128,
    order_quantity: Amount,
    inventory_side: u128,
    inventory_position: Amount,
) -> Result<(Amount, u128), Vec<u8>> {
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
    Ok((new_inventory_position, new_inventory_side))
}

struct VolleySizeCalc {
    total_volley_size: Amount,
}

impl VolleySizeCalc {
    fn new() -> Self {
        Self {
            total_volley_size: Amount::ZERO,
        }
    }

    fn update_total_volley(
        &mut self,
        side: u128,
        position: Amount,
        price: Amount,
        slope: Amount,
    ) -> Result<Amount, Vec<u8>> {
        let inventory_asset_volley_price = compute_effective_price(side, position, price, slope)?;

        let inventory_asset_volley_size =
            position.checked_mul(inventory_asset_volley_price).unwrap();

        self.total_volley_size = self
            .total_volley_size
            .checked_add(inventory_asset_volley_size)
            .unwrap();

        Ok(inventory_asset_volley_size)
    }
}

enum MergeJoinBranch {
    SkipInventoryInner,
    SkipAssetOuter,
    SkipAssetInner,
    Matched,
}

fn merge_join<FMatcher>(
    assets: &Labels,
    inventory_assets: &mut Labels,
    mut f_matcher: FMatcher,
) -> Result<(), Vec<u8>>
where
    FMatcher:
        FnMut(u128, u128, usize, usize, &mut Labels, MergeJoinBranch) -> Result<bool, Vec<u8>>,
{
    let mut inventory_index = 0;
    for asset_index in 0..assets.data.len() {
        let asset = assets.data[asset_index]; // asset_id + side
        let asset_id = get_asset_id(asset);

        let mut inventory_matched = false;
        while inventory_index < inventory_assets.data.len() {
            let inventory_asset = inventory_assets.data[inventory_index];
            let inventory_asset_id = get_asset_id(inventory_asset);

            if inventory_asset_id < asset_id {
                if f_matcher(
                    asset,
                    asset_id,
                    asset_index,
                    inventory_index,
                    inventory_assets,
                    MergeJoinBranch::SkipInventoryInner,
                )? {
                    inventory_index += 1;
                }
                continue;
            } else if inventory_asset_id > asset_id {
                if f_matcher(
                    asset,
                    asset_id,
                    asset_index,
                    inventory_index,
                    inventory_assets,
                    MergeJoinBranch::SkipAssetInner,
                )? {
                    inventory_index += 1;
                }
                inventory_matched = true;
                break;
            } else {
                if !f_matcher(
                    asset,
                    asset_id,
                    asset_index,
                    inventory_index,
                    inventory_assets,
                    MergeJoinBranch::Matched,
                )? {
                    return Ok(());
                }
                inventory_matched = true;
                break;
            }
        }

        if !inventory_matched {
            if !f_matcher(
                asset,
                asset_id,
                asset_index,
                inventory_index,
                inventory_assets,
                MergeJoinBranch::SkipAssetOuter,
            )? {
                return Ok(());
            }
        }
    }
    Ok(())
}

#[storage]
#[entrypoint]
pub struct Dres {
    assets: StorageBytes, // labels identifying assets (Vec<u128> encoded as Vec<u8>)
    positions: StorageBytes, // quantity of each asset (Vec<Amount> encoded as Vec<u8>)
    prices: StorageBytes, // volume weighted mid-point price (Vec<Amount> encoded as Vec<u8>)
    liquidity: StorageBytes, // total liquidity available (Vec<Amount> encoded as Vec<u8>)
    slopes: StorageBytes, // price liquidity slope (Vec<Amount> encoded as Vec<u8>)
    // --
    max_asset_volley: StorageU128,
    max_total_volley: StorageU128,
}

#[public]
impl Dres {
    pub fn set_thresholds(&mut self, max_asset_volley: U128, max_total_volley: U128) {
        self.max_asset_volley.set(max_asset_volley);
        self.max_total_volley.set(max_total_volley);
    }

    pub fn submit_inventory(
        &mut self,
        assets_bytes: Vec<u8>,
        positions_bytes: Vec<u8>,
        prices_bytes: Vec<u8>,
        liquidity_bytes: Vec<u8>,
        slopes_bytes: Vec<u8>,
    ) -> Result<(), Vec<u8>> {
        let _supplier = self.vm().tx_origin();

        let assets = Labels::from_vec(assets_bytes);
        let positions = Vector::from_vec(positions_bytes);
        let prices = Vector::from_vec(prices_bytes);
        let liquidity = Vector::from_vec(liquidity_bytes);
        let slopes = Vector::from_vec(slopes_bytes);

        check_assets_sorted(&assets)?;

        // merge inventory
        let mut inventory_assets = Labels::from_vec(self.assets.get_bytes());
        let mut inventory_positions = Vector::from_vec(self.positions.get_bytes());
        let mut inventory_prices = Vector::from_vec(self.prices.get_bytes());
        let mut inventory_liquidity = Vector::from_vec(self.liquidity.get_bytes());
        let mut inventory_slopes = Vector::from_vec(self.slopes.get_bytes());

        merge_join(
            &assets,
            &mut inventory_assets,
            |asset, _, asset_index, inventory_index, inventory_assets, case| match case {
                MergeJoinBranch::SkipInventoryInner => {
                    // this inventory asset has no update from supplier, we skip it
                    Ok(true)
                }
                MergeJoinBranch::SkipAssetInner => {
                    // this is new asset from supplier, which does not exist in the inventory
                    // but there is more inventory entries after it
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
                    Ok(true)
                }
                MergeJoinBranch::SkipAssetOuter => {
                    // this is new asset from supplier, which does not exist in the inventory
                    // and there is no more inventory entries after it
                    inventory_assets.data.push(asset);
                    inventory_positions.data.push(positions.data[asset_index]);
                    inventory_prices.data.push(prices.data[asset_index]);
                    inventory_liquidity.data.push(liquidity.data[asset_index]);
                    inventory_slopes.data.push(slopes.data[asset_index]);
                    Ok(true)
                }
                MergeJoinBranch::Matched => {
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
                    Ok(true)
                }
            },
        )?;

        log(
            self.vm(),
            NewInventory {
                assets: assets.to_vec(),
            },
        );
        Ok(())
    }

    pub fn get_inventory(
        &self,
        assets_bytes: Vec<u8>,
    ) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>/* , Vec<u8>*/), Vec<u8>> {
        let assets = Labels::from_vec(assets_bytes);

        check_assets_sorted(&assets)?;

        //
        // We fetch asset from inventory by first fetching all inventory
        // and then retaining only onces that are on the list, and for
        // those on the list that there is no match in the inventory we
        // insert zeroes, so that result is perfectly aligned with the list.
        //

        let mut inventory_assets = Labels::from_vec(self.assets.get_bytes());
        let mut inventory_positions = Vector::from_vec(self.positions.get_bytes());
        let mut inventory_prices = Vector::from_vec(self.prices.get_bytes());
        let mut inventory_liquidity = Vector::from_vec(self.liquidity.get_bytes());
        let mut inventory_slopes = Vector::from_vec(self.slopes.get_bytes());
        // let mut inventory_volleys = Vector::new();

        // let mut total_volley_calc = VolleySizeCalc::new();

        merge_join(
            &assets,
            &mut inventory_assets,
            |_, _, _, inventory_index, inventory_assets, case| match case {
                MergeJoinBranch::SkipInventoryInner => {
                    // inventory asset not included on list of assets
                    inventory_assets.data.remove(inventory_index);
                    inventory_positions.data.remove(inventory_index);
                    inventory_prices.data.remove(inventory_index);
                    inventory_liquidity.data.remove(inventory_index);
                    inventory_slopes.data.remove(inventory_index);
                    // inventory_index remains the same as we removed item at
                    // that index, so next item is now occupying that index.
                    Ok(false)
                }
                MergeJoinBranch::SkipAssetInner => {
                    // asset on the list, but not in the inventory, so we return
                    // flat position.
                    inventory_assets.data.insert(inventory_index, 0);
                    inventory_positions
                        .data
                        .insert(inventory_index, Amount::ZERO);
                    inventory_prices.data.insert(inventory_index, Amount::ZERO);
                    inventory_liquidity
                        .data
                        .insert(inventory_index, Amount::ZERO);
                    inventory_slopes.data.insert(inventory_index, Amount::ZERO);
                    // inventory_volleys.data.push(Amount::ZERO);
                    // inventory_index remains unchanged, we'll match next
                    // incoming asset against that inventory asset.
                    Ok(false)
                }
                MergeJoinBranch::SkipAssetOuter => {
                    inventory_positions.data.push(Amount::ZERO);
                    inventory_prices.data.push(Amount::ZERO);
                    inventory_liquidity.data.push(Amount::ZERO);
                    inventory_slopes.data.insert(inventory_index, Amount::ZERO);
                    // inventory_volleys.data.push(Amount::ZERO);
                    Ok(true)
                }
                MergeJoinBranch::Matched => {
                    // asset on the list and in the inventory, we can continue
                    // as positions are already there in the state we want to
                    // return them.
                    // let volley_size = total_volley_calc.update_total_volley(
                    //     get_side(inventory_assets.data[inventory_index]),
                    //     inventory_positions.data[inventory_index],
                    //     inventory_prices.data[inventory_index],
                    //     inventory_slopes.data[inventory_index],
                    // )?;
                    // inventory_volleys.data.push(volley_size);
                    Ok(true)
                }
            },
        )?;

        Ok((
            inventory_positions.to_vec(),
            inventory_prices.to_vec(),
            inventory_liquidity.to_vec(),
            inventory_slopes.to_vec(),
            // inventory_volleys.to_vec(),
        ))
    }

    /*
    pub fn match_inventory(
        &mut self,
        order_id: U256,
        order_type: u8,
        assets_bytes: Vec<u8>,
        quantities_bytes: Vec<u8>,
    ) -> Result<(Vec<u8>, Vec<u8>), Vec<u8>> {
        let order_assets = Labels::from_vec(assets_bytes);
        let order_quantities = Vector::from_vec(quantities_bytes);

        if order_type != 0 {
            Err(b"Unsupported order type")?;
        }

        check_assets_sorted(&order_assets)?;
        check_assets_aligned(&order_assets, &order_quantities)?;

        let mut inventory_assets = Labels::from_vec(self.assets.get_bytes());
        let mut inventory_positions = Vector::from_vec(self.positions.get_bytes());

        let inventory_prices = Vector::from_vec(self.prices.get_bytes());
        let inventory_slopes = Vector::from_vec(self.slopes.get_bytes());

        let max_asset_volley = Amount::from_u128(self.max_asset_volley.get());
        let max_total_volley = Amount::from_u128(self.max_total_volley.get());

        let mut total_volley_calc = VolleySizeCalc::new();

        let mut executed_prices = Vector::new();
        let mut executed_quantities = Vector::new();

        executed_prices
            .data
            .resize(order_assets.data.len(), Amount::ZERO);

        executed_quantities
            .data
            .resize(order_assets.data.len(), Amount::ZERO);

        merge_join(
            &order_assets,
            &mut inventory_assets,
            |order_asset, order_asset_id, order_index, inventory_index, inventory_assets, case| {
                match case {
                    MergeJoinBranch::SkipInventoryInner => {
                        // skip this inventory position, because there is no order
                        // for that asset.
                        Ok(true)
                    }
                    MergeJoinBranch::SkipAssetOuter => {
                        // we should have inventory positions submitted for all
                        // supported assets, even if those positions are zero.
                        Err(b"Missing inventory position for asset".to_vec())
                    }
                    MergeJoinBranch::SkipAssetInner => {
                        // we should have inventory positions submitted for all
                        // supported assets, even if those positions are zero.
                        Err(b"Missing inventory position for asset".to_vec())
                    }
                    MergeJoinBranch::Matched => {
                        let inventory_asset = inventory_assets.data[inventory_index];

                        let order_side = get_side(order_asset);
                        let order_quantity = order_quantities.data[order_index];

                        //
                        // Calculate executed price
                        //

                        let price = inventory_prices.data[inventory_index];
                        let slope = inventory_slopes.data[inventory_index];

                        let executed_price =
                            compute_effective_price(order_side, order_quantity, price, slope)?;

                        //
                        // Calculate position resulting from matching
                        //

                        let (new_inventory_position, new_inventory_side) =
                            compute_effective_position_and_side(
                                order_side,
                                order_quantity,
                                get_side(inventory_asset),
                                inventory_positions.data[inventory_index],
                            )?;

                        //
                        // Calcualte how deep is the volley to apply limits
                        //

                        let inventory_asset_volley_size = total_volley_calc.update_total_volley(
                            new_inventory_side,
                            new_inventory_position,
                            price,
                            slope,
                        )?;

                        if max_asset_volley.is_less_than(&inventory_asset_volley_size) {
                            Err(b"Max asset volley size reached")?;
                        }

                        if max_total_volley.is_less_than(&total_volley_calc.total_volley_size) {
                            Err(b"Max total volley size reached")?;
                        }

                        executed_prices.data[order_index] = executed_price;
                        executed_quantities.data[order_index] = order_quantity;

                        inventory_positions.data[inventory_index] = new_inventory_position;
                        inventory_assets.data[inventory_index] =
                            make_asset(order_asset_id, new_inventory_side);
                        Ok(true)
                    }
                }
            },
        )?;

        self.assets.set_bytes(inventory_assets.to_vec());
        self.positions.set_bytes(inventory_positions.to_vec());

        log(
            self.vm(),
            InventoryMatched {
                order_id,
                assets: order_assets.to_vec(),
            },
        );

        Ok((executed_prices.to_vec(), executed_quantities.to_vec()))
    }*/
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use alloy_primitives::{address, Address};
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
        let _emitted_logs = vm.get_emitted_logs();

        let mut total_positions = BTreeMap::new();
        let (positions, _prices, _liquidity, _slopes, _volleys) =
            contract.get_inventory(inventory_assets.to_vec()).unwrap();
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
        log_msg!("\ninventory:");
        for (asset, position) in total_positions {
            log_msg!("\tposition [{}]: {}", asset, position);
            let _ = (asset, position);
        }
    }
}
