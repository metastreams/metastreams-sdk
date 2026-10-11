//! The token object — the platform's most-consumed schema. One object carries a token's
//! identity, its pools, its pricing and liquidity, and its windowed stats.

pub mod annotation;
pub mod fees;
pub mod funding;
pub mod graduation;
pub mod holders;
pub mod platform;
pub mod pool;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::chain::SpotChain;
use crate::decimal::Decimal;
use crate::money::UiMoney;
use crate::time::UnixMilliseconds;
use crate::token::annotation::TokenAnnotation;
use crate::token::fees::UiTokenTradeFees;
use crate::token::funding::{FundingSourceKind, FundingStatus};
use crate::token::platform::UiPlatform;
use crate::token::pool::UiTokenPool;

/// `GET /v1/tokens/{chain}/{address}` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotToken {
    /// Chain the token trades on.
    pub chain: SpotChain,
    /// Token address.
    pub address: String,
    /// Token name.
    pub name: String,
    /// Token symbol.
    pub symbol: String,
    /// Token decimals.
    pub decimals: u8,
    /// Total supply.
    #[schema(value_type = Decimal, examples("1000000000"))]
    pub supply: Decimal,
    /// Creator address.
    pub creator: String,
    /// The creator wallet's first inbound funding. Always present; its status says whether the
    /// lookup has settled.
    pub dev_funding: UiDevFunding,
    /// Creation timestamp.
    pub created_at: UnixMilliseconds,
    /// Launchpad the token launched through; `null` for non-launchpad tokens.
    pub launchpad: Option<String>,
    /// Pool the launchpad pool migrated into; `null` until graduation.
    pub graduated_to_pool: Option<String>,
    /// Venue the graduation pool trades on; `null` until graduation.
    pub graduated_to_dex: Option<String>,
    /// When the token graduated off its launchpad; `null` until graduation.
    pub graduated_at: Option<UnixMilliseconds>,
    /// Bonding-curve completion, 0-100; `null` off launchpad curves.
    #[schema(value_type = Option<Decimal>, examples("87.5"))]
    pub bonding_curve_pct: Option<Decimal>,
    /// Venues the token's top pools trade on, deepest first.
    #[schema(example = json!(["pumpswap", "raydium"]))]
    pub dexes: Vec<String>,
    /// The token's top pools by liquidity, deepest first.
    #[serde(default)]
    pub pools: Vec<UiTokenPool>,

    /// Address of the quote asset in the token's deepest pool; `null` on a row an earlier build wrote.
    pub anchor_address: Option<String>,
    /// Display symbol of that quote asset, on `anchorAddress`'s terms.
    pub anchor_symbol: Option<String>,
    /// Current price.
    pub price: UiMoney,
    /// Current market cap.
    pub market_cap: UiMoney,
    /// Current liquidity.
    pub liquidity: UiMoney,
    /// All-time-high price and market cap.
    pub ath: UiTokenAth,

    /// Trailing-window stats, one key per rolling window.
    pub stats: UiTokenStats,
    /// All-time volume and fee totals.
    pub totals: UiTokenTotals,
    /// Holder-risk breakdown.
    pub holders: UiTokenHolders,
    /// On-chain security flags.
    pub security: UiTokenSecurity,
    /// What a trade costs here and where the tax goes; absent where no source states either.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fees: Option<UiTokenTradeFees>,
    /// Off-chain project metadata.
    pub metadata: UiTokenMetadata,
    /// The token's external-platform listings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<UiPlatform>,
    /// Platform-scoped quirk flags the launch carried.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schema(example = json!([{"kind": "mayhem_mode"}]))]
    pub annotations: Vec<TokenAnnotation>,

    /// Last update timestamp.
    pub updated_at: UnixMilliseconds,
}

/// The creator wallet's first inbound funding.
///
/// `status` separates a lookup still in flight from one that settled on a wallet with no
/// funder to name. Every other field is `null` in both cases.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiDevFunding {
    /// Whether the lookup has settled.
    pub status: FundingStatus,
    /// Funder wallet or exchange address.
    pub address: Option<String>,
    /// What the funder is (exchange, bridge, wallet, …).
    pub source_kind: Option<FundingSourceKind>,
    /// Display label for the funder, e.g. an exchange name.
    pub source_label: Option<String>,
}

