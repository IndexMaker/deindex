use alloc::vec::Vec;

use alloy_primitives::{ruint::UintTryTo, U128, U256};

#[cfg(feature = "with-ethers")]
use ethers::types::U256 as EthersU256;

use crate::uint;

#[inline]
fn try_convert_to_u128(value: U256) -> Option<u128> {
    value.uint_try_to().ok()
}

#[inline]
fn convert_to_u128(value: U128) -> u128 {
    value.to::<u128>()
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Amount(pub u128);

impl Amount {
    pub const ZERO: Amount = Amount(0);
    pub const ONE: Amount = Amount(Self::SCALE);
    pub const SCALE: u128 = 1_000_000_000__000_000_000;
    pub const DECIMALS: usize = 18;

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        let result = U256::from(self.0) + U256::from(rhs.0);
        Some(Self(try_convert_to_u128(result)?))
    }

    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        let result = U256::from(self.0) - U256::from(rhs.0);
        Some(Self(try_convert_to_u128(result)?))
    }

    pub fn checked_mul(self, rhs: Self) -> Option<Self> {
        let result = U256::from(self.0) * U256::from(rhs.0) / U256::from(Self::SCALE);
        Some(Self(try_convert_to_u128(result)?))
    }

    pub fn checked_div(self, rhs: Self) -> Option<Self> {
        let result = U256::from(self.0) * U256::from(Self::SCALE) / U256::from(rhs.0);
        Some(Self(try_convert_to_u128(result)?))
    }

    pub fn is_less_than(&self, other: &Self) -> bool {
        self.0 < other.0
    }

    pub fn from_u128_with_scale(value: u128, scale: u8) -> Self {
        let result =
            U256::from(value) * U256::from(Self::SCALE) / U256::from(10).pow(U256::from(scale));
        Self(try_convert_to_u128(result).unwrap())
    }

    pub fn from_slice(slice: &[u8]) -> Self {
        Self(uint::read_u128(slice))
    }

    pub fn to_vec(&self, output: &mut Vec<u8>) {
        uint::write_u128(self.0, output);
    }

    pub fn from_u128_raw(value: u128) -> Self {
        Self(value)
    }

    pub fn to_u128_raw(&self) -> u128 {
        self.0
    }

    pub fn from_u128(value: U128) -> Self {
        Self(convert_to_u128(value))
    }

    pub fn to_u128(&self) -> U128 {
        U128::from(self.0)
    }

    pub fn try_from_u256(value: U256) -> Option<Self> {
        Some(Self(try_convert_to_u128(value)?))
    }

    pub fn to_u256(&self) -> U256 {
        U256::from(self.0)
    }

    #[cfg(feature = "with-ethers")]
    pub fn try_from_u256_ethers(value: EthersU256) -> Option<Self> {
        Some(Self(value.try_into().ok()?))
    }

    #[cfg(feature = "with-ethers")]
    pub fn to_u256_ethers(&self) -> EthersU256 {
        EthersU256::from(self.0)
    }
}

#[cfg(any(not(feature = "stylus"), feature = "debug"))]
impl core::fmt::Display for Amount {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        #[cfg(feature = "stylus")]
        use alloc::format;

        let big_value = U256::from(self.0);
        let big_scale = U256::from(Self::SCALE);

        let integral = big_value / big_scale;
        let fraction = big_value % big_scale;

        let max_scale_len = Amount::DECIMALS;
        let frac_str = format!(
            "{:0>max_scale_len$}",
            fraction,
            max_scale_len = max_scale_len
        );

        let requested_precision = f.precision();
        let final_frac_str = match requested_precision {
            Some(p) => {
                let len = p.min(max_scale_len);
                &frac_str[0..len]
            }

            None => {
                let trimmed = frac_str.trim_end_matches('0');
                if trimmed.is_empty() {
                    "0"
                } else {
                    trimmed
                }
            }
        };

        write!(f, "{}.{}", integral, final_frac_str)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn do_test_amount(lhs: Amount, rhs: Amount) {
        assert_eq!(lhs.0, rhs.0);
    }

    #[test]
    fn test_amount() {
        do_test_amount(Amount::from_u128_with_scale(1_00, 2), Amount::ONE);
        do_test_amount(Amount::from_u128_with_scale(1_000_000, 6), Amount::ONE);

        do_test_amount(
            Amount::from_u128_with_scale(1, 6),
            Amount::from_u128_with_scale(1_000, 9),
        );
        do_test_amount(
            Amount::from_u128_with_scale(1, 15),
            Amount::from_u128_with_scale(1_000, Amount::DECIMALS as u8),
        );

        do_test_amount(
            Amount::from_u128_with_scale(1_50, 2)
                .checked_add(Amount::from_u128_with_scale(2, 0))
                .unwrap(),
            Amount::from_u128_with_scale(3_5, 1),
        );

        do_test_amount(
            Amount::from_u128_with_scale(3, 0)
                .checked_sub(Amount::from_u128_with_scale(0_5, 1))
                .unwrap(),
            Amount::from_u128_with_scale(2_5, 1),
        );

        do_test_amount(
            Amount::from_u128_with_scale(3, 0)
                .checked_sub(Amount::from_u128_with_scale(3_0, 1))
                .unwrap(),
            Amount::ZERO,
        );

        do_test_amount(
            Amount::from_u128_with_scale(1_50, 2)
                .checked_mul(Amount::from_u128_with_scale(2, 0))
                .unwrap(),
            Amount::from_u128_with_scale(3_0, 1),
        );

        do_test_amount(
            Amount::from_u128_with_scale(1_50, 2)
                .checked_mul(Amount::from_u128_with_scale(0_500, 3))
                .unwrap(),
            Amount::from_u128_with_scale(0_75, 2),
        );

        do_test_amount(
            Amount::from_u128_with_scale(3_0, 1)
                .checked_div(Amount::from_u128_with_scale(1_50, 2))
                .unwrap(),
            Amount::from_u128_with_scale(2, 0),
        );

        assert!(
            Amount::from_u128_with_scale(1, 0).is_less_than(&Amount::from_u128_with_scale(2, 0))
        );
        assert!(
            Amount::from_u128_with_scale(2, 1).is_less_than(&Amount::from_u128_with_scale(1, 0))
        );
    }
}
