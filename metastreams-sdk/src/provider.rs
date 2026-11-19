//! [`MetastreamsProvider`]: one method per `/v1` operation, and the `/v1/stream` socket; each sends the bearer key.

use std::fmt;
use std::io;
use std::sync::Arc;

use reqwest::{Client, RequestBuilder, StatusCode};
use rustls::crypto::aws_lc_rs;
use rustls::{ClientConfig, RootCertStore};
use serde::de::DeserializeOwned;
use strum::IntoEnumIterator;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::{Connector, connect_async_tls_with_config, tungstenite};
use tracing::instrument;
use url::Url;

use metastreams_types::address::MultiChainAddress;
use metastreams_types::balance::{UiWalletBalances, WalletBalancesQuery};
use metastreams_types::candle::{CandleSeriesQuery, UiCandleSeries};
use metastreams_types::chain::SpotChain;
use metastreams_types::discovery::{SpotPulseSubscriptionConfig, UiSpotPulseUpdate};
use metastreams_types::native_price::{NativePricesQuery, UiNativePrice};
use metastreams_types::portfolio::{PortfolioQuery, UiPortfolioResponse};
use metastreams_types::position::{PositionsQuery, UiSpotPositions};
use metastreams_types::session::{CreateSessionTokenRequest, UiSessionToken};
use metastreams_types::thesis::{TokenThesesQuery, UiTokenTheses};
use metastreams_types::token::holders::{HolderSeriesQuery, UiHolderSeries};
use metastreams_types::token::{TokensQuery, UiSpotToken};
use metastreams_types::trade::{TradesQuery, UiSpotTrades};
use metastreams_types::trending::{SpotTrendingFilter, UiSpotTrendingDelta};

use crate::constants::{BASE_URL, CONNECT_TIMEOUT, REQUEST_TIMEOUT};
use crate::error::{MetastreamsError, Result, request_id};
use crate::stream::StreamConnection;

/// Typed client for the Metastreams API. Each call sends one request and never retries.
#[derive(Clone)]
pub struct MetastreamsProvider {
    client: Client,
    /// The stream socket's TLS setup.
    tls: Arc<ClientConfig>,
    base: Url,
    api_key: String,
}

