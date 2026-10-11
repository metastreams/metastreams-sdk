//! The balance object: `UiWalletBalance`, one wallet's balance of one asset.

use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer, Serialize};
use strum::Display;
use utoipa::openapi::schema::Object;
use utoipa::{IntoParams, ToSchema};

use crate::address::MultiChainAddress;
use crate::chain::SpotChain;
use crate::decimal::Decimal;
use crate::schema::{enum_values, open_enumeration};
use crate::time::UnixMilliseconds;

/// Every trigger the surface publishes, which the document lists as examples.
const KNOWN_TRIGGERS: [BalanceTrigger; 4] = [
    BalanceTrigger::Trade,
    BalanceTrigger::Deposit,
    BalanceTrigger::Withdrawal,
    BalanceTrigger::FailedTx,
];

/// Query for `GET /v1/wallets/balances`.
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WalletBalancesQuery {
    /// Wallet addresses, CSV; one to 100.
    #[serde(with = "crate::csv::wallets")]
    #[param(value_type = String, example = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin")]
    pub wallets: Vec<MultiChainAddress>,
    /// Chains to scope to, CSV of the chain slugs. Absent means every spot chain.
    #[serde(default, with = "crate::csv::chains", skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, example = "solana,base")]
    pub chains: Option<Vec<SpotChain>>,
}

/// One wallet's balance of one asset, as of the transaction that last moved it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiWalletBalance {
    /// Chain the balance lives on.
    pub chain: SpotChain,
    /// Wallet holding it.
    pub wallet: String,
    /// Gas token or stablecoin the balance is denominated in.
    #[schema(schema_with = open_asset)]
    pub asset: DepositAsset,
    /// Post-transaction balance, human units.
    #[schema(value_type = Decimal, examples("1.5"))]
    pub balance: Decimal,
    /// What moved it.
    #[schema(schema_with = open_trigger)]
    pub trigger: BalanceTrigger,
    /// Block or Solana slot where the balance was observed.
    pub block_number: u64,
    /// Transaction that moved it.
    pub signature: String,
    /// Transaction index within the observed block or slot.
    pub tx_index: u64,
    /// Block time of the transaction that moved it.
    pub updated_at: UnixMilliseconds,
}

/// The latest balance of each wallet and asset a read named.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiWalletBalances {
    /// One row per observed wallet and asset, ordered by chain, wallet, then asset.
    #[serde(deserialize_with = "known_balances")]
    pub balances: Vec<UiWalletBalance>,
}

/// A deposit or settlement asset symbol: the gas tokens and per-chain stables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
#[strum(serialize_all = "UPPERCASE")]
pub enum DepositAsset {
    /// Solana native gas token.
    Sol,
    /// Native gas token on Ethereum, Base, Arbitrum, and Robinhood.
    Eth,
    /// BNB Smart Chain native gas token.
    Bnb,
    /// Polygon native gas token (formerly MATIC).
    Pol,
    /// USD Coin. The common bridge asset on most corridors.
    Usdc,
    /// Tron native gas token.
    Trx,
    /// Tether USD. Tron's deposit stable.
    Usdt,
    /// Global Dollar (USDG). Robinhood's deposit stable.
    Usdg,
    /// Polymarket pUSD trading collateral on Polygon (USDC.e wrapped 1:1).
    Pusd,
}

/// What caused a wallet's balance to change.
#[derive(Debug, Clone, PartialEq, Eq, Display, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum BalanceTrigger {
    /// Balance moved via a trade (swap).
    Trade,
    /// Funds received into the wallet.
    Deposit,
    /// Funds sent out of the wallet.
    Withdrawal,
    /// Fees deducted on a failed transaction.
    FailedTx,
    /// Authoritative point-in-time chain read, not caused by an observed transaction.
    Snapshot,
}

/// The published `asset` schema: an open string, since the asset set grows.
fn open_asset() -> Object {
    open_enumeration(
        "The asset the balance is in. This is an open enumeration: keep a value you do not recognize.",
        enum_values::<DepositAsset>(),
    )
}

/// The rows this build can read. A reader drops a row whose asset or trigger a newer server adds.
fn known_balances<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<UiWalletBalance>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum MaybeBalance {
        Known(UiWalletBalance),
        Unknown(IgnoredAny),
    }

    let rows = Vec::<MaybeBalance>::deserialize(d)?;
    Ok(rows
        .into_iter()
        .filter_map(|row| match row {
            MaybeBalance::Known(balance) => Some(balance),
            MaybeBalance::Unknown(_) => None,
        })
        .collect())
}

