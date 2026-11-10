//! Reads one token and prints its name, symbol and pools.
//!
//! `METASTREAMS_API_KEY=... cargo run -p metastreams-sdk --example token_details [address]`

use metastreams_sdk::{MetastreamsProvider, MultiChainAddress, SpotChain};

const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

#[tokio::main]
async fn main() -> metastreams_sdk::Result<()> {
    let key = std::env::var("METASTREAMS_API_KEY").expect("METASTREAMS_API_KEY is set");
    let address: MultiChainAddress = std::env::args()
        .nth(1)
        .unwrap_or_else(|| USDC.to_owned())
        .parse()
        .expect("a Solana or EVM address");

    let provider = MetastreamsProvider::new(key)?;
    let token = provider.token_details(SpotChain::Solana, &address).await?;

    let card = &token.card;
    println!("{} ({}) on {}", card.name, card.symbol, card.chain);
    println!("supply {} across {} pools", token.supply, card.pools.len());
    for pool in &card.pools {
        println!("  {pool:?}");
    }
    Ok(())
}
