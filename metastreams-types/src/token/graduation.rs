//! [`GraduationStage`], where a launchpad token sits in its launch.

use serde::{Deserialize, Serialize};

/// A launchpad token's stage, from launch to its DEX listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraduationStage {
    /// The bonding curve is below 40%.
    NewlyCreated,
    /// The bonding curve is at 40% or above.
    AboutToGraduate,
    /// The token migrated to a DEX.
    Graduated,
}
