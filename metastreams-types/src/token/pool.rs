//! [`UiTokenPool`], one of the token's top pools, and the asset it quotes in.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::money::UiMoney;

/// One of a token's top pools.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenPool {
    /// Pool address; the `pool` parameter of the candles endpoint.
    pub pool: String,
    /// Venue the pool trades on.
    pub dex: String,
    /// The asset the pool quotes the token in; `null` until a swap names it.
    pub quote: Option<UiPoolQuote>,
    /// Pool liquidity.
    pub liquidity: UiMoney,
}

/// The asset a pool quotes its token in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiPoolQuote {
    /// Quote asset's address.
    pub address: String,
    /// Quote asset's display symbol; `null` where the registry names none.
    pub symbol: Option<String>,
}
