//! [`UiTokenTradeFees`], what a trade on the token costs and where its tax goes.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::decimal::Decimal;

/// What a trade at this token's venue costs, and where its tax goes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenTradeFees {
    /// What a buy costs in total, in basis points — every layer below, summed.
    #[schema(value_type = Option<u16>, example = 25)]
    pub total_buy_bps: Option<u16>,
    /// What a sell costs in total, in basis points; see `totalBuyBps`.
    #[schema(value_type = Option<u16>, example = 125)]
    pub total_sell_bps: Option<u16>,
    /// The layers the totals are made of. A layer nobody states is absent, never zero.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breakdown: Vec<UiTokenFeeComponent>,
    /// Where the tax goes. Absent where the venue states nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocation: Option<UiTokenFeeAllocation>,
    /// What a wallet holds to qualify for a payout. Absent at a venue gating on nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rewards: Option<UiTokenFeeRewards>,
}

/// One layer of a total: who charges it, and what each side pays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenFeeComponent {
    /// Which layer this is.
    pub kind: UiTokenFeeKind,
    /// Taken from a buy, in basis points. A layer that varies states its floor here.
    #[schema(value_type = Option<u16>, example = 0)]
    pub buy_bps: Option<u16>,
    /// Taken from a sell, in basis points. A layer that varies states its floor here.
    #[schema(value_type = Option<u16>, example = 100)]
    pub sell_bps: Option<u16>,
    /// The most a varying buy layer takes. A fixed layer leaves this unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<u16>, example = 1000)]
    pub buy_bps_max: Option<u16>,
    /// The most a varying sell layer takes. A fixed layer leaves this unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<u16>, example = 1000)]
    pub sell_bps_max: Option<u16>,
    /// The asset the layer is charged in, where it is not the traded token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<UiFeeAsset>,
}

/// The layers a venue can charge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UiTokenFeeKind {
    /// The venue's own cut, charged on every launch it runs.
    ProtocolFee,
    /// The token contract's own cut, set by whoever launched it.
    CreatorTax,
    /// The fee the paired asset charges, on a market quoted in another token.
    QuoteTokenFee,
    /// The pool's fee, on a token trading at an AMM.
    PoolFee,
    /// A hook's own cut, taken beside the pool's fee rather than as part of it.
    HookFee,
    /// A launch-time cut that decays, charged to price out snipers.
    SnipeTax,
    /// The mint's own cut, taken on every transfer rather than on a trade.
    TransferFee,
    /// The launchpad's fee to the coin's creator on each trade.
    CreatorFee,
    /// The launchpad's fee to the coin's holders on each trade.
    HolderFee,
    /// A layer this build does not name. Read-only: the server never sends it.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// Where a token's tax goes, and to whom.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenFeeAllocation {
    /// What the recipients are.
    pub kind: UiTokenFeeAllocationKind,
    /// Who is paid, and what share each takes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recipients: Vec<UiTokenFeeRecipient>,
}

/// What a venue does with the tax it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UiTokenFeeAllocationKind {
    /// Paid back to the token's own holders.
    HoldersDividend,
    /// Paid to whoever launched the token.
    Creator,
    /// Paid to an address the launch named.
    CustomWallet,
    /// Paid to those who stake the token.
    Staking,
    /// Paid to one holder, drawn at random.
    Lottery,
    /// Spent buying the token back.
    Buyback,
    /// Kept by the venue.
    Protocol,
    /// Taken and paid to nobody, as the venue states it.
    None,
    /// A destination this build does not name. Read-only: the server never sends it.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// One party a token's tax is paid to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenFeeRecipient {
    /// The address paid. Absent where the venue names a payout without naming who holds it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// This recipient's share of the tax, in basis points; `10000` is all of it.
    #[schema(value_type = Option<u16>, example = 10000)]
    pub share_bps: Option<u16>,
    /// The asset this recipient is paid in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payout_asset: Option<UiFeeAsset>,
}

/// An asset a fee is charged or paid in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiFeeAsset {
    /// The asset's contract address.
    pub address: String,
    /// Its display symbol; `null` where the registry names none.
    pub symbol: Option<String>,
}

/// What a wallet holds to qualify for a payout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenFeeRewards {
    /// The qualifying balance, in whole tokens of the taxed token.
    #[schema(value_type = Decimal, examples("10000"))]
    pub min_holding: Decimal,
    /// That balance at the token's current price. `null` until the token prices.
    #[schema(value_type = Option<Decimal>, examples("315"))]
    pub min_holding_usd: Option<Decimal>,
}
