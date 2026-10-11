//! [`SortOrder`], the direction a ranked or paged read takes.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Sort direction.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    /// Ascending order (lowest first)
    Asc,
    /// Descending order (highest first)
    #[default]
    Desc,
}
