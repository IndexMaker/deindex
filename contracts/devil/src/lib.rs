// Allow `cargo stylus export-abi` to generate a main function.
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use alloy_primitives::{Address, U128};
use deli::{labels::Labels, vector::Vector};
use stylus_sdk::{
    prelude::*,
    storage::{StorageAddress, StorageBytes, StorageMap},
};

use crate::program::{ErrorCode, Program, VectorIO};

pub mod program;

#[cfg(test)]
pub mod test;

#[storage]
#[entrypoint]
pub struct Devil {
    owner: StorageAddress,
    vectors: StorageMap<U128, StorageBytes>,
}

impl Devil {
    fn check_owner(&self, address: Address) -> Result<(), Vec<u8>> {
        let current_owner = self.owner.get();
        if !current_owner.is_zero() && address != current_owner {
            Err(b"Mut be owner")?;
        }
        Ok(())
    }
}

impl VectorIO for Devil {
    fn load_labels(&self, id: u128) -> Result<Labels, ErrorCode> {
        let vector = self.vectors.getter(U128::from(id));
        if vector.is_empty() {
            Err(ErrorCode::NotFound)?;
        }
        Ok(Labels::from_vec(vector.get_bytes()))
    }

    fn load_vector(&self, id: u128) -> Result<Vector, ErrorCode> {
        let vector = self.vectors.getter(U128::from(id));
        if vector.is_empty() {
            Err(ErrorCode::NotFound)?;
        }
        Ok(Vector::from_vec(vector.get_bytes()))
    }

    fn store_labels(&mut self, id: u128, input: Labels) -> Result<(), ErrorCode> {
        let mut vector = self.vectors.setter(U128::from(id));
        vector.set_bytes(input.to_vec());
        Ok(())
    }

    fn store_vector(&mut self, id: u128, input: Vector) -> Result<(), ErrorCode> {
        let mut vector = self.vectors.setter(U128::from(id));
        vector.set_bytes(input.to_vec());
        Ok(())
    }
}

#[public]
impl Devil {
    pub fn setup(&mut self, owner: Address)  -> Result<(), Vec<u8>> {
        // Note it's cheaper in terms of KiB to not use contructor
        self.check_owner(self.vm().msg_sender())?;
        self.owner.set(owner);
        Ok(())
    }

    pub fn submit(&mut self, id: U128, data: Vec<u8>) -> Result<(), Vec<u8>> {
        self.check_owner(self.vm().msg_sender())?;
        // Note it's cheaper in terms of KiB to limit public interface
        let mut vector = self.vectors.setter(id);
        if !vector.is_empty() {
            Err(b"Duplicate data")?;
        }
        vector.set_bytes(data);
        Ok(())
    }

    pub fn get(&self, id: U128) -> Result<Vec<u8>, Vec<u8>> {
        self.check_owner(self.vm().msg_sender())?;
        let vector = self.vectors.getter(id);
        if !vector.is_empty() {
            Err(b"No data")?;
        }
        Ok(vector.get_bytes())
    }

    pub fn execute(&mut self, code: Vec<u8>, num_registry: u128) -> Result<(), Vec<u8>> {
        self.check_owner(self.vm().msg_sender())?;
        let mut program = Program::new(self);
        program.execute(code, num_registry as usize).map_err(|_| b"Program error")?;
        Ok(())
    }
}
