//! Live checks of every REST operation and every stream channel against a real API.
//!
//! They need a key, so they are ignored by default. Run them with
//! `METASTREAMS_API_KEY=... cargo test -p metastreams-sdk --test live -- --ignored`.
//! `METASTREAMS_BASE_URL` points them at another deployment.

use std::time::Duration;

use futures::StreamExt;
use tokio::time::timeout;

use metastreams_sdk::types::address::MultiChainAddress;
use metastreams_sdk::types::balance::WalletBalancesQuery;
use metastreams_sdk::types::candle::{CandleSeriesQuery, Timeframe};
use metastreams_sdk::types::chain::SpotChain;
use metastreams_sdk::types::discovery::{SpotPulseColumnConfig, SpotPulseSubscriptionConfig};
use metastreams_sdk::types::error::ErrorCode;
use metastreams_sdk::types::filter::{CandleFilter, TokenFilter, TradeFilter, TransferFilter};
use metastreams_sdk::types::native_price::NativePricesQuery;
use metastreams_sdk::types::position::PositionsQuery;
use metastreams_sdk::types::session::CreateSessionTokenRequest;
use metastreams_sdk::types::stream::{
    NativePricesFilter, SpotHoldersFilter, SpotPositionsFilter, Subscription, WalletBalancesFilter,
};
use metastreams_sdk::types::thesis::TokenThesesQuery;
use metastreams_sdk::types::token::TokensQuery;
use metastreams_sdk::types::token::holders::HolderSeriesQuery;
use metastreams_sdk::types::trade::{IdentityFeed, TradesQuery};
use metastreams_sdk::types::trending::SpotTrendingFilter;
use metastreams_sdk::{
    CandleOptions, Event, MetastreamsError, MetastreamsProvider, StreamConnection,
    SubscriptionStream, TrendingWindow,
};

/// How long a busy channel may stay silent before the check fails.
const FIRST_UPDATE: Duration = Duration::from_secs(30);

/// A token and a wallet that trades it, both live.
struct Sample {
    token: MultiChainAddress,
    wallet: MultiChainAddress,
}

fn provider() -> MetastreamsProvider {
    let key = std::env::var("METASTREAMS_API_KEY").expect("METASTREAMS_API_KEY is set");
    match std::env::var("METASTREAMS_BASE_URL") {
        Ok(base) => MetastreamsProvider::new_with_base_url(key, &base).expect("provider"),
        Err(_) => MetastreamsProvider::new(key).expect("provider"),
    }
}

/// The top Solana trending token, and the trader of its latest trade.
async fn sample(provider: &MetastreamsProvider) -> Sample {
    let request = SpotTrendingFilter {
        chains: vec![SpotChain::Solana],
        window: TrendingWindow::OneHour,
        limit: Some(5),
        filters: None,
    };
    let board = provider.trending_board(&request).await.expect("trending board");
    let top = board.first().expect("a trending token");
    let token: MultiChainAddress = top.address.to_string().parse().expect("a token address");
    let query = TradesQuery {
        tokens: Some(vec![token.clone()]),
        chains: Some(vec![SpotChain::Solana]),
        limit: Some(1),
        ..TradesQuery::default()
    };
    let trades = provider.trades(&query).await.expect("token trades");
    let trade = trades.trades.first().expect("a trade on the top trending token");
    let wallet = trade.trader.parse().expect("a trader address");
    Sample { token, wallet }
}

/// The first update a subscription sends, decoded.
async fn first_update<F: Subscription>(mut stream: SubscriptionStream<F>) -> F::Update
where
    F::Update: std::fmt::Debug,
    F::Exit: std::fmt::Debug,
{
    loop {
        let item = timeout(FIRST_UPDATE, stream.next())
            .await
            .unwrap_or_else(|_| panic!("{} sent nothing in {FIRST_UPDATE:?}", F::CHANNEL))
            .unwrap_or_else(|| panic!("{} ended", F::CHANNEL));
        match item.unwrap_or_else(|error| panic!("{}: {error}", F::CHANNEL)) {
            Event::Update(update) => return update,
            Event::Exit(_) => continue,
        }
    }
}

