//! [`SpotTokenFilters`], the filters a discovery column takes, and the [`FilterRange`] each range
//! filter carries.

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize};
use utoipa::ToSchema;

use crate::decimal::Decimal;
use crate::token::annotation::TokenAnnotation;

/// An inclusive `[min, max]` range. An absent bound is unbounded.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct FilterRange<T: Clone + PartialEq + ToSchema> {
    /// Minimum value (inclusive), null means unbounded
    pub min: Option<T>,
    /// Maximum value (inclusive), null means unbounded
    pub max: Option<T>,
}

impl<'de, T> Deserialize<'de> for FilterRange<T>
where
    T: Clone + PartialEq + PartialOrd + ToSchema + Deserialize<'de>,
{
    /// Rejects an inverted range (`min > max`), which would otherwise silently match nothing.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawRange<T> {
            min: Option<T>,
            max: Option<T>,
        }

        let raw = RawRange::deserialize(deserializer)?;
        if let (Some(min), Some(max)) = (&raw.min, &raw.max)
            && min > max
        {
            return Err(D::Error::custom("min must be less than or equal to max"));
        }
        Ok(Self { min: raw.min, max: raw.max })
    }
}

/// Filters over a discovery column. Every range is inclusive on both ends.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
// Published schema name; generated clients key on it.
#[schema(as = DiscoveryTokenFilters)]
#[serde(rename_all = "camelCase")]
pub struct SpotTokenFilters {
    /// Filter by token launch protocol (e.g., pumpfun, moonit, boop). The value `mayhem_mode`
    /// selects Pump.fun Mayhem Mode tokens instead. The list is a union.
    pub protocols: Option<Vec<String>>,

    /// Exclude tokens that carry any of these annotations, e.g. `mayhem_mode` from a `pumpfun` search.
    #[serde(default, deserialize_with = "deserialize_known_annotations")]
    pub exclude_annotations: Option<Vec<TokenAnnotation>>,

    /// Filter by the quote asset symbol the token's deepest pool prices against, as the anchor
    /// registry names it (e.g. WSOL, USDC, WBNB).
    pub anchor_symbols: Option<Vec<String>>,

    /// Only show tokens that have at least one social link
    pub with_at_least_one_social: Option<bool>,

    /// Include tokens whose symbol or name matches any keyword (case-insensitive)
    pub symbol_or_name: Option<Vec<String>>,

    /// Exclude tokens whose symbol or name matches any keyword (case-insensitive)
    pub exclude_symbol_or_name: Option<Vec<String>>,

    /// Exclude tokens whose website host matches any entry (URL or bare domain)
    pub exclude_websites: Option<Vec<String>>,

    /// Exclude tokens whose X (Twitter) handle matches any entry (handle or URL)
    pub exclude_twitter: Option<Vec<String>>,

    /// Exclude tokens created by any of these developer wallets
    pub exclude_dev_addresses: Option<Vec<String>>,

    /// Exclude tokens whose creator was funded by any of these wallets.
    pub exclude_funding_addresses: Option<Vec<String>>,

    /// Filter by bonding curve completion percentage
    pub bonding_curve_percent: Option<FilterRange<f64>>,

    /// Filter by number of token holders
    pub holder_count: Option<FilterRange<u64>>,

    /// Filter by developer holding percentage
    pub dev_holding_percent: Option<FilterRange<f64>>,

    /// Filter by top 10 holders' combined percentage
    pub top10_holders_percent: Option<FilterRange<f64>>,

    /// Filter by snipers' supply percentage
    pub snipers_supply_percent: Option<FilterRange<f64>>,

    /// Percentage of supply held by bundler wallets.
    pub bundler_supply_percent: Option<FilterRange<Decimal>>,

    /// Percentage of supply held by insider wallets.
    pub insider_supply_percent: Option<FilterRange<Decimal>>,

    /// Number of wallets that traded the token through a known trading terminal.
    pub pro_trader_count: Option<FilterRange<u64>>,

    /// Keep only tokens whose DexScreener profile payment status equals this value.
    pub dex_paid: Option<bool>,

    /// Filter by how many times the Twitter account has been reused. Not yet counted, so every
    /// token passes.
    pub twitter_reuse_count: Option<FilterRange<u64>>,

    /// Filter by token age in minutes since creation
    pub age_minutes: Option<FilterRange<u64>>,

    /// Filter by pool liquidity in USD
    pub liquidity_usd: Option<FilterRange<Decimal>>,

    /// Filter by all-time global fees paid in USD. Ignored on chains without fee data.
    pub global_fees_paid_usd: Option<FilterRange<Decimal>>,

    /// Filter by all-time global fees paid in the chain's native asset.
    /// Ignored on chains without fee data.
    pub global_fees_paid_native: Option<FilterRange<Decimal>>,

    /// Filter by 24h trading volume in USD
    pub volume_usd: Option<FilterRange<Decimal>>,

    /// Filter by market capitalization in USD
    pub market_cap_usd: Option<FilterRange<Decimal>>,

    /// Filter by total transaction count
    pub txns: Option<FilterRange<u64>>,

    /// Filter by buy transaction count
    pub buys: Option<FilterRange<u64>>,

    /// Filter by sell transaction count
    pub sells: Option<FilterRange<u64>>,

    /// Maximum minutes since last trading activity. Tokens with no trades
    /// within this window are excluded. Uses `last_updated` as proxy since
    /// discovery tokens are only emitted on trades.
    pub last_activity_minutes: Option<u64>,

    /// Maximum minutes an `AboutToGraduate` token may go without its bonding
    /// curve advancing before it ages out. Tokens at/above the graduation edge
    /// or without a progress reading yet are kept.
    pub max_graduation_stale_minutes: Option<u32>,
}

/// The annotations this build names. It refuses an unknown kind rather than read it as `Unknown`.
fn deserialize_known_annotations<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<TokenAnnotation>>, D::Error> {
    let values = Option::<Vec<TokenAnnotation>>::deserialize(deserializer)?;
    if values.as_deref().is_some_and(|v| v.contains(&TokenAnnotation::Unknown)) {
        return Err(DeError::custom("unknown token annotation kind"));
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_inverted_range_is_refused() {
        let error = serde_json::from_value::<FilterRange<u64>>(serde_json::json!({
            "min": 10, "max": 2,
        }))
        .expect_err("inverted");

        assert_eq!(error.to_string(), "min must be less than or equal to max");
    }

    #[test]
    fn an_unknown_annotation_kind_is_refused() {
        serde_json::from_value::<SpotTokenFilters>(serde_json::json!({
            "excludeAnnotations": [{ "kind": "nope" }],
        }))
        .expect_err("unknown kind");
    }
}
