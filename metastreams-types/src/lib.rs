//! The Metastreams wire objects.
//!
//! The API server serves the responses and reads the requests; `metastreams-sdk` reads the
//! responses and writes the requests.
//! <https://docs.metastreams.io> documents the contract they implement.

pub mod address;
pub mod balance;
pub mod candle;
pub mod chain;
mod csv;
pub mod decimal;
pub mod discovery;
pub mod error;
pub mod filter;
pub mod money;
pub mod position;
pub mod schema;
pub mod search;
pub mod session;
pub mod sort;
pub mod stream;
pub mod time;
pub mod token;
pub mod trade;
pub mod transfer;
