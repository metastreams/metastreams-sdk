# AGENTS.md

A guide for coding agents and humans working in this repository.

## What this is

One Cargo workspace with two crates: [`metastreams-types`](metastreams-types/AGENTS.md), the wire objects of the Metastreams API, and [`metastreams-sdk`](metastreams-sdk/AGENTS.md), the client built on them. Each crate's `AGENTS.md` maps its modules and states its rules. Read the one for the crate you edit first.

The API these crates speak is documented at <https://docs.metastreams.io>. The API server consumes `metastreams-types` from this repository, so a wire change lands here first.

## Rules

1. **A wire type is a contract.** A renamed field, a removed variant or a changed serde attribute breaks every consumer. Add; never rename or remove without a major version bump.
2. **Shared versions.** Both crates carry the workspace version and release together under one `vX.Y.Z` tag.
3. **Every dependency lives in the root `[workspace.dependencies]`**, and a crate consumes it with `<crate>.workspace = true`.
4. **The gate is CI's.** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features`, `cargo test --workspace --all-features` and `cargo doc --workspace --no-deps`, each with `-D warnings`. Run them before a PR.
5. **Comments are short.** Doc comments on every public item, one line each, saying what the item is for. Active voice, present tense.

## Releasing

1. Bump `version` in the root `Cargo.toml` and add the section to `CHANGELOG.md`.
2. Merge, then tag `vX.Y.Z` on `main` and push the tag. The release workflow checks the tag against the manifest, runs the tests and publishes the GitHub release with that changelog section.
