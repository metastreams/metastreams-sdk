//! The one error every provider call and every subscription returns.

use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use thiserror::Error;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::error::ProtocolError;

use metastreams_types::error::{ErrorDetail, ErrorEnvelope};

use crate::stream::CloseReason;

const REQUEST_ID: &str = "x-request-id";

/// The result every provider call returns.
pub type Result<T> = std::result::Result<T, MetastreamsError>;

/// Why a call to the API fails.
#[derive(Debug, Error)]
pub enum MetastreamsError {
    /// The request does not complete: connect, TLS, timeout or body read.
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    /// The socket does not open: connect, TLS or the WebSocket handshake.
    #[error(transparent)]
    WebSocket(Box<tungstenite::Error>),
    /// The base URL does not parse.
    #[error("invalid base URL: {0}")]
    Url(#[from] url::ParseError),
    /// The base URL is not `http` or `https`.
    #[error("the base URL scheme `{0}` is not http or https")]
    UnsupportedScheme(String),
    /// The key holds a character that an HTTP header cannot carry.
    #[error("the API key is not a valid header value")]
    InvalidKey,
    /// The key has no request budget left, or holds its most sockets.
    #[error("metastreams rate limited the request (429)")]
    RateLimited {
        /// The wait the `Retry-After` header names.
        retry_after: Option<Duration>,
        /// The `X-Request-ID` the response carries.
        request_id: Option<String>,
    },
    /// The API refused the request with its error envelope.
    #[error("metastreams returned {status} {:?}: {}", .error.code, .error.message)]
    Api {
        /// The response status.
        status: StatusCode,
        /// The envelope's error; switch on its `code`, never on its `message`.
        error: Box<ErrorDetail>,
        /// The `X-Request-ID` the response carries.
        request_id: Option<String>,
    },
    /// A failure body without the error envelope, such as a proxy's error page.
    #[error("metastreams returned {status} without an error envelope: {body}")]
    Unexpected {
        /// The response status.
        status: StatusCode,
        /// The raw body.
        body: String,
        /// The `X-Request-ID` the response carries.
        request_id: Option<String>,
    },
    /// A success body that does not read as the documented response.
    #[error("metastreams returned an unreadable {status} body: {source}")]
    Decode {
        /// The response status.
        status: StatusCode,
        /// Why the body does not read.
        source: serde_json::Error,
        /// The `X-Request-ID` the response carries.
        request_id: Option<String>,
    },
    /// The server refused a subscribe with an `error` frame.
    #[error("metastreams refused the {channel} subscription {:?}: {}", .error.code, .error.message)]
    Rejected {
        /// The channel the subscribe named.
        channel: &'static str,
        /// The frame's error; switch on its `code`, never on its `message`.
        error: Box<ErrorDetail>,
    },
    /// The subscription or its socket closes.
    #[error("the metastreams subscription closed: {0}")]
    Closed(CloseReason),
    /// An update or exit that does not read as its channel's payload.
    #[error("metastreams sent an unreadable {channel} frame: {source}")]
    FrameDecode {
        /// The channel the frame arrived on.
        channel: &'static str,
        /// Why the payload does not read.
        source: serde_json::Error,
    },
}

impl MetastreamsError {
    /// Whether the same call can succeed later: a transport failure, a 429, a 5xx, or a
    /// transient close.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Http(error) => !(error.is_builder() || error.is_redirect()),
            // `HandshakeIncomplete` is a peer that hangs up during the upgrade, as a draining proxy does.
            Self::WebSocket(error) => matches!(
                **error,
                tungstenite::Error::Io(_)
                    | tungstenite::Error::Tls(_)
                    | tungstenite::Error::ConnectionClosed
                    | tungstenite::Error::Protocol(ProtocolError::HandshakeIncomplete)
            ),
            Self::RateLimited { .. } => true,
            Self::Api { status, .. } | Self::Unexpected { status, .. } => status.is_server_error(),
            Self::Closed(reason) => reason.is_transient(),
            Self::Url(_)
            | Self::UnsupportedScheme(_)
            | Self::InvalidKey
            | Self::Decode { .. }
            | Self::Rejected { .. }
            | Self::FrameDecode { .. } => false,
        }
    }

    /// The error a non-2xx response carries: a 429, the API's envelope, or a foreign body.
    pub(crate) fn from_response(status: StatusCode, headers: &HeaderMap, body: &[u8]) -> Self {
        let request_id = request_id(headers);
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Self::RateLimited { retry_after: retry_after(headers), request_id };
        }
        match serde_json::from_slice::<ErrorEnvelope>(body) {
            Ok(ErrorEnvelope { error }) => Self::Api { status, error: Box::new(error), request_id },
            Err(_) => Self::Unexpected {
                status,
                body: String::from_utf8_lossy(body).into_owned(),
                request_id,
            },
        }
    }
}

impl From<tungstenite::Error> for MetastreamsError {
    fn from(error: tungstenite::Error) -> Self {
        // A refused upgrade is an HTTP answer, so it reads as a refused REST call does.
        let tungstenite::Error::Http(response) = error else {
            return Self::WebSocket(Box::new(error));
        };
        let (parts, body) = response.into_parts();
        Self::from_response(parts.status, &parts.headers, body.as_deref().unwrap_or_default())
    }
}

/// The `X-Request-ID` a response carries.
pub(crate) fn request_id(headers: &HeaderMap) -> Option<String> {
    headers.get(REQUEST_ID)?.to_str().ok().map(str::to_owned)
}

/// The wait a `Retry-After` header names, in whole seconds.
fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers.get(RETRY_AFTER)?.to_str().ok()?.trim().parse().ok().map(Duration::from_secs)
}
