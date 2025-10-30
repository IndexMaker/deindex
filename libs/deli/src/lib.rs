#![cfg_attr(all(feature = "stylus", not(feature = "stylus-test")), no_std)]

//#[macro_use]
extern crate alloc;

pub mod amount;
pub mod asset;
pub mod labels;
pub mod log;
pub mod uint;
pub mod vector;
