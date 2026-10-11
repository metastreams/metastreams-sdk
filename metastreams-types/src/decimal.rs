//! [`Decimal`], the exact number every amount on the surface carries.

use std::cmp::Ordering;
use std::fmt;
use std::ops::{Add, Deref};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use utoipa::ToSchema;

/// An exact decimal number. JSON carries it as a string.
#[derive(Debug, Clone, Hash, Copy, PartialEq, Default, ToSchema)]
#[schema(value_type = String, format = "decimal", example = "123.456789")]
pub struct Decimal(pub rust_decimal::Decimal);

impl Decimal {
    /// Zero.
    pub const ZERO: Self = Self(rust_decimal::Decimal::ZERO);
    /// One.
    pub const ONE: Self = Self(rust_decimal::Decimal::ONE);
}

impl Serialize for Decimal {
    /// Every format takes the string. No surface writes these types in a binary format.
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Decimal {
    /// Reads a string only, and refuses a JSON number.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        rust_decimal::Decimal::from_str(&raw).map(Self).map_err(serde::de::Error::custom)
    }
}

impl Deref for Decimal {
    type Target = rust_decimal::Decimal;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Decimal {
    type Err = rust_decimal::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(rust_decimal::Decimal::from_str(s)?))
    }
}

impl Add for Decimal {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_decimal_reads_and_writes_its_string() {
        let value: Decimal = serde_json::from_value(serde_json::json!("1.50")).expect("parses");

        assert_eq!(serde_json::to_value(value).expect("serializes"), "1.50");
    }

    #[test]
    fn a_json_number_is_refused() {
        serde_json::from_value::<Decimal>(serde_json::json!(1.5)).expect_err("a number");
    }
}
