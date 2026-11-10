//! The thesis object — one social post about a token, its author, and the position it reports.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use utoipa::openapi::schema::Object;
use utoipa::{IntoParams, ToSchema};

use crate::address::MultiChainAddress;
use crate::decimal::Decimal;
use crate::position::UiSpotPosition;
use crate::schema::{enum_values, open_enumeration};
use crate::sort::SortOrder;
use crate::time::UnixMilliseconds;

/// Theses returned when a read names no `limit`.
pub const THESES_LIMIT_DEFAULT: u32 = 50;

/// Most theses one page may carry.
pub const THESES_LIMIT_MAX: u32 = 100;

/// Query for `GET /v1/tokens/{chain}/{address}/theses`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenThesesQuery {
    /// Theses per page, clamped to 1-100; 50 when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[param(minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
    /// Opaque cursor from the previous page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Apps to keep, comma-separated: `fomo`, `pump`, `gmgn` or `axiom`. Absent keeps every app.
    #[serde(default, with = "crate::csv", skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, example = "fomo,pump")]
    pub sources: Option<Vec<ThesisSourceFilter>>,
    /// Author wallets to keep, comma-separated; 1 to 100. Absent keeps every author.
    #[serde(default, with = "crate::csv", skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, example = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin")]
    pub wallets: Option<Vec<MultiChainAddress>>,
    /// Author handles to keep, comma-separated and matched exactly; 1 to 100. Absent keeps every
    /// author.
    #[serde(default, with = "crate::csv", skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, example = "cooker")]
    pub handles: Option<Vec<String>>,
    /// Earliest `postedAt` kept, inclusive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_at: Option<UnixMilliseconds>,
    /// Latest `postedAt` kept, inclusive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_at: Option<UnixMilliseconds>,
    /// Smallest `positionUsd` kept, inclusive. A floor drops theses whose app reports no position.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, example = "1000")]
    pub min_position_usd: Option<Decimal>,
    /// Largest `positionUsd` kept, inclusive.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, example = "50000")]
    pub max_position_usd: Option<Decimal>,
    /// Fewest likes kept, inclusive. A set filter keeps only apps that report likes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_likes: Option<u32>,
    /// Most likes kept, inclusive. A set filter keeps only apps that report likes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_likes: Option<u32>,
    /// `true` keeps verified authors only; `false` keeps unverified authors only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified: Option<bool>,
    /// `true` keeps the token's deployer only; `false` keeps every other author. A set filter
    /// keeps only apps that report the deployer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_dev: Option<bool>,
    /// Figure the page ranks on; `postedAt` when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<ThesisSortKey>,
    /// Rank direction; `desc` when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<SortOrder>,
}

impl TokenThesesQuery {
    /// Theses per page, clamped to `1..=`[`THESES_LIMIT_MAX`].
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(THESES_LIMIT_DEFAULT).clamp(1, THESES_LIMIT_MAX)
    }

    /// The figure the page ranks on; [`ThesisSortKey::PostedAt`] when absent.
    #[must_use]
    pub fn sort_by(&self) -> ThesisSortKey {
        self.sort_by.unwrap_or_default()
    }

    /// The rank direction; [`SortOrder::Desc`] when absent.
    #[must_use]
    pub fn order(&self) -> SortOrder {
        self.order.unwrap_or_default()
    }
}

/// The app a thesis was published on.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ThesisSource {
    /// fomo.family.
    Fomo,
    /// pump.fun.
    Pump,
    /// GMGN.
    Gmgn,
    /// Axiom.
    Axiom,
    /// An app this build does not name. Read-only: the server never sends it.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// An app a theses query keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ThesisSourceFilter {
    /// fomo.family.
    Fomo,
    /// pump.fun.
    Pump,
    /// GMGN.
    Gmgn,
    /// Axiom.
    Axiom,
}

/// A figure the theses read ranks on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ThesisSortKey {
    /// When the app recorded the thesis.
    #[default]
    PostedAt,
    /// The author's position size in USD, as the app reported it.
    PositionUsd,
}

