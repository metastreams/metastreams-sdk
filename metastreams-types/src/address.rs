//! [`MultiChainAddress`], a wallet or token address on any spot chain.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize, Serializer};
use thiserror::Error;

use crate::chain::AddressFamily;

/// Longest base58 text a 32-byte Solana address encodes to.
const MAX_BASE58_LEN: usize = 44;

/// Bytes in a Solana address.
const SOLANA_ADDRESS_BYTES: usize = 32;

/// Hex digits in an EVM address, after its `0x`.
const EVM_HEX_DIGITS: usize = 40;

/// A wallet or token address on any spot chain.
///
/// The text is canonical: base58 for Solana, lowercase `0x`-hex for EVM. So EVM casing does not
/// affect equality or hashing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct MultiChainAddress {
    family: AddressFamily,
    text: String,
}

impl MultiChainAddress {
    /// The address family this address belongs to.
    #[must_use]
    pub fn family(&self) -> AddressFamily {
        self.family
    }
}

impl FromStr for MultiChainAddress {
    type Err = AddressParseError;

    /// Text that starts with `0x` or `0X` is an EVM candidate. Any other text is a Solana
    /// candidate, so bare hex cannot pass as an EVM address.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let address = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            Some(hex) => is_evm_hex(hex).then(|| Self {
                family: AddressFamily::Evm,
                text: format!("0x{}", hex.to_ascii_lowercase()),
            }),
            None => is_solana_base58(s)
                .then(|| Self { family: AddressFamily::Solana, text: s.to_owned() }),
        };
        address.ok_or_else(|| AddressParseError(s.to_owned()))
    }
}

impl TryFrom<String> for MultiChainAddress {
    type Error = AddressParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl fmt::Display for MultiChainAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl Serialize for MultiChainAddress {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// The refusal of a string that is neither a Solana pubkey nor an EVM address.
#[derive(Debug, Error)]
#[error("invalid token address {0:?}: expected a Solana pubkey or EVM address")]
pub struct AddressParseError(String);

/// Whether `hex` is the 40 hex digits of an EVM address, in any case.
fn is_evm_hex(hex: &str) -> bool {
    hex.len() == EVM_HEX_DIGITS && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether `text` is base58 that decodes to exactly the 32 bytes of a Solana address.
fn is_solana_base58(text: &str) -> bool {
    let mut bytes = [0; SOLANA_ADDRESS_BYTES];
    text.len() <= MAX_BASE58_LEN
        && bs58::decode(text).onto(&mut bytes).is_ok_and(|len| len == SOLANA_ADDRESS_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
    const EVM_ADDR: &str = "0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c";

    #[test]
    fn an_address_reads_and_writes_its_canonical_text() {
        for s in [SOL_MINT, EVM_ADDR] {
            let address: MultiChainAddress = s.parse().expect("parses");
            assert_eq!(address.to_string(), s);
            assert_eq!(serde_json::to_value(&address).expect("serializes"), s);
            let read: MultiChainAddress = serde_json::from_value(s.into()).expect("deserializes");
            assert_eq!(read, address);
        }
    }

    #[test]
    fn evm_casing_normalizes_under_either_prefix() {
        for s in [
            "0xBB4CDb9CBd36B01bD1cBaEBF2De08d9173bc095c",
            "0XBB4CDb9CBd36B01bD1cBaEBF2De08d9173bc095c",
        ] {
            let address: MultiChainAddress = s.parse().expect("parses");
            assert_eq!(address.to_string(), EVM_ADDR);
            assert_eq!(address.family(), AddressFamily::Evm);
        }
    }

    #[test]
    fn malformed_text_is_refused() {
        for bad in [
            "",
            "not-an-address",
            "abcdabcdabcdabcdabcdabcdabcdabcdabcdabcd",
            "0xabcd",
            "0x0xabcdabcdabcdabcdabcdabcdabcdabcdabcdab",
            "1111111111111111111111111111111",
            "111111111111111111111111111111111",
        ] {
            assert!(bad.parse::<MultiChainAddress>().is_err(), "{bad}");
        }
    }

    #[test]
    fn the_refusal_names_the_text() {
        let error = "nope".parse::<MultiChainAddress>().expect_err("refused");

        assert_eq!(
            error.to_string(),
            "invalid token address \"nope\": expected a Solana pubkey or EVM address"
        );
    }
}
