//! The native price object: one chain's gas asset priced in USD.

use serde::{Deserialize, Serialize};
use utoipa::openapi::schema::Object;
use utoipa::{IntoParams, ToSchema};

use crate::balance::DepositAsset;
use crate::chain::SpotChain;
use crate::money::UiMoney;
use crate::schema::open_enumeration;
use crate::time::UnixMilliseconds;

/// Every gas asset the surface prices, which the document lists as examples.
const KNOWN_ASSETS: [DepositAsset; 3] = [DepositAsset::Sol, DepositAsset::Eth, DepositAsset::Bnb];

/// Query for `GET /v1/native-prices`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativePricesQuery {
    /// Chains to scope to, CSV of the chain slugs. Absent means every spot chain.
    #[serde(default, with = "crate::csv::chains", skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>, example = "solana,base")]
    pub chains: Option<Vec<SpotChain>>,
}

/// One chain's gas asset, priced in USD.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiNativePrice {
    /// Chain whose gas asset this prices.
    pub chain: SpotChain,
    /// The chain's gas asset.
    #[schema(schema_with = open_asset)]
    pub asset: DepositAsset,
    /// One whole unit of `asset`; `native` is always 1.
    pub price: UiMoney,
    /// The instant the price source reports the rate was true.
    pub observed_at: UnixMilliseconds,
}

/// The latest price of each chain's gas asset.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiNativePrices {
    /// One row per chain with a known price, in `SpotChain` order.
    pub native_prices: Vec<UiNativePrice>,
}

/// The published `asset` schema: an open string, since a new chain can bring a new gas asset.
fn open_asset() -> Object {
    open_enumeration(
        "The chain's gas asset. This is an open enumeration: keep a value you do not recognize.",
        KNOWN_ASSETS.iter().map(|asset| asset.to_string().into()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_asset_schema_stays_open_and_lists_the_priced_gas_assets() {
        let schema = serde_json::to_value(open_asset()).expect("serializes");

        assert!(schema.get("enum").is_none(), "the asset publishes a closed value list");
        assert_eq!(schema["examples"], serde_json::json!(["SOL", "ETH", "BNB"]));
    }
}
