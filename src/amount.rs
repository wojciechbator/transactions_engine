use std::fmt;
use std::str::FromStr;

use serde::{Serialize, Serializer};

use crate::error::TransactionError;

static DECIMAL_PLACES: usize = 4;

/// Represents a monetary amount with 4 decimal places of precision.
/// 
/// Internally stores values as i64 to avoid floating-point precision issues.
/// Supports addition, subtraction, parsing from strings, and serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Amount(pub(crate) i64);

impl Amount {
    /// Constant representing zero amount.
    pub const ZERO: Self = Self(0);
    /// Scale factor for converting between decimal and internal representation.
    const SCALE: i64 = 10i64.pow(DECIMAL_PLACES as u32);

    /// Adds two amounts with overflow checking.
    pub fn checked_add(self, amount: Self) -> Option<Self> {
        self.0.checked_add(amount.0).map(Self)
    }

    /// Subtracts two amounts with overflow checking.
    pub fn checked_sub(self, amount: Self) -> Option<Self> {
        self.0.checked_sub(amount.0).map(Self)
    }

    /// Parses amount from byte slice, useful for CSV parsing.
    pub fn from_bytes(b: &[u8]) -> Result<Self, TransactionError> {
        std::str::from_utf8(b)
            .map_err(|_| TransactionError::InvalidAmount("invalid utf8".into()))?
            .parse()
    }
}

impl FromStr for Amount {
    type Err = TransactionError;

    /// Parses decimal string into Amount with exactly 4 decimal places.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();

        let (integer_part, decimal_part) = match s.split_once('.') {
            Some((i, f)) => (i, f),
            None => (s, ""),
        };

        if decimal_part.len() > DECIMAL_PLACES {
            return Err(TransactionError::InvalidAmount(s.to_string()));
        }

        let integer: i64 = parse_numeric_bytes(integer_part.as_bytes())
            .ok_or_else(|| TransactionError::InvalidAmount(s.to_string()))?;

        let fraction: i64 = if decimal_part.is_empty() {
            0
        } else {
            let digits = parse_numeric_bytes(decimal_part.as_bytes())
                .ok_or_else(|| TransactionError::InvalidAmount(s.to_string()))?;
            digits * 10_i64.pow((DECIMAL_PLACES - decimal_part.len()) as u32)
        };

        let raw = integer
            .checked_mul(Self::SCALE)
            .and_then(|v| v.checked_add(fraction))
            .ok_or_else(|| TransactionError::InvalidAmount(s.to_string()))?;

        Ok(Self(raw))
    }
}

/// Parses numeric byte slice into i64, rejecting non-digit characters.
fn parse_numeric_bytes(byte_numbers: &[u8]) -> Option<i64> {
    if byte_numbers.is_empty() {
        return None;
    }

    let mut accumulated: i64 = 0;
    for &byte in byte_numbers {
        let digit = byte.wrapping_sub(b'0');
        if digit > 9 {
            return None;
        }
        accumulated = accumulated.checked_mul(10)?.checked_add(digit as i64)?;
    }

    Some(accumulated)
}

impl fmt::Display for Amount {
    /// Formats amount as decimal string with exactly 4 decimal places.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let integer = self.0 / Self::SCALE;
        let decimal = (self.0 % Self::SCALE).abs();
        write!(f, "{}.{:0width$}", integer, decimal, width = DECIMAL_PLACES)
    }
}

impl Serialize for Amount {
    /// Serializes amount as string for CSV output.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_integer() {
        let a: Amount = "100".parse().unwrap();
        assert_eq!(a.0, 1_000_000);
    }

    #[test]
    fn test_parse_four_dp() {
        let a: Amount = "1.5000".parse().unwrap();
        assert_eq!(a.0, 15_000);
    }

    #[test]
    fn test_parse_short_fraction_is_padded() {
        let a: Amount = "1.5".parse().unwrap();
        assert_eq!(a.0, 15_000);
    }

    #[test]
    fn test_display_four_dp() {
        let a: Amount = "1.5".parse().unwrap();
        assert_eq!(a.to_string(), "1.5000");
    }

    #[test]
    fn test_display_zero() {
        assert_eq!(Amount::ZERO.to_string(), "0.0000");
    }

    #[test]
    fn test_add() {
        let a: Amount = "1.0001".parse().unwrap();
        let b: Amount = "5.0055".parse().unwrap();
        assert_eq!(a.checked_add(b).unwrap().to_string(), "6.0056");
    }

    #[test]
    fn test_sub() {
        let a: Amount = "5.0000".parse().unwrap();
        let b: Amount = "3.0000".parse().unwrap();
        assert_eq!(a.checked_sub(b).unwrap().to_string(), "2.0000");
    }

    #[test]
    fn test_too_many_decimals_is_error() {
        assert!("1.00001".parse::<Amount>().is_err());
    }

    #[test]
    fn test_from_bytes_matches_from_str() {
        let cases = ["0", "1.5", "100.0000", "9999.9999", "0.0001"];
        for case in cases {
            let from_str: Amount = case.parse().unwrap();
            let from_bytes = Amount::from_bytes(case.as_bytes()).unwrap();
            assert_eq!(from_str, from_bytes, "mismatch for {case}");
        }
    }

    #[test]
    fn test_parse_digits_rejects_non_numeric() {
        assert!(Amount::from_bytes(b"abc").is_err());
        assert!(Amount::from_bytes(b"1.2x").is_err());
    }

    #[test]
    fn test_negative_amount_is_error() {
        assert!("-1.0".parse::<Amount>().is_err());
    }

    #[test]
    fn test_empty_string_is_error() {
        assert!("".parse::<Amount>().is_err());
        assert!(Amount::from_bytes(b"").is_err());
    }
}