/// All-time-high price and market cap, each with the timestamp it was set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenAth {
    /// All-time-high price.
    pub price: UiMoney,
    /// When the all-time-high price was set.
    pub price_at: UnixMilliseconds,
    /// All-time-high market cap.
    pub market_cap: UiMoney,
    /// When the all-time-high market cap was set.
    pub market_cap_at: UnixMilliseconds,
}

/// Trade stats over a trailing window that ends now, not a calendar bucket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct UiTokenStats {
    /// Trailing 5 minutes.
    #[serde(rename = "5m")]
    pub m5: UiTokenWindowStats,
    /// Trailing 30 minutes.
    #[serde(rename = "30m")]
    pub m30: UiTokenWindowStats,
    /// Trailing 1 hour.
    #[serde(rename = "1h")]
    pub h1: UiTokenWindowStats,
    /// Trailing 6 hours.
    #[serde(rename = "6h")]
    pub h6: UiTokenWindowStats,
    /// Trailing 24 hours.
    #[serde(rename = "24h")]
    pub h24: UiTokenWindowStats,
}

/// Trade activity over one trailing window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenWindowStats {
    /// Price change over the window, percent.
    #[schema(value_type = Decimal, examples("-3.4"))]
    pub price_change_pct: Decimal,
    /// Trade volume over the window.
    pub volume: UiMoney,
    /// Buy-side trade count.
    pub buy_count: u64,
    /// Sell-side trade count.
    pub sell_count: u64,
    /// Buy-side volume.
    pub buy_volume: UiMoney,
    /// Sell-side volume.
    pub sell_volume: UiMoney,
}

/// All-time trade volume and fee totals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenTotals {
    /// All-time trade volume.
    pub volume: UiMoney,
    /// All-time fee totals by kind.
    pub fees: UiTokenFees,
}

/// All-time fee totals by kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenFees {
    /// Transaction fees. Solana trades older than the venue leg carry their venue fee here.
    pub transaction: UiMoney,
    /// Validator fees.
    pub validator: UiMoney,
    /// Platform fees.
    pub platform: UiMoney,
    /// Venue fees: what traders paid the pools and launchpads.
    #[serde(default)]
    pub dex: UiMoney,
}

/// Holder-risk breakdown. Every `*Pct` field is a percent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenHolders {
    /// Current holder count.
    pub count: u64,
    /// Share of supply held by the top 10 current holders.
    #[schema(value_type = Decimal, examples("34.5"))]
    pub top10_pct: Decimal,
    /// Dev wallet's share of supply.
    #[schema(value_type = Decimal, examples("2.1"))]
    pub dev_pct: Decimal,
    /// Sniper wallets' share of supply.
    #[schema(value_type = Decimal, examples("1.2"))]
    pub sniper_pct: Decimal,
    /// Sniper wallet count.
    pub sniper_count: u64,
    /// Bundler wallets' share of supply.
    #[schema(value_type = Decimal, examples("0.8"))]
    pub bundler_pct: Decimal,
    /// Bundler wallet count.
    pub bundler_count: u64,
    /// Insider wallets' share of supply.
    #[schema(value_type = Decimal, examples("0.5"))]
    pub insider_pct: Decimal,
    /// Insider wallet count.
    pub insider_count: u64,
    /// Phishing wallets' share of supply.
    #[schema(value_type = Decimal, examples("0.1"))]
    pub phishing_pct: Decimal,
    /// Wallets that ever traded this token while carrying a malicious-actor label. Never
    /// decremented, unlike `phishing_pct`.
    pub phishing_count: u64,
    /// Fresh-wallet share of supply.
    #[schema(value_type = Decimal, examples("5.0"))]
    pub fresh_wallet_pct: Decimal,
    /// Fresh wallet count.
    pub fresh_wallet_count: u64,
    /// fomo.family wallets' share of supply.
    #[schema(value_type = Decimal, examples("1.1"))]
    pub fomo_pct: Decimal,
    /// fomo.family wallet count.
    pub fomo_count: u64,
    /// Pro-trader wallets' share of supply.
    #[schema(value_type = Decimal, examples("3.2"))]
    pub pro_trader_pct: Decimal,
    /// Pro-trader wallet count.
    pub pro_trader_count: u64,
    /// Share of current holders a known exchange funded, 0-100. Counted against every holder,
    /// so a token the resolver has not caught up on reads low rather than absent.
    #[schema(value_type = Decimal, examples("18.0"))]
    pub funded_by_cex_pct: Decimal,
}

