use alloc::vec::Vec;

use crate::uint::{read_u128, write_u128};

pub struct Labels {
    pub data: Vec<u128>,
}

impl Labels {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn from_vec(data: Vec<u8>) -> Self {
        let mut this = Self::new();
        let len = data.len();

        if 0 != len % size_of::<u128>() {
            panic!("Incorrect data length");
        };

        let vec_len = len / size_of::<u128>();
        let mut data_offet = 0;

        for _ in 0..vec_len {
            let data_start = data_offet;
            data_offet += size_of::<u128>();
            let val = read_u128(&data[data_start..data_offet]);
            this.data.push(val);
        }
        this
    }

    pub fn to_vec(&self) -> Vec<u8> {
        let mut output = Vec::new();
        for val in &self.data {
            write_u128(*val, &mut output);
        }
        output
    }
}

#[cfg(any(not(feature = "stylus"), feature = "debug"))]
impl core::fmt::Display for Labels {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut sepa = "";
        for x in &self.data {
            write!(f, "{}{}", sepa, x,)?;
            sepa = ",";
        }
        Ok(())
    }
}
