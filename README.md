# metastreams-sdk

The Rust client for the [Metastreams](https://docs.metastreams.io) market-data API. Two crates:

| Crate | What it is |
|-------|------------|
| `metastreams-types` | Every request and response of the API, as serde types. The server and this SDK share them, so the two cannot drift. |
| `metastreams-sdk` | `MetastreamsProvider`, one method per `/v1` operation, and `StreamConnection`, typed subscriptions over the `/v1/stream` socket. |

A consumer that only needs the wire types takes `metastreams-types` alone and pulls in no HTTP or WebSocket stack.

## Install

The crates are not on crates.io yet. Take them as a git dependency pinned to a tag:

```toml
[dependencies]
metastreams-sdk = { git = "https://github.com/tradeonapex/metastreams-sdk", tag = "v0.1.0" }
tokio           = { version = "1", features = ["rt-multi-thread", "macros"] }
```

Pin a `rev` instead of a `tag` to track a commit. The minimum supported Rust version is 1.93.

## Read

Every call sends the key as a bearer token and never retries. Get a key at <https://docs.metastreams.io/get-started/api-keys>.

```rust
use metastreams_sdk::{MetastreamsProvider, MultiChainAddress, SpotChain};

#[tokio::main]
async fn main() -> metastreams_sdk::Result<()> {
    let provider = MetastreamsProvider::new(std::env::var("METASTREAMS_API_KEY").unwrap())?;
    let address: MultiChainAddress = "So11111111111111111111111111111111111111112".parse().unwrap();
    let token = provider.token_details(SpotChain::Solana, &address).await?;
    println!("{} ({}) has {} pools", token.name, token.symbol, token.pools.len());
    Ok(())
}
```

## Stream

One socket carries many subscriptions. Each subscription is a `futures::Stream` of `Event`s: an `Update` that matches the filter, or an `Exit` when something the subscription matched leaves it. The stream sends no snapshot, so read the REST view after the subscribe returns.

```rust
use futures::StreamExt;
use metastreams_sdk::types::filter::TradeFilter;
use metastreams_sdk::{Event, MetastreamsProvider, SpotChain};

#[tokio::main]
async fn main() -> metastreams_sdk::Result<()> {
    let provider = MetastreamsProvider::new(std::env::var("METASTREAMS_API_KEY").unwrap())?;
    let connection = provider.connect().await?;
    let filter = TradeFilter { chains: vec![SpotChain::Solana], ..Default::default() };
    let mut trades = connection.subscribe(&filter).await?;
    while let Some(event) = trades.next().await {
        match event? {
            Event::Update(trade) => println!("{:?} {} {}", trade.side, trade.amount, trade.symbol),
            Event::Exit(exit) => match exit {},
        }
    }
    Ok(())
}
```

The SDK never reconnects. The server keeps no subscription across sockets and replays nothing, so a reconnect always leaves a gap that only the consumer can fill, from REST. `MetastreamsError::is_transient` says whether the same call can succeed later.

`metastreams-sdk/examples/` holds both programs. Run one with:

```sh
METASTREAMS_API_KEY=... cargo run -p metastreams-sdk --example token_details
```

## Errors

Every call returns `MetastreamsError`. A refused request carries the API's error envelope as `Api { status, error, .. }`; switch on `error.code`, never on its message. A 429 is `RateLimited` with the `Retry-After` wait. The codes are documented at <https://docs.metastreams.io/concepts/errors>.

## Docs

- API reference and guides: <https://docs.metastreams.io>
- Stream protocol and filters: <https://docs.metastreams.io/streams/overview>
- `cargo doc --open` for the crate docs

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
