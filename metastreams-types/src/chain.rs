//! [`SpotChain`], the chain slugs on the wire, and the parse that refuses one the Metastreams way.
//!
//! A slug outside the set answers the typed `UNSUPPORTED_CHAIN`, not serde's own text.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString, IntoStaticStr};
use utoipa::ToSchema;

use crate::error::ErrorDetail;

/// The chains the API serves spot data for.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    ToSchema,
    Display,
    EnumIter,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum SpotChain {
    /// Solana mainnet.
    #[default]
    Solana,
    /// Base mainnet (EVM).
    Base,
    /// BNB Smart Chain (EVM).
    Bsc,
    /// Robinhood Chain mainnet (EVM L2).
    Robinhood,
    /// Arc mainnet (EVM L1). Native gas token is USDC.
    Arc,
}

impl SpotChain {
    /// The address family the chain's wallets and tokens use.
    #[must_use]
    pub fn address_family(&self) -> AddressFamily {
        match self {
            Self::Solana => AddressFamily::Solana,
            Self::Base | Self::Bsc | Self::Robinhood | Self::Arc => AddressFamily::Evm,
        }
    }
}

/// The address scheme a chain's wallets and tokens share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddressFamily {
    /// Solana's ed25519 addresses.
    Solana,
    /// The secp256k1 addresses every EVM chain shares.
    Evm,
}

/// The quote asset a token's deepest pool prices against.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    ToSchema,
    Display,
    EnumIter,
    EnumString,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum AnchorToken {
    /// The chain's gas asset.
    #[default]
    Native,
    /// USD Coin.
    Usdc,
    /// Tether USD.
    Usdt,
    /// World Liberty Financial USD.
    Usd1,
    /// Binance USD.
    Busd,
    /// four.meme's launchpad quote.
    Uusd,
    /// Global Dollar (USDG).
    Usdg,
    /// A quote asset this build does not name.
    #[serde(other)]
    Unknown,
}

/// The chain `slug` names. Case-sensitive, with no aliases, so one spelling names each chain.
pub fn parse(slug: &str) -> Result<SpotChain, ErrorDetail> {
    slug.parse().map_err(|_| ErrorDetail::unsupported_chain(slug))
}

/// `#[serde(deserialize_with)]` helpers that refuse an unknown slug with `UNSUPPORTED_CHAIN`.
pub mod de {
    use serde::{Deserialize, Deserializer};

    use super::SpotChain;
    use crate::csv::split_csv;
    use crate::error::{ErrorDetail, FieldError};

    /// One slug.
    pub fn required<'de, D: Deserializer<'de>>(d: D) -> Result<SpotChain, D::Error> {
        parse(&String::deserialize(d)?)
    }

    /// An optional slug. Pair it with `#[serde(default)]`.
    pub fn optional<'de, D: Deserializer<'de>>(d: D) -> Result<Option<SpotChain>, D::Error> {
        Option::<String>::deserialize(d)?.map(|slug| parse(&slug)).transpose()
    }

    /// A list of slugs.
    pub fn list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<SpotChain>, D::Error> {
        Vec::<String>::deserialize(d)?.iter().map(|slug| parse(slug)).collect()
    }

    /// A `?chains=` CSV of slugs, each kept once. Pair it with `#[serde(default)]`.
    ///
    /// A list that names nothing is refused: an empty scope would read as every chain.
    pub fn csv<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<SpotChain>>, D::Error> {
        let Some(raw) = Option::<String>::deserialize(d)? else {
            return Ok(None);
        };
        let mut chains = Vec::new();
        for slug in split_csv(&raw) {
            let chain = parse(slug)?;
            if !chains.contains(&chain) {
                chains.push(chain);
            }
        }
        if chains.is_empty() {
            let field = FieldError::new("chains", "Name at least one chain.");
            return Err(ErrorDetail::validation(vec![field]).into_de_error());
        }
        Ok(Some(chains))
    }

    fn parse<E: serde::de::Error>(slug: &str) -> Result<SpotChain, E> {
        super::parse(slug).map_err(ErrorDetail::into_de_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use strum::IntoEnumIterator;

    use crate::error::ErrorCode;

    #[test]
    fn every_supported_slug_parses_back_to_itself() {
        for chain in SpotChain::iter() {
            let slug = chain.to_string();
            assert_eq!(parse(&slug).expect("supported slug parses"), chain);
        }
    }

    #[test]
    fn chains_outside_the_set_are_unsupported() {
        // Real `Chain` variants Metastreams does not serve. Spot rows exist only
        // for the `SpotChain` set, and a chain that answers empty is not
        // published.
        for slug in ["arbitrum", "tron", "hyper_evm", "polygon", "ethereum"] {
            let error = parse(slug).expect_err("not served by Metastreams");
            assert_eq!(error.code, ErrorCode::UnsupportedChain);
        }
    }

    #[test]
    fn aliases_and_casing_variants_are_rejected() {
        for slug in ["sol", "eth", "mainnet", "56", "8453", "Solana", "SOLANA", "Bsc"] {
            let error = parse(slug).expect_err("not a supported slug");
            assert_eq!(error.code, ErrorCode::UnsupportedChain);
        }
    }
}
