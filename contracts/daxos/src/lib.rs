// Allow `cargo stylus export-abi` to generate a main function.
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use alloy_primitives::U128;
use alloy_sol_types::sol;
use stylus_sdk::{
    prelude::*,
    storage::{StorageAddress, StorageMap},
};

sol! {
    interface IGateway  {
        function submitSupply() external;

        function getSupply() external view returns (uint128, uint128);

        function getDemand() external view returns (uint128, uint128);

        function getDelta() external view returns (uint128, uint128);

        function getLiquidity() external view returns (uint128);

        function getPrices() external view returns (uint128);

        function getSlopes() external view returns (uint128);
    }

    interface IVault  {
        function submitOrder(address user, uint128 collateral_amount) external;

        function getQueue() external view returns (uint128);

        function getAssets() external view returns (uint128);

        function getWeights() external view returns (uint128);

        function getQuote() external view returns (uint128);
    }

    event SomeEvent(address sender);

}

#[storage]
#[entrypoint]
pub struct Daxos {
    owner: StorageAddress,
    gateway: StorageAddress,
    vaults: StorageMap<U128, StorageAddress>,
}

#[public]
impl Daxos {
    pub fn submit_order(&mut self, index: U128, collateral_amount: U128) -> Result<(), Vec<u8>> {
        let vault_access = self.vaults.getter(index);
        let vault_address = vault_access.get();
        if vault_address.is_zero() {
            Err(b"Index does not exist")?;
        }
        let _ = collateral_amount;
        Ok(())
    }
}

#[cfg(test)]
mod test {}
