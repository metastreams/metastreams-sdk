//! Trending — the tokens ranked by the trending score over every chain requested.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::chain::{SpotChain, de};
use crate::discovery::filters::SpotTokenFilters;
use crate::token::UiSpotToken;

/// Most tokens one board may carry, and the size when the request names no `limit`.
pub const MAX_TRENDING_LIMIT: u32 = 100;

/// A trending request: one or more chains, one window, and the filters the board must satisfy.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpotTrendingFilter {
    /// Chains the board ranks over, as one mixed set.
    #[serde(deserialize_with = "de::list")]
    pub chains: Vec<SpotChain>,
    /// Window the score reads; `24h` when absent.
    #[serde(default)]
    pub window: TrendingWindow,
    /// Ranked-set size, clamped to 1-100; 100 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Filters the ranked tokens must satisfy; absent ranks every candidate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<SpotTokenFilters>,
}

impl SpotTrendingFilter {
    /// Ranked-set size, clamped to `1..=`[`MAX_TRENDING_LIMIT`].
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(MAX_TRENDING_LIMIT).clamp(1, MAX_TRENDING_LIMIT)
    }

    /// The requested chains, sorted and each kept once, so equal sets compare equal.
    #[must_use]
    pub fn canonical_chains(&self) -> Vec<SpotChain> {
        let mut chains = self.chains.clone();
        chains.sort_by_key(|chain| *chain as u8);
        chains.dedup();
        chains
    }
}

/// Window the trending score's volume and velocity terms read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub enum TrendingWindow {
    /// The last minute.
    #[serde(rename = "1m")]
    OneMinute,
    /// The last 5 minutes.
    #[serde(rename = "5m")]
    FiveMinutes,
    /// The last 30 minutes.
    #[serde(rename = "30m")]
    ThirtyMinutes,
    /// The last hour.
    #[serde(rename = "1h")]
    OneHour,
    /// The last 6 hours.
    #[serde(rename = "6h")]
    SixHours,
    /// The last 24 hours.
    #[default]
    #[serde(rename = "24h")]
    TwentyFourHours,
}

/// `POST /v1/trending` response: the ranked board, rank 1 first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotTrendingUpdate {
    /// The whole board, one entry per rank.
    pub deltas: Vec<UiSpotTrendingDelta>,
}

/// One token's place on a trending board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotTrendingDelta {
    /// Chain the token trades on; with `address` it keys the row.
    pub chain: SpotChain,
    /// Token address.
    pub address: String,
    /// 1-indexed rank; `null` drops the token off the board.
    pub rank: Option<u32>,
    /// Token state; absent on eviction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<UiSpotToken>,
}

impl UiSpotTrendingDelta {
    /// `token` placed at `rank`, with its whole state.
    #[must_use]
    pub fn whole(rank: u32, token: UiSpotToken) -> Self {
        let moved = Self::moved(rank, &token);
        Self { data: Some(token), ..moved }
    }

    /// `token` moved to `rank`, without its state.
    #[must_use]
    pub fn moved(rank: u32, token: &UiSpotToken) -> Self {
        Self { rank: Some(rank), ..Self::evicted(token) }
    }

    /// `token` with no rank, which drops it off the board.
    #[must_use]
    pub fn evicted(token: &UiSpotToken) -> Self {
        let card = &token.card;
        Self { chain: card.chain, address: card.address.clone(), rank: None, data: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(json: &str) -> Result<SpotTrendingFilter, serde_json::Error> {
        serde_json::from_str(json)
    }

    #[test]
    fn a_body_naming_only_its_chains_takes_every_default() {
        let filter = filter(r#"{"chains":["solana"]}"#).expect("chains alone is a valid body");

        assert_eq!(filter.window, TrendingWindow::TwentyFourHours);
        assert_eq!(filter.limit(), MAX_TRENDING_LIMIT);
        assert!(filter.filters.is_none());
    }

    #[test]
    fn a_limit_outside_the_cap_is_clamped() {
        for (limit, clamped) in [(0, 1), (7, 7), (500, MAX_TRENDING_LIMIT)] {
            let filter = SpotTrendingFilter { limit: Some(limit), ..SpotTrendingFilter::default() };
            assert_eq!(filter.limit(), clamped);
        }
    }

    #[test]
    fn canonical_chains_are_sorted_and_kept_once() {
        let filter = filter(r#"{"chains":["bsc","solana","bsc"]}"#).expect("valid body");

        assert_eq!(filter.canonical_chains(), [SpotChain::Solana, SpotChain::Bsc]);
    }

    #[test]
    fn an_unknown_key_is_refused() {
        assert!(filter(r#"{"chains":["solana"],"deltaOnly":true}"#).is_err());
    }
}