/// One thesis: a social post about a token.
///
/// The position figures are what the app reported, in USD alone, as of its latest publish.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenThesis {
    /// The app the thesis was published on.
    #[schema(schema_with = open_thesis_source)]
    pub source: ThesisSource,
    /// Stable id of the thesis, unique within its `source`.
    pub thesis_id: String,
    /// The app's own id for the parent thesis; `null` for a top-level thesis.
    /// It need not match any `thesisId`.
    pub parent_id: Option<String>,
    /// When the app recorded the thesis.
    pub posted_at: UnixMilliseconds,
    /// The thesis's text.
    pub text: String,
    /// Likes the thesis has drawn; `null` when the app does not report likes.
    pub like_count: Option<u32>,
    /// Who wrote the thesis.
    pub author: UiThesisAuthor,
    /// The author's trading wallet on this token; `null` when none is attributed.
    pub wallet: Option<String>,
    /// The author's position size in USD, as the app reports it, as of the latest publish.
    /// It can be negative; `null` when the API could not read the position.
    #[schema(value_type = Option<Decimal>, examples("142.5"))]
    pub position_usd: Option<Decimal>,
    /// The author's token balance, in whole token units.
    /// `null` when the app does not report it or the API could not read the position.
    #[schema(value_type = Option<Decimal>, examples("1234.5"))]
    pub token_amount: Option<Decimal>,
    /// Unrealized PnL on the position in USD; `null` when the app reports none.
    #[schema(value_type = Option<Decimal>)]
    pub unrealized_pnl_usd: Option<Decimal>,
    /// Unrealized PnL as a percent of cost (`"5"` is 5%); `null` when the app reports none.
    #[schema(value_type = Option<Decimal>)]
    pub unrealized_pnl_pct: Option<Decimal>,
    /// Realized PnL on the position in USD; `null` when the app reports none.
    #[schema(value_type = Option<Decimal>)]
    pub realized_pnl_usd: Option<Decimal>,
    /// Cost basis of the position in USD; `null` when the app reports none.
    #[schema(value_type = Option<Decimal>)]
    pub cost_basis_usd: Option<Decimal>,
    /// The token's USD market cap when the author made the call; `null` when the app sends none.
    #[schema(value_type = Option<Decimal>)]
    pub called_out_at_market_cap_usd: Option<Decimal>,
    /// The token's gain since the call, as a multiple of that market cap; `null` when unreported.
    #[schema(value_type = Option<Decimal>)]
    pub multiple: Option<Decimal>,
    /// When the author closed the position, as the app reported it; `null` when unreported.
    pub closed_at: Option<UnixMilliseconds>,
    /// The author's live position on this token, as `/v1/positions` serves it. `null` when no
    /// wallet is attributed or the read did not resolve; a closed position is not `null`.
    pub author_position: Option<UiSpotPosition>,
}

/// The author of a thesis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiThesisAuthor {
    /// The app's own id for the author.
    pub id: String,
    /// The author's handle on the app.
    pub handle: String,
    /// The author's display name; `null` when the app sends none.
    pub display_name: Option<String>,
    /// URL of the author's avatar; `null` when none is known.
    pub avatar_url: Option<String>,
    /// Whether the app verifies the author.
    pub verified: bool,
    /// Whether the author deployed this token; `null` when the app does not report it.
    pub is_dev: Option<bool>,
    /// The author's linked X handle; `null` when unlinked.
    pub twitter_username: Option<String>,
}

/// A page of a token's theses (`GET /v1/tokens/{chain}/{address}/theses`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenTheses {
    /// Theses on this page, in the requested order.
    pub theses: Vec<UiTokenThesis>,
    /// Cursor for the next page; `null` on the last page.
    pub next_cursor: Option<String>,
    /// Whether a next page exists.
    pub has_more: bool,
}

/// The published `source` schema: an open string, since new apps join the feed.
fn open_thesis_source() -> Object {
    open_enumeration(
        "The app the thesis was published on. This is an open enumeration: new apps join, \
         so keep a value you do not recognize.",
        enum_values::<ThesisSource>(),
    )
}
