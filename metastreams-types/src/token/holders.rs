//! The holders read: `UiSpotHolder`, the ranked list it sits in, and the read's query.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::address::MultiChainAddress;
use crate::decimal::Decimal;
use crate::money::UiMoney;
use crate::position::UiWalletFunding;
use crate::time::UnixMilliseconds;
use crate::token::platform::UiPlatform;
use crate::trade::{UiWalletIdentity, WalletFilter, WalletFlag};

/// Holders returned when a read names no `limit`.
pub const HOLDERS_LIMIT_DEFAULT: u32 = 30;

/// Most holders one read may return.
pub const HOLDERS_LIMIT_MAX: u32 = 150;

/// Query for `GET /v1/tokens/{chain}/{address}/holders`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenHoldersQuery {
    /// Rows to return, clamped to 1-150; 30 when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[param(minimum = 1, maximum = 150)]
    pub limit: Option<u32>,
    /// Restrict to one holder wallet.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>)]
    pub trader: Option<MultiChainAddress>,
    /// Wallet classifications to keep, comma-separated. Absent or empty keeps every holder.
    #[serde(default, with = "crate::csv", skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, example = "bundler,insider")]
    pub flags: Option<Vec<WalletFilter>>,
    /// Span the trading figures cover; `lifetime` when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<HolderPeriod>,
}

impl TokenHoldersQuery {
    /// Rows to return, clamped to `1..=`[`HOLDERS_LIMIT_MAX`].
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(HOLDERS_LIMIT_DEFAULT).clamp(1, HOLDERS_LIMIT_MAX)
    }

    /// The span the figures cover; [`HolderPeriod::Lifetime`] when absent.
    #[must_use]
    pub fn period(&self) -> HolderPeriod {
        self.period.unwrap_or_default()
    }
}

/// Which span of a wallet's trading on a token a holder row reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HolderPeriod {
    /// The open round trip: everything since the wallet last held none of the token.
    Cycle,
    /// Every round trip the wallet made on the token.
    #[default]
    Lifetime,
}

/// One wallet's holding and trading figures on one token.
///
/// Volumes, token totals, trade counts, entry and exit prices and realized PnL cover the read's
/// `period`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotHolder {
    /// 1-based rank in the requested ordering; a `flags` filter ranks within the filtered
    /// cohort. Absent on live `spot.holders` updates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u64>,
    /// Wallet address.
    pub address: String,
    /// The wallet's balance of the chain's native asset; `null` until the chain reports one.
    #[schema(value_type = Option<Decimal>, examples("4.2"))]
    pub native_balance: Option<Decimal>,
    /// Token units the wallet holds now.
    #[schema(value_type = Decimal, examples("500000"))]
    pub balance: Decimal,
    /// Share of total supply the wallet holds, 0-100.
    #[schema(value_type = Decimal, examples("5.2"))]
    pub supply_pct: Decimal,
    /// Spent buying this token over the period.
    pub buy_volume: UiMoney,
    /// Token units bought over the period, including any sold since.
    #[schema(value_type = Decimal, examples("500000"))]
    pub tokens_bought: Decimal,
    /// Buy trades over the period.
    pub buy_count: u32,
    /// Received from selling this token over the period.
    pub sell_volume: UiMoney,
    /// Token units sold over the period.
    #[schema(value_type = Decimal, examples("0"))]
    pub tokens_sold: Decimal,
    /// Sell trades over the period.
    pub sell_count: u32,
    /// The wallet's first buy of this token; `null` for a wallet that only received transfers.
    pub first_purchase_at: Option<UnixMilliseconds>,
    /// Entry price per token over the period; `null` before the period's first buy. `cycle`
    /// reads the open position's cost basis, and `lifetime` averages every buy.
    pub avg_entry_price: Option<UiMoney>,
    /// Volume-weighted exit price over the period; `null` until the wallet sells in it.
    pub avg_exit_price: Option<UiMoney>,
    /// Size-weighted market cap across the wallet's buys; `null` when none was recorded.
    pub avg_entry_market_cap: Option<UiMoney>,
    /// Size-weighted market cap across the wallet's sells; `null` when none was recorded.
    pub avg_exit_market_cap: Option<UiMoney>,
    /// Profit or loss the wallet locked in by selling over the period.
    pub realized_pnl: UiMoney,
    /// Share of its peak holdings the wallet still holds, 0-100.
    #[schema(value_type = Decimal, examples("83.3"))]
    pub remaining_pct: Decimal,
    /// The wallet's first inbound funding. Always present; its status says whether the lookup
    /// has settled.
    pub funding: UiWalletFunding,
    /// Classification flags for this wallet.
    #[serde(
        default,
        deserialize_with = "crate::trade::known_flags",
        skip_serializing_if = "Vec::is_empty"
    )]
    #[schema(schema_with = crate::trade::open_wallet_flags)]
    pub flags: Vec<WalletFlag>,
    /// The wallet's trading-platform identities.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<UiPlatform>,
    /// The wallet's resolved identities, one element per source.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identities: Vec<UiWalletIdentity>,
}

