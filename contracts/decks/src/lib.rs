// Allow `cargo stylus export-abi` to generate a main function.
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use alloy_primitives::{Address, U128, U256};
use alloy_sol_types::sol;
use deli::{amount::Amount, asset::*, labels::Labels, math::solve_quadratic};
use stylus_sdk::{
    prelude::*,
    storage::{StorageAddress, StorageBool, StorageBytes, StorageMap, StorageU128},
};

sol! {
    event NewIndex(uint256 index_id);
}


#[storage]
pub struct Index {
    active: StorageBool,
    owner: StorageAddress,
    assets: StorageBytes,
    weights: StorageBytes,
    capacity: StorageU128,
    price: StorageU128,
    slope: StorageU128,
}

impl Index {
    pub fn init(
        &mut self,
        owner: Address,
        assets_bytes: Vec<u8>,
        weights_bytes: Vec<u8>,
    ) -> Result<(), Vec<u8>> {
        let assets = Labels::from_vec(assets_bytes);
        if !assets.data.is_sorted_by_key(|x| get_asset_id(*x)) {
            Err(b"Assets must be sorted")?;
        }

        self.active.set(true);
        self.owner.set(owner);
        self.assets.set_bytes(assets.to_vec());
        self.weights.set_bytes(weights_bytes);

        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active.get()
    }

    fn is_owner(&self, address: Address) -> bool {
        self.owner.get() == address
    }

    fn submit_quote(&mut self, capacity: Amount, price: Amount, slope: Amount) {
        self.capacity.set(capacity.to_u128());
        self.price.set(price.to_u128());
        self.slope.set(slope.to_u128());
    }

    fn get_capacity(&self) -> Amount {
        Amount::from_u128(self.capacity.get())
    }

    fn get_price(&self) -> Amount {
        Amount::from_u128(self.price.get())
    }

    fn get_slope(&self) -> Amount {
        Amount::from_u128(self.slope.get())
    }

    /// Tell quantity of index possible to obtain for given amount of collateral
    /// and the amount of collateral that would be used to obtain such quantity.
    /// Note that possible quantity is capped by index capacit, i.e. assets in
    /// stock and market liquidity.
    fn get_quote(&self, collateral_amount: Amount) -> Result<(Amount, Amount), Vec<u8>> {
        let capacity = self.get_capacity();
        let price = self.get_price();
        let slope = self.get_slope();

        // given:
        //  C : given collateral
        //  P : index price
        //  S : index price slope
        //
        // the formula for the quantity possible for given collateral
        //  Q = C / (P + S * Q)
        //
        // we can derive quadratic equation:
        //  Q * (P + S * Q) = C
        //  Q * (P + S * Q) - C = 0
        //  Q * P + Q * S * Q - C = 0
        //
        // this is quadratic equation:
        //  S * Q^2 + P * Q - C = 0
        //
        // where:
        //  A = S : slope
        //  B = P : index price
        //  C = C : collateral amount
        //
        // solution:
        //  Q = (-B + sqrt(B^2 + 4 * A * C)) / (2 * A)
        //

        let quote = solve_quadratic(slope, price, collateral_amount)
            .ok_or_else(|| b"Failed to solve quadratic price equation")?;

        if capacity.is_less_than(&quote) {
            let slippage = slope
                .checked_mul(capacity)
                .ok_or_else(|| b"Failed to compute slippage")?;

            let effective_price = price
                .checked_add(slippage)
                .ok_or_else(|| b"Failed to compute effective price")?;

            let capped_collateral = capacity
                .checked_mul(effective_price)
                .ok_or_else(|| b"Failed to capped collateral")?;

            Ok((capacity, capped_collateral))
        } else {
            Ok((quote, collateral_amount))
        }
    }
}

#[storage]
#[entrypoint]
pub struct Decks {
    indexes: StorageMap<U256, Index>,
}

#[public]
impl Decks {
    pub fn create_index(
        &mut self,
        index_id: U256,
        assets: Vec<u8>,
        weights: Vec<u8>,
    ) -> Result<(), Vec<u8>> {
        let sender = self.vm().tx_origin();
        let mut index = self.indexes.setter(index_id);
        if index.is_active() {
            Err(b"Index already exists")?;
        }
        index.init(sender, assets, weights)?;
        log(self.vm(), NewIndex { index_id });
        Ok(())
    }

    pub fn submit_index_quote(
        &mut self,
        index_id: U256,
        capacity: U128,
        price: U128,
        slope: U128,
    ) -> Result<(), Vec<u8>> {
        let sender = self.vm().tx_origin();
        let mut index = self.indexes.setter(index_id);
        if !index.is_active() {
            Err(b"No such index")?;
        }
        if !index.is_owner(sender) {
            Err(b"Unauthorised access")?;
        }
        index.submit_quote(
            Amount::from_u128(capacity),
            Amount::from_u128(price),
            Amount::from_u128(slope),
        );
        Ok(())
    }

    pub fn get_quote(
        &self,
        index_id: U256,
        collateral_amount: U128,
    ) -> Result<(U128, U128), Vec<u8>> {
        let index = self.indexes.getter(index_id);
        if !index.is_active() {
            Err(b"No such index")?;
        }
        let collateral_amount = Amount::from_u128(collateral_amount);
        let (quote, collateral_used) = index.get_quote(collateral_amount)?;
        Ok((quote.to_u128(), collateral_used.to_u128()))
    }
}

#[cfg(test)]
mod test {

}
