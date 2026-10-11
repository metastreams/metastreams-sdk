//! The stream filter language: lists, inclusive ranges, and the token predicate built from them.
//! <https://docs.metastreams.io/streams/filters> documents the matching rules.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::address::MultiChainAddress;
use crate::candle::Timeframe;
use crate::chain::{AnchorToken, SpotChain, de};
use crate::decimal::Decimal;
use crate::error::{ErrorDetail, FieldError};
use crate::token::annotation::TokenAnnotation;
use crate::token::funding::FundingSourceKind;
use crate::token::graduation::GraduationStage;
use crate::trade::{TradeDirection, TradeFlag, TraderSource};

/// The EVM zero address, in [`MultiChainAddress`]'s canonical text.
const EVM_ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";

/// An inclusive `{min, max}` bound. An absent end leaves that side open.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Range<T> {
    /// Lowest value kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<T>,
    /// Highest value kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<T>,
}

impl<T: PartialOrd> Range<T> {
    /// Whether `value` is known and inside the bound. An unknown value fails.
    pub fn contains(&self, value: Option<T>) -> bool {
        let Some(value) = value else { return false };
        self.min.as_ref().is_none_or(|min| &value >= min)
            && self.max.as_ref().is_none_or(|max| &value <= max)
    }

    /// Whether `min` exceeds `max`, so that no value can satisfy the bound.
    #[must_use]
    pub fn is_inverted(&self) -> bool {
        matches!((&self.min, &self.max), (Some(min), Some(max)) if min > max)
    }
}

/// A social link a token can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Social {
    /// An X profile.
    Twitter,
    /// A Telegram group or channel.
    Telegram,
    /// A website.
    Website,
    /// A Discord server.
    Discord,
}

/// A predicate on one token's current state.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenFilter {
    /// Chains to keep; empty keeps every chain.
    #[serde(default, deserialize_with = "de::list", skip_serializing_if = "Vec::is_empty")]
    pub chains: Vec<SpotChain>,
    /// Token addresses to keep, 1 to 100 of them.
    #[serde(default, deserialize_with = "tokens", skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Vec<MultiChainAddress>>,
    /// Creator wallets to keep.
    #[serde(default, deserialize_with = "dev_addresses", skip_serializing_if = "Vec::is_empty")]
    pub dev_addresses: Vec<MultiChainAddress>,
    /// Wallets that funded the creator.
    #[serde(
        default,
        deserialize_with = "funding_addresses",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub funding_addresses: Vec<MultiChainAddress>,
    /// Kinds of source that funded the creator.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub funding_kinds: Vec<FundingSourceKind>,
    /// Substrings of the symbol or name; an address-shaped value matches the token address exactly.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbol_or_name: Vec<String>,

    /// Launchpads the token launched on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocols: Vec<String>,
    /// Venues of any of the token's pools.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dexes: Vec<String>,
    /// Venues the token graduated to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graduated_to_dexes: Vec<String>,
    /// Launchpad stages; a token with no launchpad has no stage.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<GraduationStage>,
    /// Quote assets the token trades against.
    #[serde(default, deserialize_with = "anchors", skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<AnchorToken>,
    /// Annotation kinds the token carries, each written as its bare `kind`.
    #[serde(
        default,
        deserialize_with = "annotations",
        serialize_with = "annotation_kinds",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub annotations: Vec<TokenAnnotation>,

    /// Price in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_usd: Option<Range<Decimal>>,
    /// Price in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_native: Option<Range<Decimal>>,
    /// Market cap in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_cap_usd: Option<Range<Decimal>>,
    /// Market cap in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_cap_native: Option<Range<Decimal>>,
    /// Liquidity in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquidity_usd: Option<Range<Decimal>>,
    /// Liquidity in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquidity_native: Option<Range<Decimal>>,
    /// Supply in whole tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supply: Option<Range<Decimal>>,
    /// All-time-high market cap in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ath_market_cap_usd: Option<Range<Decimal>>,
    /// All-time-high market cap in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ath_market_cap_native: Option<Range<Decimal>>,
    /// Percent the USD market cap sits below its all-time high.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawdown_from_ath_pct: Option<Range<Decimal>>,
    /// Percent of the bonding curve filled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bonding_curve_pct: Option<Range<Decimal>>,
    /// Time since creation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age_ms: Option<Range<u64>>,
    /// Time since graduation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_graduation_ms: Option<Range<u64>>,
    /// Time since the token's state last changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_ms: Option<Range<u64>>,

    /// Rolling-window activity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<WindowStatsFilter>,

    /// Number of holders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holder_count: Option<Range<u64>>,
    /// Percent of supply the ten largest holders hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top10_holders_pct: Option<Range<Decimal>>,
    /// Percent of supply the creator holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev_holding_pct: Option<Range<Decimal>>,
    /// Percent of supply snipers hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snipers_supply_pct: Option<Range<Decimal>>,
    /// Percent of supply bundlers hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundler_supply_pct: Option<Range<Decimal>>,
    /// Percent of supply insiders hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insider_supply_pct: Option<Range<Decimal>>,
    /// Percent of supply fresh wallets hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fresh_wallet_supply_pct: Option<Range<Decimal>>,
    /// Percent of supply fomo traders hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fomo_supply_pct: Option<Range<Decimal>>,
    /// Percent of supply phishing wallets hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phishing_supply_pct: Option<Range<Decimal>>,
    /// Percent of supply pro traders hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pro_trader_supply_pct: Option<Range<Decimal>>,
    /// Percent of holders an exchange funded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub funded_by_cex_pct: Option<Range<Decimal>>,
    /// Number of sniper wallets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sniper_count: Option<Range<u64>>,
    /// Number of bundler wallets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundler_count: Option<Range<u64>>,
    /// Number of insider wallets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insider_count: Option<Range<u64>>,
    /// Number of fresh wallets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fresh_wallet_count: Option<Range<u64>>,
    /// Number of fomo traders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fomo_count: Option<Range<u64>>,
    /// Number of phishing wallets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phishing_count: Option<Range<u64>>,
    /// Number of pro traders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pro_trader_count: Option<Range<u64>>,

    /// Whether minting is disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mint_disabled: Option<bool>,
    /// Whether an authority can block transfers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer_blockable: Option<bool>,
    /// Whether the token paid for a DEX Screener profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dex_paid: Option<bool>,
    /// Percent of LP tokens burnt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lp_burnt_pct: Option<Range<Decimal>>,
    /// Transfer fee in basis points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer_fee_bps: Option<Range<u64>>,
    /// Buy tax in basis points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buy_tax_bps: Option<Range<u64>>,
    /// Sell tax in basis points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sell_tax_bps: Option<Range<u64>>,

    /// Social links the token must carry at least one of.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub socials: Vec<Social>,
    /// All-time DEX fees in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fees_dex_usd: Option<Range<Decimal>>,
    /// All-time DEX fees in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fees_dex_native: Option<Range<Decimal>>,
    /// All-time platform fees in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fees_platform_usd: Option<Range<Decimal>>,
    /// All-time platform fees in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fees_platform_native: Option<Range<Decimal>>,
    /// All-time validator fees in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fees_validator_usd: Option<Range<Decimal>>,
    /// All-time validator fees in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fees_validator_native: Option<Range<Decimal>>,
    /// All-time volume in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_volume_usd: Option<Range<Decimal>>,
    /// All-time volume in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_volume_native: Option<Range<Decimal>>,

    /// Values that drop a token that matches any of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<TokenExclude>,
}

