# Changelog

All notable changes to the two crates in this repository. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and both crates share one version.

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
