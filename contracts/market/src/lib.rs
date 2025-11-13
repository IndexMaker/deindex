// Allow `cargo stylus export-abi` to generate a main function.
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use alloy_primitives::{Address, U128};
use alloy_sol_types::sol;
use stylus_sdk::{
    prelude::*,
    storage::{StorageAddress, StorageU128},
};

sol! {
    event SomeEvent(address sender);

}

#[storage]
#[entrypoint]
pub struct Market {
    owner: StorageAddress,
    devil: StorageAddress,
    supply_long: StorageU128,  // Vector = [+Supply; num_assets]
    supply_short: StorageU128, // Vector = [-Supply; num_assets]
    demand_long: StorageU128,  // Vector = [+Demand; num_assets]
    demand_short: StorageU128, // Vector = [-Demand; num_assets]
    delta_long: StorageU128,   // Vector = [+Delta; num_assets]
    delta_short: StorageU128,  // Vector = [-Delta; num_assets]
    liquidity: StorageU128,    // Vector = [Liquidity; num_assets]
    prices: StorageU128,       // Vector = [Price; num_assets]
    slopes: StorageU128,       // Vector = [Slope; num_assets]
}

impl Market {
    fn check_owner(&self, address: Address) -> Result<(), Vec<u8>> {
        let current_owner = self.owner.get();
        if !current_owner.is_zero() && address != current_owner {
            Err(b"Mut be owner")?;
        }
        Ok(())
    }
}

#[public]
impl Market {
    pub fn setup(&mut self, owner: Address, devil: Address) -> Result<(), Vec<u8>> {
        self.check_owner(self.vm().msg_sender())?;
        self.owner.set(owner);
        self.devil.set(devil);
        Ok(())
    }

    pub fn submit_supply(&mut self) -> Result<(), Vec<u8>> {
        self.check_owner(self.vm().msg_sender())?;
        todo!()
    }

    pub fn get_supply(&self) -> (U128, U128) {
        (self.supply_long.get(), self.supply_short.get())
    }

    pub fn get_demand(&self) -> (U128, U128) {
        (self.demand_long.get(), self.demand_short.get())
    }

    pub fn get_delta(&self) -> (U128, U128) {
        (self.delta_long.get(), self.delta_short.get())
    }

    pub fn get_liquidity(&self) -> U128 {
        self.liquidity.get()
    }

    pub fn get_prices(&self) -> U128 {
        self.prices.get()
    }

    pub fn get_slopes(&self) -> U128 {
        self.slopes.get()
    }
}

#[cfg(test)]
mod test {}
