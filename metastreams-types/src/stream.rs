//! The WebSocket frames: `subscribe`, `unsubscribe` and `ping` in; `ack`, `update`, `exit`,
//! `error` and `pong` out. Each channel's filter is here too, typed by [`Subscription`].
//! <https://docs.metastreams.io/streams/overview> documents the protocol.

use std::borrow::Cow;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use crate::address::MultiChainAddress;
use crate::balance::UiWalletBalance;
use crate::candle::{CandleDenomination, CandleMetric, UiCandleUpdate, optional_camel_case_metric};
use crate::chain::{SpotChain, de};
use crate::error::{ErrorDetail, FieldError};
use crate::filter::{CandleFilter, TokenFilter, TradeFilter, TransferFilter};
use crate::position::UiSpotPosition;
use crate::token::UiSpotToken;
use crate::token::holders::UiSpotHolder;
use crate::trade::{UiSpotTrade, WalletFilter};
use crate::transfer::UiSpotTransfer;

/// The subprotocol a browser offers beside its session token on the handshake, and the one the
/// server selects.
pub const SUBPROTOCOL: &str = "metastreams.v1";

/// A channel's filter object, typed to the channel it opens and the payloads its frames carry.
pub trait Subscription: Serialize {
    /// The channel name.
    const CHANNEL: &'static str;
    /// The payload one update carries.
    type Update: DeserializeOwned;
    /// The payload of the channel's `exit` frame.
    type Exit: DeserializeOwned;
}

/// A channel that takes `throttleMs`.
pub trait Throttled: Subscription {}

/// The exit payload of a channel without `exit` frames. It has no value, so no payload reads as it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum NoExit {}

impl Subscription for TradeFilter {
    const CHANNEL: &'static str = "spot.trades";
    type Update = UiSpotTrade;
    type Exit = NoExit;
}

impl Subscription for TransferFilter {
    const CHANNEL: &'static str = "spot.transfers";
    type Update = UiSpotTransfer;
    type Exit = NoExit;
}

impl Subscription for CandleFilter {
    const CHANNEL: &'static str = "spot.candles";
    type Update = UiCandleUpdate;
    type Exit = NoExit;
}

impl Throttled for CandleFilter {}

impl Subscription for TokenFilter {
    const CHANNEL: &'static str = "spot.tokens";
    type Update = UiSpotToken;
    type Exit = UiTokenExit;
}

impl Throttled for TokenFilter {}

/// The token a `spot.tokens` `exit` frame names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenExit {
    /// Chain the token trades on.
    pub chain: SpotChain,
    /// Token address.
    pub address: String,
}

/// `wallet.balances` filter: one or more wallets on one chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WalletBalancesFilter {
    /// Chain the wallets hold balances on.
    #[serde(deserialize_with = "de::required")]
    pub chain: SpotChain,
    /// Wallet addresses on `chain`; the server also reads `wallet` with one address.
    #[serde(alias = "wallet", deserialize_with = "wallets")]
    pub wallets: Vec<MultiChainAddress>,
}

impl Subscription for WalletBalancesFilter {
    const CHANNEL: &'static str = "wallet.balances";
    type Update = UiWalletBalance;
    type Exit = NoExit;
}

/// `spot.positions` filter: one or more wallets on one chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpotPositionsFilter {
    /// Chain the positions are on.
    #[serde(deserialize_with = "de::required")]
    pub chain: SpotChain,
    /// Wallet addresses on `chain`; the server also reads `wallet` with one address.
    #[serde(alias = "wallet", deserialize_with = "wallets")]
    pub wallets: Vec<MultiChainAddress>,
}

impl Subscription for SpotPositionsFilter {
    const CHANNEL: &'static str = "spot.positions";
    type Update = UiSpotPosition;
    type Exit = NoExit;
}

/// `spot.holders` filter: one token on one chain, optionally narrowed to wallet classifications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpotHoldersFilter {
    /// Chain the token trades on.
    #[serde(deserialize_with = "de::required")]
    pub chain: SpotChain,
    /// Token address on `chain`.
    #[serde(deserialize_with = "token")]
    pub token: MultiChainAddress,
    /// Wallet classifications to keep, as the holders read's `flags`; empty keeps every holder.
    #[serde(default, deserialize_with = "flags", skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<WalletFilter>,
}

