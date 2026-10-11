//! [`FundingStatus`] and [`FundingSourceKind`], what a wallet's first-funding lookup found.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Where a wallet's first inbound transaction came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum FundingSourceKind {
    /// Funded from a known centralized-exchange deposit address.
    Cex,
    /// Funded from another wallet (peer-to-peer transfer or unknown source).
    Wallet,
}

/// Whether a wallet's first-funding lookup has settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum FundingStatus {
    /// The lookup has not settled, and may still resolve.
    Unresolved,
    /// The lookup settled, whether or not it named a funder.
    Resolved,
}