impl TokenFilter {
    /// The most addresses `tokens` takes.
    pub const MAX_TOKENS: usize = 100;
    /// The most values any other list takes, on either side.
    pub const MAX_LIST_VALUES: usize = 100;

    /// Refuse a filter that is out of bounds: a `tokens` list outside 1 to 100, any other list
    /// over [`Self::MAX_LIST_VALUES`], or a range with `min` above `max`.
    pub fn validate(&self) -> Result<(), ErrorDetail> {
        let mut errors = Vec::new();
        self.check("", &mut errors);
        verdict(errors)
    }

    /// Collect each refusal of [`Self::validate`], its field named under `prefix`.
    fn check(&self, prefix: &str, errors: &mut Vec<FieldError>) {
        if let Some(tokens) = &self.tokens
            && !(1..=Self::MAX_TOKENS).contains(&tokens.len())
        {
            errors
                .push(FieldError::new(format!("{prefix}tokens"), "Name 1 to 100 token addresses."));
        }
        let exclude_lists = self.exclude.as_ref().map(TokenExclude::list_lens).unwrap_or_default();
        check_list_lens(prefix, self.list_lens().into_iter().chain(exclude_lists), errors);

        let mut ranges = Ranges::new(prefix, errors);
        ranges.check("priceUsd", &self.price_usd);
        ranges.check("priceNative", &self.price_native);
        ranges.check("marketCapUsd", &self.market_cap_usd);
        ranges.check("marketCapNative", &self.market_cap_native);
        ranges.check("liquidityUsd", &self.liquidity_usd);
        ranges.check("liquidityNative", &self.liquidity_native);
        ranges.check("supply", &self.supply);
        ranges.check("athMarketCapUsd", &self.ath_market_cap_usd);
        ranges.check("athMarketCapNative", &self.ath_market_cap_native);
        ranges.check("drawdownFromAthPct", &self.drawdown_from_ath_pct);
        ranges.check("bondingCurvePct", &self.bonding_curve_pct);
        ranges.check("ageMs", &self.age_ms);
        ranges.check("sinceGraduationMs", &self.since_graduation_ms);
        ranges.check("lastActivityMs", &self.last_activity_ms);
        ranges.check("holderCount", &self.holder_count);
        ranges.check("top10HoldersPct", &self.top10_holders_pct);
        ranges.check("devHoldingPct", &self.dev_holding_pct);
        ranges.check("snipersSupplyPct", &self.snipers_supply_pct);
        ranges.check("bundlerSupplyPct", &self.bundler_supply_pct);
        ranges.check("insiderSupplyPct", &self.insider_supply_pct);
        ranges.check("freshWalletSupplyPct", &self.fresh_wallet_supply_pct);
        ranges.check("fomoSupplyPct", &self.fomo_supply_pct);
        ranges.check("phishingSupplyPct", &self.phishing_supply_pct);
        ranges.check("proTraderSupplyPct", &self.pro_trader_supply_pct);
        ranges.check("fundedByCexPct", &self.funded_by_cex_pct);
        ranges.check("sniperCount", &self.sniper_count);
        ranges.check("bundlerCount", &self.bundler_count);
        ranges.check("insiderCount", &self.insider_count);
        ranges.check("freshWalletCount", &self.fresh_wallet_count);
        ranges.check("fomoCount", &self.fomo_count);
        ranges.check("phishingCount", &self.phishing_count);
        ranges.check("proTraderCount", &self.pro_trader_count);
        ranges.check("lpBurntPct", &self.lp_burnt_pct);
        ranges.check("transferFeeBps", &self.transfer_fee_bps);
        ranges.check("buyTaxBps", &self.buy_tax_bps);
        ranges.check("sellTaxBps", &self.sell_tax_bps);
        ranges.check("feesDexUsd", &self.fees_dex_usd);
        ranges.check("feesDexNative", &self.fees_dex_native);
        ranges.check("feesPlatformUsd", &self.fees_platform_usd);
        ranges.check("feesPlatformNative", &self.fees_platform_native);
        ranges.check("feesValidatorUsd", &self.fees_validator_usd);
        ranges.check("feesValidatorNative", &self.fees_validator_native);
        ranges.check("totalVolumeUsd", &self.total_volume_usd);
        ranges.check("totalVolumeNative", &self.total_volume_native);
        if let Some(stats) = &self.stats {
            for (window, filter) in stats.windows() {
                if let Some(filter) = filter {
                    filter.check(&mut Ranges::new(&format!("{prefix}stats.{window}."), errors));
                }
            }
        }
    }

    /// Each list key but `tokens`, by wire name, with its length.
    fn list_lens(&self) -> [(&'static str, usize); 12] {
        [
            ("chains", self.chains.len()),
            ("devAddresses", self.dev_addresses.len()),
            ("fundingAddresses", self.funding_addresses.len()),
            ("fundingKinds", self.funding_kinds.len()),
            ("symbolOrName", self.symbol_or_name.len()),
            ("protocols", self.protocols.len()),
            ("dexes", self.dexes.len()),
            ("graduatedToDexes", self.graduated_to_dexes.len()),
            ("stages", self.stages.len()),
            ("anchors", self.anchors.len()),
            ("annotations", self.annotations.len()),
            ("socials", self.socials.len()),
        ]
    }
}

/// Activity filters per rolling window.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowStatsFilter {
    /// The last 5 minutes.
    #[serde(default, rename = "5m", skip_serializing_if = "Option::is_none")]
    pub five_minutes: Option<StatsFilter>,
    /// The last 30 minutes.
    #[serde(default, rename = "30m", skip_serializing_if = "Option::is_none")]
    pub thirty_minutes: Option<StatsFilter>,
    /// The last hour.
    #[serde(default, rename = "1h", skip_serializing_if = "Option::is_none")]
    pub one_hour: Option<StatsFilter>,
    /// The last 6 hours.
    #[serde(default, rename = "6h", skip_serializing_if = "Option::is_none")]
    pub six_hours: Option<StatsFilter>,
    /// The last 24 hours.
    #[serde(default, rename = "24h", skip_serializing_if = "Option::is_none")]
    pub one_day: Option<StatsFilter>,
}

