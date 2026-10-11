//! The error body every refusal carries, and the code registry it names.

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use utoipa::ToSchema;

use crate::chain::SpotChain;

/// Marker prefixing an [`ErrorDetail`] that a `Deserialize` impl carries through serde's
/// string-only error channel. The server reads it back.
pub const DE_MARKER: &str = "__METASTREAMS_ERROR__";

/// Longest fragment of caller input a rejection repeats back.
pub const ECHO_MAX_CHARS: usize = 64;

/// Every wire error code the API can return.
///
/// Append-only: a new code is non-breaking, since clients fall back on the
/// HTTP status for one they do not recognize, and a published code is never
/// renamed or removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// A key or session token that is missing, malformed, unknown, revoked or expired.
    /// On the dashboard, no valid session.
    Unauthorized,
    /// A valid key whose plan excludes this endpoint, or a session token where only a key may act.
    /// On the dashboard, an admin action by a non-admin.
    Forbidden,
    /// Malformed body or parameters.
    ValidationError,
    /// Chain slug outside the supported set.
    UnsupportedChain,
    /// Resource does not exist.
    NotFound,
    /// Rate limit exceeded; retry after `Retry-After`.
    RateLimited,
    /// The organization's credits this billing period reach its plan's limit.
    CreditsExhausted,
    /// Unexpected server failure.
    InternalError,
    /// Dependency down or maintenance.
    ServiceUnavailable,
    /// A `fromAt`–`toAt` range wider than the endpoint allows.
    RangeTooLarge,
    /// Dashboard sign-in with a wrong email and password pair, or an unknown email.
    InvalidCredentials,
    /// A dashboard write without an `Origin` on the CORS allowlist.
    /// Also a session token presented without an `Origin` that the token names.
    OriginNotAllowed,
    /// The signed-in user has no membership in the organization.
    NotOrgMember,
    /// The signed-in user's membership in the organization is inactive.
    MembershipInactive,
    /// An admin action by a user whose email is not verified.
    EmailNotVerified,
    /// Sign-up with an email that already has an account.
    EmailTaken,
    /// The account is already a member of the organization.
    AlreadyMember,
    /// The address already holds a pending invitation to the organization.
    InvitationAlreadySent,
    /// The invitation was accepted already.
    InvitationAlreadyAccepted,
    /// The write would leave the organization without an active admin.
    LastAdmin,
    /// An admin tried to remove their own membership.
    CannotRemoveSelf,
    /// A mint would take the organization past its active-key cap.
    KeyLimitReached,
    /// A mailed token or invitation that is expired, spent or cancelled.
    TokenExpired,
    /// An upload over its size limit.
    PayloadTooLarge,
    /// An upload whose bytes are not an accepted type.
    UnsupportedMediaType,
    /// An image over its pixel or frame limit.
    ImageTooLarge,
    /// A code this build does not name. Read-only: the server never sends it.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// One invalid field, as carried in a `VALIDATION_ERROR`'s `details`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FieldError {
    /// The offending field or query parameter, in its wire (camelCase) form.
    pub field: String,
    /// What is wrong with it, and what a valid value looks like.
    pub message: String,
}

impl FieldError {
    /// Build a field error.
    #[must_use]
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self { field: field.into(), message: message.into() }
    }
}

/// The `error` object inside the envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDetail {
    /// Stable, machine-readable. Clients switch on this, never on `message`.
    pub code: ErrorCode,
    /// Human-readable, written for the integrating developer:
    /// `[What happened]. [What to do next — if actionable].`
    pub message: String,
    /// Optional; shape is fixed per code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorDetail {
    /// A `VALIDATION_ERROR` that names each invalid field.
    #[must_use]
    pub fn validation(fields: Vec<FieldError>) -> Self {
        Self {
            code: ErrorCode::ValidationError,
            message: "One or more parameters are invalid.".to_owned(),
            details: Some(serde_json::json!(fields)),
        }
    }

    /// An `UNSUPPORTED_CHAIN` for `slug`, listing the supported slugs.
    #[must_use]
    pub fn unsupported_chain(slug: &str) -> Self {
        let slug = echoed(slug);
        let supported: Vec<String> = SpotChain::iter().map(|chain| chain.to_string()).collect();
        Self {
            code: ErrorCode::UnsupportedChain,
            message: format!(
                "Chain '{slug}' is not supported. Supported chains: {}.",
                supported.join(", ")
            ),
            details: Some(serde_json::json!({ "chain": slug })),
        }
    }

    /// This detail behind [`DE_MARKER`], as a serde error.
    pub fn into_de_error<E: serde::de::Error>(self) -> E {
        let detail = serde_json::to_string(&self).expect("detail serializes");
        E::custom(format!("{DE_MARKER}{detail}"))
    }
}

/// The complete error response body — every error is exactly this.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ErrorEnvelope {
    /// The error.
    pub error: ErrorDetail,
}

/// `raw` without control characters, cut to [`ECHO_MAX_CHARS`] and marked `…` where cut.
#[must_use]
pub fn echoed(raw: &str) -> String {
    let visible = || raw.chars().filter(|c| !c.is_control());
    let safe: String = visible().take(ECHO_MAX_CHARS).collect();
    if safe.chars().count() < visible().count() {
        return format!("{safe}…");
    }
    safe
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_code_reads_as_unknown() {
        let detail: ErrorDetail =
            serde_json::from_str(r#"{"code":"SOME_NEW_CODE","message":"New."}"#).expect("parses");
        assert_eq!(detail.code, ErrorCode::Unknown);
    }

    #[test]
    fn an_envelope_round_trips() {
        let envelope = ErrorEnvelope {
            error: ErrorDetail {
                code: ErrorCode::ValidationError,
                message: "One or more parameters are invalid.".to_owned(),
                details: Some(serde_json::json!([FieldError::new("q", "Must not be blank.")])),
            },
        };

        let body = serde_json::to_value(&envelope).expect("serializes");
        assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
        assert_eq!(serde_json::from_value::<ErrorEnvelope>(body).expect("deserializes"), envelope);
    }
}
