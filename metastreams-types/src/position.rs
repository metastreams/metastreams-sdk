//! The position object: `UiSpotPosition`, one wallet's position in one token.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use strum::Display;
use utoipa::{IntoParams, ToSchema};

use crate::address::MultiChainAddress;
use crate::chain::{SpotChain, de};
use crate::decimal::Decimal;
use crate::error::{ErrorDetail, FieldError};
use crate::money::UiMoney;
use crate::sort::SortOrder;
use crate::time::UnixMilliseconds;
use crate::token::funding::{FundingSourceKind, FundingStatus};
use crate::trade::UiWalletIdentity;

/// Positions returned when a read names no `limit`.
pub const POSITIONS_LIMIT_DEFAULT: u32 = 50;

/// Most positions one page may carry.
pub const POSITIONS_LIMIT_MAX: u32 = 100;

/// Query for `GET /v1/positions`.
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PositionsQuery {
    /// Wallet addresses, CSV; one to 100.
    #[serde(with = "crate::csv::wallets")]
    #[param(value_type = String, example = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin")]
    pub wallets: Vec<MultiChainAddress>,
    /// Chains to scope to, CSV of the chain slugs. Absent means every spot chain.
    #[serde(default, with = "crate::csv::chains", skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, example = "solana,base")]
    pub chains: Option<Vec<SpotChain>>,
    /// Lifecycle to list; `open` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<Lifecycle>,
    /// Drop open positions whose token liquidity is below the low-liquidity floor.
    #[serde(default = "default_true")]
    #[param(default = true)]
    pub hide_low_liquidity: bool,
    /// Drop open positions whose remaining value is below the dust floor.
    #[serde(default)]
    pub hide_dust: bool,
    /// Drop positions never acquired through a buy.
    #[serde(default)]
    pub hide_transfers: bool,
    /// Exact-token drill-down chain; requires `token`.
    #[serde(default, deserialize_with = "de::optional", skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, example = "solana")]
    pub chain: Option<SpotChain>,
    /// Exact-token drill-down address; requires `chain`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>)]
    pub token: Option<MultiChainAddress>,
    /// Case-insensitive substring of the token symbol or name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    /// Sort field and direction, `field:asc|desc`. `value:desc` when absent, `closedAt:desc` for closed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, example = "value:desc")]
    pub sort: Option<PositionSort>,
    /// Rows per page, clamped to 1-100; 50 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
    /// Opaque cursor from the previous page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

impl PositionsQuery {
    /// Every open position the wallets hold, with the default filters.
    #[must_use]
    pub fn wallets(wallets: Vec<MultiChainAddress>) -> Self {
        Self {
            wallets,
            chains: None,
            lifecycle: None,
            hide_low_liquidity: true,
            hide_dust: false,
            hide_transfers: false,
            chain: None,
            token: None,
            q: None,
            sort: None,
            limit: None,
            cursor: None,
        }
    }

    /// The lifecycle the page serves.
    #[must_use]
    pub fn lifecycle(&self) -> Lifecycle {
        self.lifecycle.unwrap_or(Lifecycle::Open)
    }

    /// The order the page takes: the named one, else the lifecycle's default.
    #[must_use]
    pub fn sort(&self) -> PositionSort {
        self.sort.unwrap_or(match self.lifecycle() {
            Lifecycle::Open => PositionSort { key: PositionSortKey::Value, order: SortOrder::Desc },
            Lifecycle::Closed => {
                PositionSort { key: PositionSortKey::ClosedAt, order: SortOrder::Desc }
            }
        })
    }

    /// The `(chain, token)` drill-down, when both are named.
    #[must_use]
    pub fn drilldown(&self) -> Option<(SpotChain, &MultiChainAddress)> {
        self.chain.zip(self.token.as_ref())
    }

    /// Rows per page, clamped to `1..=`[`POSITIONS_LIMIT_MAX`].
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(POSITIONS_LIMIT_DEFAULT).clamp(1, POSITIONS_LIMIT_MAX)
    }
}

/// A sort field and its direction, `field:asc|desc` on the wire.
#[derive(Debug, Clone, Copy)]
pub struct PositionSort {
    /// Field the page orders on.
    pub key: PositionSortKey,
    /// Direction of that order.
    pub order: SortOrder,
}

impl fmt::Display for PositionSort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let order = match self.order {
            SortOrder::Asc => "asc",
            SortOrder::Desc => "desc",
        };
        write!(f, "{}:{order}", self.key)
    }
}

impl FromStr for PositionSort {
    type Err = ErrorDetail;

    /// `field:direction`, with `desc` when the direction is left out.
    fn from_str(raw: &str) -> Result<Self, ErrorDetail> {
        let (field, direction) = raw.split_once(':').unwrap_or((raw, "desc"));
        let key = serde_json::from_value(field.into()).map_err(|_| {
            sort_error("Sort on `value`, `pnl`, `pnlPct`, `openedAt` or `closedAt`.")
        })?;
        let order = serde_json::from_value(direction.into())
            .map_err(|_| sort_error("Sort direction is `asc` or `desc`."))?;
        Ok(Self { key, order })
    }
}

