//! Typed client for the Metastreams market-data API; see [`MetastreamsProvider`].
//! [`MetastreamsProvider::connect`] opens the `/v1/stream` socket.

mod constants;
mod error;
mod provider;
mod stream;

pub use metastreams_types as types;
pub use metastreams_types::address::MultiChainAddress;
pub use metastreams_types::candle::{CandleDenomination, CandleMetric, Timeframe};
pub use metastreams_types::chain::SpotChain;
pub use metastreams_types::discovery::SpotSortField;
pub use metastreams_types::discovery::filters::SpotTokenFilters;
pub use metastreams_types::sort::SortOrder;
pub use metastreams_types::time::UnixMilliseconds;

pub use constants::BASE_URL;
pub use error::{MetastreamsError, Result};
pub use provider::MetastreamsProvider;
pub use stream::{
    CandleOptions, CloseReason, Event, LostCause, StreamConnection, SubscriptionStream,
};
