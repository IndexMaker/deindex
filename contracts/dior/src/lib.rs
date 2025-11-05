// Allow `cargo stylus export-abi` to generate a main function.
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use alloy_primitives::{Address, U128, U256};
use alloy_sol_types::sol;
use deli::amount::Amount;
use stylus_sdk::{
    prelude::*,
    storage::{StorageAddress, StorageBool, StorageMap, StorageU128},
};

sol! {
    event NewIndexOrder(address sender);

    event EngageIndexOrder(
        address addressee,
        uint128 engaged_amount);

    event FillIndexOrder(
        address addressee,
        uint128 collateral_amount,
        uint128 quantity_filled);
}

#[storage]
pub struct IndexOrder {
    /// Total amount of collateral user sent us
    collateral_amount: StorageU128,

    /// Total amount of collateral converted to index tokens
    collateral_spent: StorageU128,

    /// Current amount of collateral in-progress
    collateral_engaged: StorageU128,

    /// Total amount of index filled
    quantity_filled: StorageU128,
}

impl IndexOrder {
    fn submit_new(&mut self, collateral_amount: Amount) {
        let current_amount = Amount::from_u128(self.collateral_amount.get());
        let new_amount = current_amount
            .checked_add(collateral_amount)
            .expect("Math overflow");
        self.collateral_amount.set(new_amount.to_u128());
    }

    fn engage(&mut self, collateral_amount: Amount) -> Amount {
        let engaged = Amount::from_u128(self.collateral_engaged.get());
        let remaining = self.get_remaining_amount();
        let new_engagement = collateral_amount.min(remaining);
        let new_engaged = engaged.checked_add(new_engagement).expect("Math overflow");
        self.collateral_engaged.set(new_engaged.to_u128());
        new_engagement
    }

    fn fill(&mut self, collateral_amount: Amount, quantity_filled: Amount) {
        let spent = Amount::from_u128(self.collateral_spent.get());
        let engaged = Amount::from_u128(self.collateral_engaged.get());
        let total_quantity_filled = Amount::from_u128(self.quantity_filled.get());
        let new_engaged = engaged
            .checked_sub(collateral_amount)
            .expect("Math underflow");
        let new_spent = spent.checked_add(collateral_amount).expect("Math overflow");
        let new_total_quantity_filled = total_quantity_filled
            .checked_add(quantity_filled)
            .expect("Math overflow");
        self.collateral_engaged.set(new_engaged.to_u128());
        self.collateral_spent.set(new_spent.to_u128());
        self.quantity_filled
            .set(new_total_quantity_filled.to_u128());
    }

    fn get_total_amount(&self) -> Amount {
        Amount::from_u128(self.collateral_amount.get())
    }

    fn get_spent_amount(&self) -> Amount {
        Amount::from_u128(self.collateral_spent.get())
    }

    fn get_engaged_amount(&self) -> Amount {
        Amount::from_u128(self.collateral_engaged.get())
    }

    fn get_remaining_amount(&self) -> Amount {
        let total_amount = self.get_total_amount();
        let spent_amount = self.get_spent_amount();
        let engaged_amount = self.get_engaged_amount();
        let remaining_amount = total_amount
            .checked_sub(
                spent_amount
                    .checked_add(engaged_amount)
                    .expect("Math overflow"),
            )
            .expect("Math underflow");
        remaining_amount
    }
}

#[storage]
pub struct IndexOrderBook {
    active: StorageBool,
    owner: StorageAddress,
    orders: StorageMap<Address, IndexOrder>,
}

impl IndexOrderBook {
    pub fn init(
        &mut self,
        owner: Address,
    ) -> Result<(), Vec<u8>> {
        self.active.set(true);
        self.owner.set(owner);
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active.get()
    }

    fn is_owner(&self, address: Address) -> bool {
        self.owner.get() == address
    }

    fn submit_order(&mut self, sender: Address, collateral_amount: Amount) {
        let mut order = self.orders.setter(sender);
        order.submit_new(collateral_amount);
    }

    fn engage_order(&mut self, sender: Address, collateral_amount: Amount) -> Amount {
        let mut order = self.orders.setter(sender);
        order.engage(collateral_amount)
    }

    fn fill_order(&mut self, sender: Address, collateral_amount: Amount, quantity_filled: Amount) {
        let mut order = self.orders.setter(sender);
        order.fill(collateral_amount, quantity_filled);
    }

    fn get_order(&self, sender: Address) -> Amount {
        let order = self.orders.getter(sender);
        order.get_remaining_amount()
    }
}

#[storage]
#[entrypoint]
pub struct Dior {
    indexes: StorageMap<U256, IndexOrderBook>,
}

