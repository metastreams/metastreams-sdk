//! Search — the kinds and sorts a query names, and the kind-tagged hits it returns.

use serde::{Deserialize, Serialize};
use strum::IntoStaticStr;
use utoipa::{IntoParams, ToSchema};

use crate::chain::{SpotChain, de};
use crate::decimal::Decimal;
use crate::time::UnixMilliseconds;

/// Hits returned when a search names no `limit`.
pub const SEARCH_LIMIT_DEFAULT: u32 = 10;

/// Most hits one search may return.
pub const SEARCH_LIMIT_MAX: u32 = 20;

/// Query params for `GET /v1/search`.
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetSearchQuery {
    /// Search text, matched against symbol, name, address, and pool address.
    pub q: String,
    /// Scope hits to one asset kind. Omit to search every kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "spotToken")]
    pub kind: Option<SearchKind>,
    /// Scope hits to one chain. Omit to search every indexed chain.
    #[serde(default, deserialize_with = "de::optional", skip_serializing_if = "Option::is_none")]
    #[param(example = "base")]
    pub chain: Option<SpotChain>,
    /// Scope hits to these chains, CSV of the chain slugs. Omit to search every indexed
    /// chain. Combined with `chain` when both are set.
    #[serde(
        default,
        serialize_with = "crate::csv::serialize",
        deserialize_with = "de::csv",
        skip_serializing_if = "Option::is_none"
    )]
    #[param(value_type = Option<String>, example = "robinhood,bsc")]
    pub chains: Option<Vec<SpotChain>>,
    /// Hits returned, clamped to 1-20; 10 when absent.
    #[serde(default = "default_search_limit")]
    #[param(default = 10, minimum = 1, maximum = 20)]
    pub limit: u32,
    /// Scope hits to tokens that have left their launchpad (`true`) or have not (`false`).
    /// `false` also admits tokens that never launched through one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graduated: Option<bool>,
    /// Order hits by this attribute over the whole candidate set, instead of by relevance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(example = "volume24h")]
    pub sort_by: Option<SearchSortField>,
    /// Direction for `sortBy`; `desc` when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_direction: Option<SearchSortDirection>,
}

impl AssetSearchQuery {
    /// A search for `q` with every other parameter at its default.
    #[must_use]
    pub fn new(q: impl Into<String>) -> Self {
        Self {
            q: q.into(),
            kind: None,
            chain: None,
            chains: None,
            limit: SEARCH_LIMIT_DEFAULT,
            graduated: None,
            sort_by: None,
            sort_direction: None,
        }
    }

    /// Hits returned, clamped to `1..=`[`SEARCH_LIMIT_MAX`].
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit.clamp(1, SEARCH_LIMIT_MAX)
    }

    /// The chains in scope, from `chain` and `chains` together; empty means every chain.
    #[must_use]
    pub fn chains(&self) -> Vec<SpotChain> {
        let mut chains: Vec<SpotChain> = self.chain.into_iter().collect();
        for chain in self.chains.iter().flatten() {
            if !chains.contains(chain) {
                chains.push(*chain);
            }
        }
        chains
    }
}

/// The asset kinds a search can be scoped to.
///
/// Metastreams is spot only, so the document must not advertise a kind no hit can carry.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum SearchKind {
    /// A spot token.
    SpotToken,
    /// A wallet.
    SpotWallet,
}

/// Sort field for search results.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, IntoStaticStr, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum SearchSortField {
    /// Relevance to the text, the default.
    Relevance,
    /// 24h trading volume in USD.
    Volume24h,
    /// Market cap in USD.
    MarketCap,
    /// Pool liquidity in USD.
    Liquidity,
    /// Creation time.
    CreatedAt,
}

/// Sort direction.
#[derive(
    Debug, Default, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, IntoStaticStr, ToSchema,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum SearchSortDirection {
    /// Smallest first.
    Asc,
    /// Largest first.
    #[default]
    Desc,
}

/// `GET /v1/search` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiAssetSearchResponse {
    /// Hits ordered by `sortBy`, or by relevance when it is absent. An unsorted page leads with
    /// the spot tokens active in the last 24h.
    pub hits: Vec<UiAssetSearchHit>,
}

