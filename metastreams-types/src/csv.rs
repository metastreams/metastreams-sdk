//! A query parameter that holds a list as one comma-separated string.

use std::collections::HashSet;
use std::fmt;
use std::hash::Hash;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serializer};
use thiserror::Error;

/// The part [`parse_csv`] rejected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown value `{0}`")]
pub struct RejectedCsvPart(pub String);

/// The trimmed, non-empty parts of a comma-separated list.
///
/// `""` and `",,"` yield no part, so the parse tolerates a trailing comma.
pub fn split_csv(raw: &str) -> impl Iterator<Item = &str> {
    raw.split(',').map(str::trim).filter(|part| !part.is_empty())
}

/// The values a comma-separated list names, parsed as `T` from [`split_csv`]'s parts.
///
/// The first part `T` rejects fails the whole parse, and the error carries that part.
pub fn parse_csv<T: FromStr>(raw: &str) -> Result<Vec<T>, RejectedCsvPart> {
    split_csv(raw).map(|part| part.parse().map_err(|_| RejectedCsvPart(part.to_string()))).collect()
}

/// [`parse_csv`], then later duplicates dropped, first-seen order kept.
pub fn parse_csv_deduped<T: FromStr + Eq + Hash + Clone>(
    raw: &str,
) -> Result<Vec<T>, RejectedCsvPart> {
    let parsed: Vec<T> = parse_csv(raw)?;
    let mut seen = HashSet::new();
    Ok(parsed.into_iter().filter(|item| seen.insert(item.clone())).collect())
}

/// Joins the items into the list [`deserialize`] reads.
pub fn serialize<S, T>(items: &Option<Vec<T>>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: fmt::Display,
{
    let Some(items) = items else {
        return s.serialize_none();
    };
    let parts: Vec<String> = items.iter().map(T::to_string).collect();
    s.serialize_str(&parts.join(","))
}

/// Splits the list on commas, trims each part, and skips the empty ones.
pub fn deserialize<'de, D, T>(d: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
{
    let Some(raw) = Option::<String>::deserialize(d)? else {
        return Ok(None);
    };
    parse_csv(&raw).map(Some).map_err(serde::de::Error::custom)
}

/// `#[serde(with)]` for an optional `chains` scope: each slug once, an unknown one refused.
/// Pair it with `#[serde(default)]`.
pub mod chains {
    use serde::{Deserializer, Serializer};

    use crate::chain::SpotChain;

    /// Joins the slugs into the list [`deserialize`] reads.
    pub fn serialize<S: Serializer>(
        chains: &Option<Vec<SpotChain>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        super::serialize(chains, s)
    }

    /// Reads the list with [`crate::chain::de::csv`].
    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<Vec<SpotChain>>, D::Error> {
        crate::chain::de::csv(d)
    }
}

/// `#[serde(with)]` for a required `wallets` list: each address once, a malformed one refused.
pub mod wallets {
    use serde::{Deserializer, Serializer};

    use crate::address::MultiChainAddress;

    /// Joins the wallets into the list [`deserialize`] reads.
    pub fn serialize<S: Serializer>(
        wallets: &[MultiChainAddress],
        s: S,
    ) -> Result<S::Ok, S::Error> {
        super::addresses::serialize(wallets, s)
    }

    /// Splits the list on commas and keeps each address once.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Vec<MultiChainAddress>, D::Error> {
        super::addresses::deserialize(d, "wallets", "Each wallet must be a Solana or EVM address.")
    }
}

/// `#[serde(with)]` for a required `tokens` list: each address once, a malformed one refused.
pub mod tokens {
    use serde::{Deserializer, Serializer};

    use crate::address::MultiChainAddress;

    /// Joins the tokens into the list [`deserialize`] reads.
    pub fn serialize<S: Serializer>(tokens: &[MultiChainAddress], s: S) -> Result<S::Ok, S::Error> {
        super::addresses::serialize(tokens, s)
    }

    /// Splits the list on commas and keeps each address once.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Vec<MultiChainAddress>, D::Error> {
        super::addresses::deserialize(d, "tokens", "Each token must be a Solana or EVM address.")
    }
}

