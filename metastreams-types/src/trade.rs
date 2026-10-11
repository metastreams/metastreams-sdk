//! The trade object: `UiSpotTrade`, one executed swap on a token.

use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer, Serialize};
use strum::{Display, EnumIter, EnumString};
use utoipa::openapi::schema::{Array, ArrayBuilder};
use utoipa::{IntoParams, ToSchema};

use crate::address::MultiChainAddress;
use crate::chain::SpotChain;
use crate::decimal::Decimal;
use crate::money::UiMoney;
use crate::schema::{enum_values, open_enumeration};
use crate::time::UnixMilliseconds;
use crate::token::platform::UiPlatform;

/// Trades returned when a read names no `limit`.
pub const TRADES_LIMIT_DEFAULT: u32 = 20;

/// Most trades one page may carry.
pub const TRADES_LIMIT_MAX: u32 = 100;

/// Query for `GET /v1/tokens/{chain}/{address}/trades`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenTradesQuery {
    /// Rows per page, clamped to 1-100; 20 when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[param(minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
    /// Opaque cursor from the previous page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Restrict to one trader wallet.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>)]
    pub trader: Option<MultiChainAddress>,
    /// Wallet classifications to keep, comma-separated. Absent or empty keeps every trade.
    #[serde(default, with = "crate::csv", skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, example = "bundler,insider")]
    pub flags: Option<Vec<WalletFilter>>,
}

impl TokenTradesQuery {
    /// Rows per page, clamped to `1..=`[`TRADES_LIMIT_MAX`].
    #[must_use]
    pub fn limit(&self) -> u32 {
        trades_limit(self.limit)
    }
}

/// Query for `GET /v1/trades`.
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityTradesQuery {
    /// Which identity feed to read.
    pub identity: IdentityFeed,
    /// Chains to scope to, CSV of the chain slugs. Absent means every spot chain.
    #[serde(default, with = "crate::csv::chains", skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, example = "solana,base")]
    pub chains: Option<Vec<SpotChain>>,
    /// Rows per page, clamped to 1-100; 20 when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[param(minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
    /// Opaque cursor from the previous page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

impl IdentityTradesQuery {
    /// Rows per page, clamped to `1..=`[`TRADES_LIMIT_MAX`].
    #[must_use]
    pub fn limit(&self) -> u32 {
        trades_limit(self.limit)
    }
}

/// One executed swap on the token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotTrade {
    /// Stable identity for this trade, opaque to the caller. One transaction can settle several
    /// trades on a token, so this is not a transaction hash — never parse it.
    pub id: String,
    /// Chain the trade executed on.
    pub chain: SpotChain,
    /// Traded token's address.
    pub token: String,
    /// Traded token's name; empty until the token's metadata resolves.
    pub name: String,
    /// Traded token's symbol; empty until the token's metadata resolves.
    pub symbol: String,
    /// Wallet that sent the swap.
    pub trader: String,
    /// On-chain transaction hash.
    pub transaction_hash: String,
    /// Buy or sell.
    pub side: TradeDirection,
    /// Token units traded.
    #[schema(value_type = Decimal, examples("1250.5"))]
    pub amount: Decimal,
    /// Token price at execution.
    pub price: UiMoney,
    /// Trade value.
    pub volume: UiMoney,
    /// Token market cap at execution.
    pub market_cap: UiMoney,
    /// Classification flags for the trader wallet.
    #[serde(default, deserialize_with = "known_flags", skip_serializing_if = "Vec::is_empty")]
    #[schema(schema_with = open_wallet_flags)]
    pub flags: Vec<WalletFlag>,
    /// Terminals this trade routed through. A trader's profile is an element of
    /// `identities`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<UiPlatform>,
    /// Trader's resolved identities, one element per source.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identities: Vec<UiWalletIdentity>,
    /// When the trade executed.
    pub timestamp: UnixMilliseconds,
}

/// A page of trades, newest first: one token's (`GET /v1/tokens/{chain}/{address}/trades`)
/// or one identity's across every token (`GET /v1/trades`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotTrades {
    /// Trades on the requested page, newest first.
    pub trades: Vec<UiSpotTrade>,
    /// Cursor for the next page; `null` on the last page.
    pub next_cursor: Option<String>,
    /// Whether a next page exists.
    pub has_more: bool,
}

