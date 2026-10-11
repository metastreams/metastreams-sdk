# AGENTS.md — metastreams-types

The Metastreams wire objects. The API server serves the responses and reads the requests; `metastreams-sdk` reads the responses and writes the requests. One definition serves both sides, so the server and the SDK cannot drift apart.

Read <https://docs.metastreams.io> before changing anything here — every type is a wire contract.

## Module map

| Module | Owns |
|--------|------|
| `balance.rs` | `UiWalletBalance`, the balances response, the balances query, and the `DepositAsset` and `BalanceTrigger` copies |
| `position.rs` | `UiSpotPosition`, its token, lifetime and funding groups, the positions page, the positions query and its sort, and the `Product` copy |
| `money.rs` | `UiMoney` — the `{native, usd}` pair every amount on the surface is |
| `decimal.rs` | The `Decimal` copy — the exact number every amount carries, a string on the wire |
| `time.rs` | The `UnixMilliseconds` copy — the instant every timestamp carries |
| `address.rs` | The `MultiChainAddress` copy — a Solana or EVM address, checked without a chain SDK |
| `sort.rs` | The `SortOrder` copy |
| `token/` | The token object, `UiSpotToken` and its groups; `pool.rs` holds its pools, `fees.rs` its trade fees, `platform.rs` `UiPlatform`, `annotation.rs` the `TokenAnnotation` copy, `funding.rs` the funding enum copies, `graduation.rs` the `GraduationStage` copy, and `holders.rs` `UiSpotHolder`, the holder list, the holders query and the `HolderPeriod` copy |
| `candle.rs` | The OHLCV bucket, the candles response, the candles query, and the `Timeframe`, `CandleMetric` and `CandleDenomination` copies |
| `trade.rs` | `UiSpotTrade`, its page, both trade queries, the trader's identities, the `flags` filter kinds, the identity feed, and the `TradeDirection`, `WalletFlag` and `IdentitySource` copies |
| `transfer.rs` | `UiSpotTransfer`, one movement of an asset between two wallets |
| `discovery/` | The Pulse columns, the request body, the ranked response, and the `SpotSortField` copy; `filters.rs` holds the `SpotTokenFilters` and `FilterRange` copies |
| `search.rs` | The search query, its kinds and sorts, and the kind-tagged hits |
| `error.rs` | `ErrorCode`, the `{error:{code,message,details?}}` envelope, and the marker that carries a detail through serde |
| `stream.rs` | The WebSocket frames, each channel's filter typed to its name, update and exit payloads by `Subscription`, the `Throttled` marker, and the subprotocol a browser offers beside its session token |
| `session.rs` | The session-token mint: its request body, and the token it answers |
| `filter.rs` | The stream filter language: `TokenFilter`, its inclusive `Range`, and its `exclude` side; `TradeFilter`, `TransferFilter` and `CandleFilter` build on them |
| `chain.rs` | The `SpotChain`, `AddressFamily` and `AnchorToken` copies, and the slug parse that refuses an unknown slug with `UNSUPPORTED_CHAIN` |
| `csv.rs` | The serde form of a query list: one comma-separated string |
| `schema.rs` | The open-enumeration schema the documents share |

## Rules

1. **Each object is the shape the API serves, field for field.** The value types every object builds on, such as `Decimal`, `UnixMilliseconds` and `MultiChainAddress`, are defined here with their serde and schema attributes, so a consumer needs no chain SDK.
2. **One request type serves both sides.** The SDK writes each request with the type here, and the server reads it with the same type. Its parse is the contract, so it refuses what the server refuses. Checks a parse cannot make stay on the server.
3. **This crate stands alone.** It depends on no private crate and no chain SDK. The server converts its own storage rows into these types on its side, so nothing here knows how a value is stored.
4. **A registry that grows reads a value it does not name.** An enum here whose response values can grow, such as `ErrorCode` or `UiPlatform`, carries a read-only `Unknown` variant that the server never sends.