impl WindowStatsFilter {
    /// Each window's wire name and its filter.
    #[must_use]
    pub fn windows(&self) -> [(&'static str, Option<&StatsFilter>); 5] {
        [
            ("5m", self.five_minutes.as_ref()),
            ("30m", self.thirty_minutes.as_ref()),
            ("1h", self.one_hour.as_ref()),
            ("6h", self.six_hours.as_ref()),
            ("24h", self.one_day.as_ref()),
        ]
    }
}

/// Activity filters for one rolling window.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatsFilter {
    /// Volume in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_usd: Option<Range<Decimal>>,
    /// Volume in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_native: Option<Range<Decimal>>,
    /// Buy volume in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buy_volume_usd: Option<Range<Decimal>>,
    /// Buy volume in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buy_volume_native: Option<Range<Decimal>>,
    /// Sell volume in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sell_volume_usd: Option<Range<Decimal>>,
    /// Sell volume in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sell_volume_native: Option<Range<Decimal>>,
    /// Buy volume less sell volume, in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_buy_volume_usd: Option<Range<Decimal>>,
    /// Buy volume less sell volume, in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_buy_volume_native: Option<Range<Decimal>>,
    /// Number of buys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buys: Option<Range<u64>>,
    /// Number of sells.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sells: Option<Range<u64>>,
    /// Number of buys and sells.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txns: Option<Range<u64>>,
    /// Signed percent change of the price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_change_pct: Option<Range<Decimal>>,
}

impl StatsFilter {
    fn check(&self, ranges: &mut Ranges<'_>) {
        ranges.check("volumeUsd", &self.volume_usd);
        ranges.check("volumeNative", &self.volume_native);
        ranges.check("buyVolumeUsd", &self.buy_volume_usd);
        ranges.check("buyVolumeNative", &self.buy_volume_native);
        ranges.check("sellVolumeUsd", &self.sell_volume_usd);
        ranges.check("sellVolumeNative", &self.sell_volume_native);
        ranges.check("netBuyVolumeUsd", &self.net_buy_volume_usd);
        ranges.check("netBuyVolumeNative", &self.net_buy_volume_native);
        ranges.check("buys", &self.buys);
        ranges.check("sells", &self.sells);
        ranges.check("txns", &self.txns);
        ranges.check("priceChangePct", &self.price_change_pct);
    }
}

/// Values that drop a token. Each list drops a token that matches any of its values.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenExclude {
    /// Chains to drop.
    #[serde(default, deserialize_with = "de::list", skip_serializing_if = "Vec::is_empty")]
    pub chains: Vec<SpotChain>,
    /// Token addresses to drop.
    #[serde(default, deserialize_with = "exclude_tokens", skip_serializing_if = "Vec::is_empty")]
    pub tokens: Vec<MultiChainAddress>,
    /// Creator wallets to drop.
    #[serde(
        default,
        deserialize_with = "exclude_dev_addresses",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub dev_addresses: Vec<MultiChainAddress>,
    /// Creator funders to drop.
    #[serde(
        default,
        deserialize_with = "exclude_funding_addresses",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub funding_addresses: Vec<MultiChainAddress>,
    /// Kinds of creator funding source to drop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub funding_kinds: Vec<FundingSourceKind>,
    /// Symbol or name substrings to drop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbol_or_name: Vec<String>,
    /// Launchpads to drop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocols: Vec<String>,
    /// Pool venues to drop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dexes: Vec<String>,
    /// Graduation venues to drop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graduated_to_dexes: Vec<String>,
    /// Launchpad stages to drop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<GraduationStage>,
    /// Quote assets to drop.
    #[serde(default, deserialize_with = "exclude_anchors", skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<AnchorToken>,
    /// Annotation kinds to drop, each written as its bare `kind`.
    #[serde(
        default,
        deserialize_with = "exclude_annotations",
        serialize_with = "annotation_kinds",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub annotations: Vec<TokenAnnotation>,
    /// Social links whose presence drops a token.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub socials: Vec<Social>,
    /// Website hosts to drop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub websites: Vec<String>,
    /// X handles to drop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub twitter: Vec<String>,
}

impl TokenExclude {
    /// Each list key, by wire path, with its length.
    fn list_lens(&self) -> [(&'static str, usize); 15] {
        [
            ("exclude.chains", self.chains.len()),
            ("exclude.tokens", self.tokens.len()),
            ("exclude.devAddresses", self.dev_addresses.len()),
            ("exclude.fundingAddresses", self.funding_addresses.len()),
            ("exclude.fundingKinds", self.funding_kinds.len()),
            ("exclude.symbolOrName", self.symbol_or_name.len()),
            ("exclude.protocols", self.protocols.len()),
            ("exclude.dexes", self.dexes.len()),
            ("exclude.graduatedToDexes", self.graduated_to_dexes.len()),
            ("exclude.stages", self.stages.len()),
            ("exclude.anchors", self.anchors.len()),
            ("exclude.annotations", self.annotations.len()),
            ("exclude.socials", self.socials.len()),
            ("exclude.websites", self.websites.len()),
            ("exclude.twitter", self.twitter.len()),
        ]
    }
}

/// A predicate on one trade. Different keys are ANDed; values in one list are ORed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TradeFilter {
    /// Chains to keep; empty keeps every chain.
    #[serde(default, deserialize_with = "de::list", skip_serializing_if = "Vec::is_empty")]
    pub chains: Vec<SpotChain>,
    /// Traded tokens to keep; absent keeps every token.
    #[serde(default, deserialize_with = "trade_tokens", skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Vec<MultiChainAddress>>,
    /// Trader wallets to keep; absent keeps every wallet.
    #[serde(default, deserialize_with = "wallets", skip_serializing_if = "Option::is_none")]
    pub wallets: Option<Vec<MultiChainAddress>>,
    /// Pools or markets to keep; absent keeps every pool.
    #[serde(default, deserialize_with = "pools", skip_serializing_if = "Option::is_none")]
    pub pools: Option<Vec<MultiChainAddress>>,