/// Reads for `quiet`, failing on any frame the SDK cannot decode.
async fn no_unreadable_frame<F: Subscription>(mut stream: SubscriptionStream<F>, quiet: Duration) {
    let _ = timeout(quiet, async {
        while let Some(item) = stream.next().await {
            if let Err(error @ MetastreamsError::FrameDecode { .. }) = item {
                panic!("{}: {error}", F::CHANNEL);
            }
        }
    })
    .await;
}

async fn connect(provider: &MetastreamsProvider) -> StreamConnection {
    provider.connect().await.expect("the stream connects")
}

#[tokio::test]
#[ignore = "needs METASTREAMS_API_KEY"]
async fn every_token_read_answers() {
    let provider = provider();
    let Sample { token, .. } = sample(&provider).await;
    let chain = SpotChain::Solana;

    let solana = Some(vec![chain]);

    let query = TokensQuery { tokens: vec![token.clone()], chains: solana.clone() };
    let tokens = provider.tokens(&query).await.expect("tokens");
    assert_eq!(tokens.len(), 1, "one token per chain in scope");
    assert_eq!(tokens[0].card.address.to_string(), token.to_string());

    let query = CandleSeriesQuery {
        tokens: vec![token.clone()],
        chains: solana.clone(),
        timeframe: Some(Timeframe::OneMin),
        metric: None,
        denomination: None,
        from: None,
        to: None,
        count_back: None,
    };
    let series = provider.ohlcv(&query).await.expect("ohlcv");
    assert!(series.iter().any(|series| !series.candles.is_empty()), "no candles");

    let query = TradesQuery {
        tokens: Some(vec![token.clone()]),
        chains: solana.clone(),
        limit: Some(5),
        ..TradesQuery::default()
    };
    let trades = provider.trades(&query).await.expect("trades");
    assert!(!trades.trades.is_empty(), "no trades");

    let query = HolderSeriesQuery {
        tokens: vec![token.clone()],
        chains: solana,
        limit: Some(5),
        flags: None,
        period: None,
    };
    let holders = provider.holders(&query).await.expect("holders");
    assert!(holders.iter().any(|series| !series.holders.is_empty()), "no holders");

    // Theses are enabled per organization; for a key without the feature, the route is not found.
    let query = TokenThesesQuery { limit: Some(5), ..TokenThesesQuery::default() };
    match provider.token_theses(chain, &token, &query).await {
        Ok(_) => {}
        Err(MetastreamsError::Api { error, .. }) if error.code == ErrorCode::NotFound => {
            eprintln!("theses are not enabled for this key's organization; skipped");
        }
        Err(error) => panic!("theses: {error}"),
    }
}

#[tokio::test]
#[ignore = "needs METASTREAMS_API_KEY"]
async fn every_wallet_and_board_read_answers() {
    let provider = provider();
    let Sample { wallet, .. } = sample(&provider).await;

    for identity in [IdentityFeed::Kol, IdentityFeed::Fomo] {
        let query =
            TradesQuery { identity: Some(identity), limit: Some(5), ..TradesQuery::default() };
        provider.trades(&query).await.expect("identity trades");
    }

    let query = WalletBalancesQuery { wallets: vec![wallet.clone()], chains: None };
    provider.wallet_balances(&query).await.expect("balances");

    let query = PositionsQuery::wallets(vec![wallet]);
    provider.positions(&query).await.expect("positions");

    let prices = provider.native_prices(&NativePricesQuery::default()).await.expect("prices");
    assert!(!prices.is_empty(), "no native prices");

    let column = Some(SpotPulseColumnConfig { limit: Some(5), ..Default::default() });
    let request = SpotPulseSubscriptionConfig {
        chains: vec![SpotChain::Solana],
        new_pairs: column.clone(),
        graduating: column.clone(),
        graduated: column,
    };
    let board = provider.discovery_board(&request).await.expect("discovery");
    assert!(!board.is_empty(), "no discovery columns");

    for window in [TrendingWindow::OneMinute, TrendingWindow::OneHour] {
        let request = SpotTrendingFilter {
            chains: vec![SpotChain::Solana],
            window,
            limit: Some(5),
            filters: None,
        };
        let board = provider.trending_board(&request).await.expect("trending");
        assert!(!board.is_empty(), "an empty {window:?} board");
    }

    let request = CreateSessionTokenRequest {
        origins: vec!["https://qa.example.com".to_owned()],
        ttl_seconds: Some(60),
        ..CreateSessionTokenRequest::default()
    };
    let minted = provider.session_token_create(&request).await.expect("a session token");
    assert!(!minted.token.is_empty());
}