/// One source's resolved profile for a trade's trader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiWalletIdentity {
    /// Who resolved this identity.
    pub source: IdentitySource,
    /// The wallet's handle on the source.
    pub handle: Option<String>,
    /// Human-readable name to render.
    pub display_name: Option<String>,
    /// CDN URL of the avatar to render; `null` until the picture is mirrored.
    pub avatar: Option<String>,
    /// The wallet's bare X handle, where the source links one.
    pub twitter: Option<String>,
    /// The wallet's Telegram handle, where the source links one.
    pub telegram: Option<String>,
    /// The wallet's Farcaster handle, where the source links one.
    pub farcaster: Option<String>,
}

/// A cross-token feed of trades by a notable identity: `GET /v1/trades?identity=`.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    ToSchema,
    Display,
    EnumIter,
    EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum IdentityFeed {
    /// Trades by a curated KOL wallet.
    Kol,
    /// Trades by a wallet resolved to a fomo.family identity.
    #[serde(rename = "fomoscan")]
    #[strum(serialize = "fomoscan")]
    Fomo,
}

/// A wallet classification a trade or holder read narrows to.
///
/// The full classification set, less the two kinds that read a signed-in user's
/// wallets (`watchlist`, `own`): the API has no user, so it has no wallet sets.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    ToSchema,
    Display,
    EnumIter,
    EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum WalletFilter {
    /// Rows carrying [`WalletFlag::Dev`].
    Dev,
    /// Rows carrying [`WalletFlag::Sniper`].
    Sniper,
    /// Rows carrying [`WalletFlag::Insider`].
    Insider,
    /// Rows carrying [`WalletFlag::Bundler`].
    Bundler,
    /// Rows carrying [`WalletFlag::Fresh`].
    Fresh,
    /// Rows carrying [`WalletFlag::Whale`].
    Whale,
    /// Rows carrying [`WalletFlag::Kol`].
    Kol,
    /// Rows carrying [`WalletFlag::ProTrader`].
    ProTrader,
    /// Trades routed through the fomo.family terminal, or holders whose wallet has traded through
    /// it. Not wallets holding a fomo identity.
    Fomo,
    /// Rows carrying [`WalletFlag::Phishing`].
    Phishing,
}

/// Which way the traded token moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TradeDirection {
    /// The trader bought the token.
    Buy,
    /// The trader sold the token.
    Sell,
}

/// A classification the classifier put on a wallet.
///
/// The predicate behind a flag varies by chain and by the pipeline that set it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WalletFlag {
    /// Wallet participated in bundle trading.
    Bundler,
    /// Wallet bought the token within two slots of its first pool.
    Sniper,
    /// Wallet received tokens via transfer or sent tokens to the creator.
    Insider,
    /// Wallet is the token developer.
    Dev,
    /// Wallet first seen less than seven days ago.
    Fresh,
    /// Wallet whose trade history classifies it as a whale; the flag never clears.
    Whale,
    /// Wallet is a Key Opinion Leader.
    Kol,
    /// Wallet is a liquidity pool vault.
    LiquidityPool,
    /// Wallet is a launchpad's lock contract, holding a token allocation or an LP position.
    Locker,
    /// Wallet uses a trading platform.
    ProTrader,
    /// System or infrastructure account: a program PDA, DEX authority, fee recipient, or bot vault.
    System,
    /// Wallet currently holds this token from a fomo trade.
    Fomo,
    /// Wallet a security source names as a malicious actor: phishing, scamming, rug pulling or
    /// hacking.
    Phishing,
}

/// Who resolved an identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdentitySource {
    /// Codex's aggregate wallet profile.
    Codex,
    /// A fomo.family profile, resolved through FomoScan.
    #[serde(rename = "fomo", alias = "fomoscan")]
    Fomo,
    /// A pump.fun profile.
    Pump,
    /// A GMGN call-out author.
    Gmgn,
    /// An Axiom-native call-out author.
    Axiom,
    /// A source this build does not name. A reader skips its element rather than fail the row.
    #[serde(other)]
    Unknown,
}

/// What named the trader of an EVM trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraderSource {
    /// The swap's own output recipient, or the wallet behind it.
    SwapRecipient,
    /// The transaction sender.
    TxOrigin,
    /// The account a launchpad fill names as its buyer.
    LaunchpadBuyer,
    /// The far end of the settlement chain, behind a venue's forwarder.
    SettlementEndpoint,
    /// The account an EIP-7702 transaction delegates its code to.
    Authority7702,
    /// The `sender` of the ERC-4337 user operation that closes the swap.
    UserOpSender,
    /// The sender of a transaction a known relay's router received.
    RelayCaller,
    /// The account that calls the pool for the wallet.
    PoolCaller,
}

