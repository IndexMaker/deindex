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
    storage::{StorageAddress, StorageMap, StorageU128},
};

sol! {
    event SomeEvent(address sender);

}

#[storage]
#[entrypoint]
pub struct Vault {
    owner: StorageAddress,
    orders: StorageMap<Address, StorageU128>, // Mapping = {User Address => Vector = [USDC Remaining, USDC Spent, ITP Minted]}
    queue: StorageU128,                       // Labels  = [u128; num_orders]
    assets: StorageU128,                      // Labels  = [u128; num_assets]
    weights: StorageU128,                     // Vector  = [Amount; num_assets]
    quote: StorageU128,                       // Vector  = [Capacity, Price, Slope]
}

impl Vault {
    fn check_owner(&self, address: Address) -> Result<(), Vec<u8>> {
        if address != self.owner.get() {
            Err(b"Mut be owner")?;
        }
        Ok(())
    }
}

#[public]
impl Vault {
    pub fn submit_order(&mut self, user: Address, collateral_amount: U128) -> Result<(), Vec<u8>> {
        self.check_owner(self.vm().msg_sender())?;
        let mut order_access = self.orders.setter(user);
        let order = order_access.get();
        if order == U128::ZERO {
            let _ = collateral_amount;
            let _ = &mut order_access;
            todo!("create new order");
        }
        else {
            todo!("update existing order");
        }
    }

    pub fn get_queue(&self) -> Result<U128, Vec<u8>> {
        self.check_owner(self.vm().msg_sender())?;
        Ok(self.queue.get())
    }

    pub fn get_assets(&self) -> Result<U128, Vec<u8>> {
        self.check_owner(self.vm().msg_sender())?;
        Ok(self.assets.get())
    }

    pub fn get_weights(&self) -> Result<U128, Vec<u8>> {
        self.check_owner(self.vm().msg_sender())?;
        Ok(self.weights.get())
    }
    
    pub fn get_quote(&self) -> Result<U128, Vec<u8>> {
        self.check_owner(self.vm().msg_sender())?;
        Ok(self.quote.get())
    }
}

#[cfg(test)]
mod test {}
