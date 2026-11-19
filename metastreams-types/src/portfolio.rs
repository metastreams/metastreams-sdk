//! The wallet portfolio: the summed spot holdings of a set of wallets.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use utoipa::{IntoParams, ToSchema};

use crate::address::MultiChainAddress;
use crate::chain::{AddressFamily, SpotChain};
use crate::decimal::Decimal;
use crate::money::UiMoney;
use crate::time::UnixMilliseconds;

/// Query for `GET /v1/portfolio`.
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortfolioQuery {
    /// Wallet addresses, CSV; one to 100.
    #[serde(with = "crate::csv::wallets")]
    #[param(value_type = String, example = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin")]
    pub wallets: Vec<MultiChainAddress>,
    /// Chains to scope to, CSV of the chain slugs. Absent means every spot chain.
    #[serde(default, with = "crate::csv::chains", skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, example = "solana,bsc")]
    pub chains: Option<Vec<SpotChain>>,
    /// Optional sections to compute, CSV; only `stats`. Absent returns none of them.
    #[serde(default, with = "crate::csv", skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, example = "stats")]
    pub include: Option<Vec<PortfolioInclude>>,
    /// Drop open positions whose token liquidity is below the low-liquidity floor, or not known.
    /// A position in a stablecoin that a balance row can hold never drops.
    #[serde(default)]
    #[param(default = false)]
    pub hide_low_liquidity: bool,
}

/// An optional section of the wallet portfolio, named in `include=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum PortfolioInclude {
    /// Performance over the standard timeframes.
    Stats,
}

/// The summed spot holdings of the named wallets.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiPortfolioResponse {
    /// Sum of every product's value; spot is the only product.
    #[schema(value_type = Decimal, examples("1250.40"))]
    pub total_value_usd: Decimal,
    /// Serve-time snapshot instant.
    pub as_of: UnixMilliseconds,
    /// Each named wallet's share, in request order. The shares sum to the matching total.
    /// Their `stats` sum to `stats`.
    pub wallets: Vec<UiWalletDescriptor>,
    /// Spot holdings of every named wallet.
    pub spot: UiSpotPortfolio,
    /// Timeframe performance; present only under `include=stats`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<UiPortfolioStats>,
}

/// One named wallet and its share of the portfolio.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiWalletDescriptor {
    /// The wallet's address, under its family.
    pub addresses: UiWalletAddresses,
    /// This wallet's spot holdings.
    pub spot: UiSpotPortfolio,
    /// This wallet's timeframe performance; present only under `include=stats`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<UiPortfolioStats>,
}

/// A wallet's per-family addresses. A named wallet has one family, so the other is absent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiWalletAddresses {
    /// Solana base58 address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solana: Option<String>,
    /// EVM `0x` address, lowercase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evm: Option<String>,
}

impl From<&MultiChainAddress> for UiWalletAddresses {
    fn from(address: &MultiChainAddress) -> Self {
        match address.family() {
            AddressFamily::Solana => Self { solana: Some(address.to_string()), evm: None },
            AddressFamily::Evm => Self { solana: None, evm: Some(address.to_string()) },
        }
    }
}

/// Spot holdings per chain: gas and stablecoin balances plus open positions.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotPortfolio {
    /// `balancesValueUsd + positionsValueUsd`.
    #[schema(value_type = Decimal, examples("1250.40"))]
    pub value_usd: Decimal,
    /// Sum of every chain's priced native and stable balances.
    #[schema(value_type = Decimal, examples("1000"))]
    pub balances_value_usd: Decimal,
    /// Sum of every chain's priced open-position value.
    #[schema(value_type = Decimal, examples("250.40"))]
    pub positions_value_usd: Decimal,
    /// Per-chain cells. The whole portfolio has one per chain in scope, zeroed where it holds
    /// nothing. A wallet's share has only the cells it holds.
    pub chains: Vec<UiSpotPortfolioChain>,
}

impl UiSpotPortfolio {
    /// Totals a cell list. An unpriced component adds nothing.
    #[must_use]
    pub fn from_cells(chains: Vec<UiSpotPortfolioChain>) -> Self {
        let balances_value_usd = chains.iter().fold(Decimal::ZERO, |sum, c| {
            sum + c.native_balance.usd.unwrap_or(Decimal::ZERO) + c.stable_balance_usd
        });
        let positions_value_usd = chains
            .iter()
            .fold(Decimal::ZERO, |sum, c| sum + c.positions_value.usd.unwrap_or(Decimal::ZERO));
        Self {
            value_usd: balances_value_usd + positions_value_usd,
            balances_value_usd,
            positions_value_usd,
            chains,
        }
    }
}