/// `#[serde(with)]` for the optional `tokens` and `wallets` lists. Pair it with `#[serde(default)]`.
pub mod optional {
    /// [`super::wallets`], for a list the read may omit.
    pub mod wallets {
        use serde::{Deserialize, Deserializer, Serializer};

        use crate::address::MultiChainAddress;

        /// Joins the wallets into the list [`deserialize`] reads.
        pub fn serialize<S: Serializer>(
            wallets: &Option<Vec<MultiChainAddress>>,
            s: S,
        ) -> Result<S::Ok, S::Error> {
            crate::csv::serialize(wallets, s)
        }

        /// Reads the list as [`super::super::wallets`] does.
        pub fn deserialize<'de, D: Deserializer<'de>>(
            d: D,
        ) -> Result<Option<Vec<MultiChainAddress>>, D::Error> {
            let Some(raw) = Option::<String>::deserialize(d)? else {
                return Ok(None);
            };
            let message = "Each wallet must be a Solana or EVM address.";
            crate::csv::addresses::parse(&raw, "wallets", message).map(Some)
        }
    }

    /// [`super::tokens`], for a list the read may omit.
    pub mod tokens {
        use serde::{Deserialize, Deserializer, Serializer};

        use crate::address::MultiChainAddress;

        /// Joins the tokens into the list [`deserialize`] reads.
        pub fn serialize<S: Serializer>(
            tokens: &Option<Vec<MultiChainAddress>>,
            s: S,
        ) -> Result<S::Ok, S::Error> {
            crate::csv::serialize(tokens, s)
        }

        /// Reads the list as [`super::super::tokens`] does.
        pub fn deserialize<'de, D: Deserializer<'de>>(
            d: D,
        ) -> Result<Option<Vec<MultiChainAddress>>, D::Error> {
            let Some(raw) = Option::<String>::deserialize(d)? else {
                return Ok(None);
            };
            let message = "Each token must be a Solana or EVM address.";
            crate::csv::addresses::parse(&raw, "tokens", message).map(Some)
        }
    }
}

/// The address-list parse that [`wallets`], [`tokens`] and [`optional`] share.
mod addresses {
    use serde::{Deserialize, Deserializer, Serializer};

    use super::parse_csv_deduped;
    use crate::address::MultiChainAddress;
    use crate::error::{ErrorDetail, FieldError};

    /// Joins the addresses with commas.
    pub fn serialize<S: Serializer>(
        addresses: &[MultiChainAddress],
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let parts: Vec<String> = addresses.iter().map(ToString::to_string).collect();
        s.serialize_str(&parts.join(","))
    }

    /// Reads a required list of `field`.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
        field: &str,
        message: &str,
    ) -> Result<Vec<MultiChainAddress>, D::Error> {
        parse(&String::deserialize(d)?, field, message)
    }

    /// Each address of `raw` once, or a validation error on `field` that says `message`.
    pub fn parse<E: serde::de::Error>(
        raw: &str,
        field: &str,
        message: &str,
    ) -> Result<Vec<MultiChainAddress>, E> {
        parse_csv_deduped(raw).map_err(|_| {
            ErrorDetail::validation(vec![FieldError::new(field, message)]).into_de_error()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_csv_trims_and_drops_empty_parts() {
        assert_eq!(split_csv(" a, b ,, c,").collect::<Vec<_>>(), vec!["a", "b", "c"]);
        assert_eq!(split_csv("").count(), 0);
        assert_eq!(split_csv(" ,, ").count(), 0);
    }

    #[test]
    fn parse_csv_names_the_rejected_part() {
        assert_eq!(parse_csv::<u32>("1, 2"), Ok(vec![1, 2]));
        assert_eq!(parse_csv::<u32>("1,x,y"), Err(RejectedCsvPart("x".to_owned())));
    }

    #[test]
    fn parse_csv_deduped_keeps_the_first_of_each() {
        assert_eq!(parse_csv_deduped::<u32>("3,1,3,2,1"), Ok(vec![3, 1, 2]));
    }
}