impl Serialize for PositionSort {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PositionSort {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(ErrorDetail::into_de_error)
    }
}

/// A position's current-cycle lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Currently held.
    Open,
    /// Fully exited this cycle.
    Closed,
}

/// A field the positions read can sort on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum PositionSortKey {
    /// Current USD value.
    Value,
    /// Total USD PnL.
    Pnl,
    /// Total PnL as a percent of invested cost.
    PnlPct,
    /// Cycle open time.
    OpenedAt,
    /// Cycle close time.
    ClosedAt,
}

/// One wallet's spot position in one token, priced at the token's latest price.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotPosition {
    /// Always `spot` today.
    pub product: Product,
    /// Chain the position is on.
    pub chain: SpotChain,
    /// The wallet holding this position, lowercase on EVM chains.
    pub wallet_address: String,
    /// The holder's resolved identities, one element per source.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identities: Vec<UiWalletIdentity>,
    /// The held token.
    pub token: UiPositionToken,
    /// This cycle's lifecycle.
    pub status: Lifecycle,
    /// Whether every unit came without a buy (airdrop, transfer-in).
    pub is_transfer: bool,
    /// Whether the token is a known USD stablecoin; clients render its value but suppress PnL.
    pub is_stablecoin: bool,
    /// Current token balance.
    #[schema(value_type = Decimal)]
    pub holdings: Decimal,
    /// Current holdings as a percent of token supply.
    #[schema(value_type = Decimal)]
    pub holdings_pct_supply: Decimal,
    /// Current value (`holdings × price`); `null` when unpriced.
    pub value: Option<UiMoney>,
    /// Per-token average entry cost.
    pub avg_cost: UiMoney,
    /// Size-weighted market cap across the position's buys; `null` when none was recorded.
    pub avg_entry_market_cap: Option<UiMoney>,
    /// Size-weighted market cap across the position's sells; `null` when none was recorded.
    pub avg_exit_market_cap: Option<UiMoney>,
    /// Spent acquiring the current cycle's holdings.
    pub invested: UiMoney,
    /// Received from sells this cycle.
    pub received: UiMoney,
    /// Realized PnL this cycle.
    pub realized_pnl: UiMoney,
    /// Unrealized PnL on current holdings; `null` when unpriced.
    pub unrealized_pnl: Option<UiMoney>,
    /// Realized plus unrealized PnL; `null` when `unrealizedPnl` is.
    pub total_pnl: Option<UiMoney>,
    /// `totalPnl.usd` as a percent of `invested.usd`; `null` without a cost basis or `totalPnl`.
    #[schema(value_type = Option<Decimal>)]
    pub pnl_pct: Option<Decimal>,
    /// Fees paid this cycle.
    pub fees: UiMoney,
    /// Buy-side trade count this cycle.
    pub buy_count: u64,
    /// Sell-side trade count this cycle.
    pub sell_count: u64,
    /// Tokens bought this cycle.
    #[schema(value_type = Decimal)]
    pub tokens_bought: Decimal,
    /// Tokens sold this cycle.
    #[schema(value_type = Decimal)]
    pub tokens_sold: Decimal,
    /// When the current cycle opened.
    pub opened_at: UnixMilliseconds,
    /// When the current cycle closed; `null` while open.
    pub closed_at: Option<UnixMilliseconds>,
    /// Block time of the position's last update. Updates in one second share it.
    pub updated_at: UnixMilliseconds,
    /// Reconciliation key. A client keeps the highest value per position and discards lower ones.
    pub version: UnixMilliseconds,
    /// Aggregates across every open/close cycle for this `(chain, wallet, token)`.
    pub lifetime: UiPositionLifetime,
    /// The wallet's first inbound funding. Every position of one wallet carries the same value.
    pub funding: UiWalletFunding,
}

/// A position's token. Every field but `chain` and `address` is `null` for a token without data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiPositionToken {
    /// Chain the token trades on.
    pub chain: SpotChain,
    /// Token address.
    pub address: String,
    /// Token symbol.
    pub symbol: Option<String>,
    /// Token name.
    pub name: Option<String>,
    /// Token image URL, as stored.
    pub image: Option<String>,
    /// Token decimals.
    pub decimals: Option<u8>,
    /// Current price.
    pub price: Option<UiMoney>,
    /// Current market cap.
    pub market_cap: Option<UiMoney>,
}

/// Aggregates across every open/close cycle a wallet has had in one token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiPositionLifetime {
    /// Total spent across every cycle.
    pub invested: UiMoney,
    /// Total received from sells across every cycle.
    pub received: UiMoney,
    /// Total realized PnL across every cycle.
    pub realized_pnl: UiMoney,
    /// Total fees paid across every cycle.
    pub fees: UiMoney,
    /// Buy-side trade count across every cycle.
    pub buy_count: u64,
    /// Sell-side trade count across every cycle.
    pub sell_count: u64,
    /// Number of cycles opened.
    pub cycle_count: u64,
    /// Number of cycles closed with positive realized PnL.
    pub winning_cycle_count: u64,
    /// Tokens bought across every cycle.
    #[schema(value_type = Decimal)]
    pub tokens_bought: Decimal,
    /// Tokens sold across every cycle.
    #[schema(value_type = Decimal)]
    pub tokens_sold: Decimal,
}

