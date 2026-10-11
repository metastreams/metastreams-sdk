# Changelog

All notable changes to the two crates in this repository. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and both crates share one version.

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
