//! [`TokenAnnotation`], a platform-scoped flag a token's launch carried.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A platform-scoped flag on a token, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenAnnotation {
    /// Pump.fun `BondingCurve.is_mayhem_mode = true`.
    MayhemMode,
    /// Pump.fun `BondingCurve.is_cashback_enabled = true`.
    CashbackCoin,
    /// A quote-only chain anchor, a stable or the wrapped native. Discovery and trending skip it.
    Anchor,
    /// A four.meme X Mode launch.
    XMode,
    /// The token contract taxes buys, sells, or both.
    TaxToken,
    /// A launch whose market quotes in a tokenized stock, such as a four.meme Stock Meme market.
    StockQuote,
    /// The launch's tax revenue pays dividends in tokenized stocks.
    StockDividend,
    /// The launch's revenue vault runs on Flap's AI Oracle.
    AiVault,
    /// A kind this build does not name. Read-only: the server never sends it.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl TokenAnnotation {
    /// The `kind` string the wire tags the annotation with.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::MayhemMode => "mayhem_mode",
            Self::CashbackCoin => "cashback_coin",
            Self::Anchor => "anchor",
            Self::XMode => "x_mode",
            Self::TaxToken => "tax_token",
            Self::StockQuote => "stock_quote",
            Self::StockDividend => "stock_dividend",
            Self::AiVault => "ai_vault",
            Self::Unknown => "unknown",
        }
    }
}
