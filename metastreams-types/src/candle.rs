//! OHLCV candles — one bucket of a token's trading.
//!
//! A token candle aggregates every pool the token trades in. A pool candle reads one pool.

use serde::{Deserialize, Deserializer, Serialize};
use strum::{Display, EnumIter, IntoStaticStr};
use utoipa::openapi::schema::{Object, ObjectBuilder, SchemaType, Type};
use utoipa::{IntoParams, ToSchema};

use crate::chain::SpotChain;
use crate::time::UnixMilliseconds;

/// Query params for `GET /v1/tokens/{chain}/{address}/ohlcv`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpotCandlesQuery {
    /// Candle bucket size; defaults to `1m`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[param(schema_with = timeframe_schema)]
    pub timeframe: Option<Timeframe>,
    /// OHLC metric, `price` or `marketCap`; defaults to `marketCap`.
    #[serde(
        default,
        deserialize_with = "optional_camel_case_metric",
        skip_serializing_if = "Option::is_none"
    )]
    #[param(value_type = Option<String>, default = "marketCap")]
    pub metric: Option<CandleMetric>,
    /// OHLC denomination; defaults to `usd`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, default = "usd")]
    pub denomination: Option<CandleDenomination>,
    /// Window start, inclusive; ignored when `countBack` is set. A window holding
    /// more candles than the cap keeps the newest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<UnixMilliseconds>,
    /// Window end, exclusive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<UnixMilliseconds>,
    /// `TradingView` bar count: the most recent N buckets at or before `to`.
    /// Takes priority over `from`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<u32>, minimum = 1, maximum = 5000)]
    pub count_back: Option<u32>,
}

/// One chart OHLC bucket. The request's metric decides whether the values are
/// prices or market caps, and its denomination decides what they are denominated in.
///
/// Values are JSON numbers, the one amount on this surface that is: a chart
/// client consumes them as floats, and every OHLCV feed it already reads is
/// shaped this way. A bucket with no trades in it is not emitted, so a series
/// is sparse.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiCandle {
    /// Candle open time.
    #[serde(rename = "time")]
    pub timestamp: UnixMilliseconds,
    /// Opening value.
    #[schema(example = 1.0)]
    pub open: f64,
    /// Highest value.
    #[schema(example = 2.0)]
    pub high: f64,
    /// Lowest value.
    #[schema(example = 0.5)]
    pub low: f64,
    /// Closing value.
    #[schema(example = 0.7)]
    pub close: f64,
    /// Traded volume, in the requested denomination.
    #[schema(example = 1000.0)]
    pub volume: f64,
}

/// `GET /v1/tokens/{chain}/{address}/ohlcv` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiCandlesResponse {
    /// Buckets, oldest first.
    pub candles: Vec<UiCandle>,
}

/// One `spot.candles` update: a bucket and the series it belongs to.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiCandleUpdate {
    /// Chain the token trades on.
    pub chain: SpotChain,
    /// Token address.
    pub token: String,
    /// Pool address of a pool bucket; `null` on a token bucket.
    pub pool: Option<String>,
    /// Bucket width.
    pub timeframe: Timeframe,
    /// The bucket, as the REST OHLCV read returns it.
    #[serde(flatten)]
    pub candle: UiCandle,
}

/// A candle bucket width: every tier the candle pipeline aggregates.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, Display, IntoStaticStr,
)]
pub enum Timeframe {
    /// One second.
    #[serde(rename = "1s")]
    #[strum(serialize = "1s")]
    OneSec,
    /// Five seconds.
    #[serde(rename = "5s")]
    #[strum(serialize = "5s")]
    FiveSec,
    /// Fifteen seconds.
    #[serde(rename = "15s")]
    #[strum(serialize = "15s")]
    FifteenSec,
    /// Thirty seconds.
    #[serde(rename = "30s")]
    #[strum(serialize = "30s")]
    ThirtySec,
    /// One minute.
    #[default]
    #[serde(rename = "1m")]
    #[strum(serialize = "1m")]
    OneMin,
    /// Three minutes.
    #[serde(rename = "3m")]
    #[strum(serialize = "3m")]
    ThreeMin,
    /// Five minutes.
    #[serde(rename = "5m")]
    #[strum(serialize = "5m")]
    FiveMin,
    /// Fifteen minutes.
    #[serde(rename = "15m")]
    #[strum(serialize = "15m")]
    FifteenMin,
    /// Thirty minutes.
    #[serde(rename = "30m")]
    #[strum(serialize = "30m")]
    ThirtyMin,
    /// One hour.
    #[serde(rename = "1h")]
    #[strum(serialize = "1h")]
    OneHour,
    /// Four hours.
    #[serde(rename = "4h")]
    #[strum(serialize = "4h")]
    FourHour,
    /// Six hours.
    #[serde(rename = "6h")]
    #[strum(serialize = "6h")]
    SixHour,
    /// Twelve hours.
    #[serde(rename = "12h")]
    #[strum(serialize = "12h")]
    TwelveHour,
    /// One day.
    #[serde(rename = "1d")]
    #[strum(serialize = "1d")]
    OneDay,
    /// Seven days.
    #[serde(rename = "7d")]
    #[strum(serialize = "7d")]
    SevenDay,
}