/// Balance-weighted entry and exit prices across a holder cohort.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiHolderCohortStats {
    /// Balance-weighted entry price across the cohort; `null` when no holder has one.
    pub avg_entry_price: Option<UiMoney>,
    /// Balance-weighted exit price across the cohort; `null` when no holder has one.
    pub avg_exit_price: Option<UiMoney>,
}

/// `GET /v1/tokens/{chain}/{address}/holders` response: current holders, largest balance first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenHolderList {
    /// Holders the read returned, largest balance first.
    pub holders: Vec<UiSpotHolder>,
    /// The token's own holder total, whatever the `limit` and any `flags` filter. It counts
    /// trading wallets only, so it can run below a full listing that includes pool vaults.
    pub holder_count: u64,
    /// Balance-weighted entry and exit prices across the first 10 holders; the filtered cohort
    /// under `flags`.
    pub top10: UiHolderCohortStats,
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    use crate::token::funding::FundingStatus;
    use crate::trade::IdentitySource;

    fn decimal(s: &str) -> Decimal {
        s.parse().expect("a decimal literal")
    }

    fn money(native: &str, usd: &str) -> UiMoney {
        UiMoney::priced(decimal(native), decimal(usd))
    }

    #[test]
    fn a_holder_list_writes_camel_case_keys() {
        let holder = UiSpotHolder {
            rank: Some(1),
            address: "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin".to_owned(),
            native_balance: Some(decimal("4.2")),
            balance: decimal("500000"),
            supply_pct: decimal("5.2"),
            buy_volume: money("1", "150"),
            tokens_bought: decimal("500000"),
            buy_count: 2,
            sell_volume: money("0", "0"),
            tokens_sold: decimal("0"),
            sell_count: 0,
            first_purchase_at: Some(UnixMilliseconds(1_753_000_000_000)),
            avg_entry_price: Some(money("0.000002", "0.0003")),
            avg_exit_price: None,
            avg_entry_market_cap: Some(money("2000", "300000")),
            avg_exit_market_cap: None,
            realized_pnl: money("0", "0"),
            remaining_pct: decimal("100"),
            funding: UiWalletFunding {
                status: FundingStatus::Unresolved,
                address: None,
                source_kind: None,
                source_label: None,
                asset: None,
                symbol: None,
                amount: None,
                timestamp: None,
                tx: None,
            },
            flags: vec![WalletFlag::Sniper],
            platforms: vec![UiPlatform::Axiom],
            identities: vec![UiWalletIdentity {
                source: IdentitySource::Pump,
                handle: Some("whale".to_owned()),
                display_name: None,
                avatar: None,
                twitter: None,
                telegram: None,
                farcaster: None,
            }],
        };
        let list = UiTokenHolderList {
            holders: vec![holder],
            holder_count: 42,
            top10: UiHolderCohortStats { avg_entry_price: None, avg_exit_price: None },
        };

        let body = serde_json::to_value(&list).expect("serializes");

        let mut keys: Vec<&String> = body.as_object().expect("object").keys().collect();
        keys.sort();
        assert_eq!(keys, ["holderCount", "holders", "top10"]);
        assert_eq!(body["top10"], json!({ "avgEntryPrice": null, "avgExitPrice": null }));
        let mut row: Vec<&String> = body["holders"][0].as_object().expect("a row").keys().collect();
        row.sort();
        assert_eq!(
            row,
            [
                "address",
                "avgEntryMarketCap",
                "avgEntryPrice",
                "avgExitMarketCap",
                "avgExitPrice",
                "balance",
                "buyCount",
                "buyVolume",
                "firstPurchaseAt",
                "flags",
                "funding",
                "identities",
                "nativeBalance",
                "platforms",
                "rank",
                "realizedPnl",
                "remainingPct",
                "sellCount",
                "sellVolume",
                "supplyPct",
                "tokensBought",
                "tokensSold",
            ]
        );
        let read: UiTokenHolderList = serde_json::from_value(body).expect("reads back");
        assert_eq!(read, list);
    }

    #[test]
    fn the_query_defaults_and_clamps_its_limit() {
        let absent = TokenHoldersQuery::default();
        assert_eq!(absent.limit(), HOLDERS_LIMIT_DEFAULT);
        assert_eq!(absent.period(), HolderPeriod::Lifetime);

        for (asked, served) in [(0, 1), (200, HOLDERS_LIMIT_MAX), (50, 50)] {
            let query = TokenHoldersQuery { limit: Some(asked), ..TokenHoldersQuery::default() };
            assert_eq!(query.limit(), served, "{asked}");
        }
    }

    #[test]
    fn the_query_reads_its_flags_and_period_and_refuses_the_rest() {
        let query: TokenHoldersQuery =
            serde_json::from_value(json!({ "flags": "bundler,insider", "period": "cycle" }))
                .expect("parses");
        assert_eq!(query.flags, Some(vec![WalletFilter::Bundler, WalletFilter::Insider]));
        assert_eq!(query.period(), HolderPeriod::Cycle);

        for refused in
            [json!({ "flags": "watchlist" }), json!({ "period": "forever" }), json!({ "foo": "1" })]
        {
            serde_json::from_value::<TokenHoldersQuery>(refused).expect_err("refused");
        }
    }
}
