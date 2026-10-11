//! The session-token mint: what `POST /v1/session-tokens` reads and answers.
//!
//! The credential itself is `metastreams-keys`' `session.rs`; §9 of
//! <https://docs.metastreams.io/get-started/authentication> is its contract.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::time::UnixMilliseconds;

/// The body of `POST /v1/session-tokens`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateSessionTokenRequest {
    /// The origins the token may be presented from, 1 to 10, each as a browser sends `Origin`.
    #[schema(min_items = 1, max_items = 10, example = json!(["https://app.example.com"]))]
    pub origins: Vec<String>,
    /// Who the token is for, in the caller's own terms: at most 128 characters, no control characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(max_length = 128, example = "user-42")]
    pub subject: Option<String>,
    /// How long the token lives, 60 to 3600 seconds; 900 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 60, maximum = 3600, example = 900)]
    pub ttl_seconds: Option<u32>,
}

/// A session token as the mint answers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSessionToken {
    /// The token, presented as `Authorization: Bearer <token>`, or on the stream handshake in `Sec-WebSocket-Protocol`.
    pub token: String,
    /// When it stops authenticating.
    pub expires_at: UnixMilliseconds,
}