impl MetastreamsProvider {
    /// Provider against the production API.
    ///
    /// # Errors
    /// Fails when the HTTP client cannot start its TLS backend.
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        Self::new_with_base_url(api_key, BASE_URL)
    }

    /// Provider against another origin, such as staging or a test server; paths append under its path.
    ///
    /// # Errors
    /// Fails when `base_url` is not an `http` or `https` URL, or the HTTP client cannot start.
    pub fn new_with_base_url(api_key: impl Into<String>, base_url: &str) -> Result<Self> {
        let base = Url::parse(base_url)?;
        if !matches!(base.scheme(), "http" | "https") {
            return Err(MetastreamsError::UnsupportedScheme(base.scheme().to_owned()));
        }
        let client =
            Client::builder().timeout(REQUEST_TIMEOUT).connect_timeout(CONNECT_TIMEOUT).build()?;
        Ok(Self { client, tls: tls_config(), base, api_key: api_key.into() })
    }

    /// Opens one `/v1/stream` socket; each [`StreamConnection::subscribe`] shares it.
    ///
    /// # Errors
    /// A refused handshake reads as a refused REST call. A bad key is `Api` with `UNAUTHORIZED`;
    /// a key at its socket limit is `RateLimited`.
    #[instrument(skip(self))]
    pub async fn connect(&self) -> Result<StreamConnection> {
        let mut url = self.url("v1/stream");
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        url.set_scheme(scheme).expect("http(s) and ws(s) are interchangeable special schemes");

        let mut request = url.as_str().into_client_request()?;
        let mut bearer = HeaderValue::from_str(&format!("Bearer {}", self.api_key))
            .map_err(|_| MetastreamsError::InvalidKey)?;
        bearer.set_sensitive(true);
        request.headers_mut().insert(AUTHORIZATION, bearer);

        let connector = Connector::Rustls(Arc::clone(&self.tls));
        let handshake = connect_async_tls_with_config(request, None, true, Some(connector));
        let (socket, _) = timeout(CONNECT_TIMEOUT, handshake).await.map_err(|_| {
            tungstenite::Error::Io(io::Error::new(io::ErrorKind::TimedOut, "stream handshake"))
        })??;
        Ok(StreamConnection::spawn(socket))
    }

    /// Each named token on each chain in scope, `GET /v1/tokens`.
    #[instrument(skip(self))]
    pub async fn tokens(&self, query: &TokensQuery) -> Result<Vec<UiSpotToken>> {
        let query = TokensQuery { chains: known_chains(&query.chains), ..query.clone() };
        self.send(self.client.get(self.url("v1/tokens")).query(&query)).await
    }

    /// Each named token's candles, oldest first, `GET /v1/ohlcv`.
    #[instrument(skip(self))]
    pub async fn ohlcv(&self, query: &CandleSeriesQuery) -> Result<Vec<UiCandleSeries>> {
        let query = CandleSeriesQuery { chains: known_chains(&query.chains), ..query.clone() };
        self.send(self.client.get(self.url("v1/ohlcv")).query(&query)).await
    }

    /// One page of trades, newest first: the named tokens', wallets' or identity's, `GET /v1/trades`.
    #[instrument(skip(self))]
    pub async fn trades(&self, query: &TradesQuery) -> Result<UiSpotTrades> {
        let query = TradesQuery { chains: known_chains(&query.chains), ..query.clone() };
        self.send(self.client.get(self.url("v1/trades")).query(&query)).await
    }

    /// Each named token's current holders, largest balance first, `GET /v1/holders`.
    #[instrument(skip(self))]
    pub async fn holders(&self, query: &HolderSeriesQuery) -> Result<Vec<UiHolderSeries>> {
        let query = HolderSeriesQuery { chains: known_chains(&query.chains), ..query.clone() };
        self.send(self.client.get(self.url("v1/holders")).query(&query)).await
    }

    /// One page of a token's theses, `GET /v1/tokens/{chain}/{address}/theses`.
    #[instrument(skip(self))]
    pub async fn token_theses(
        &self,
        chain: SpotChain,
        address: &MultiChainAddress,
        query: &TokenThesesQuery,
    ) -> Result<UiTokenTheses> {
        let url = self.url(&format!("v1/tokens/{chain}/{address}/theses"));
        self.send(self.client.get(url).query(query)).await
    }

    /// The latest balance of each named wallet, per chain and asset, `GET /v1/balances`.
    #[instrument(skip(self))]
    pub async fn wallet_balances(&self, query: &WalletBalancesQuery) -> Result<UiWalletBalances> {
        let query = WalletBalancesQuery { chains: known_chains(&query.chains), ..query.clone() };
        self.send(self.client.get(self.url("v1/balances")).query(&query)).await
    }

    /// The summed spot holdings of the named wallets, `GET /v1/portfolio`.
    #[instrument(skip(self))]
    pub async fn wallet_portfolio(&self, query: &PortfolioQuery) -> Result<UiPortfolioResponse> {
        let query = PortfolioQuery { chains: known_chains(&query.chains), ..query.clone() };
        self.send(self.client.get(self.url("v1/portfolio")).query(&query)).await
    }

    /// The newest USD price of each chain's gas asset, `GET /v1/native-prices`.
    #[instrument(skip(self))]
    pub async fn native_prices(&self, query: &NativePricesQuery) -> Result<Vec<UiNativePrice>> {
        let query = NativePricesQuery { chains: known_chains(&query.chains) };
        self.send(self.client.get(self.url("v1/native-prices")).query(&query)).await
    }

    /// One page of the named wallets' positions, `GET /v1/positions`.
    #[instrument(skip(self))]
    pub async fn positions(&self, query: &PositionsQuery) -> Result<UiSpotPositions> {
        let query = PositionsQuery { chains: known_chains(&query.chains), ..query.clone() };
        self.send(self.client.get(self.url("v1/positions")).query(&query)).await
    }

    /// The requested discovery columns, each ranked over every requested chain,
    /// `POST /v1/discovery`.
    #[instrument(skip(self))]
    pub async fn discovery_board(
        &self,
        request: &SpotPulseSubscriptionConfig,
    ) -> Result<Vec<UiSpotPulseUpdate>> {
        self.send(self.client.post(self.url("v1/discovery")).json(request)).await
    }

    /// The tokens ranked by trending score over every requested chain, `POST /v1/trending`.
    #[instrument(skip(self))]
    pub async fn trending_board(
        &self,
        request: &SpotTrendingFilter,
    ) -> Result<Vec<UiSpotTrendingDelta>> {
        self.send(self.client.post(self.url("v1/trending")).json(request)).await
    }

    /// A session token for one browser or app user, `POST /v1/session-tokens`.
    ///
    /// A deployment that seals no tokens answers `Api` with `NOT_FOUND`. A session token
    /// presented as this provider's key answers `Api` with `FORBIDDEN`.
    #[instrument(skip(self))]
    pub async fn session_token_create(
        &self,
        request: &CreateSessionTokenRequest,
    ) -> Result<UiSessionToken> {
        self.send(self.client.post(self.url("v1/session-tokens")).json(request)).await
    }

    /// Joins `path` below any path the base URL carries.
    fn url(&self, path: &str) -> Url {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .expect("an http(s) URL carries a path")
            .pop_if_empty()
            .extend(path.split('/'));
        url
    }

    /// Sends `request` with the key. Reads the body as `T`, or the failure as a [`MetastreamsError`].
    async fn send<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T> {
        let response = request.bearer_auth(&self.api_key).send().await?;
        let status = response.status();
        // A 429 reads from its headers alone, so a failed body read cannot lose its `Retry-After`.
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(MetastreamsError::from_response(status, response.headers(), &[]));
        }
        let headers = response.headers().clone();
        let body = response.bytes().await?;

        if !status.is_success() {
            return Err(MetastreamsError::from_response(status, &headers, &body));
        }
        serde_json::from_slice(&body).map_err(|source| MetastreamsError::Decode {
            status,
            source,
            request_id: request_id(&headers),
        })
    }
}