/// A wallet's first inbound funding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiWalletFunding {
    /// Whether the lookup has settled.
    pub status: FundingStatus,
    /// Funder wallet or exchange address.
    pub address: Option<String>,
    /// What the funder is (exchange, bridge, wallet, …).
    pub source_kind: Option<FundingSourceKind>,
    /// Display label for the funder, e.g. an exchange name.
    pub source_label: Option<String>,
    /// Asset the funding moved; `null` for the chain's native asset.
    pub asset: Option<String>,
    /// Display ticker of `asset`.
    pub symbol: Option<String>,
    /// Amount received, denominated in `asset`.
    #[schema(value_type = Option<Decimal>, examples("2.5"))]
    pub amount: Option<Decimal>,
    /// When the funding transfer occurred.
    pub timestamp: Option<UnixMilliseconds>,
    /// The funding transaction's signature or hash.
    pub tx: Option<String>,
}

/// One page of the named wallets' positions.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotPositions {
    /// Positions on this page.
    pub positions: Vec<UiSpotPosition>,
    /// Cursor for the next page; `null` on the last page.
    pub next_cursor: Option<String>,
    /// Whether a next page exists.
    pub has_more: bool,
    /// Serve-time snapshot timestamp.
    pub as_of: UnixMilliseconds,
    /// Per-product counts, taken before the lifecycle, display, search and drill-down filters.
    pub counts: UiPositionCounts,
}

/// Per-product position counts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiPositionCounts {
    /// Spot counts.
    pub spot: UiSpotCounts,
}

/// Spot position counts across the named wallets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotCounts {
    /// Open positions.
    pub open: u64,
    /// Closed positions.
    pub closed: u64,
}

/// The financial product an event or credit pertains to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Product {
    /// Spot trading.
    Spot,
    /// Perpetual futures.
    Perps,
    /// Prediction markets.
    PredictionMarkets,
}

/// A `sort` refusal that names the field.
fn sort_error(message: &str) -> ErrorDetail {
    ErrorDetail::validation(vec![FieldError::new("sort", message)])
}

/// `#[serde(default = ...)]` for a `bool` field that defaults on.
const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::error::ErrorCode;

    const SOL_WALLET: &str = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";

    fn query(json: serde_json::Value) -> PositionsQuery {
        serde_json::from_value(json).expect("parses")
    }

    #[test]
    fn the_query_defaults_are_open_by_value_desc() {
        let parsed = query(serde_json::json!({ "wallets": SOL_WALLET }));

        assert_eq!(parsed.lifecycle(), Lifecycle::Open);
        assert!(parsed.hide_low_liquidity && !parsed.hide_dust && !parsed.hide_transfers);
        let sort = parsed.sort();
        assert_eq!(sort.key, PositionSortKey::Value);
        assert!(matches!(sort.order, SortOrder::Desc));
        assert_eq!(parsed.limit(), POSITIONS_LIMIT_DEFAULT);
    }

    #[test]
    fn a_closed_page_sorts_by_its_close_unless_told_otherwise() {
        let closed = query(serde_json::json!({ "wallets": SOL_WALLET, "lifecycle": "closed" }));
        assert_eq!(closed.sort().key, PositionSortKey::ClosedAt);

        let named = query(serde_json::json!({
            "wallets": SOL_WALLET, "lifecycle": "closed", "sort": "pnlPct:asc",
        }));
        let sort = named.sort();
        assert_eq!(sort.key, PositionSortKey::PnlPct);
        assert!(matches!(sort.order, SortOrder::Asc));
    }

    #[test]
    fn a_sort_without_a_direction_descends_and_a_bad_one_is_refused() {
        assert!(matches!("pnl".parse::<PositionSort>().expect("valid").order, SortOrder::Desc));
        for bad in ["price:desc", "value:down", ""] {
            let error = bad.parse::<PositionSort>().expect_err("refused");
            assert_eq!(error.code, ErrorCode::ValidationError, "{bad}");
        }
    }

    #[test]
    fn a_query_writes_what_it_reads() {
        let mut written = PositionsQuery::wallets(vec![SOL_WALLET.parse().expect("a wallet")]);
        written.lifecycle = Some(Lifecycle::Closed);
        written.sort = Some(PositionSort { key: PositionSortKey::OpenedAt, order: SortOrder::Asc });
        written.limit = Some(10);

        let body = serde_json::to_value(&written).expect("serializes");
        assert_eq!(body["sort"], "openedAt:asc");
        let read: PositionsQuery = serde_json::from_value(body).expect("reads back");
        assert_eq!(read.lifecycle(), Lifecycle::Closed);
        assert_eq!(read.sort().key, PositionSortKey::OpenedAt);
        assert!(read.hide_low_liquidity);
        assert_eq!(read.limit(), 10);
    }
}