/// One chain's balance and open-position value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotPortfolioChain {
    /// Chain this cell covers.
    pub chain: SpotChain,
    /// The chain's gas asset balance; zero when no balance row exists.
    pub native_balance: UiMoney,
    /// Sum of the stablecoin balances other than the gas asset, priced 1:1.
    #[schema(value_type = Decimal, examples("1000"))]
    pub stable_balance_usd: Decimal,
    /// Sum over open positions of `holdings × token price`.
    pub positions_value: UiMoney,
    /// `nativeBalance.usd + stableBalanceUsd + positionsValue.usd`, priced components only.
    #[schema(value_type = Decimal, examples("1250.40"))]
    pub value_usd: Decimal,
}

/// `include=stats`: performance over the four standard timeframes. A position joins a
/// timeframe by the age of its first purchase, never its last activity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiPortfolioStats {
    /// Positions whose first purchase falls in the last day.
    pub one_day: UiTimeframeStats,
    /// Positions whose first purchase falls in the last seven days.
    pub seven_days: UiTimeframeStats,
    /// Positions whose first purchase falls in the last thirty days.
    pub thirty_days: UiTimeframeStats,
    /// Every position in scope.
    pub max: UiTimeframeStats,
}

/// One timeframe's aggregate over the positions inside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTimeframeStats {
    /// Open positions in this timeframe, priced or not.
    pub open: u64,
    /// Sum of every chain's unrealized PnL in USD.
    #[schema(value_type = Decimal, examples("24.50"))]
    pub unrealized_usd: Decimal,
    /// Sum of every chain's realized PnL in USD.
    #[schema(value_type = Decimal, examples("-35"))]
    pub realized_usd: Decimal,
    /// Trade counts across every cycle.
    pub trades: UiTradeCounts,
    /// Positions per realized-PnL tier.
    pub buckets: UiPnlBuckets,
    /// Per-chain PnL. The whole portfolio has one per chain in scope, zeroed where it has no
    /// position. A wallet's share has only the chains it has a position on.
    pub chains: Vec<UiTimeframeChainStats>,
}

/// One chain's PnL within a timeframe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTimeframeChainStats {
    /// Chain this cell covers.
    pub chain: SpotChain,
    /// Open positions on this chain, priced or not.
    pub open: u64,
    /// Unrealized PnL on what is still held.
    pub unrealized: UiMoney,
    /// Realized PnL across every cycle, from open and closed positions alike.
    pub realized: UiMoney,
}

/// Trade counts within a timeframe, over every cycle each position has had.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTradeCounts {
    /// `buys + sells`.
    pub total: u64,
    /// Buy-side trades.
    pub buys: u64,
    /// Sell-side trades.
    pub sells: u64,
}

/// Positions per realized-PnL tier within a timeframe.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiPnlBuckets {
    /// Realized 5x or better.
    pub moonshot: u64,
    /// Realized 2x to 5x.
    pub pump: u64,
    /// Realized 1x to 2x.
    pub win: u64,
    /// Realized down to -50%.
    pub loss: u64,
    /// Realized worse than -50%.
    pub rekt: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOL_WALLET: &str = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";

    #[test]
    fn the_include_list_reads_stats_and_refuses_anything_else() {
        let query: PortfolioQuery = serde_json::from_value(serde_json::json!({
            "wallets": SOL_WALLET,
            "include": "stats",
        }))
        .expect("parses");
        assert_eq!(query.include, Some(vec![PortfolioInclude::Stats]));
        assert!(!query.hide_low_liquidity, "it hides nothing by default");

        let error = serde_json::from_value::<PortfolioQuery>(serde_json::json!({
            "wallets": SOL_WALLET,
            "include": "stats,breakdown",
        }))
        .expect_err("refused");
        assert!(error.to_string().contains("breakdown"), "{error}");
    }

    #[test]
    fn a_timeframe_without_its_open_count_is_refused() {
        let stats = serde_json::json!({
            "unrealizedUsd": "0",
            "realizedUsd": "0",
            "trades": UiTradeCounts::default(),
            "buckets": UiPnlBuckets::default(),
            "chains": [],
        });

        let error = serde_json::from_value::<UiTimeframeStats>(stats).expect_err("refused");

        assert!(error.to_string().contains("open"), "{error}");
    }
}