impl Timeframe {
    /// Every timeframe with a candle contract, ascending.
    pub const CANDLES: [Timeframe; 15] = [
        Timeframe::OneSec,
        Timeframe::FiveSec,
        Timeframe::FifteenSec,
        Timeframe::ThirtySec,
        Timeframe::OneMin,
        Timeframe::ThreeMin,
        Timeframe::FiveMin,
        Timeframe::FifteenMin,
        Timeframe::ThirtyMin,
        Timeframe::OneHour,
        Timeframe::FourHour,
        Timeframe::SixHour,
        Timeframe::TwelveHour,
        Timeframe::OneDay,
        Timeframe::SevenDay,
    ];

    /// The canonical short string, such as `1m`.
    #[must_use]
    pub fn window(self) -> &'static str {
        self.into()
    }
}

/// What a candle's OHLC values measure.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, EnumIter, IntoStaticStr,
)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "snake_case")]
pub enum CandleMetric {
    /// Token price.
    Price,
    /// Fully-diluted market cap.
    #[default]
    #[serde(alias = "market_cap")]
    MarketCap,
}

/// The currency a candle's OHLC and volume values are denominated in.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, EnumIter, IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum CandleDenomination {
    /// USD.
    #[default]
    Usd,
    /// The chain's gas token.
    Native,
}

/// An optional `metric`, in its camelCase spelling only. Pair it with `#[serde(default)]`.
/// [`CandleMetric`] also accepts a legacy snake_case alias; this surface never had one.
pub(crate) fn optional_camel_case_metric<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<CandleMetric>, D::Error> {
    Option::<String>::deserialize(d)?.map(parse_metric).transpose()
}

fn parse_metric<E: serde::de::Error>(raw: String) -> Result<CandleMetric, E> {
    if raw.contains('_') {
        return Err(E::unknown_variant(&raw, &["price", "marketCap"]));
    }
    serde_json::from_value(serde_json::Value::String(raw)).map_err(E::custom)
}

/// The published `timeframe` schema: every width the candle pipeline aggregates, straight from
/// [`Timeframe::CANDLES`], so the document cannot advertise a width no rows stand behind.
fn timeframe_schema() -> Object {
    ObjectBuilder::new()
        .schema_type(SchemaType::new(Type::String))
        .description(Some("Candle bucket size; defaults to `1m`."))
        .enum_values(Some(Timeframe::CANDLES.iter().map(|timeframe| timeframe.window())))
        .default(Some(Timeframe::default().window().into()))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_advertises_exactly_the_candle_tiers() {
        let schema = serde_json::to_value(timeframe_schema()).expect("serializes");
        let published: Vec<&str> =
            schema["enum"].as_array().expect("enum").iter().filter_map(|v| v.as_str()).collect();

        assert_eq!(
            published,
            [
                "1s", "5s", "15s", "30s", "1m", "3m", "5m", "15m", "30m", "1h", "4h", "6h", "12h",
                "1d", "7d"
            ]
        );
        assert_eq!(schema["default"], "1m");
    }

    #[test]
    fn the_metric_takes_its_camel_case_spelling_only() {
        let query: SpotCandlesQuery =
            serde_json::from_value(serde_json::json!({ "metric": "marketCap" })).expect("parses");
        assert_eq!(query.metric, Some(CandleMetric::MarketCap));

        serde_json::from_value::<SpotCandlesQuery>(serde_json::json!({ "metric": "market_cap" }))
            .expect_err("snake_case never appears on this wire");
    }
}
