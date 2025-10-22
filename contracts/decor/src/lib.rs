#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
#![cfg_attr(not(any(test, feature = "export-abi")), no_std)]

#[macro_use]
extern crate alloc;

use alloc::vec::Vec;

use stylus_sdk::prelude::*;

#[storage]
#[entrypoint]
pub struct Decor {}

#[public]
impl Decor {}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_disolver() {
        use stylus_sdk::testing::*;
        let vm = TestVM::default();
        let mut contract = Decor::from(&vm);
    }
}