/// One search hit.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UiAssetSearchHit {
    /// A spot token.
    SpotToken(Box<UiSpotSearchToken>),
    /// A wallet.
    SpotWallet(UiSpotSearchWallet),
    /// A hit kind this build does not name. Read-only: the server never sends it.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// One spot-token hit.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotSearchToken {
    /// Chain the token trades on.
    pub chain: SpotChain,
    /// Token address on `chain`.
    pub address: String,
    /// Token name.
    pub name: String,
    /// Token symbol.
    pub symbol: String,
    /// Token logo URL; omitted when no source published one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    /// Venue of the token's primary pool; omitted before the first trade.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dex: Option<String>,
    /// Launchpad the token launched through; omitted for non-launchpad tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launchpad: Option<String>,
    /// Whether the token has left its launchpad.
    pub graduated: bool,
    /// Current price in USD.
    #[schema(value_type = Decimal, examples("0.0421"))]
    pub price_usd: Decimal,
    /// Market cap in USD.
    #[schema(value_type = Decimal, examples("1250000"))]
    pub market_cap_usd: Decimal,
    /// 24h trading volume in USD.
    #[schema(value_type = Decimal, examples("310000"))]
    pub volume_24h_usd: Decimal,
    /// Pool liquidity in USD, both legs.
    #[schema(value_type = Decimal, examples("98000"))]
    pub liquidity_usd: Decimal,
    /// 24h price change, percent.
    #[schema(value_type = Decimal, examples("-3.2"))]
    pub price_change_24h_pct: Decimal,
    /// Holder count.
    pub holders_count: u64,
    /// Creation timestamp.
    pub created_at: UnixMilliseconds,
    /// The token's last on-chain activity.
    pub last_active_at: UnixMilliseconds,
}

/// One wallet hit — the same identity a holders row carries.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotSearchWallet {
    /// Chain the wallet lives on.
    pub chain: SpotChain,
    /// Wallet address on `chain`.
    pub address: String,
}

const fn default_search_limit() -> u32 {
    SEARCH_LIMIT_DEFAULT
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(json: serde_json::Value) -> AssetSearchQuery {
        serde_json::from_value(json).expect("deserializes")
    }

    #[test]
    fn a_bare_query_takes_the_defaults() {
        let query = query(serde_json::json!({ "q": "bonk" }));

        assert_eq!(query.limit(), SEARCH_LIMIT_DEFAULT);
        assert_eq!(query.chains(), []);
        assert_eq!(query.sort_by, None);
    }

    #[test]
    fn chain_and_chains_combine_without_repeats() {
        let query =
            query(serde_json::json!({ "q": "bonk", "chain": "base", "chains": "robinhood, base" }));

        assert_eq!(query.chains(), [SpotChain::Base, SpotChain::Robinhood]);
    }

    #[test]
    fn a_limit_outside_the_cap_is_clamped_not_rejected() {
        for (limit, clamped) in [(0, 1), (21, SEARCH_LIMIT_MAX)] {
            let query = query(serde_json::json!({ "q": "bonk", "limit": limit }));

            assert_eq!(query.limit(), clamped);
        }
    }

    #[test]
    fn an_unsupported_slug_and_a_perps_kind_are_rejected_at_deserialization() {
        for json in [
            serde_json::json!({ "q": "bonk", "chains": "tron" }),
            serde_json::json!({ "q": "bonk", "kind": "perpsMarket" }),
        ] {
            serde_json::from_value::<AssetSearchQuery>(json).expect_err("not served");
        }
    }

    #[test]
    fn an_unknown_parameter_is_rejected() {
        serde_json::from_value::<AssetSearchQuery>(
            serde_json::json!({ "q": "bonk", "query": "bonk" }),
        )
        .expect_err("unknown parameters are rejected, not dropped");
    }

    #[test]
    fn a_query_reads_back_what_it_writes() {
        let mut written = AssetSearchQuery::new("bonk");
        written.chains = Some(vec![SpotChain::Base, SpotChain::Solana]);
        written.sort_by = Some(SearchSortField::MarketCap);

        let read = query(serde_json::to_value(&written).expect("serializes"));

        assert_eq!(read.chains(), [SpotChain::Base, SpotChain::Solana]);
        assert_eq!(read.sort_by, Some(SearchSortField::MarketCap));
        assert_eq!(read.limit(), SEARCH_LIMIT_DEFAULT);
    }

    #[test]
    fn a_hit_is_tagged_by_kind() {
        let hit = UiAssetSearchHit::SpotWallet(UiSpotSearchWallet {
            chain: SpotChain::Solana,
            address: "So11111111111111111111111111111111111111112".to_owned(),
        });

        let body = serde_json::to_value(hit).expect("serializes");
        assert_eq!(body["kind"], "spotWallet");
        assert_eq!(body["chain"], "solana");
    }

    #[test]
    fn an_unknown_hit_kind_reads_as_unknown() {
        let hit: UiAssetSearchHit =
            serde_json::from_str(r#"{"kind":"perpsMarket","symbol":"BTC"}"#).expect("parses");
        assert!(matches!(hit, UiAssetSearchHit::Unknown));
    }
}