    /// Venues the trade executed on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dexes: Vec<String>,
    /// Launchpads the traded token launched on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub launchpads: Vec<String>,
    /// Quote assets the trade priced against.
    #[serde(default, deserialize_with = "anchors", skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<AnchorToken>,
    /// Trading terminals the trade routed through.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
    /// Trader classifications to keep.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<TradeFlag>,
    /// What named the trader of an EVM trade. A Solana trade has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trader_sources: Vec<TraderSource>,

    /// Buy or sell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<TradeDirection>,
    /// Trade value in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_usd: Option<Range<Decimal>>,
    /// Trade value in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_native: Option<Range<Decimal>>,
    /// Token units traded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<Range<Decimal>>,
    /// Fill price in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_usd: Option<Range<Decimal>>,
    /// Fill price in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_native: Option<Range<Decimal>>,
    /// Market cap at the fill price, in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_cap_usd: Option<Range<Decimal>>,
    /// Market cap at the fill price, in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_cap_native: Option<Range<Decimal>>,
    /// Pool liquidity at the trade, in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquidity_usd: Option<Range<Decimal>>,
    /// Pool liquidity at the trade, in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquidity_native: Option<Range<Decimal>>,
    /// Percent the fill price moved from the price before the trade.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_impact_pct: Option<Range<Decimal>>,
    /// Validator tips and MEV bribes, in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fees_validator_usd: Option<Range<Decimal>>,
    /// Validator tips and MEV bribes, in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fees_validator_native: Option<Range<Decimal>>,
    /// The trading terminal's cut, in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fees_platform_usd: Option<Range<Decimal>>,
    /// The trading terminal's cut, in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fees_platform_native: Option<Range<Decimal>>,
    /// Whether the trade is a sell that leaves the trader none of the token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_exit: Option<bool>,

    /// The traded token's current state, as a `spot.tokens` filter tests it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<TokenFilter>,

    /// Values that drop a trade that matches any of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<TradeExclude>,
}

impl TradeFilter {
    /// Refuse a filter that is out of bounds. That is an empty address list, a list over
    /// [`TokenFilter::MAX_LIST_VALUES`], or the zero address in `pools`. It is also a range with
    /// `min` above `max`, here or in `token`.
    pub fn validate(&self) -> Result<(), ErrorDetail> {
        let mut errors = Vec::new();
        check_address_lists(
            [("tokens", &self.tokens), ("wallets", &self.wallets), ("pools", &self.pools)],
            &mut errors,
        );
        let exclude_lists = self.exclude.as_ref().map(TradeExclude::list_lens).unwrap_or_default();
        check_list_lens("", self.list_lens().into_iter().chain(exclude_lists), &mut errors);
        let mut ranges = Ranges::new("", &mut errors);
        ranges.check("volumeUsd", &self.volume_usd);
        ranges.check("volumeNative", &self.volume_native);
        ranges.check("amount", &self.amount);
        ranges.check("priceUsd", &self.price_usd);
        ranges.check("priceNative", &self.price_native);
        ranges.check("marketCapUsd", &self.market_cap_usd);
        ranges.check("marketCapNative", &self.market_cap_native);
        ranges.check("liquidityUsd", &self.liquidity_usd);
        ranges.check("liquidityNative", &self.liquidity_native);
        ranges.check("priceImpactPct", &self.price_impact_pct);
        ranges.check("feesValidatorUsd", &self.fees_validator_usd);
        ranges.check("feesValidatorNative", &self.fees_validator_native);
        ranges.check("feesPlatformUsd", &self.fees_platform_usd);
        ranges.check("feesPlatformNative", &self.fees_platform_native);
        check_pools_and_token(&self.pools, self.token.as_ref(), &mut errors);
        verdict(errors)
    }

    /// Each list key but the address lists, which the key's address budget bounds, with its
    /// length.
    fn list_lens(&self) -> [(&'static str, usize); 7] {
        [
            ("chains", self.chains.len()),
            ("dexes", self.dexes.len()),
            ("launchpads", self.launchpads.len()),
            ("anchors", self.anchors.len()),
            ("platforms", self.platforms.len()),
            ("flags", self.flags.len()),
            ("traderSources", self.trader_sources.len()),
        ]
    }
}

/// Values that drop a trade. Each list drops a trade that matches any of its values.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TradeExclude {
    /// Trader wallets to drop.
    #[serde(default, deserialize_with = "exclude_wallets", skip_serializing_if = "Vec::is_empty")]
    pub wallets: Vec<MultiChainAddress>,
    /// Trader classifications to drop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<TradeFlag>,
    /// Trading terminals to drop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
}

impl TradeExclude {
    /// Each list key, by wire path, with its length.
    fn list_lens(&self) -> [(&'static str, usize); 3] {
        [
            ("exclude.wallets", self.wallets.len()),
            ("exclude.flags", self.flags.len()),
            ("exclude.platforms", self.platforms.len()),
        ]
    }
}

/// A predicate on one transfer. Different keys are ANDed; values in one list are ORed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferFilter {
    /// Chains to keep; empty keeps every chain.
    #[serde(default, deserialize_with = "de::list", skip_serializing_if = "Vec::is_empty")]
    pub chains: Vec<SpotChain>,
    /// Transferred tokens to keep; absent keeps every token. A native transfer fails the list.
    #[serde(default, deserialize_with = "tokens", skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Vec<MultiChainAddress>>,
    /// Wallets to keep on either side; absent keeps every wallet.
    #[serde(default, deserialize_with = "wallets", skip_serializing_if = "Option::is_none")]
    pub wallets: Option<Vec<MultiChainAddress>>,
    /// Sending wallets to keep; absent keeps every sender.
    #[serde(default, deserialize_with = "senders", skip_serializing_if = "Option::is_none")]
    pub senders: Option<Vec<MultiChainAddress>>,
    /// Receiving wallets to keep; absent keeps every recipient.
    #[serde(default, deserialize_with = "recipients", skip_serializing_if = "Option::is_none")]
    pub recipients: Option<Vec<MultiChainAddress>>,

    /// `true` keeps only native-asset transfers; `false` keeps only token transfers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<bool>,
    /// Token units moved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<Range<Decimal>>,

    /// The transferred token's current state, as a `spot.tokens` filter tests it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<TokenFilter>,

    /// Values that drop a transfer that matches any of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<TransferExclude>,
}

