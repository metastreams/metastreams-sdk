# Changelog

All notable changes to the two crates in this repository. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and both crates share one version.

## 0.5.0 - 2026-11-23

### Breaking

- `SpotChain::Ethereum`. The API serves Ethereum mainnet, so a `match` on `SpotChain` needs the new arm. Before this, every Ethereum payload, such as an Ethereum `chain.nativePrices` update, failed to decode.

## 0.4.0 - 2026-11-23

### Added

- `StreamRoute` picks the path the stream socket takes. The frames, limits and billing are the same on both.
  - `Direct` sends a free `POST /v1/stream/connect` and opens the socket on the direct stream URL it returns, with no proxy between. It falls back to `Proxied` when the server has no direct stream, the connect call fails, or the direct handshake fails or takes over 3 seconds. A `401`, `402` or `429` on the connect call returns at once.
  - `Proxied` opens `/v1/stream` on the base URL, through the proxy in front of the API, with no connect call.
- `connect_via(route)` opens the socket on a route. `connect()` takes `StreamRoute::Direct`.
- `types::connect::UiStreamConnection`, the answer of `POST /v1/stream/connect`.

## 0.3.0 - 2026-11-19

The API moved to one collection endpoint per resource. Each takes a list of addresses and reads every chain in scope of each address.

### Breaking

- `token_details` is `tokens` (`GET /v1/tokens`), and answers one `UiSpotToken` per token and chain.
- `token_ohlcv` is `ohlcv` (`GET /v1/ohlcv`), and answers one `UiCandleSeries` per token and chain.
- `token_holders` is `holders` (`GET /v1/holders`), and answers one `UiHolderSeries` per token and chain.
- `token_trades` and `identity_trades` are `trades` (`GET /v1/trades`). `TradesQuery` takes `tokens`, `wallets` or `identity`.
- `native_prices`, `discovery_board` and `trending_board` answer a bare list, and `UiWalletBalances` wraps one. `UiNativePrices` and `UiSpotPulseResponse` are gone.
- A `spot.trending` update is a bare list of deltas.
- `SpotChain::Base` and `SpotChain::Arc` are gone. The API no longer serves them.

### Added

- `wallet_portfolio` (`GET /v1/portfolio`): the summed spot holdings of up to 100 wallets, with optional stats, and the `portfolio` module.
- `CloseReason::Failed`: the server ended one subscription with an `error` frame that names it. It is transient only for an internal or unavailable error.

## 0.2.0 - 2026-11-10

### Breaking

- The fomo.family identity is spelled `fomo` everywhere. `IdentityFeed::Fomo` and the trade `flags` filter no longer read `fomoscan`.
- `TradeFlag` is now `WalletFilter`, and `TradeFilter.flags` and `TransferFilter.flags` take it.
- A `spot.discovery` column serves `UiSpotPulseToken`, the token's market state without `decimals` and `supply`. `UiSpotToken` flattens it into `card` and adds those two fields, so Rust code reads `token.card.name`; the JSON shape is unchanged.
- `SpotTokenFilters` refuses an unknown key.

### metastreams-types

- `thesis`: a token's theses, social posts with their author and reported position, and their query.
- `native_price`: each chain's gas asset priced in USD, over REST and the `chain.nativePrices` channel.
- `trending`: the trending board over REST and the `spot.trending` channel, with the `TrendingWindow` sizes.
- `spot.discovery` streams the Pulse columns, as a whole column first and then only the changed rows.
- `UiTokenLogo` on the token object, one CDN URL per size, and `logoUrl` on the position token.
- `SpotChain::Elysium`.
- The creator's funding amount and time on the token, the `TaxVault` and `DexProtocolFee` fee kinds, and the `Hype` deposit asset.

### metastreams-sdk

- `token_theses`, `native_prices` and `trending_board` on `MetastreamsProvider`.
- `TrendingWindow` re-exported at the crate root.
- `tests/live.rs`: ignored checks of every REST operation and every stream channel against the live API. They run with `METASTREAMS_API_KEY` set and `--ignored`.

### Fixed

- The `token_details` example and the README read USDC. The API does not serve wSOL as a token.
- The README and the manifests point at `github.com/metastreams/metastreams-sdk`.

## 0.1.0 - 2026-10-11

The first release.

### metastreams-types

- Every request and response of the `/v1` REST surface: tokens, OHLCV candles, trades, holders, identity trades, wallet balances, positions, the discovery board and session tokens.
- The `/v1/stream` frames, each channel's filter, and the update and exit payloads of `spot.trades`, `spot.transfers`, `spot.candles`, `spot.tokens`, `spot.holders`, `spot.positions` and `wallet.balances`.
- The error envelope and `ErrorCode`.

### metastreams-sdk

- `MetastreamsProvider`: one method per `/v1` operation, against production or a custom base URL.
- `StreamConnection` and `SubscriptionStream`: typed subscriptions over one socket, with throttled and candle subscribe variants, and a `CloseReason` for every way a subscription ends.
- `MetastreamsError`, with `is_transient` for retry decisions.
