use alloc::vec::Vec;

use crate::amount::Amount;

pub struct Vector {
    pub data: Vec<Amount>,
}

impl Vector {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    #[cfg(feature = "vec-u128")]
    pub fn from_vec_u128(data: Vec<u128>) -> Self {
        let mut this = Self::new();
        let len = data.len();
        this.data.reserve(len);
        for val in data {
            this.data.push(Amount::from_u128_raw(val));
        }
        this
    }

    #[cfg(feature = "vec-u128")]
    pub fn to_vec_u128(&self) -> Vec<u128> {
        let mut res = Vec::new();
        for val in &self.data {
            res.push(val.to_u128_raw());
        }
        res
    }

    #[cfg(feature = "vec-u8")]
    pub fn from_vec(data: Vec<u8>) -> Self {
        let mut this = Self::new();
        let len = data.len();

        if 0 != len % size_of::<Amount>() {
            panic!("Incorrect data length");
        };

        let vec_len = len / size_of::<Amount>();
        let mut data_offet = 0;

        for _ in 0..vec_len {
            let data_start = data_offet;
            data_offet += size_of::<Amount>();
            let val = Amount::from_slice(&data[data_start..data_offet]);
            this.data.push(val);
        }
        this
    }

    #[cfg(feature = "vec-u8")]
    pub fn to_vec(&self) -> Vec<u8> {
        let mut output = Vec::new();
        for val in &self.data {
            val.to_vec(&mut output);
        }
        output
    }
}

#[cfg(any(not(feature = "stylus"), feature = "debug"))]
impl core::fmt::Display for Vector {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let max_scale_len = f.precision().unwrap_or(18).min(18);
        let n_cols_option = f.width();
        let len = self.data.len();

        match n_cols_option {
            Some(n_cols) if n_cols > 0 && len % n_cols == 0 => {
                // --- MATRIX MODE ---
                for (i, x) in self.data.iter().enumerate() {
                    write!(f, "{:0.max_scale_len$}", x, max_scale_len = max_scale_len)?;

                    let is_last_element = i == len - 1;
                    let is_end_of_row = (i + 1) % n_cols == 0;

                    if !is_last_element {
                        if is_end_of_row {
                            write!(f, "\n\t")?;
                        } else {
                            write!(f, " ")?;
                        }
                    }
                }
                Ok(())
            }
            _ => {
                // --- LIST MODE ---
                let separator = if f.alternate() { "\n\t" } else { "," };
                let mut sepa = "";

                for x in &self.data {
                    write!(
                        f,
                        "{}{:0.max_scale_len$}",
                        sepa,
                        x,
                        max_scale_len = max_scale_len
                    )?;
                    sepa = separator;
                }
                Ok(())
            }
        }
    }
}