impl TransferFilter {
    /// Refuse a filter that is out of bounds. That is an empty address list, a list over
    /// [`TokenFilter::MAX_LIST_VALUES`], or a range with `min` above `max`, here or in `token`.
    /// It is also `native: true` beside `tokens` or `token`, which no transfer can match.
    pub fn validate(&self) -> Result<(), ErrorDetail> {
        let mut errors = Vec::new();
        check_address_lists(
            [
                ("tokens", &self.tokens),
                ("wallets", &self.wallets),
                ("senders", &self.senders),
                ("recipients", &self.recipients),
            ],
            &mut errors,
        );
        let exclude_lists =
            self.exclude.as_ref().map(TransferExclude::list_lens).unwrap_or_default();
        let lens = [("chains", self.chains.len())].into_iter().chain(exclude_lists);
        check_list_lens("", lens, &mut errors);
        Ranges::new("", &mut errors).check("amount", &self.amount);
        if let Some(token) = &self.token {
            token.check("token.", &mut errors);
        }
        if self.native == Some(true) {
            for (key, sent) in [("tokens", self.tokens.is_some()), ("token", self.token.is_some())]
            {
                if sent {
                    let message = format!("A native transfer has no token to match `{key}`.");
                    errors.push(FieldError::new("native", message));
                }
            }
        }
        verdict(errors)
    }
}

/// Values that drop a transfer. Each list drops a transfer that matches any of its values.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferExclude {
    /// Wallets to drop on either side.
    #[serde(default, deserialize_with = "exclude_wallets", skip_serializing_if = "Vec::is_empty")]
    pub wallets: Vec<MultiChainAddress>,
    /// Transferred tokens to drop.
    #[serde(default, deserialize_with = "exclude_tokens", skip_serializing_if = "Vec::is_empty")]
    pub tokens: Vec<MultiChainAddress>,
}

impl TransferExclude {
    /// Each list key, by wire path, with its length.
    fn list_lens(&self) -> [(&'static str, usize); 2] {
        [("exclude.wallets", self.wallets.len()), ("exclude.tokens", self.tokens.len())]
    }
}

/// A predicate on one candle bucket. Different keys are ANDed; values in one list are ORed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandleFilter {
    /// Chains to keep; empty keeps every chain.
    #[serde(default, deserialize_with = "de::list", skip_serializing_if = "Vec::is_empty")]
    pub chains: Vec<SpotChain>,
    /// Tokens to keep; absent keeps every token.
    #[serde(default, deserialize_with = "trade_tokens", skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Vec<MultiChainAddress>>,
    /// Pools to keep. Set, the buckets are pool candles; absent, they are token candles.
    #[serde(default, deserialize_with = "pools", skip_serializing_if = "Option::is_none")]
    pub pools: Option<Vec<MultiChainAddress>>,
    /// Bucket widths to keep; empty keeps every width.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timeframes: Vec<Timeframe>,

    /// Traded value in the bucket, in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_usd: Option<Range<Decimal>>,
    /// Traded value in the bucket, in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_native: Option<Range<Decimal>>,
    /// Buys in the bucket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buy_count: Option<Range<u64>>,
    /// Sells in the bucket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sell_count: Option<Range<u64>>,
    /// Percent the close moved from the open, signed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_pct: Option<Range<Decimal>>,
    /// High less low, as a percent of the open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range_pct: Option<Range<Decimal>>,
    /// The token's liquidity at the close, in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquidity_usd: Option<Range<Decimal>>,
    /// The token's liquidity at the close, in the chain's gas asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquidity_native: Option<Range<Decimal>>,

    /// The token's current state, as a `spot.tokens` filter tests it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<TokenFilter>,
}

impl CandleFilter {
    /// Refuse a filter that is out of bounds. That is an empty address list, a list over
    /// [`TokenFilter::MAX_LIST_VALUES`], or the zero address in `pools`. It is also a range with
    /// `min` above `max`, here or in `token`.
    pub fn validate(&self) -> Result<(), ErrorDetail> {
        let mut errors = Vec::new();
        check_address_lists([("tokens", &self.tokens), ("pools", &self.pools)], &mut errors);
        check_list_lens("", [("chains", self.chains.len())], &mut errors);
        let mut ranges = Ranges::new("", &mut errors);
        ranges.check("volumeUsd", &self.volume_usd);
        ranges.check("volumeNative", &self.volume_native);
        ranges.check("buyCount", &self.buy_count);
        ranges.check("sellCount", &self.sell_count);
        ranges.check("changePct", &self.change_pct);
        ranges.check("rangePct", &self.range_pct);
        ranges.check("liquidityUsd", &self.liquidity_usd);
        ranges.check("liquidityNative", &self.liquidity_native);
        check_pools_and_token(&self.pools, self.token.as_ref(), &mut errors);
        verdict(errors)
    }
}

/// Collects one validation error per inverted range, each named by its wire path.
struct Ranges<'a> {
    prefix: &'a str,
    errors: &'a mut Vec<FieldError>,
}

impl<'a> Ranges<'a> {
    fn new(prefix: &'a str, errors: &'a mut Vec<FieldError>) -> Self {
        Self { prefix, errors }
    }

    fn check<T: PartialOrd>(&mut self, field: &str, range: &Option<Range<T>>) {
        if range.as_ref().is_some_and(Range::is_inverted) {
            self.errors.push(FieldError::new(
                format!("{}{field}", self.prefix),
                "`min` must not exceed `max`.",
            ));
        }
    }
}

/// Refuse each address list in `lists` that is sent empty.
fn check_address_lists<const N: usize>(
    lists: [(&str, &Option<Vec<MultiChainAddress>>); N],
    errors: &mut Vec<FieldError>,
) {
    for (field, addresses) in lists {
        if addresses.as_ref().is_some_and(Vec::is_empty) {
            errors.push(FieldError::new(field, "Name at least one address, or leave the key out."));
        }
    }
}

/// Refuse each list in `lens` that holds more than [`TokenFilter::MAX_LIST_VALUES`], named under
/// `prefix`.
fn check_list_lens<'a>(
    prefix: &str,
    lens: impl IntoIterator<Item = (&'a str, usize)>,
    errors: &mut Vec<FieldError>,
) {
    for (field, len) in lens {
        if len > TokenFilter::MAX_LIST_VALUES {
            let message = format!("Name at most {} values.", TokenFilter::MAX_LIST_VALUES);
            errors.push(FieldError::new(format!("{prefix}{field}"), message));
        }
    }
}

/// Refuse the zero address in `pools`, and each refusal of the nested `token` filter.
fn check_pools_and_token(
    pools: &Option<Vec<MultiChainAddress>>,
    token: Option<&TokenFilter>,
    errors: &mut Vec<FieldError>,
) {
    // A trade row names the zero address when its pool is unknown.
    if pools.iter().flatten().any(|pool| pool.to_string() == EVM_ZERO_ADDRESS) {
        errors.push(FieldError::new("pools", "The zero address names no pool."));
    }
    if let Some(token) = token {
        token.check("token.", errors);
    }
}

