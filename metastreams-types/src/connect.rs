//! What `POST /v1/stream/connect` answers; <https://docs.metastreams.io/reference/rest/stream-connect>.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::time::UnixMilliseconds;

/// Where to open the stream socket directly, and the token that opens it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiStreamConnection {
    /// The answering server's direct stream URL.
    #[schema(example = "wss://stream-<region>.metastreams.io:9443/v1/stream")]
    pub url: String,
    /// The token that opens sockets at `url`, as `Authorization: Bearer <token>`.
    pub token: String,
    /// When the token stops opening sockets.
    pub expires_at: UnixMilliseconds,
}