#[public]
impl Dior {
    pub fn create_order_book(
        &mut self,
        index_id: U256,
    ) -> Result<(), Vec<u8>> {
        let sender = self.vm().tx_origin();
        let mut index = self.indexes.setter(index_id);
        if index.is_active() {
            Err(b"Index already exists")?;
        }
        index.init(sender)?;
        Ok(())
    }

    pub fn submit_order(&mut self, index_id: U256, collateral_amount: U128) -> Result<(), Vec<u8>> {
        let sender = self.vm().tx_origin();
        let mut index = self.indexes.setter(index_id);
        if !index.is_active() {
            Err(b"No such index")?;
        }
        index.submit_order(sender, Amount::from_u128(collateral_amount));
        log(self.vm(), NewIndexOrder { sender });
        Ok(())
    }

    pub fn engage_order(
        &mut self,
        index_id: U256,
        addressee: Address,
        collateral_amount: U128,
    ) -> Result<(), Vec<u8>> {
        let sender = self.vm().tx_origin();
        let mut index = self.indexes.setter(index_id);
        if !index.is_active() {
            Err(b"No such index")?;
        }
        if !index.is_owner(sender) {
            Err(b"Unauthorised access")?;
        }
        let engaged_amount = index.engage_order(addressee, Amount::from_u128(collateral_amount));
        log(
            self.vm(),
            EngageIndexOrder {
                addressee,
                engaged_amount: engaged_amount.to_u128_raw(),
            },
        );
        Ok(())
    }

    pub fn fill_order(
        &mut self,
        index_id: U256,
        addressee: Address,
        collateral_amount: U128,
        quantity_filled: U128,
    ) -> Result<(), Vec<u8>> {
        let sender = self.vm().tx_origin();
        let mut index = self.indexes.setter(index_id);
        if !index.is_active() {
            Err(b"No such index")?;
        }
        if !index.is_owner(sender) {
            Err(b"Unauthorised access")?;
        }
        index.fill_order(
            addressee,
            Amount::from_u128(collateral_amount),
            Amount::from_u128(quantity_filled),
        );
        log(
            self.vm(),
            FillIndexOrder {
                addressee,
                collateral_amount: collateral_amount.to::<u128>(),
                quantity_filled: quantity_filled.to::<u128>(),
            },
        );
        Ok(())
    }

    pub fn get_orders(&self, index_id: U256, users: Vec<Address>) -> Result<Vec<u8>, Vec<u8>> {
        let index = self.indexes.getter(index_id);
        if !index.is_active() {
            Err(b"No such index")?;
        }
        let mut output = Vec::new();
        for user in users {
            let order = index.get_order(user);
            order.to_vec(&mut output);
        }
        Ok(output)
    }
}

#[cfg(test)]
mod test {

    use alloy_primitives::address;
    use alloy_sol_types::SolEvent;
    use deli::{amount::Amount, log_msg, vector::Vector};

    use super::*;

    const ADMIN: Address = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
    const USER_1: Address = address!("0x70997970C51812dc3A010C7d01b50e0d17dc79C8");
    const USER_2: Address = address!("0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC");
    const SOLVER: Address = address!("0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65");

    #[test]
    fn test_dior() {
        use stylus_sdk::testing::*;
        let vm = TestVM::default();
        let mut contract = Dior::from(&vm);

        vm.set_sender(ADMIN);

        log_msg!("\ncreating order book...");
        log_msg!("assets: \n\t{}", assets);
        log_msg!("weights: \n\t{:1.3}", weights);

        let index_id = U256::from(1001);
        contract
            .create_order_book(index_id)
            .unwrap();

        log_msg!("\nuser_1 submits order");
        vm.set_sender(USER_1);
        contract
            .submit_order(index_id, Amount::from_u128_with_scale(150_00, 2).to_u128())
            .unwrap();

        log_msg!("\nuser_2 submits order");
        vm.set_sender(USER_2);
        contract
            .submit_order(index_id, Amount::from_u128_with_scale(150_00, 2).to_u128())
            .unwrap();

        log_msg!("\nsolver collecting events...");
        vm.set_sender(SOLVER);
        let emitted_logs = vm.get_emitted_logs();

        let users: Vec<_> = emitted_logs
            .iter()
            .filter_map(|(topics, data)| {
                let order = NewIndexOrder::decode_raw_log(topics, data, true);
                order.ok()
            })
            .map(|order| order.sender)
            .collect();

        log_msg!("\norders:");
        let orders = contract.get_orders(index_id, users.clone()).unwrap();
        let orders = Vector::from_vec(orders);
        for i in 0..orders.data.len() {
            let order = orders.data[i];
            log_msg!("\tfrom [{}]: {}", users[i], order);
            let _ = order;
        }
    }
}
