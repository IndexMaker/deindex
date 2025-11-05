// Allow `cargo stylus export-abi` to generate a main function.
#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use alloy_sol_types::sol;
use stylus_sdk::{
    prelude::*,
    storage::StorageAddress,
};

sol! {
    event SomeEvent(address sender);

}

#[storage]
#[entrypoint]
pub struct Dion {
    // TODO: inherit IERC20
    owner: StorageAddress
}

#[public]
impl Dion {
}

#[cfg(test)]
mod test {

}
