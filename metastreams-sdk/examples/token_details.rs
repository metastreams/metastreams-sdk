//! Reads one token and prints its name, symbol and pools.
//!
//! `METASTREAMS_API_KEY=... cargo run -p metastreams-sdk --example token_details [address]`

use metastreams_sdk::{MetastreamsProvider, MultiChainAddress, SpotChain};

const WSOL: &str = "So11111111111111111111111111111111111111112";

#[tokio::main]
async fn main() -> metastreams_sdk::Result<()> {
    let key = std::env::var("METASTREAMS_API_KEY").expect("METASTREAMS_API_KEY is set");
    let address: MultiChainAddress = std::env::args()
        .nth(1)
        .unwrap_or_else(|| WSOL.to_owned())
        .parse()
        .expect("a Solana or EVM address");

    let provider = MetastreamsProvider::new(key)?;
    let token = provider.token_details(SpotChain::Solana, &address).await?;

    println!("{} ({}) on {}", token.name, token.symbol, token.chain);
    println!("supply {} across {} pools", token.supply, token.pools.len());
    for pool in &token.pools {
        println!("  {pool:?}");
    }
    Ok(())
}