/// `Ok` when `errors` is empty, else one validation error that carries them all.
fn verdict(errors: Vec<FieldError>) -> Result<(), ErrorDetail> {
    if errors.is_empty() { Ok(()) } else { Err(ErrorDetail::validation(errors)) }
}

fn tokens<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<MultiChainAddress>>, D::Error> {
    addresses(d, "tokens").map(Some)
}

fn dev_addresses<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<MultiChainAddress>, D::Error> {
    addresses(d, "devAddresses")
}

fn funding_addresses<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<MultiChainAddress>, D::Error> {
    addresses(d, "fundingAddresses")
}

fn trade_tokens<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<Vec<MultiChainAddress>>, D::Error> {
    addresses(d, "tokens").map(Some)
}

fn wallets<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<MultiChainAddress>>, D::Error> {
    addresses(d, "wallets").map(Some)
}

fn senders<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<MultiChainAddress>>, D::Error> {
    addresses(d, "senders").map(Some)
}

fn recipients<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<MultiChainAddress>>, D::Error> {
    addresses(d, "recipients").map(Some)
}

fn pools<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<MultiChainAddress>>, D::Error> {
    addresses(d, "pools").map(Some)
}

fn exclude_wallets<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<MultiChainAddress>, D::Error> {
    addresses(d, "exclude.wallets")
}

fn exclude_tokens<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<MultiChainAddress>, D::Error> {
    addresses(d, "exclude.tokens")
}

fn exclude_dev_addresses<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<MultiChainAddress>, D::Error> {
    addresses(d, "exclude.devAddresses")
}

fn exclude_funding_addresses<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<MultiChainAddress>, D::Error> {
    addresses(d, "exclude.fundingAddresses")
}

fn anchors<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<AnchorToken>, D::Error> {
    known_anchors(d, "anchors")
}

fn exclude_anchors<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<AnchorToken>, D::Error> {
    known_anchors(d, "exclude.anchors")
}

fn annotations<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<TokenAnnotation>, D::Error> {
    known_annotations(d, "annotations")
}

fn exclude_annotations<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<TokenAnnotation>, D::Error> {
    known_annotations(d, "exclude.annotations")
}

/// A list of addresses. A malformed one is refused under `field`.
fn addresses<'de, D: Deserializer<'de>>(
    d: D,
    field: &str,
) -> Result<Vec<MultiChainAddress>, D::Error> {
    Vec::<String>::deserialize(d)?
        .iter()
        .map(|raw| raw.parse().map_err(|_| refused(field, "The value is not a valid address.")))
        .collect()
}

/// A list of quote assets. `AnchorToken`'s catch-all is refused, so a typo cannot match.
fn known_anchors<'de, D: Deserializer<'de>>(
    d: D,
    field: &str,
) -> Result<Vec<AnchorToken>, D::Error> {
    let anchors = Vec::<AnchorToken>::deserialize(d)?;
    if anchors.contains(&AnchorToken::Unknown) {
        return Err(refused(field, "The value is not a known quote asset."));
    }
    Ok(anchors)
}

/// A list of bare annotation kinds. A kind this build does not know is refused, so a typo cannot
/// match.
fn known_annotations<'de, D: Deserializer<'de>>(
    d: D,
    field: &str,
) -> Result<Vec<TokenAnnotation>, D::Error> {
    Vec::<String>::deserialize(d)?
        .into_iter()
        .map(|kind| {
            serde_json::from_value::<TokenAnnotation>(serde_json::json!({ "kind": kind }))
                .ok()
                .filter(|annotation| *annotation != TokenAnnotation::Unknown)
                .ok_or_else(|| refused(field, "The value is not a known annotation kind."))
        })
        .collect()
}

/// Writes each annotation as its bare `kind`, the shape [`known_annotations`] reads.
fn annotation_kinds<S: Serializer>(
    annotations: &[TokenAnnotation],
    s: S,
) -> Result<S::Ok, S::Error> {
    s.collect_seq(annotations.iter().map(TokenAnnotation::kind))
}

