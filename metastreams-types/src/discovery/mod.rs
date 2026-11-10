//! Discovery: the Pulse columns, ranked over every chain requested.
//!
//! A board is a stage of the launchpad lifecycle: new pairs, graduating, and graduated.

pub mod filters;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::chain::{SpotChain, de};
use crate::discovery::filters::SpotTokenFilters;
use crate::sort::SortOrder;
use crate::token::UiSpotPulseToken;

/// Tokens a column carries when the request names no `limit`.
pub const DEFAULT_COLUMN_LIMIT: u32 = 20;

/// Most tokens one column may carry.
pub const MAX_COLUMN_LIMIT: u32 = 100;

/// A discovery request: one or more chains, up to three columns.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SpotPulseSubscriptionConfig {
    /// Chains every column ranks over, as one mixed set.
    #[serde(deserialize_with = "de::list")]
    pub chains: Vec<SpotChain>,
    /// New-pairs column; omitted means not requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_pairs: Option<SpotPulseColumnConfig>,
    /// Graduating column; omitted means not requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graduating: Option<SpotPulseColumnConfig>,
    /// Graduated column; omitted means not requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graduated: Option<SpotPulseColumnConfig>,
}

impl SpotPulseSubscriptionConfig {
    /// The requested chains, in request order, each one kept once.
    #[must_use]
    pub fn unique_chains(&self) -> Vec<SpotChain> {
        let mut unique: Vec<SpotChain> = Vec::with_capacity(self.chains.len());
        for chain in &self.chains {
            if !unique.contains(chain) {
                unique.push(*chain);
            }
        }
        unique
    }

    /// The requested columns, in page order.
    #[must_use]
    pub fn columns(&self) -> Vec<(PulseColumn, &SpotPulseColumnConfig)> {
        [
            (PulseColumn::NewPairs, &self.new_pairs),
            (PulseColumn::Graduating, &self.graduating),
            (PulseColumn::Graduated, &self.graduated),
        ]
        .into_iter()
        .filter_map(|(column, config)| config.as_ref().map(|config| (column, config)))
        .collect()
    }
}

/// How one discovery column is ranked and filtered.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SpotPulseColumnConfig {
    /// Field to rank on; the column's default when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<SpotSortField>,
    /// Rank direction; descending when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<SortOrder>,
    /// Ranked-set size, clamped to 1-100; 20 when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Filters over the column. They add to the column's own defaults and never loosen them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<SpotTokenFilters>,
}

impl SpotPulseColumnConfig {
    /// Ranked-set size, clamped to `1..=`[`MAX_COLUMN_LIMIT`].
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(DEFAULT_COLUMN_LIMIT).clamp(1, MAX_COLUMN_LIMIT)
    }
}

/// The field a discovery column ranks on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
// Published schema name; generated clients key on it.
#[schema(as = DiscoverySortField)]
#[serde(rename_all = "camelCase")]
pub enum SpotSortField {
    /// Creation time.
    CreatedAt,
    /// 24h trading volume in USD.
    #[default]
    Volume24hUsd,
    /// Market cap in USD.
    MarketCapUsd,
    /// Pool liquidity in USD.
    LiquidityUsd,
    /// Holder count.
    HoldersCount,
    /// 24h trade count.
    TradeCount24h,
    /// Bonding-curve completion.
    BondingCurvePct,
    /// Bonding-curve completion, weighted by recent volume.
    BondingCurveActivity,
    /// Graduation time.
    GraduatedAt,
}

/// One of the three discovery columns.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PulseColumn {
    /// Tokens still trading on their launchpad.
    NewPairs,
    /// Tokens about to leave their launchpad.
    Graduating,
    /// Tokens that left their launchpad.
    Graduated,
}

impl PulseColumn {
    /// Ranking this column takes when the request names none.
    #[must_use]
    pub const fn default_sort_field(self) -> SpotSortField {
        match self {
            Self::NewPairs => SpotSortField::CreatedAt,
            Self::Graduating => SpotSortField::BondingCurveActivity,
            Self::Graduated => SpotSortField::GraduatedAt,
        }
    }
}

/// One column's ranked tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotPulseUpdate {
    /// Column these deltas apply to.
    pub column: PulseColumn,
    /// The rows to apply in order: the whole column at first, then only the rows that changed.
    pub deltas: Vec<UiSpotPulseDelta>,
}

/// One token's place on a ranked board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotPulseDelta {
    /// Chain the token trades on; with `address` it keys the row.
    pub chain: SpotChain,
    /// Token address.
    pub address: String,
    /// 1-indexed rank; `null` drops the token off the board.
    pub rank: Option<u32>,
    /// Token state; absent on eviction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<UiSpotPulseToken>,
}

impl UiSpotPulseDelta {
    /// `token` placed at `rank`, with its whole state.
    #[must_use]
    pub fn whole(rank: u32, token: UiSpotPulseToken) -> Self {
        let moved = Self::moved(rank, &token);
        Self { data: Some(token), ..moved }
    }

    /// `token` moved to `rank`, without its state.
    #[must_use]
    pub fn moved(rank: u32, token: &UiSpotPulseToken) -> Self {
        Self { rank: Some(rank), ..Self::evicted(token) }
    }

    /// `token` with no rank, which drops it off the column.
    #[must_use]
    pub fn evicted(token: &UiSpotPulseToken) -> Self {
        Self { chain: token.chain, address: token.address.clone(), rank: None, data: None }
    }
}

/// `POST /v1/discovery` response: one snapshot per requested column, in page order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotPulseResponse {
    /// The requested columns' snapshots.
    pub columns: Vec<UiSpotPulseUpdate>,
}
