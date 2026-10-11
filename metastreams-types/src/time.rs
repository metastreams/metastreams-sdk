//! [`UnixMilliseconds`], the instant every timestamp on the surface carries.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// An instant, as milliseconds since the Unix epoch.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[serde(transparent)]
pub struct UnixMilliseconds(pub i64);