/// The published `trigger` schema: an open string, since the trigger set grows.
fn open_trigger() -> Object {
    open_enumeration(
        "What moved the balance. This is an open enumeration: keep a value you do not recognize.",
        KNOWN_TRIGGERS.iter().map(|trigger| trigger.to_string().into()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOL_WALLET: &str = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";
    const EVM_WALLET: &str = "0x1234567890abcdef1234567890abcdef12345678";

    fn balance(asset: DepositAsset) -> UiWalletBalance {
        UiWalletBalance {
            chain: SpotChain::Solana,
            wallet: SOL_WALLET.to_owned(),
            asset,
            balance: Decimal::ONE,
            trigger: BalanceTrigger::Deposit,
            block_number: 371_044_912,
            signature: "5hR9qT2".to_owned(),
            tx_index: 14,
            updated_at: UnixMilliseconds(1_790_930_462_000),
        }
    }

    #[test]
    fn the_wallets_list_keeps_each_address_once_and_writes_back() {
        let query: WalletBalancesQuery = serde_json::from_value(serde_json::json!({
            "wallets": format!("{SOL_WALLET}, {EVM_WALLET},{SOL_WALLET}"),
        }))
        .expect("parses");

        assert_eq!(query.wallets.len(), 2);
        assert!(query.chains.is_none());
        assert_eq!(
            serde_json::to_value(&query).expect("serializes"),
            serde_json::json!({ "wallets": format!("{SOL_WALLET},{EVM_WALLET}") })
        );
    }

    #[test]
    fn a_wallet_that_is_not_an_address_is_refused() {
        let error = serde_json::from_value::<WalletBalancesQuery>(
            serde_json::json!({ "wallets": "not-a-wallet" }),
        )
        .expect_err("refused");

        assert!(error.to_string().contains("Each wallet must be"));
    }

    #[test]
    fn a_balance_renders_its_wire_shape_and_reads_back() {
        let balance = balance(DepositAsset::Sol);
        let rendered = serde_json::to_value(&balance).expect("serializes");

        assert_eq!(
            rendered,
            serde_json::json!({
                "chain": "solana",
                "wallet": SOL_WALLET,
                "asset": "SOL",
                "balance": "1",
                "trigger": "deposit",
                "blockNumber": 371_044_912,
                "signature": "5hR9qT2",
                "txIndex": 14,
                "updatedAt": 1_790_930_462_000_u64,
            })
        );
        let read: UiWalletBalance = serde_json::from_value(rendered).expect("deserializes");
        assert_eq!(read, balance);
    }

    #[test]
    fn the_trigger_examples_name_every_trigger() {
        // The match is exhaustive, so a new trigger fails to compile here.
        // Its author must also add it to `KNOWN_TRIGGERS`.
        let position = |trigger: &BalanceTrigger| match trigger {
            BalanceTrigger::Trade => Some(0),
            BalanceTrigger::Deposit => Some(1),
            BalanceTrigger::Withdrawal => Some(2),
            BalanceTrigger::FailedTx => Some(3),
            BalanceTrigger::Snapshot => None,
        };
        for (index, trigger) in KNOWN_TRIGGERS.iter().enumerate() {
            assert_eq!(position(trigger), Some(index), "{trigger}");
        }

        let schema = serde_json::to_value(open_trigger()).expect("serializes");
        assert!(schema.get("enum").is_none(), "the trigger publishes a closed value list");
        assert_eq!(schema["examples"][3], "failed_tx");
    }

    #[test]
    fn the_asset_schema_stays_open_and_lists_the_known_assets() {
        let schema = serde_json::to_value(open_asset()).expect("serializes");

        assert!(schema.get("enum").is_none(), "the asset publishes a closed value list");
        let examples = schema["examples"].as_array().expect("the known assets");
        assert!(examples.iter().any(|asset| asset == "SOL"), "examples are the wire form");
    }

    #[test]
    fn a_reader_drops_a_row_it_cannot_name() {
        let known = serde_json::to_value(balance(DepositAsset::Sol)).expect("serializes");
        let mut newer = known.clone();
        newer["asset"] = "PYUSD".into();

        let read: UiWalletBalances =
            serde_json::from_value(serde_json::json!({ "balances": [known, newer] }))
                .expect("reads the known row");
        assert_eq!(read.balances.len(), 1);
    }
}