impl Subscription for SpotHoldersFilter {
    const CHANNEL: &'static str = "spot.holders";
    type Update = UiSpotHolder;
    type Exit = NoExit;
}

/// Backend-assigned identity of one subscription, carried on every update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubscriptionId(Uuid);

impl SubscriptionId {
    /// A fresh, random identity.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SubscriptionId {
    fn default() -> Self {
        Self::new()
    }
}

/// A frame the client sends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum ClientMessage {
    /// Answered with `pong`.
    Ping,
    /// Open one subscription on `channel`.
    Subscribe {
        /// The channel name.
        channel: String,
        /// The channel's filter object.
        #[serde(default)]
        filters: serde_json::Value,
        /// Client-chosen request id; the ack or error answering this echoes it.
        #[serde(default)]
        id: Option<String>,
        /// What the subscribe asks beside its filter.
        #[serde(flatten)]
        options: SubscribeOptions,
    },
    /// Close one subscription, or every one on `channel` when no id is named.
    Unsubscribe {
        /// The channel name.
        channel: String,
        /// The subscription to close.
        #[serde(default)]
        subscription_id: Option<SubscriptionId>,
    },
}

/// What a subscribe frame carries beside its filter. A channel refuses an option it does not take.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeOptions {
    /// The shortest gap between two updates of one series, in milliseconds; absent or 0 sends
    /// every update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throttle_ms: Option<u64>,
    /// Whether to send only finished `spot.candles` buckets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_only: Option<bool>,
    /// What `spot.candles` values measure; absent is `marketCap`.
    #[serde(
        default,
        deserialize_with = "optional_camel_case_metric",
        skip_serializing_if = "Option::is_none"
    )]
    pub metric: Option<CandleMetric>,
    /// What `spot.candles` values are denominated in; absent is `usd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denomination: Option<CandleDenomination>,
}

/// A frame the server sends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum ServerMessage {
    /// A subscribe or unsubscribe succeeded.
    Ack {
        /// The channel the action named.
        channel: String,
        /// The subscription opened or closed; absent when a whole channel was closed.
        #[serde(skip_serializing_if = "Option::is_none")]
        subscription_id: Option<SubscriptionId>,
        /// The subscribe request's `id`, when it carried one.
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Always `success`; a failure is an `error` frame.
        status: AckStatus,
        /// What succeeded.
        message: String,
    },
    /// One payload for one subscription.
    Update {
        /// The channel the subscription is on.
        channel: Cow<'static, str>,
        /// The subscription this payload is for.
        subscription_id: SubscriptionId,
        /// The channel's payload shape.
        data: serde_json::Value,
    },
    /// Something the subscription matched stopped matching. Unbilled.
    Exit {
        /// The channel the subscription is on.
        channel: Cow<'static, str>,
        /// The subscription that no longer matches.
        subscription_id: SubscriptionId,
        /// What stopped matching, in the channel's exit shape.
        data: serde_json::Value,
    },
    /// A frame was refused; the REST error registry names why.
    Error {
        /// The subscribe request's `id`, when it carried one.
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// The error, as the REST envelope carries it.
        #[serde(flatten)]
        error: ErrorDetail,
    },
    /// Answers `ping`.
    Pong,
    /// A frame type this build does not name. Read-only: the server never sends it.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// The one ack status; a failure is an `error` frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AckStatus {
    /// The action took effect.
    Success,
}

/// One `wallets` address or a list of them, read as a list.
fn wallets<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<MultiChainAddress>, D::Error> {
    one_or_many(d, "wallets")
}

/// One `token` address. `parse_address` refuses a malformed one under `token`.
fn token<'de, D: Deserializer<'de>>(d: D) -> Result<MultiChainAddress, D::Error> {
    parse_address(&String::deserialize(d)?, "token")
}

/// The `flags` kinds, each kept once. Every row tests each kind, so a repeated kind adds only cost.
fn flags<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<WalletFilter>, D::Error> {
    let mut kinds = Vec::new();
    for kind in Vec::<WalletFilter>::deserialize(d)? {
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    Ok(kinds)
}

/// One address or a list of them, read as a list. A malformed one is refused under `field`.
fn one_or_many<'de, D: Deserializer<'de>>(
    d: D,
    field: &str,
) -> Result<Vec<MultiChainAddress>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }

    let raw = match OneOrMany::deserialize(d)? {
        OneOrMany::One(address) => vec![address],
        OneOrMany::Many(addresses) => addresses,
    };
    raw.iter().map(|address| parse_address(address, field)).collect()
}

