// Allow `cargo stylus export-abi` to generate a main function.
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use alloy_primitives::{Address, U128};
use deli::{amount::Amount, labels::Labels, vector::Vector};
use stylus_sdk::{
    prelude::*,
    storage::{StorageAddress, StorageBytes, StorageMap, StorageU128},
};

use crate::program::{ErrorCode, Program, VectorIO};

pub mod program;

#[storage]
#[entrypoint]
pub struct Devil {
    owner: StorageAddress,
    scalars: StorageMap<U128, StorageU128>,
    vectors: StorageMap<U128, StorageBytes>,
}

impl Devil {
    fn check_owner(&self) -> Result<(), Vec<u8>> {
        if self.vm().msg_sender() != self.owner.get() {
            Err(b"Must be owner")?;
        }
        Ok(())
    }
}

impl VectorIO for Devil {
    fn load_labels(&self, id: U128) -> Result<Labels, ErrorCode> {
        let vector = self.vectors.getter(id);
        if vector.is_empty() {
            Err(ErrorCode::NotFound)?;
        }
        Ok(Labels::from_vec(vector.get_bytes()))
    }

    fn load_vector(&self, id: U128) -> Result<Vector, ErrorCode> {
        let vector = self.vectors.getter(id);
        if vector.is_empty() {
            Err(ErrorCode::NotFound)?;
        }
        Ok(Vector::from_vec(vector.get_bytes()))
    }

    fn load_scalar(&self, id: U128) -> Result<Amount, ErrorCode> {
        let scalar = self.scalars.getter(id);
        Ok(Amount::from_u128(scalar.get()))
    }

    fn store_labels(&mut self, id: U128, input: Labels) -> Result<(), ErrorCode> {
        let mut vector = self.vectors.setter(id);
        vector.set_bytes(input.to_vec());
        Ok(())
    }

    fn store_vector(&mut self, id: U128, input: Vector) -> Result<(), ErrorCode> {
        let mut vector = self.vectors.setter(id);
        vector.set_bytes(input.to_vec());
        Ok(())
    }

    fn store_scalar(&mut self, id: U128, input: Amount) -> Result<(), ErrorCode> {
        let mut scalar = self.scalars.setter(id);
        scalar.set(input.to_u128());
        Ok(())
    }
}

#[public]
impl Devil {
    pub fn set_owner(&mut self, owner: Address)  -> Result<(), Vec<u8>> {
        // Note it's cheaper in terms of KiB to not use contructor
        let current_owner = self.owner.get();
        if !current_owner.is_zero() {
            Err(b"Cannot change owner")?;
        }
        self.owner.set(owner);
        Ok(())
    }

    pub fn submit(&mut self, id: U128, data: Vec<u8>) -> Result<(), Vec<u8>> {
        self.check_owner()?;
        // Note it's cheaper in terms of KiB to limit public interface
        let mut vector = self.vectors.setter(id);
        if !vector.is_empty() {
            Err(b"Duplicate data")?;
        }
        vector.set_bytes(data);
        Ok(())
    }

    pub fn get(&self, id: U128) -> Result<Vec<u8>, Vec<u8>> {
        self.check_owner()?;
        let vector = self.vectors.getter(id);
        if !vector.is_empty() {
            Err(b"No data")?;
        }
        Ok(vector.get_bytes())
    }

    pub fn execute(&mut self, code: Vec<u8>) -> Result<(), Vec<u8>> {
        self.check_owner()?;
        let mut program = Program::new(self);
        program.execute(code).map_err(|_| b"Program error")?;
        Ok(())
    }
}

#[cfg(test)]
mod test {}
