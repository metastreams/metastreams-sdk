//! [`UiMoney`], the two-legged amount every priced field on the surface carries.

use std::ops::Add;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::decimal::Decimal;

/// A chain-denominated value with its USD equivalent. The enclosing object carries the
/// `chain` that names `native`'s unit.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiMoney {
    /// Value in the owning chain's gas asset.
    #[schema(value_type = Decimal, examples("1.5"))]
    pub native: Decimal,
    /// Value in USD; `null` when the native asset is unpriced.
    #[schema(value_type = Option<Decimal>, examples("220.10"))]
    pub usd: Option<Decimal>,
}

impl UiMoney {
    /// Both legs known — the common case for spot data, which always carries a USD price.
    pub fn priced(native: Decimal, usd: Decimal) -> Self {
        Self { native, usd: Some(usd) }
    }

    /// An accumulator's value, or `null` when it holds nothing: zero on both legs.
    pub fn recorded(native: Decimal, usd: Decimal) -> Option<Self> {
        (!native.is_zero() || !usd.is_zero()).then(|| Self::priced(native, usd))
    }
}

impl Add for UiMoney {
    type Output = Self;

    /// Adds two values denominated in the same gas asset. An unpriced side leaves the sum
    /// unpriced.
    fn add(self, rhs: Self) -> Self {
        Self { native: self.native + rhs.native, usd: self.usd.zip(rhs.usd).map(|(a, b)| a + b) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_legs_serialize_as_strings() {
        let money = UiMoney::priced(
            "1.5".parse::<Decimal>().expect("decimal"),
            "220.10".parse::<Decimal>().expect("decimal"),
        );
        let body = serde_json::to_value(money).expect("serializes");

        assert_eq!(body, serde_json::json!({ "native": "1.5", "usd": "220.10" }));
    }

    #[test]
    fn an_unpriced_side_leaves_the_sum_unpriced() {
        let priced = UiMoney::priced(Decimal::ONE, Decimal::ONE);
        let unpriced = UiMoney { native: Decimal::ONE, usd: None };

        let sum = priced + unpriced;

        assert_eq!(sum.native, Decimal::ONE + Decimal::ONE);
        assert_eq!(sum.usd, None);
    }

    #[test]
    fn a_zero_accumulator_is_absent() {
        assert_eq!(UiMoney::recorded(Decimal::ZERO, Decimal::ZERO), None);
        assert!(UiMoney::recorded(Decimal::ONE, Decimal::ZERO).is_some());
    }
}
