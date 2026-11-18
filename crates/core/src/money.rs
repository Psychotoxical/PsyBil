//! Money: amounts in whole cents, decimal input parsing and rounding.

use std::iter::Sum;
use std::ops::{Add, Neg, Sub};
use std::str::FromStr;

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Highest number of decimal places allowed for quantities and unit prices.
pub const MAX_INPUT_SCALE: u32 = 4;

/// Errors from parsing and calculating amounts.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CalcError {
    #[error("value is empty")]
    Empty,
    #[error("value is not a number")]
    Invalid,
    #[error("too many decimal places")]
    TooManyDecimals,
    #[error("amount is too large")]
    Overflow,
    #[error("tax rate must be between 0 and 100")]
    InvalidRate,
    #[error("total is negative")]
    NegativeTotal,
}

/// An amount of money in whole cents (or the smallest unit of the currency).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Cents(pub i64);

impl Cents {
    pub const ZERO: Cents = Cents(0);

    pub fn checked_add(self, other: Cents) -> Result<Cents, CalcError> {
        self.0
            .checked_add(other.0)
            .map(Cents)
            .ok_or(CalcError::Overflow)
    }

    pub fn checked_sub(self, other: Cents) -> Result<Cents, CalcError> {
        self.0
            .checked_sub(other.0)
            .map(Cents)
            .ok_or(CalcError::Overflow)
    }
}

impl Add for Cents {
    type Output = Cents;
    fn add(self, other: Cents) -> Cents {
        Cents(self.0 + other.0)
    }
}

impl Sub for Cents {
    type Output = Cents;
    fn sub(self, other: Cents) -> Cents {
        Cents(self.0 - other.0)
    }
}

impl Neg for Cents {
    type Output = Cents;
    fn neg(self) -> Cents {
        Cents(-self.0)
    }
}

impl Sum for Cents {
    fn sum<I: Iterator<Item = Cents>>(iter: I) -> Cents {
        iter.fold(Cents::ZERO, Add::add)
    }
}

/// Rounds a decimal amount to whole cents, half away from zero.
pub fn round_to_cents(value: Decimal) -> Result<Cents, CalcError> {
    let scaled = value
        .checked_mul(Decimal::ONE_HUNDRED)
        .ok_or(CalcError::Overflow)?;
    scaled
        .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero)
        .to_i64()
        .map(Cents)
        .ok_or(CalcError::Overflow)
}

/// Converts cents back to a decimal amount with two places.
pub fn cents_to_decimal(cents: Cents) -> Decimal {
    Decimal::new(cents.0, 2)
}

/// Parses user input such as "1,5" or "-2.25". Comma or dot is the decimal
/// separator; thousands separators are not accepted.
pub fn parse_decimal(input: &str) -> Result<Decimal, CalcError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CalcError::Empty);
    }
    if trimmed.contains(',') && trimmed.contains('.') {
        return Err(CalcError::Invalid);
    }
    let normalized = trimmed.replace(',', ".");
    let value = Decimal::from_str(&normalized).map_err(|_| CalcError::Invalid)?;
    let value = value.normalize();
    if value.scale() > MAX_INPUT_SCALE {
        return Err(CalcError::TooManyDecimals);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::dec;

    #[test]
    fn rounds_half_away_from_zero() {
        assert_eq!(round_to_cents(dec!(0.125)).unwrap(), Cents(13));
        assert_eq!(round_to_cents(dec!(-0.125)).unwrap(), Cents(-13));
        assert_eq!(round_to_cents(dec!(0.124)).unwrap(), Cents(12));
    }

    #[test]
    fn parses_comma_and_dot() {
        assert_eq!(parse_decimal(" 1,5 ").unwrap(), dec!(1.5));
        assert_eq!(parse_decimal("0").unwrap(), dec!(0));
        assert_eq!(parse_decimal("-2.25").unwrap(), dec!(-2.25));
        assert_eq!(parse_decimal("1,50000").unwrap(), dec!(1.5));
        assert_eq!(parse_decimal(""), Err(CalcError::Empty));
        assert_eq!(parse_decimal("   "), Err(CalcError::Empty));
        assert_eq!(parse_decimal("1.2.3"), Err(CalcError::Invalid));
        assert_eq!(parse_decimal("1.234,5"), Err(CalcError::Invalid));
        assert_eq!(parse_decimal("abc"), Err(CalcError::Invalid));
        assert_eq!(parse_decimal("1,23456"), Err(CalcError::TooManyDecimals));
    }

    #[test]
    fn overflow_is_an_error() {
        assert_eq!(round_to_cents(Decimal::MAX), Err(CalcError::Overflow));
    }

    #[test]
    fn cents_convert_back_to_decimal() {
        assert_eq!(cents_to_decimal(Cents(12390)), dec!(123.90));
        assert_eq!(cents_to_decimal(Cents(-5)), dec!(-0.05));
    }
}