/// Refuse a malformed address with a validation error that names `field`, so a subscriber
/// learns which filter field failed.
fn parse_address<E: serde::de::Error>(raw: &str, field: &str) -> Result<MultiChainAddress, E> {
    raw.parse().map_err(|_| {
        let error = FieldError::new(field, "The value is not a valid address.");
        ErrorDetail::validation(vec![error]).into_de_error()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    fn round_trip(frame: ServerMessage) {
        let text = serde_json::to_string(&frame).expect("serializes");
        assert_eq!(serde_json::from_str::<ServerMessage>(&text).expect("deserializes"), frame);
    }

    #[test]
    fn every_server_frame_round_trips() {
        let subscription_id = SubscriptionId::new();
        round_trip(ServerMessage::Ack {
            channel: "spot.trades".to_owned(),
            subscription_id: Some(subscription_id),
            id: Some("1".to_owned()),
            status: AckStatus::Success,
            message: "Subscribed.".to_owned(),
        });
        round_trip(ServerMessage::Update {
            channel: Cow::Borrowed("spot.trades"),
            subscription_id,
            data: serde_json::json!({ "id": "t1" }),
        });
        round_trip(ServerMessage::Exit {
            channel: Cow::Borrowed("spot.tokens"),
            subscription_id,
            data: serde_json::json!({ "chain": "solana", "address": "t1" }),
        });
        round_trip(ServerMessage::Error {
            id: None,
            error: ErrorDetail {
                code: ErrorCode::NotFound,
                message: "No such subscription.".to_owned(),
                details: None,
            },
        });
        round_trip(ServerMessage::Pong);
    }

    #[test]
    fn an_unknown_frame_type_reads_as_unknown() {
        let frame: ServerMessage =
            serde_json::from_str(r#"{"type":"notice","text":"Maintenance at noon."}"#)
                .expect("parses");
        assert_eq!(frame, ServerMessage::Unknown);
    }

    #[test]
    fn a_client_frame_round_trips() {
        let frame = ClientMessage::Subscribe {
            channel: "spot.tokens".to_owned(),
            filters: serde_json::json!({ "chains": ["solana"] }),
            id: Some("7".to_owned()),
            options: SubscribeOptions { throttle_ms: Some(250), ..SubscribeOptions::default() },
        };
        let body = serde_json::to_value(&frame).expect("serializes");

        assert_eq!(body["action"], "subscribe");
        assert_eq!(body["throttleMs"], 250);
        assert_eq!(serde_json::from_value::<ClientMessage>(body).expect("deserializes"), frame);
    }

    #[test]
    fn candle_options_sit_beside_the_filters() {
        let subscribe = |metric: &str| {
            serde_json::from_value::<ClientMessage>(serde_json::json!({
                "action": "subscribe",
                "channel": "spot.candles",
                "filters": { "timeframes": ["1m"] },
                "closedOnly": true,
                "metric": metric,
                "denomination": "native",
            }))
        };

        let Ok(ClientMessage::Subscribe { options, .. }) = subscribe("marketCap") else {
            panic!("a subscribe frame");
        };
        assert_eq!(
            options,
            SubscribeOptions {
                throttle_ms: None,
                closed_only: Some(true),
                metric: Some(CandleMetric::MarketCap),
                denomination: Some(CandleDenomination::Native),
            }
        );
        subscribe("market_cap").expect_err("snake_case never appears on this wire");
    }

    #[test]
    fn a_malformed_address_names_its_filter_field() {
        let field_of = |error: serde_json::Error| {
            let text = error.to_string();
            let (_, packed) = text.split_once(crate::error::DE_MARKER).expect("marker");
            let detail: ErrorDetail = serde_json::Deserializer::from_str(packed)
                .into_iter()
                .next()
                .expect("one")
                .expect("detail");
            assert_eq!(detail.code, ErrorCode::ValidationError);
            detail.details.expect("details")[0]["field"].as_str().expect("field").to_owned()
        };

        let candles =
            serde_json::from_value::<CandleFilter>(serde_json::json!({ "pools": ["nope"] }));
        assert_eq!(field_of(candles.expect_err("malformed")), "pools");

        let trades =
            serde_json::from_value::<TradeFilter>(serde_json::json!({ "wallets": ["nope"] }));
        assert_eq!(field_of(trades.expect_err("malformed")), "wallets");

        let tokens = serde_json::from_value::<TokenFilter>(serde_json::json!({
            "tokens": ["So11111111111111111111111111111111111111112", "nope"],
        }));
        assert_eq!(field_of(tokens.expect_err("malformed")), "tokens");

        let balances = serde_json::from_value::<WalletBalancesFilter>(serde_json::json!({
            "chain": "solana",
            "wallet": "nope",
        }));
        assert_eq!(field_of(balances.expect_err("malformed")), "wallets");

        let holders = serde_json::from_value::<SpotHoldersFilter>(serde_json::json!({
            "chain": "solana",
            "token": "nope",
        }));
        assert_eq!(field_of(holders.expect_err("malformed")), "token");
    }

    #[test]
    fn an_exit_frame_names_the_token_and_the_subscription() {
        let subscription_id = SubscriptionId::new();
        let exit = UiTokenExit { chain: SpotChain::Solana, address: "t1".to_owned() };
        let frame = serde_json::to_value(ServerMessage::Exit {
            channel: Cow::Borrowed("spot.tokens"),
            subscription_id,
            data: serde_json::to_value(exit).expect("serializes"),
        })
        .expect("serializes");

        assert_eq!(
            frame,
            serde_json::json!({
                "type": "exit",
                "channel": "spot.tokens",
                "subscriptionId": subscription_id,
                "data": { "chain": "solana", "address": "t1" },
            })
        );
    }

    #[test]
    fn no_exit_refuses_every_payload() {
        for payload in [
            serde_json::json!(null),
            serde_json::json!("exit"),
            serde_json::json!(1),
            serde_json::json!([]),
            serde_json::json!({}),
            serde_json::json!({ "chain": "solana" }),
            serde_json::json!({ "chain": "solana", "address": "t1" }),
        ] {
            let read = serde_json::from_value::<NoExit>(payload.clone());
            assert!(read.is_err(), "{payload} reads as no exit");
        }
    }

    #[test]
    fn a_balances_filter_reads_one_wallet_or_many() {
        let wallet = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";
        for json in [
            serde_json::json!({ "chain": "solana", "wallet": wallet }),
            serde_json::json!({ "chain": "solana", "wallets": [wallet] }),
        ] {
            let filter: WalletBalancesFilter = serde_json::from_value(json).expect("parses");
            assert_eq!(filter.wallets.len(), 1);
        }
    }

    #[test]
    fn a_holders_filter_round_trips_with_its_flags() {
        let token = "So11111111111111111111111111111111111111112";
        let json =
            serde_json::json!({ "chain": "solana", "token": token, "flags": ["fomo", "sniper"] });
        let filter: SpotHoldersFilter = serde_json::from_value(json.clone()).expect("parses");

        assert_eq!(filter.flags, [WalletFilter::Fomo, WalletFilter::Sniper]);
        assert_eq!(serde_json::to_value(&filter).expect("serializes"), json);
    }

    #[test]
    fn a_holders_filter_keeps_each_flag_once() {
        let token = "So11111111111111111111111111111111111111112";
        let repeated = ["fomo", "sniper"].repeat(1_000);
        let json = serde_json::json!({ "chain": "solana", "token": token, "flags": repeated });
        let filter: SpotHoldersFilter = serde_json::from_value(json).expect("parses");

        assert_eq!(filter.flags, [WalletFilter::Fomo, WalletFilter::Sniper]);
    }

    #[test]
    fn a_holders_filter_refuses_an_unknown_key_or_flag_and_needs_a_token() {
        let token = "So11111111111111111111111111111111111111112";
        for json in [
            serde_json::json!({ "chain": "solana", "token": token, "wallets": [token] }),
            serde_json::json!({ "chain": "solana", "token": token, "flags": ["watchlist"] }),
            serde_json::json!({ "chain": "solana" }),
        ] {
            let read = serde_json::from_value::<SpotHoldersFilter>(json.clone());
            assert!(read.is_err(), "{json} parses");
        }
    }
}
