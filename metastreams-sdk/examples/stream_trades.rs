//! Subscribes to every Solana trade and prints each one until the socket closes.
//!
//! `METASTREAMS_API_KEY=... cargo run -p metastreams-sdk --example stream_trades`

use futures::StreamExt;
use metastreams_sdk::types::filter::TradeFilter;
use metastreams_sdk::{Event, MetastreamsProvider, SpotChain};

#[tokio::main]
async fn main() -> metastreams_sdk::Result<()> {
    let key = std::env::var("METASTREAMS_API_KEY").expect("METASTREAMS_API_KEY is set");
    let provider = MetastreamsProvider::new(key)?;
    let connection = provider.connect().await?;

    let filter = TradeFilter { chains: vec![SpotChain::Solana], ..Default::default() };
    let mut trades = connection.subscribe(&filter).await?;
    println!("subscribed as {:?}", trades.id());

    while let Some(event) = trades.next().await {
        match event? {
            Event::Update(trade) => println!(
                "{:?} {} {} at {} by {}",
                trade.side, trade.amount, trade.symbol, trade.price.native, trade.trader
            ),
            Event::Exit(exit) => match exit {},
        }
    }
    Ok(())
}
