// Allow `cargo stylus export-abi` to generate a main function.
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use alloy_primitives::{Address, U128};
use alloy_sol_types::{sol, SolCall};
use deli::labels::Labels;
use icore::vil::{execute_buy_order::execute_buy_order, update_supply::update_supply};
use stylus_sdk::{
    prelude::*,
    storage::{StorageAddress, StorageMap},
};

sol! {
    /// Vector IL (VIL) virtual machine
    ///
    /// Performs operations on vectors stored on-chain as opaque blobs.  By
    /// using dedicated VIL for vector processing we save on (de)serialisation
    /// of blobs and also on SLOAD/SSTORE operations, because we have all vector
    /// operations integrated with storage of vectors as the blobs, meaning that
    /// we can submit VIL program that will perform number of vector
    /// instructions on vectors using only one SLOAD for each vector load, and
    /// one SSTORE, as well as we don't need to SSTORE intermediate results as
    /// they are stored on internal stack of the virtual machine.
    interface IDevil  {
        function setup(address owner) external;

        function submit(uint128 id, uint8[] memory data) external;

        function get(uint128 id) external view returns (uint8[] memory);

        function execute(uint8[] memory code, uint128 num_registry) external;
    }

    /// Gateway monitors supply and demand for assets
    ///
    /// Vault orders update demand, while authorised provider updates supply.
    /// The delta monitors difference between suppy and demand, and is critical
    /// metric for:
    ///     a) authorised provider to know which assets to buy/sell
    ///     b) daxos to match new orders or halt (throttle order over time)
    ///
    /// All data is stored as vectors on DeVIL virtual machine, and Gateway
    /// itself only organises handles to those vectors and submits VIL programs
    /// to execute. The results of those programs executions stay on DeVIL, but
    /// can be accessed when required by calling Devil::get(vector_id) method.
    ///
    interface IGateway  {
        function setup(address owner, address devil) external;

        function submitSupply() external;

        function getSupply() external view returns (uint128, uint128);

        function getDemand() external view returns (uint128, uint128);

        function getDelta() external view returns (uint128, uint128);

        function getLiquidity() external view returns (uint128);

        function getPrices() external view returns (uint128);

        function getSlopes() external view returns (uint128);
    }

    /// Vault (a.k.a. Index) tracks its price and orders
    ///
    /// Vault stores:
    ///     - asset weights
    ///     - latest quote, which consists of: Capacity, Price, and Slope (Price
    ///     change with quantity)
    ///     - order queue
    ///
    /// All data is stored as vectors on DeVIL virtual machine, and Vault itself
    /// only organises handles to those vectors and submits VIL programs to
    /// execute.
    interface IVault  {
        function setup(address owner, address devil) external;

        function submitOrder(address user, uint128 collateral_amount) external;

        function getQueue() external view returns (uint128);

        function getAssets() external view returns (uint128);

        function getWeights() external view returns (uint128);

        function getQuote() external view returns (uint128);
    }
}

#[storage]
#[entrypoint]
pub struct Daxos {
    owner: StorageAddress,
    devil: StorageAddress,
    gateway: StorageAddress,
    vaults: StorageMap<U128, StorageAddress>,
}

impl Daxos {
    fn check_owner(&self, address: Address) -> Result<(), Vec<u8>> {
        let current_owner = self.owner.get();
        if !current_owner.is_zero() && address != current_owner {
            Err(b"Mut be owner")?;
        }
        Ok(())
    }

    fn send_to_devil(&mut self, code: Vec<u128>, num_registry: u128) -> Result<(), Vec<u8>> {
        let devil_call = IDevil::executeCall {
            code: Labels { data: code }.to_vec(),
            num_registry,
        };
        self.vm()
            .call(&self, self.devil.get(), &devil_call.abi_encode())?;
        Ok(())
    }
}

#[public]
impl Daxos {
    pub fn setup(
        &mut self,
        owner: Address,
        devil: Address,
        gateway: Address,
    ) -> Result<(), Vec<u8>> {
        self.check_owner(self.vm().tx_origin())?;
        self.owner.set(owner);
        self.devil.set(devil);
        self.gateway.set(gateway);
        Ok(())
    }

    /// Issuer has deployed Vault contract and now we need to set it up
    pub fn setup_vault(
        &mut self,
        vault_id: U128,
        vault_address: Address, /* ... setup params ...*/
    ) -> Result<(), Vec<u8>> {
        self.check_owner(self.vm().tx_origin())?;
        let mut vault_access = self.vaults.setter(vault_id);
        if !vault_access.get().is_zero() {
            Err(b"Duplicate Vault")?;
        }
        vault_access.set(vault_address);
        let me = self.vm().contract_address();
        let devil_address = self.devil.get();
        let vault_setup = IVault::setupCall {
            owner: me,
            devil: devil_address,
            /* ...setup params... */
        };
        self.vm()
            .call(&self, vault_address, &vault_setup.abi_encode())?;
        Ok(())
    }

    pub fn submit_order(&mut self, index: U128, collateral_amount: u128) -> Result<(), Vec<u8>> {
        let user = self.vm().msg_sender();
        let vault_access = self.vaults.getter(index);
        let vault_address = vault_access.get();
        if vault_address.is_zero() {
            Err(b"Vault Not Found")?;
        }
        let submit = IVault::submitOrderCall {
            user,
            collateral_amount,
        };
        self.vm().call(&self, vault_address, &submit.abi_encode())?;

        // TODO: We need to set these up. They are from Vault and Gateway.
        let index_order_id = 10001;
        let executed_asset_quantities_id = 10002;
        let executed_index_quantities_id = 10003;
        let asset_names_id = 1001;
        let weights_id = 1002;
        let quote_id = 1003;
        let inventory_asset_names_id = 101;
        let supply_long_id = 102;
        let supply_short_id = 103;
        let demand_long_id = 104;
        let demand_short_id = 105;
        let delta_long_id = 106;
        let delta_short_id = 107;
        let solve_quadratic_id = 10;

        // TODO: get those from Vault and Gateway
        let update = execute_buy_order(
            index_order_id,
            executed_index_quantities_id,
            executed_asset_quantities_id,
            asset_names_id,
            weights_id,
            quote_id,
            inventory_asset_names_id,
            supply_long_id,
            supply_short_id,
            demand_long_id,
            demand_short_id,
            delta_long_id,
            delta_short_id,
            solve_quadratic_id,
        );
        let num_registry = 16;
        self.send_to_devil(update, num_registry)?;
        Ok(())
    }

    pub fn submit_supply(&mut self) -> Result<(), Vec<u8>> {
        let gateway_address = self.gateway.get();
        let submit = IGateway::submitSupplyCall {};
        self.vm()
            .call(&self, gateway_address, &submit.abi_encode())?;

        let [inventory_asset_names_id, supply_long_id, supply_short_id, demand_long_id, demand_short_id, delta_long_id, delta_short_id] =
            [0; 7];

        // TODO: get those from Gateway
        let update = update_supply(
            inventory_asset_names_id,
            supply_long_id,
            supply_short_id,
            demand_long_id,
            demand_short_id,
            delta_long_id,
            delta_short_id,
        );
        let num_registry = 16;
        self.send_to_devil(update, num_registry)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {}