/// A trader classification a `spot.trades` filter keeps or drops.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumIter, EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum TradeFlag {
    /// The token's creator.
    Dev,
    /// [`WalletFlag::Sniper`].
    Sniper,
    /// [`WalletFlag::Insider`].
    Insider,
    /// [`WalletFlag::Bundler`].
    Bundler,
    /// [`WalletFlag::Fresh`].
    Fresh,
    /// [`WalletFlag::Whale`].
    Whale,
    /// [`WalletFlag::Kol`].
    Kol,
    /// [`WalletFlag::Fomo`].
    Fomo,
    /// A trader resolved to a fomo.family identity.
    Fomoscan,
    /// [`WalletFlag::ProTrader`].
    ProTrader,
    /// [`WalletFlag::Phishing`].
    Phishing,
}

/// `limit`, or the default when absent, clamped to `1..=`[`TRADES_LIMIT_MAX`].
fn trades_limit(limit: Option<u32>) -> u32 {
    limit.unwrap_or(TRADES_LIMIT_DEFAULT).clamp(1, TRADES_LIMIT_MAX)
}

/// The published `flags` schema: an open string list, not the closed `enum`
/// [`WalletFlag`] derives, so a generated client survives a flag added
/// upstream. The known values reach the document as examples, lifted from the
/// derived schema, so a new variant cannot leave them stale.
pub(crate) fn open_wallet_flags() -> Array {
    ArrayBuilder::new()
        .items(open_enumeration(
            "A flag the classifier put on the wallet. This is an open enumeration: \
             the classifier learns new flags, so keep a value you do not recognize.",
            enum_values::<WalletFlag>(),
        ))
        .build()
}

/// The `flags` this build names.
/// A reader drops a flag a newer server adds, because the set is open.
pub(crate) fn known_flags<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<WalletFlag>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum MaybeFlag {
        Known(WalletFlag),
        Unknown(IgnoredAny),
    }

    let flags = Vec::<MaybeFlag>::deserialize(d)?;
    Ok(flags
        .into_iter()
        .filter_map(|flag| match flag {
            MaybeFlag::Known(flag) => Some(flag),
            MaybeFlag::Unknown(_) => None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    use strum::IntoEnumIterator;

    #[test]
    fn flags_parse_from_a_csv_and_an_unknown_kind_is_rejected() {
        let query: TokenTradesQuery =
            serde_json::from_value(serde_json::json!({ "flags": "bundler,insider" }))
                .expect("parses");
        assert_eq!(query.flags, Some(vec![WalletFilter::Bundler, WalletFilter::Insider]));

        for raw in ["watchlist", "own", "banana"] {
            serde_json::from_value::<TokenTradesQuery>(serde_json::json!({ "flags": raw }))
                .expect_err("not a public kind");
        }
    }

    #[test]
    fn the_identity_feed_takes_the_fomoscan_spelling() {
        let query: IdentityTradesQuery =
            serde_json::from_value(serde_json::json!({ "identity": "fomoscan" })).expect("parses");
        assert_eq!(query.identity, IdentityFeed::Fomo);
        assert_eq!(IdentityFeed::iter().count(), 2);
    }

    #[test]
    fn the_flags_schema_stays_open_and_lists_every_known_flag() {
        let schema = serde_json::to_value(open_wallet_flags()).expect("serializes");
        let items = &schema["items"];

        assert_eq!(items["type"], "string");
        assert!(items.get("enum").is_none(), "flags publish a closed value list");

        let examples = items["examples"].as_array().expect("the known flags");
        assert!(examples.iter().any(|flag| flag == "locker"), "examples come from the enum");
        assert!(examples.iter().any(|flag| flag == "pro_trader"), "examples are the wire form");
    }

    #[test]
    fn a_flag_this_build_does_not_name_is_dropped() {
        #[derive(Deserialize)]
        struct Flags {
            #[serde(deserialize_with = "known_flags")]
            flags: Vec<WalletFlag>,
        }

        let read: Flags =
            serde_json::from_value(serde_json::json!({ "flags": ["bundler", "mev_bot", "kol"] }))
                .expect("an unknown flag never fails the read");

        assert_eq!(read.flags, [WalletFlag::Bundler, WalletFlag::Kol]);
    }
}