impl fmt::Debug for MetastreamsProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetastreamsProvider")
            .field("base", &self.base.as_str())
            .field("api_key", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// The scope a query sends: its own, else every chain this build knows.
/// The server's default scope can hold a chain this build cannot decode.
fn known_chains(chains: &Option<Vec<SpotChain>>) -> Option<Vec<SpotChain>> {
    Some(chains.clone().unwrap_or_else(|| SpotChain::iter().collect()))
}

/// TLS on the public web roots with its own crypto provider, not a process-wide one.
fn tls_config() -> Arc<ClientConfig> {
    let roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    let config = ClientConfig::builder_with_provider(Arc::new(aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("aws-lc-rs supports the default TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use metastreams_types::candle::Timeframe;
    use metastreams_types::discovery::SpotPulseColumnConfig;
    use metastreams_types::error::ErrorCode;
    use metastreams_types::portfolio::PortfolioInclude;
    use metastreams_types::position::{Lifecycle, PositionSort, PositionSortKey};
    use metastreams_types::sort::SortOrder;
    use metastreams_types::thesis::{ThesisSortKey, ThesisSource, ThesisSourceFilter};
    use metastreams_types::token::holders::HolderPeriod;
    use metastreams_types::trade::{IdentityFeed, TradeDirection, WalletFilter};
    use metastreams_types::trending::TrendingWindow;

    const KEY: &str = "meta_test";
    const MINT: &str = "So11111111111111111111111111111111111111112";

    fn mint() -> MultiChainAddress {
        MINT.parse().expect("a valid mint")
    }

    fn kol() -> TradesQuery {
        TradesQuery { identity: Some(IdentityFeed::Kol), ..TradesQuery::default() }
    }

    fn empty_page() -> serde_json::Value {
        json!({ "trades": [], "nextCursor": null, "hasMore": false })
    }

    fn provider(server: &MockServer) -> MetastreamsProvider {
        MetastreamsProvider::new_with_base_url(KEY, &server.uri()).expect("provider")
    }

    #[tokio::test]
    async fn a_token_trades_read_sends_the_key_its_tokens_and_its_flags() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/trades"))
            .and(header("authorization", format!("Bearer {KEY}")))
            .and(query_param("tokens", MINT))
            .and(query_param("limit", "50"))
            .and(query_param("flags", "bundler,pro_trader"))
            .respond_with(ResponseTemplate::new(200).set_body_json(empty_page()))
            .expect(1)
            .mount(&server)
            .await;

        let query = TradesQuery {
            tokens: Some(vec![mint()]),
            limit: Some(50),
            flags: Some(vec![WalletFilter::Bundler, WalletFilter::ProTrader]),
            ..TradesQuery::default()
        };
        let page = provider(&server).trades(&query).await.expect("page");

        assert!(page.trades.is_empty());
        assert!(!page.has_more);
    }

    #[tokio::test]
    async fn a_wallet_trades_read_sends_its_wallets_side_and_tokens() {
        let wallet = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/trades"))
            .and(query_param("wallets", wallet))
            .and(query_param("side", "sell"))
            .and(query_param("tokens", MINT))
            .respond_with(ResponseTemplate::new(200).set_body_json(empty_page()))
            .expect(1)
            .mount(&server)
            .await;

        let query = TradesQuery {
            wallets: Some(vec![wallet.parse().expect("a wallet")]),
            tokens: Some(vec![mint()]),
            side: Some(TradeDirection::Sell),
            ..TradesQuery::default()
        };
        provider(&server).trades(&query).await.expect("page");
    }

    #[tokio::test]
    async fn a_holders_read_sends_the_key_its_tokens_flags_and_period() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/holders"))
            .and(header("authorization", format!("Bearer {KEY}")))
            .and(query_param("tokens", MINT))
            .and(query_param("limit", "50"))
            .and(query_param("flags", "bundler,insider"))
            .and(query_param("period", "cycle"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
                "chain": "solana",
                "address": MINT,
                "holders": [],
                "holderCount": 0,
                "top10": { "avgEntryPrice": null, "avgExitPrice": null },
            }])))
            .expect(1)
            .mount(&server)
            .await;

        let query = HolderSeriesQuery {
            tokens: vec![mint()],
            chains: None,
            limit: Some(50),
            flags: Some(vec![WalletFilter::Bundler, WalletFilter::Insider]),
            period: Some(HolderPeriod::Cycle),
        };
        let list = provider(&server).holders(&query).await.expect("a holder list");

        assert_eq!(list.len(), 1);
        assert_eq!(list[0].holder_count, 0);
    }

    #[tokio::test]
    async fn a_token_theses_read_sends_its_filters_and_keeps_a_thesis_from_an_unnamed_app() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/v1/tokens/solana/{MINT}/theses")))
            .and(header("authorization", format!("Bearer {KEY}")))
            .and(query_param("sources", "fomo,axiom"))
            .and(query_param("minPositionUsd", "1000"))
            .and(query_param("sortBy", "positionUsd"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "theses": [{
                    "source": "newapp", "thesisId": "p-1", "parentId": null, "postedAt": 1,
                    "text": "gm", "likeCount": 0,
                    "author": {
                        "id": "u-1", "handle": "cooker", "displayName": null, "avatarUrl": null,
                        "verified": false, "isDev": false, "twitterUsername": null,
                    },
                    "wallet": null, "positionUsd": "0", "tokenAmount": "0",
                    "unrealizedPnlUsd": null, "unrealizedPnlPct": null, "realizedPnlUsd": null,
                    "costBasisUsd": null, "calledOutAtMarketCapUsd": null, "multiple": null,
                    "closedAt": null, "authorPosition": null,
                }],
                "nextCursor": null,
                "hasMore": false,
            })))
            .expect(1)
            .mount(&server)
            .await;

        let query = TokenThesesQuery {
            sources: Some(vec![ThesisSourceFilter::Fomo, ThesisSourceFilter::Axiom]),
            min_position_usd: Some("1000".parse().expect("a decimal")),
            sort_by: Some(ThesisSortKey::PositionUsd),
            ..TokenThesesQuery::default()
        };
        let page =
            provider(&server).token_theses(SpotChain::Solana, &mint(), &query).await.expect("page");

        assert_eq!(page.theses[0].source, ThesisSource::Unknown);
        assert!(!page.has_more);
    }

    #[tokio::test]
    async fn an_identity_read_joins_its_chains() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/trades"))
            .and(query_param("identity", "fomo"))
            .and(query_param("chains", "solana,bsc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(empty_page()))
            .expect(1)
            .mount(&server)
            .await;

        let query = TradesQuery {
            identity: Some(IdentityFeed::Fomo),
            chains: Some(vec![SpotChain::Solana, SpotChain::Bsc]),
            ..kol()
        };
        provider(&server).trades(&query).await.expect("page");
    }

    #[tokio::test]
    async fn an_identity_read_without_chains_names_every_chain_this_build_knows() {
        let every_chain: Vec<String> = SpotChain::iter().map(|chain| chain.to_string()).collect();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/trades"))
            .and(query_param("chains", every_chain.join(",")))
            .respond_with(ResponseTemplate::new(200).set_body_json(empty_page()))
            .expect(1)
            .mount(&server)
            .await;

        provider(&server).trades(&kol()).await.expect("page");
    }

    #[tokio::test]
    async fn a_balances_read_joins_its_wallets_and_names_every_chain_this_build_knows() {
        let wallet = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";
        let every_chain: Vec<String> = SpotChain::iter().map(|chain| chain.to_string()).collect();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/balances"))
            .and(query_param("wallets", wallet))
            .and(query_param("chains", every_chain.join(",")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .expect(1)
            .mount(&server)
            .await;

        let query =
            WalletBalancesQuery { wallets: vec![wallet.parse().expect("a wallet")], chains: None };
        let balances = provider(&server).wallet_balances(&query).await.expect("balances");

        assert!(balances.0.is_empty());
    }

    #[tokio::test]
    async fn a_portfolio_read_sends_its_wallets_chains_and_sections() {
        let wallet = "0x1234567890abcdef1234567890abcdef12345678";
        let empty = json!({ "valueUsd": "0", "balancesValueUsd": "0", "positionsValueUsd": "0", "chains": [] });
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/portfolio"))
            .and(query_param("wallets", wallet))
            .and(query_param("chains", "robinhood,bsc"))
            .and(query_param("include", "stats"))
            .and(query_param("hideLowLiquidity", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "totalValueUsd": "0",
                "asOf": 1,
                "wallets": [{ "addresses": { "evm": wallet }, "spot": empty }],
                "spot": empty,
            })))
            .expect(1)
            .mount(&server)
            .await;

        let query = PortfolioQuery {
            wallets: vec![wallet.parse().expect("a wallet")],
            chains: Some(vec![SpotChain::Robinhood, SpotChain::Bsc]),
            include: Some(vec![PortfolioInclude::Stats]),
            hide_low_liquidity: true,
        };
        let portfolio = provider(&server).wallet_portfolio(&query).await.expect("portfolio");

        assert_eq!(portfolio.wallets.len(), 1);
        assert!(portfolio.stats.is_none());
    }

    #[tokio::test]
    async fn a_native_prices_read_names_every_chain_this_build_knows_and_reads_a_price() {
        let every_chain: Vec<String> = SpotChain::iter().map(|chain| chain.to_string()).collect();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/native-prices"))
            .and(query_param("chains", every_chain.join(",")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
                "chain": "solana",
                "asset": "SOL",
                "price": { "native": "1", "usd": "150.25" },
                "observedAt": 1_790_930_462_000_u64,
            }])))
            .expect(1)
            .mount(&server)
            .await;

        let prices = provider(&server).native_prices(&NativePricesQuery::default()).await;
        let prices = prices.expect("prices");

        assert_eq!(prices.len(), 1);
        assert_eq!(prices[0].chain, SpotChain::Solana);
        assert_eq!(prices[0].price.usd.map(|usd| usd.to_string()).as_deref(), Some("150.25"));
    }

    #[tokio::test]
    async fn a_positions_read_sends_its_sort_and_filters() {
        let wallet = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/positions"))
            .and(query_param("wallets", wallet))
            .and(query_param("lifecycle", "closed"))
            .and(query_param("sort", "pnl:asc"))
            .and(query_param("hideLowLiquidity", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "positions": [],
                "nextCursor": null,
                "hasMore": false,
                "asOf": 1_790_930_462_000_u64,
                "counts": { "spot": { "open": 0, "closed": 0 } },
            })))
            .expect(1)
            .mount(&server)
            .await;

        let mut query = PositionsQuery::wallets(vec![wallet.parse().expect("a wallet")]);
        query.lifecycle = Some(Lifecycle::Closed);
        query.sort = Some(PositionSort { key: PositionSortKey::Pnl, order: SortOrder::Asc });
        let page = provider(&server).positions(&query).await.expect("page");

        assert!(page.positions.is_empty());
    }

    #[tokio::test]
    async fn an_ohlcv_read_sends_its_tokens_and_the_timeframe_slug() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/ohlcv"))
            .and(query_param("tokens", MINT))
            .and(query_param("timeframe", "5m"))
            .and(query_param("countBack", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .expect(1)
            .mount(&server)
            .await;

        let query = CandleSeriesQuery {
            tokens: vec![mint()],
            chains: None,
            timeframe: Some(Timeframe::FiveMin),
            metric: None,
            denomination: None,
            from: None,
            to: None,
            count_back: Some(100),
        };
        let list = provider(&server).ohlcv(&query).await.expect("candles");

        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn a_discovery_read_posts_the_request_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/discovery"))
            .and(body_json(json!({ "chains": ["solana"], "graduating": { "limit": 5 } })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .expect(1)
            .mount(&server)
            .await;

        let request = SpotPulseSubscriptionConfig {
            chains: vec![SpotChain::Solana],
            graduating: Some(SpotPulseColumnConfig { limit: Some(5), ..Default::default() }),
            ..SpotPulseSubscriptionConfig::default()
        };
        let board = provider(&server).discovery_board(&request).await.expect("board");

        assert!(board.is_empty());
    }

    #[tokio::test]
    async fn a_trending_read_posts_the_request_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/trending"))
            .and(body_json(json!({ "chains": ["solana"], "window": "1h", "limit": 5 })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .expect(1)
            .mount(&server)
            .await;

        let request = SpotTrendingFilter {
            chains: vec![SpotChain::Solana],
            window: TrendingWindow::OneHour,
            limit: Some(5),
            filters: None,
        };
        let board = provider(&server).trending_board(&request).await.expect("board");

        assert!(board.is_empty());
    }

    #[tokio::test]
    async fn a_session_token_mint_posts_its_request_with_the_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/session-tokens"))
            .and(header("authorization", format!("Bearer {KEY}")))
            .and(body_json(json!({ "origins": ["https://app.example.com"], "ttlSeconds": 600 })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "token": "metasession_sealed",
                "expiresAt": 1_790_930_462_000_u64,
            })))
            .expect(1)
            .mount(&server)
            .await;

        let request = CreateSessionTokenRequest {
            origins: vec!["https://app.example.com".to_owned()],
            ttl_seconds: Some(600),
            ..CreateSessionTokenRequest::default()
        };
        let minted = provider(&server).session_token_create(&request).await.expect("a token");

        assert_eq!(minted.token, "metasession_sealed");
    }

    #[tokio::test]
    async fn a_path_on_the_base_url_is_kept() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/gateway/v1/trades"))
            .respond_with(ResponseTemplate::new(200).set_body_json(empty_page()))
            .expect(1)
            .mount(&server)
            .await;

        let provider =
            MetastreamsProvider::new_with_base_url(KEY, &format!("{}/gateway/", server.uri()))
                .expect("provider");
        provider.trades(&kol()).await.expect("page");
    }

    #[tokio::test]
    async fn an_error_envelope_reads_as_its_code() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/tokens"))
            .respond_with(
                ResponseTemplate::new(400).insert_header("x-request-id", "req-1").set_body_json(
                    json!({
                        "error": { "code": "VALIDATION_ERROR", "message": "Name at least one address." },
                    }),
                ),
            )
            .mount(&server)
            .await;

        let query = TokensQuery { tokens: vec![mint()], chains: None };
        let error = provider(&server).tokens(&query).await.expect_err("refused");

        let MetastreamsError::Api { status, error: detail, request_id } = &error else {
            panic!("expected an API error, got {error:?}");
        };
        assert_eq!(*status, StatusCode::BAD_REQUEST);
        assert_eq!(detail.code, ErrorCode::ValidationError);
        assert_eq!(request_id.as_deref(), Some("req-1"));
        assert!(!error.is_transient());
    }

    #[tokio::test]
    async fn a_429_carries_its_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/trades"))
            .respond_with(
                ResponseTemplate::new(429).insert_header("retry-after", "7").set_body_json(
                    json!({ "error": { "code": "RATE_LIMITED", "message": "Slow down." } }),
                ),
            )
            .mount(&server)
            .await;

        let error = provider(&server).trades(&kol()).await.expect_err("limited");

        assert!(matches!(
            error,
            MetastreamsError::RateLimited { retry_after: Some(wait), .. } if wait == Duration::from_secs(7)
        ));
        assert!(error.is_transient());
    }

    #[tokio::test]
    async fn a_429_whose_body_breaks_off_still_carries_its_retry_after() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        let address = listener.local_addr().expect("an address");
        tokio::spawn(async move {
            let (mut tcp, _) = listener.accept().await.expect("a client");
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut chunk = [0; 1024];
                let read = tcp.read(&mut chunk).await.expect("a read");
                request.extend_from_slice(&chunk[..read]);
            }
            // The body stops short of its length, and then the connection ends.
            let head =
                "HTTP/1.1 429 Too Many Requests\r\nretry-after: 7\r\ncontent-length: 100\r\n\r\n";
            tcp.write_all(format!("{head}{{\"error\"").as_bytes()).await.expect("a write");
        });
        let provider = MetastreamsProvider::new_with_base_url(KEY, &format!("http://{address}"))
            .expect("provider");

        let error = provider.trades(&kol()).await.expect_err("limited");

        assert!(
            matches!(
                error,
                MetastreamsError::RateLimited { retry_after: Some(wait), .. } if wait == Duration::from_secs(7)
            ),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn a_failure_without_the_envelope_keeps_its_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/trades"))
            .respond_with(ResponseTemplate::new(502).set_body_string("<html>Bad Gateway</html>"))
            .mount(&server)
            .await;

        let error = provider(&server).trades(&kol()).await.expect_err("bad gateway");

        assert!(
            matches!(&error, MetastreamsError::Unexpected { body, .. } if body.contains("Bad Gateway"))
        );
        assert!(error.is_transient());
    }

    #[test]
    fn debug_never_prints_the_key() {
        let provider = MetastreamsProvider::new("meta_secret").expect("provider");
        assert!(!format!("{provider:?}").contains("meta_secret"));
    }

    #[test]
    fn a_non_http_base_url_is_refused() {
        let error =
            MetastreamsProvider::new_with_base_url(KEY, "ftp://example.com").expect_err("not http");
        assert!(matches!(error, MetastreamsError::UnsupportedScheme(scheme) if scheme == "ftp"));
    }
}
