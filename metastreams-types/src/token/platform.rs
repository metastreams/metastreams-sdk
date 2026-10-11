//! [`UiPlatform`], the terminals and listings attached to a token, a wallet, or a trade.

use serde::{Deserialize, Serialize};
use strum::{EnumIter, IntoEnumIterator};
use utoipa::ToSchema;

use crate::time::UnixMilliseconds;

/// A platform attached to a token, a wallet, or a trade.
///
/// The fieldless variants are trading terminals. Each doc gives the producer string its
/// variant maps from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, EnumIter)]
#[serde(tag = "platform", rename_all = "camelCase")]
pub enum UiPlatform {
    /// fomo.family, the terminal that routed the trade. The trader's profile is the `fomoscan`
    /// element of `identities`.
    Fomo,
    /// A token's DexScreener listing; present only while its profile is paid.
    #[serde(rename_all = "camelCase")]
    DexScreener {
        /// Whether the token's DexScreener profile is paid.
        is_paid: bool,
        /// Earliest profile payment time.
        paid_at: Option<UnixMilliseconds>,
    },
    /// Axiom.
    Axiom,
    /// "Banana Gun".
    BananaGun,
    /// Bloom.
    Bloom,
    /// "BONKbot".
    Bonkbot,
    /// BtcTurk.
    BtcTurk,
    /// BullX.
    BullX,
    /// Click, identified by its fee vault.
    Click,
    /// "GMGN".
    Gmgn,
    /// "Lucky Block".
    LuckyBlock,
    /// Maestro.
    Maestro,
    /// "Manifold Trading".
    ManifoldTrading,
    /// "MEVX".
    Mevx,
    /// Mintech.
    Mintech,
    /// Nighthawk.
    Nighthawk,
    /// Nova.
    Nova,
    /// "OX.FUN".
    OxFun,
    /// Padre.
    Padre,
    /// PepeBoost.
    PepeBoost,
    /// Phantom.
    Phantom,
    /// Photon.
    Photon,
    /// SexBotSolana.
    SexBotSolana,
    /// Shuriken.
    Shuriken,
    /// "SOL Sniper Bot".
    SolSniperBot,
    /// "Sol Trading Bot".
    SolTradingBot,
    /// Trojan.
    Trojan,
    /// A platform this build does not name. Read-only: the server never sends it.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl UiPlatform {
    /// The platform a trade's `platform_name` names, or `None` for a string no variant covers.
    /// The server produces each name; a new platform appears here when it ships there.
    #[must_use]
    pub fn from_platform_name(name: &str) -> Option<Self> {
        Some(match name {
            "Axiom" => Self::Axiom,
            "Banana Gun" => Self::BananaGun,
            "Bloom" => Self::Bloom,
            "BONKbot" => Self::Bonkbot,
            "BtcTurk" => Self::BtcTurk,
            "BullX" => Self::BullX,
            "Click" => Self::Click,
            "GMGN" => Self::Gmgn,
            "Lucky Block" => Self::LuckyBlock,
            "Maestro" => Self::Maestro,
            "Manifold Trading" => Self::ManifoldTrading,
            "MEVX" => Self::Mevx,
            "Mintech" => Self::Mintech,
            "Nighthawk" => Self::Nighthawk,
            "Nova" => Self::Nova,
            "OX.FUN" => Self::OxFun,
            "Padre" => Self::Padre,
            "PepeBoost" => Self::PepeBoost,
            "Phantom" => Self::Phantom,
            "Photon" => Self::Photon,
            "SexBotSolana" => Self::SexBotSolana,
            "Shuriken" => Self::Shuriken,
            "SOL Sniper Bot" => Self::SolSniperBot,
            "Sol Trading Bot" => Self::SolTradingBot,
            "Trojan" => Self::Trojan,
            "fomo" => Self::Fomo,
            _ => return None,
        })
    }

    /// The platform whose wire tag is `tag` in any ASCII case, such as `bullx` for `bullX`.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Self> {
        Self::iter().find(|platform| {
            // `Unknown` skips serializing, so it has no tag and matches nothing.
            let Ok(body) = serde_json::to_value(platform) else {
                return false;
            };
            body["platform"].as_str().is_some_and(|known| known.eq_ignore_ascii_case(tag))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_platform_reads_as_unknown() {
        let platform: UiPlatform =
            serde_json::from_str(r#"{"platform":"someNewTerminal"}"#).expect("parses");
        assert_eq!(platform, UiPlatform::Unknown);
    }

    #[test]
    fn a_terminal_serializes_as_its_tag_alone() {
        let body = serde_json::to_value(UiPlatform::Axiom).expect("serializes");
        assert_eq!(body, serde_json::json!({ "platform": "axiom" }));
    }

    #[test]
    fn a_paid_listing_carries_its_fields_next_to_the_tag() {
        let body = serde_json::to_value(UiPlatform::DexScreener {
            is_paid: true,
            paid_at: Some(UnixMilliseconds(1_723_180_800_000)),
        })
        .expect("serializes");

        assert_eq!(
            body,
            serde_json::json!({ "platform": "dexScreener", "isPaid": true, "paidAt": 1_723_180_800_000_i64 })
        );
    }

    #[test]
    fn an_unknown_producer_string_maps_to_nothing() {
        assert_eq!(UiPlatform::from_platform_name("Axiom"), Some(UiPlatform::Axiom));
        assert_eq!(UiPlatform::from_platform_name("fomo"), Some(UiPlatform::Fomo));
        assert_eq!(UiPlatform::from_platform_name("nope"), None);
    }

    #[test]
    fn a_tag_names_its_platform_in_any_ascii_case() {
        for tag in ["axiom", "Axiom", "AXIOM"] {
            assert_eq!(UiPlatform::from_tag(tag), Some(UiPlatform::Axiom), "{tag}");
        }
        for tag in ["bullX", "bullx", "BullX"] {
            assert_eq!(UiPlatform::from_tag(tag), Some(UiPlatform::BullX), "{tag}");
        }
        assert_eq!(UiPlatform::from_tag("not_a_platform"), None);
        assert_eq!(UiPlatform::from_tag("unknown"), None, "the catch-all names no platform");
    }
}