/// Contract-security flags. Solana reads them on-chain; a contract scan reads them on the EVM
/// chains.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenSecurity {
    /// Mint authority revoked.
    pub mint_disabled: bool,
    /// Freeze authority present.
    pub transfer_blockable: bool,
    /// LP permanently unavailable (burnt + locked), 0-100.
    #[schema(value_type = Option<Decimal>, examples("100"))]
    pub lp_burnt_pct: Option<Decimal>,
    /// Transfer fee the token's own contract charges, in basis points — `250` is 2.50%.
    /// `null` when no source has scored the contract.
    #[schema(value_type = Option<u16>, example = 250)]
    pub transfer_fee_bps: Option<u16>,
    /// What a buy costs in total, in basis points — every fee layer the token's venue charges,
    /// summed. `null` when no source states a rate.
    #[schema(value_type = Option<u16>, example = 100)]
    pub total_buy_bps: Option<u16>,
    /// What a sell costs in total, in basis points; see `totalBuyBps`.
    #[schema(value_type = Option<u16>, example = 200)]
    pub total_sell_bps: Option<u16>,
    /// The token contract's own cut of a buy, in basis points; one layer of `totalBuyBps`.
    /// `null` when the launchpad states no rate.
    #[schema(value_type = Option<u16>, example = 100)]
    pub buy_tax_bps: Option<u16>,
    /// The token contract's own cut of a sell, in basis points; see `buyTaxBps`.
    #[schema(value_type = Option<u16>, example = 200)]
    pub sell_tax_bps: Option<u16>,
    /// Address of the asset the token pays its tax back to holders in.
    /// `null` where no holder payout is known; it does not prove the token pays none.
    #[schema(value_type = Option<String>)]
    pub dividend_token: Option<String>,
    /// Balance of this token a wallet holds to qualify for a payout, in whole tokens.
    #[schema(value_type = Option<Decimal>, examples("10000"))]
    pub dividend_min_holding: Option<Decimal>,
    /// Share of supply held by wallets scored as rug risks, 0-100.
    // The token view has no rug column, so this is a floor, not a reading.
    #[schema(value_type = Decimal, examples("2.0"))]
    pub rug_pct: Decimal,
}

/// Off-chain project metadata; every field nullable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenMetadata {
    /// Project description.
    pub description: Option<String>,
    /// Project website.
    pub website: Option<String>,
    /// Project Twitter/X handle or URL.
    pub twitter: Option<String>,
    /// Project Telegram handle or URL.
    pub telegram: Option<String>,
    /// Project Discord invite URL.
    pub discord: Option<String>,
    /// How many other tokens have claimed the same Twitter/X account. Above zero, the handle
    /// is recycled, which is the scam signal it exists to carry.
    // The token view stores the handle but never counts its reuse.
    pub twitter_reuse_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> UiTokenWindowStats {
        UiTokenWindowStats {
            price_change_pct: "-3.4".parse().expect("decimal"),
            volume: UiMoney::priced(Decimal::ONE, "2".parse::<Decimal>().expect("decimal")),
            buy_count: 3,
            sell_count: 4,
            buy_volume: UiMoney::priced(Decimal::ONE, Decimal::ONE),
            sell_volume: UiMoney::priced(Decimal::ONE, Decimal::ONE),
        }
    }

    #[test]
    fn stats_are_keyed_by_window() {
        let stats =
            UiTokenStats { m5: window(), m30: window(), h1: window(), h6: window(), h24: window() };

        let body = serde_json::to_value(stats).expect("serializes");
        let mut keys: Vec<&String> = body.as_object().expect("object").keys().collect();
        keys.sort();

        assert_eq!(keys, ["1h", "24h", "30m", "5m", "6h"]);
        assert_eq!(body["24h"]["priceChangePct"], "-3.4");
        assert_eq!(body["24h"]["volume"], serde_json::json!({ "native": "1", "usd": "2" }));
    }
}