/// A validation error on `field`, as a serde error.
fn refused<E: serde::de::Error>(field: &str, message: &str) -> E {
    ErrorDetail::validation(vec![FieldError::new(field, message)]).into_de_error()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::error::ErrorCode;

    const SOL_TOKEN: &str = "So11111111111111111111111111111111111111112";

    fn filter(json: serde_json::Value) -> Result<TokenFilter, serde_json::Error> {
        serde_json::from_value(json)
    }

    #[test]
    fn a_range_keeps_known_values_inside_both_ends() {
        let range = Range { min: Some(10u64), max: Some(20) };

        assert!(range.contains(Some(10)));
        assert!(range.contains(Some(20)));
        assert!(!range.contains(Some(21)));
        assert!(!range.contains(None), "an unknown value passed");
        assert!(Range { min: None, max: Some(5u64) }.contains(Some(0)));
    }

    #[test]
    fn the_spec_example_parses() {
        let parsed = filter(serde_json::json!({
            "chains": ["solana", "base"],
            "protocols": ["pumpfun", "clanker"],
            "marketCapUsd": { "min": "100000", "max": "5000000" },
            "stats": { "1h": { "priceChangePct": { "min": "20" } } },
            "top10HoldersPct": { "max": "30" },
            "mintDisabled": true,
            "socials": ["twitter", "telegram"],
            "exclude": { "annotations": ["mayhem_mode"] }
        }))
        .expect("parses");

        assert_eq!(parsed.chains, [SpotChain::Solana, SpotChain::Base]);
        assert!(parsed.top10_holders_pct.is_some());
        let one_hour = parsed.stats.and_then(|stats| stats.one_hour).expect("1h");
        assert_eq!(
            one_hour.price_change_pct.and_then(|range| range.min),
            Some("20".parse().expect("a decimal"))
        );
    }

    #[test]
    fn an_empty_filter_is_the_firehose() {
        assert_eq!(filter(serde_json::json!({})).expect("parses"), TokenFilter::default());
    }

    #[test]
    fn an_unknown_key_is_refused() {
        filter(serde_json::json!({ "marketcap": { "min": "1" } })).expect_err("unknown key");
        filter(serde_json::json!({ "stats": { "2h": {} } })).expect_err("unknown window");
        filter(serde_json::json!({ "liquidityUsd": { "low": "1" } })).expect_err("unknown bound");
    }

    #[test]
    fn an_inverted_range_is_refused_by_its_path() {
        let parsed = filter(serde_json::json!({
            "marketCapUsd": { "min": "10", "max": "1" },
            "stats": { "5m": { "buys": { "min": 3, "max": 2 } } }
        }))
        .expect("parses");

        let error = parsed.validate().expect_err("inverted");
        assert_eq!(error.code, ErrorCode::ValidationError);
        let details = error.details.expect("details");
        assert_eq!(details[0]["field"], "marketCapUsd");
        assert_eq!(details[1]["field"], "stats.5m.buys");
    }

    #[test]
    fn a_tokens_list_takes_1_to_100_addresses() {
        let many = vec![SOL_TOKEN; TokenFilter::MAX_TOKENS + 1];
        for json in [serde_json::json!({ "tokens": [] }), serde_json::json!({ "tokens": many })] {
            filter(json).expect("parses").validate().expect_err("out of bounds");
        }
        filter(serde_json::json!({ "tokens": [SOL_TOKEN] }))
            .expect("parses")
            .validate()
            .expect("one address");
    }

    #[test]
    fn the_trade_filter_reads_every_spec_key() {
        let parsed: TradeFilter = serde_json::from_value(serde_json::json!({
            "chains": ["solana"],
            "tokens": [SOL_TOKEN],
            "wallets": [SOL_TOKEN],
            "pools": [SOL_TOKEN],
            "dexes": ["raydium_amm_v4"],
            "launchpads": ["pumpfun"],
            "anchors": ["usdc"],
            "platforms": ["axiom"],
            "flags": ["kol", "fomoscan", "pro_trader"],
            "traderSources": ["tx_origin"],
            "side": "sell",
            "volumeUsd": { "min": "100" },
            "amount": { "max": "5" },
            "priceImpactPct": { "min": "1" },
            "feesValidatorNative": { "min": "0.001" },
            "fullExit": true,
            "token": { "marketCapUsd": { "max": "1000000" } },
            "exclude": { "wallets": [SOL_TOKEN], "flags": ["sniper"], "platforms": ["photon"] }
        }))
        .expect("parses");

        assert_eq!(parsed.flags, [TradeFlag::Kol, TradeFlag::Fomoscan, TradeFlag::ProTrader]);
        assert_eq!(parsed.side, Some(TradeDirection::Sell));
        assert!(parsed.token.is_some());
        parsed.validate().expect("valid");
    }

    #[test]
    fn an_absent_address_list_keeps_every_address_and_an_empty_one_is_refused() {
        let parse = |json| serde_json::from_value::<TradeFilter>(json).expect("parses");

        let every = parse(serde_json::json!({}));
        assert!(every.tokens.is_none() && every.wallets.is_none() && every.pools.is_none());
        every.validate().expect("valid");
        for field in ["tokens", "wallets", "pools"] {
            let empty = parse(serde_json::json!({ field: [] }));
            let details = empty.validate().expect_err("empty").details.expect("details");
            assert_eq!(details[0]["field"], field);
        }
    }

    #[test]
    fn fomo_and_fomoscan_are_two_trade_flags() {
        let parsed: TradeFilter =
            serde_json::from_value(serde_json::json!({ "flags": ["fomo", "fomoscan"] }))
                .expect("parses");
        assert_eq!(parsed.flags, [TradeFlag::Fomo, TradeFlag::Fomoscan]);
    }

    #[test]
    fn a_trade_filter_refuses_an_inverted_range_by_its_path() {
        let parsed: TradeFilter = serde_json::from_value(serde_json::json!({
            "volumeUsd": { "min": "10", "max": "1" },
            "token": { "stats": { "1h": { "txns": { "min": 3, "max": 2 } } } }
        }))
        .expect("parses");

        let details = parsed.validate().expect_err("inverted").details.expect("details");
        assert_eq!(details[0]["field"], "volumeUsd");
        assert_eq!(details[1]["field"], "token.stats.1h.txns");
    }

    #[test]
    fn a_pool_is_an_address_and_never_the_zero_address() {
        let parse =
            |pools| serde_json::from_value::<TradeFilter>(serde_json::json!({ "pools": pools }));
        parse(serde_json::json!(["not a pool"])).expect_err("malformed");

        let zero = parse(serde_json::json!(["0x0000000000000000000000000000000000000000"]))
            .expect("parses");
        let details = zero.validate().expect_err("zero").details.expect("details");
        assert_eq!(details[0]["field"], "pools");
    }

    #[test]
    fn a_trade_filter_refuses_an_unknown_anchor_and_a_long_list() {
        serde_json::from_value::<TradeFilter>(serde_json::json!({ "anchors": ["usdcc"] }))
            .expect_err("unknown anchor");

        let many = vec!["axiom"; TokenFilter::MAX_LIST_VALUES + 1];
        let parsed: TradeFilter =
            serde_json::from_value(serde_json::json!({ "exclude": { "platforms": many } }))
                .expect("parses");
        let details = parsed.validate().expect_err("too long").details.expect("details");
        assert_eq!(details[0]["field"], "exclude.platforms");
    }

    #[test]
    fn a_trade_exclude_takes_only_wallets_flags_and_platforms() {
        serde_json::from_value::<TradeFilter>(serde_json::json!({ "exclude": { "dexes": ["x"] } }))
            .expect_err("unknown exclude key");
    }

    #[test]
    fn an_unknown_venue_name_is_accepted() {
        let parsed = filter(serde_json::json!({ "protocols": ["not_a_launchpad"] }));
        parsed.expect("parses").validate().expect("valid");
    }

    #[test]
    fn an_unknown_anchor_or_annotation_is_refused() {
        for json in [
            serde_json::json!({ "anchors": ["usdcc"] }),
            serde_json::json!({ "anchors": ["unknown"] }),
            serde_json::json!({ "exclude": { "anchors": ["usdcc"] } }),
            serde_json::json!({ "annotations": ["mayhem"] }),
            serde_json::json!({ "annotations": ["unknown"] }),
            serde_json::json!({ "exclude": { "annotations": ["mayhem_mod"] } }),
        ] {
            filter(json.clone()).expect_err(&format!("{json} parsed"));
        }
        filter(
            serde_json::json!({ "anchors": ["usdc"], "exclude": { "annotations": ["tax_token"] } }),
        )
        .expect("known values");
    }

    #[test]
    fn annotations_round_trip_as_bare_kinds() {
        let json = serde_json::json!({
            "annotations": ["tax_token"],
            "exclude": { "annotations": ["mayhem_mode"] }
        });
        let parsed = filter(json.clone()).expect("parses");

        assert_eq!(parsed.annotations, [TokenAnnotation::TaxToken]);
        assert_eq!(serde_json::to_value(&parsed).expect("writes"), json);
    }

    #[test]
    fn the_candle_filter_reads_every_spec_key() {
        let parsed: CandleFilter = serde_json::from_value(serde_json::json!({
            "chains": ["solana"],
            "tokens": [SOL_TOKEN],
            "pools": [SOL_TOKEN],
            "timeframes": ["1s", "7d"],
            "volumeUsd": { "min": "1000" },
            "volumeNative": { "min": "5" },
            "buyCount": { "min": 3 },
            "sellCount": { "max": 9 },
            "changePct": { "min": "-5", "max": "5" },
            "rangePct": { "min": "10" },
            "liquidityUsd": { "min": "50000" },
            "liquidityNative": { "min": "300" },
            "token": { "dexPaid": true }
        }))
        .expect("parses");

        assert_eq!(parsed.timeframes, [Timeframe::OneSec, Timeframe::SevenDay]);
        parsed.validate().expect("valid");
    }

    #[test]
    fn a_candle_filter_refuses_what_no_bucket_can_match() {
        let candles = |json| serde_json::from_value::<CandleFilter>(json);
        candles(serde_json::json!({ "timeframes": ["2m"] })).expect_err("unknown width");
        candles(serde_json::json!({ "chain": "solana" })).expect_err("the singular key");
        candles(serde_json::json!({ "metric": "price" })).expect_err("an option, not a filter");

        let error = candles(serde_json::json!({
            "pools": [],
            "changePct": { "min": "5", "max": "-5" },
            "token": { "marketCapUsd": { "min": "10", "max": "1" } }
        }))
        .expect("parses")
        .validate()
        .expect_err("refused");
        let details = error.details.expect("details");
        let fields: Vec<_> =
            details.as_array().expect("a list").iter().map(|d| d["field"].clone()).collect();
        assert_eq!(fields, ["pools", "changePct", "token.marketCapUsd"]);
    }

    #[test]
    fn a_list_takes_at_most_100_values() {
        let many = vec!["x"; TokenFilter::MAX_LIST_VALUES + 1];
        let parsed = filter(serde_json::json!({
            "symbolOrName": many,
            "exclude": { "symbolOrName": many }
        }))
        .expect("parses");

        let details = parsed.validate().expect_err("too many").details.expect("details");
        assert_eq!(details[0]["field"], "symbolOrName");
        assert_eq!(details[1]["field"], "exclude.symbolOrName");
    }

    fn transfers(json: serde_json::Value) -> Result<TransferFilter, serde_json::Error> {
        serde_json::from_value(json)
    }

    /// The fields `filter`'s refusal names, in order.
    fn refused_fields(filter: &TransferFilter) -> Vec<serde_json::Value> {
        let details = filter.validate().expect_err("refused").details.expect("details");
        details.as_array().expect("a list").iter().map(|d| d["field"].clone()).collect()
    }

    #[test]
    fn the_transfer_spec_example_parses() {
        let parsed = transfers(serde_json::json!({
            "chains": ["solana"],
            "wallets": ["8D2YW555vKLbTjhhhJggAEbcioLEGzgiQpFVXq3LxLHA"],
            "native": false,
            "amount": { "min": "1000" },
            "token": { "marketCapUsd": { "max": "5000000" } },
            "exclude": { "tokens": [SOL_TOKEN] }
        }))
        .expect("parses");

        assert_eq!(parsed.native, Some(false));
        assert!(parsed.token.is_some());
        parsed.validate().expect("valid");
    }

    #[test]
    fn the_transfer_filter_reads_every_key() {
        let parsed = transfers(serde_json::json!({
            "chains": ["solana"],
            "tokens": [SOL_TOKEN],
            "wallets": [SOL_TOKEN],
            "senders": [SOL_TOKEN],
            "recipients": [SOL_TOKEN],
            "native": false,
            "amount": { "min": "1", "max": "5" },
            "token": { "dexPaid": true },
            "exclude": { "wallets": [SOL_TOKEN], "tokens": [SOL_TOKEN] }
        }))
        .expect("parses");

        assert!(parsed.senders.is_some() && parsed.recipients.is_some());
        parsed.validate().expect("valid");
        assert_eq!(transfers(serde_json::json!({})).expect("parses"), TransferFilter::default());
    }

    #[test]
    fn a_transfer_filter_has_no_value_keys() {
        for key in ["valueUsd", "valueNative", "volumeUsd"] {
            let refused = transfers(serde_json::json!({ key: { "min": "1" } })).expect_err(key);
            assert!(refused.to_string().contains(key), "{refused}");
        }
        transfers(serde_json::json!({ "exclude": { "flags": ["kol"] } }))
            .expect_err("unknown exclude key");
    }

    #[test]
    fn a_transfer_filter_refuses_an_inverted_range_by_its_path() {
        let parsed = transfers(serde_json::json!({
            "amount": { "min": "10", "max": "1" },
            "token": { "marketCapUsd": { "min": "10", "max": "1" } }
        }))
        .expect("parses");

        assert_eq!(refused_fields(&parsed), ["amount", "token.marketCapUsd"]);
    }

    #[test]
    fn a_transfer_filter_refuses_an_empty_address_list() {
        for field in ["tokens", "wallets", "senders", "recipients"] {
            let empty = transfers(serde_json::json!({ field: [] })).expect("parses");
            assert_eq!(refused_fields(&empty), [field]);
        }
        transfers(serde_json::json!({ "senders": ["not an address"] })).expect_err("malformed");
    }

    #[test]
    fn a_transfer_exclude_list_takes_at_most_100_values() {
        let many = vec![SOL_TOKEN; TokenFilter::MAX_LIST_VALUES + 1];
        let parsed = transfers(serde_json::json!({
            "exclude": { "wallets": many, "tokens": many }
        }))
        .expect("parses");

        assert_eq!(refused_fields(&parsed), ["exclude.wallets", "exclude.tokens"]);
    }

    #[test]
    fn native_true_refuses_a_token_key() {
        let both = transfers(serde_json::json!({
            "native": true,
            "tokens": [SOL_TOKEN],
            "token": { "dexPaid": true }
        }))
        .expect("parses");
        assert_eq!(refused_fields(&both), ["native", "native"]);

        for json in [
            serde_json::json!({ "native": true, "exclude": { "tokens": [SOL_TOKEN] } }),
            serde_json::json!({ "native": false, "tokens": [SOL_TOKEN], "token": {} }),
        ] {
            assert!(transfers(json.clone()).expect("parses").validate().is_ok(), "{json}");
        }
    }
}