#[tokio::test]
#[ignore = "needs METASTREAMS_API_KEY"]
async fn every_busy_channel_sends_a_readable_update() {
    let provider = provider();
    let Sample { token, .. } = sample(&provider).await;
    let stream = connect(&provider).await;
    let solana = vec![SpotChain::Solana];

    let trades = TradeFilter { chains: solana.clone(), ..TradeFilter::default() };
    first_update(stream.subscribe(&trades).await.expect("spot.trades")).await;

    let transfers = TransferFilter { chains: solana.clone(), ..TransferFilter::default() };
    first_update(stream.subscribe(&transfers).await.expect("spot.transfers")).await;

    let tokens = TokenFilter { tokens: Some(vec![token.clone()]), ..TokenFilter::default() };
    first_update(stream.subscribe(&tokens).await.expect("spot.tokens")).await;

    let candles = CandleFilter {
        tokens: Some(vec![token.clone()]),
        timeframes: vec![Timeframe::OneSec],
        ..CandleFilter::default()
    };
    let opened = stream.subscribe_candles(&candles, CandleOptions::default()).await;
    first_update(opened.expect("spot.candles")).await;

    let holders = SpotHoldersFilter { chain: SpotChain::Solana, token, flags: vec![] };
    first_update(stream.subscribe(&holders).await.expect("spot.holders")).await;

    let prices = NativePricesFilter::default();
    first_update(stream.subscribe(&prices).await.expect("chain.nativePrices")).await;

    let trending = SpotTrendingFilter {
        chains: solana.clone(),
        window: TrendingWindow::OneHour,
        limit: Some(5),
        filters: None,
    };
    first_update(stream.subscribe(&trending).await.expect("spot.trending")).await;

    let column = Some(SpotPulseColumnConfig { limit: Some(5), ..Default::default() });
    let discovery = SpotPulseSubscriptionConfig {
        chains: solana,
        new_pairs: column,
        ..SpotPulseSubscriptionConfig::default()
    };
    first_update(stream.subscribe(&discovery).await.expect("spot.discovery")).await;

    stream.close().await;
}

#[tokio::test]
#[ignore = "needs METASTREAMS_API_KEY"]
async fn every_wallet_channel_subscribes_and_reads_cleanly() {
    let provider = provider();
    let Sample { wallet, .. } = sample(&provider).await;
    let stream = connect(&provider).await;
    let quiet = Duration::from_secs(10);

    let balances = WalletBalancesFilter { chain: SpotChain::Solana, wallets: vec![wallet.clone()] };
    let opened = stream.subscribe(&balances).await.expect("wallet.balances");
    no_unreadable_frame(opened, quiet).await;

    let positions = SpotPositionsFilter { chain: SpotChain::Solana, wallets: vec![wallet] };
    let opened = stream.subscribe(&positions).await.expect("spot.positions");
    no_unreadable_frame(opened, quiet).await;

    stream.close().await;
}
