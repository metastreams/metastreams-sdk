//! The transfer object: `UiSpotTransfer`, one movement of an asset between two wallets.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::chain::SpotChain;
use crate::decimal::Decimal;
use crate::time::UnixMilliseconds;

/// One transfer between two wallets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSpotTransfer {
    /// Chain the transfer executed on.
    pub chain: SpotChain,
    /// Token address; `None` for the chain's native asset.
    pub token: Option<String>,
    /// Wallet that sent the transfer.
    pub sender: String,
    /// Wallet that received the transfer.
    pub recipient: String,
    /// Token units moved.
    #[schema(value_type = Decimal, examples("250.5"))]
    pub amount: Decimal,
    /// The sender's balance of `token` after the transaction; `None` when the chain read none.
    #[schema(value_type = Option<Decimal>, examples("749.5"))]
    pub sender_balance_after: Option<Decimal>,
    /// The recipient's balance of `token` after the transaction; `None` when the chain read none.
    #[schema(value_type = Option<Decimal>, examples("250.5"))]
    pub recipient_balance_after: Option<Decimal>,
    /// On-chain transaction hash.
    pub transaction_hash: String,
    /// This movement's ordinal within the transaction; `None` for an EVM native transfer.
    pub event_index: Option<u64>,
    /// When the transfer executed on chain.
    pub timestamp: UnixMilliseconds,
}
