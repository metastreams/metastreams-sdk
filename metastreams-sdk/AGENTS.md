# AGENTS.md — metastreams-sdk

The Rust client for the Metastreams API. Every request and response is the `metastreams-types` type that the API serves. The client therefore cannot drift from the server.

Read <https://docs.metastreams.io> before changing anything here — every method implements it.

## Module map

| Module | Owns |
|--------|------|
| `provider.rs` | `MetastreamsProvider` — the bearer key, one method per `/v1` operation (the reads, and the session-token mint), and `connect` for the `/v1/stream` socket |
| `stream.rs` | `StreamConnection` and `SubscriptionStream` — typed subscriptions over one socket, each an in-order `Event` stream of updates and exits, and why each one ends |
| `error.rs` | `MetastreamsError` — a transport failure, a 429 and its `Retry-After`, the API's error envelope, or a subscription's end |
| `constants.rs` | The production origin, the request deadlines, and the stream socket's ping and buffer limits |

## Rules

1. **No wire type is defined here.** A request or response shape lives in `metastreams-types`, and a new endpoint adds its type there first.
2. **The SDK never paces a call.** The server sets each key's ceiling, and the SDK cannot see it. A 429 surfaces as `RateLimited` with its `Retry-After` wait. `subscribe_throttled` asks the server to pace a `Throttled` channel, and only the server checks the bound.
3. **The SDK never reconnects a stream.** The server keeps no subscription across sockets and replays nothing, so a reconnect always leaves a gap. Only the consumer can fill it, from REST.
4. **The key never leaves the `Authorization` header.** `Debug` redacts it, and no log or error carries it.
5. **The SDK depends on this repository alone.** A consumer pulls in no chain SDK and no private crate, and keeps its own Solana and EVM versions.
