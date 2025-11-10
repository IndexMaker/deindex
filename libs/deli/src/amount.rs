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

#[inline]
fn convert_from_u128(value: u128) -> U256 {
    U256::from_limbs([value as u64, (value >> 64) as u64, 0, 0])
}

#[inline]
fn convert_from_u8(value: u8) -> U256 {
    U256::from_limbs([value as u64, 0, 0, 0])
}

/// Optimized integer square root for U256 using the Babylonian method.
/// This method is gas-efficient, relying on quadratic convergence.
/// It returns the floor of the square root.
#[cfg(feature = "amount-sqrt")]
fn sqrt_u256(n: U256) -> Option<U256> {
    if n.is_zero() {
        return Some(U256::ZERO);
    }

    // Newton method:
    //  x_{k+1} = x_k - f(x_k) / f'(x_k)
    //
    // And for square root we have:
    //  f(x) = x^2 - N
    //  f'(x) = 2x
    //  x_{k+1} = x_k - (x_k^2 - N) / (2 x_k) = ((N / x_k) + x_k) / 2
    //
    // This is also known as Babylonian method or Heron's method.
    //
    // Note the x_k series is monotonically decreasing (non-oscilating), i.e.
    // for every (next) k+1 it is always true that: x_{k+1} <= x_k
    //
    let two = convert_from_u128(2);
    let mut current = n;
    let mut next = U256::ONE.checked_add(n)?.checked_div(two)?;

    while next < current {
        current = next;
        next = n.checked_div(next)?.checked_add(next)?.checked_div(two)?;
    }
    Some(current)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Amount(pub u128);

impl Amount {
    pub const ZERO: Amount = Amount(0);
    pub const EPSILON: Amount = Amount(1);
    pub const MAX: Amount = Amount(u128::MAX);
    pub const ONE: Amount = Amount(Self::SCALE);
    pub const TWO: Amount = Amount(2 * Self::SCALE);
    pub const FOUR: Amount = Amount(4 * Self::SCALE);
    pub const SCALE: u128 = 1_000_000_000__000_000_000;
    pub const SCALE_SQRT: u128 = 1_000_000_000;
    pub const DECIMALS: usize = 18;

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        let result = self.to_u256() + rhs.to_u256();
        Some(Self(try_convert_to_u128(result)?))
    }

    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        let result = self.to_u256() - rhs.to_u256();
        Some(Self(try_convert_to_u128(result)?))
    }

    pub fn saturating_sub(self, rhs: Self) -> Option<Self> {
        let a = self.to_u256();
        let b = rhs.to_u256();
        let result = a - a.min(b);
        Some(Self(try_convert_to_u128(result)?))
    }

    pub fn checked_mul(self, rhs: Self) -> Option<Self> {
        let result = (self.to_u256() * rhs.to_u256()) / Self::u256_scale();
        Some(Self(try_convert_to_u128(result)?))
    }

    pub fn checked_div(self, rhs: Self) -> Option<Self> {
        let result = (self.to_u256() * Self::u256_scale()) / rhs.to_u256();
        Some(Self(try_convert_to_u128(result)?))
    }

    pub fn checked_sq(self) -> Option<Self> {
        let this = self.to_u256();
        let result = (this * this) / Self::u256_scale();
        Some(Self(try_convert_to_u128(result)?))
    }

    #[cfg(feature = "amount-sqrt")]
    pub fn checked_sqrt(self) -> Option<Self> {
        let result = sqrt_u256(self.to_u256())? * convert_from_u128(Self::SCALE_SQRT);
        Some(Self(try_convert_to_u128(result)?))
    }

    #[inline]
    pub fn is_less_than(&self, other: &Self) -> bool {
        self.0 < other.0
    }

    #[inline]
    pub fn min(&self, other: &Self) -> Self {
        if self.is_less_than(other) {
            *self
        } else {
            *other
        }
    }

    #[inline]
    pub fn is_not(&self) -> bool {
        // Note: we don't want to call this function is_zero(), because we're
        // representing decimal numbers, and we should compare against some
        // threshold and not against absolute 0. The function of is_not() is to
        // tell that amount is not set rather than having zero value.
        self.0 == 0
    }

    pub fn from_u128_with_scale(value: u128, scale: u8) -> Self {
        let result = convert_from_u128(value) * convert_from_u128(Self::SCALE)
            / convert_from_u128(10).pow(convert_from_u8(scale));
        Self(try_convert_to_u128(result).unwrap())
    }

    #[inline]
    pub fn from_slice(slice: &[u8]) -> Self {
        Self(uint::read_u128(slice))
    }

    #[inline]
    pub fn to_vec(&self, output: &mut Vec<u8>) {
        uint::write_u128(self.0, output);
    }

    #[inline]
    pub fn from_u128_raw(value: u128) -> Self {
        Self(value)
    }

    #[inline]
    pub fn to_u128_raw(&self) -> u128 {
        self.0
    }

    #[inline]
    pub fn from_u128(value: U128) -> Self {
        Self(convert_to_u128(value))
    }

    #[inline]
    pub fn to_u128(&self) -> U128 {
        U128::from(self.0)
    }

    #[inline]
    pub fn try_from_u256(value: U256) -> Option<Self> {
        Some(Self(try_convert_to_u128(value)?))
    }

    #[inline]
    pub fn to_u256(&self) -> U256 {
        convert_from_u128(self.0)
    }

    #[inline]
    pub fn u256_scale() -> U256 {
        convert_from_u128(Self::SCALE)
    }

    #[cfg(feature = "with-ethers")]
    #[inline]
    pub fn try_from_u256_ethers(value: EthersU256) -> Option<Self> {
        Some(Self(value.try_into().ok()?))
    }

    #[cfg(feature = "with-ethers")]
    #[inline]
    pub fn to_u256_ethers(&self) -> EthersU256 {
        EthersU256::from(self.0)
    }
}

#[cfg(any(not(feature = "stylus"), feature = "debug"))]
impl core::fmt::Display for Amount {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        #[cfg(feature = "stylus")]
        use alloc::format;

        let big_value = convert_from_u128(self.0);
        let big_scale = convert_from_u128(Self::SCALE);

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
