// Allow `cargo stylus export-abi` to generate a main function.
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use alloy_primitives::{Address, U128, U256};
use alloy_sol_types::sol;
use stylus_sdk::{
    prelude::*,
    storage::{StorageBool, StorageMap, StorageU128},
};

use deli::{amount::Amount, asset::*, labels::Labels, vector::Vector};

sol! {
    // event allows us to know suppliers joining us
    event NewInventory(address supplier);

    // event allows us to know executed orders against suppliers
    event InventoryMatched(address supplier, uint256 order_id);
}

#[storage]
pub struct InventoryAsset {
    asset: StorageU128,
    position: StorageU128,
    price: StorageU128,
    liquidity: StorageU128,
    slope: StorageU128,
}

#[storage]
pub struct Inventory {
    active: StorageBool,
    assets: StorageMap<U128, InventoryAsset>,
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
        Self::check_assets_aligned(&assets, &positions)?;
        Self::check_assets_aligned(&assets, &prices)?;
        Self::check_assets_aligned(&assets, &liquidity)?;
        Self::check_assets_aligned(&assets, &slopes)?;

        for asset_index in 0..assets.data.len() {
            let asset = assets.data[asset_index]; // asset_id + side
            let asset_id = get_asset_id(asset);

            let mut inventory_asset = self.assets.setter(U128::from(asset_id));

            // asset = asset_id + side
            inventory_asset.asset.set(U128::from(asset));

            inventory_asset
                .position
                .set(positions.data[asset_index].to_u128());

            inventory_asset
                .price
                .set(prices.data[asset_index].to_u128());

            inventory_asset
                .liquidity
                .set(liquidity.data[asset_index].to_u128());

            inventory_asset
                .slope
                .set(slopes.data[asset_index].to_u128());
        }

        Ok(())
    }

    fn get_inventory(&self, assets: Labels) -> Result<(Vector, Vector, Vector, Vector), Vec<u8>> {
        Self::check_assets_sorted(&assets)?;

        let mut inventory_assets = Labels::new();
        let mut inventory_positions = Vector::new();
        let mut inventory_prices = Vector::new();
        let mut inventory_liquidity = Vector::new();
        let mut inventory_slopes = Vector::new();

        for asset_index in 0..assets.data.len() {
            let asset = assets.data[asset_index]; // asset_id + side
            let asset_id = get_asset_id(asset);

            let inventory_asset = self.assets.getter(U128::from(asset_id));

            inventory_assets
                .data
                .push(inventory_asset.asset.get().to::<u128>());
            inventory_positions
                .data
                .push(Amount::from_u128(inventory_asset.position.get()));
            inventory_prices
                .data
                .push(Amount::from_u128(inventory_asset.price.get()));
            inventory_liquidity
                .data
                .push(Amount::from_u128(inventory_asset.liquidity.get()));
            inventory_slopes
                .data
                .push(Amount::from_u128(inventory_asset.slope.get()));
        }

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

        let mut executed_prices = Vector::new();
        let mut executed_quantities = Vector::new();

        executed_prices
            .data
            .resize(order_assets.data.len(), Amount::ZERO);

        executed_quantities
            .data
            .resize(order_assets.data.len(), Amount::ZERO);

        for order_index in 0..order_assets.data.len() {
            let order_asset = order_assets.data[order_index];
            let order_quantity = order_quantities.data[order_index];

            let order_asset_id = get_asset_id(order_asset);
            let order_side = get_side(order_asset);

            let mut inventory_asset = self.assets.setter(U128::from(order_asset_id));

            let asset = inventory_asset.asset.get().to::<u128>();

            // compute excuted price using volume weighted approximation
            let price = Amount::from_u128(inventory_asset.price.get());
            let slope = Amount::from_u128(inventory_asset.slope.get());
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
            let inventory_position = Amount::from_u128(inventory_asset.position.get());
            let inventory_side = get_side(asset);

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

            inventory_asset
                .position
                .set(new_inventory_position.to_u128());
            inventory_asset
                .asset
                .set(U128::from(make_asset(order_asset_id, new_inventory_side)));
        }

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
        assets_bytes: Vec<u128>,
        positions_bytes: Vec<u128>,
        prices_bytes: Vec<u128>,
        liquidity_bytes: Vec<u128>,
        slopes_bytes: Vec<u128>,
    ) -> Result<(), Vec<u8>> {
        let supplier = self.vm().tx_origin();
        let mut inventory = self.inventory.setter(supplier);
        if !inventory.is_active() {
            Err(b"No such supplier")?;
        }
        let assets = Labels::from_vec_u128(assets_bytes);
        let positions = Vector::from_vec_u128(positions_bytes);
        let prices = Vector::from_vec_u128(prices_bytes);
        let liquidity = Vector::from_vec_u128(liquidity_bytes);
        let slopes = Vector::from_vec_u128(slopes_bytes);
        inventory.submit(assets, positions, prices, liquidity, slopes)?;
        log(self.vm(), NewInventory { supplier });
        Ok(())
    }

    pub fn get_inventory(
        &self,
        supplier: Address,
        assets_bytes: Vec<u128>,
    ) -> Result<(Vec<u128>, Vec<u128>, Vec<u128>, Vec<u128>), Vec<u8>> {
        let inventory = self.inventory.getter(supplier);
        if !inventory.is_active() {
            Err(b"No such supplier")?;
        }
        let assets = Labels::from_vec_u128(assets_bytes);
        let (positions, prices, liquidity, slopes) = inventory.get_inventory(assets)?;
        Ok((
            positions.to_vec_u128(),
            prices.to_vec_u128(),
            liquidity.to_vec_u128(),
            slopes.to_vec_u128(),
        ))
    }

    pub fn match_inventory(
        &mut self,
        supplier: Address,
        order_id: U256,
        order_type: u8,
        assets_bytes: Vec<u128>,
        quantities_bytes: Vec<u128>,
    ) -> Result<(Vec<u128>, Vec<u128>), Vec<u8>> {
        let mut inventory = self.inventory.setter(supplier);
        if !inventory.is_active() {
            Err(b"Supplier not active")?;
        }

        let assets = Labels::from_vec_u128(assets_bytes);
        let quantities = Vector::from_vec_u128(quantities_bytes);

        let (executed_prices, executed_quantities) =
            inventory.match_inventory(order_type, assets, quantities)?;

        log(self.vm(), InventoryMatched { supplier, order_id });
        Ok((
            executed_prices.to_vec_u128(),
            executed_quantities.to_vec_u128(),
        ))
    }
}
