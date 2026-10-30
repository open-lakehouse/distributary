# Repository Guidelines

The always-applied rules are in [`AGENTS.md`](AGENTS.md). The layout is in
[`README.md`](README.md), and the design in [`ARCHITECTURE.md`](ARCHITECTURE.md).
The commit message contract and the signing flow are machine-wide (see
`~/.claude/CLAUDE.md`).

## Codegen gotchas

- Run `buf generate` **before** `trestle generate`. Raw buffa output carries
  camelCase JSON names, and trestle rewrites them to the snake_case wire names.
  `just generate` runs both, in order.
- Use a trestle CLI built from the `../trestle` source checkout
  (`cargo build --release -p olai-trestle`). A stale installed binary rejects
  `json_field_names`.
- The NDJSON table query RPCs (`version`, `metadata`, `query`) are not in the
  proto services. They are hand-written in
  `crates/common/src/query_extractors.rs` and `crates/api`.

## Toolchain

CI pins Rust 1.97. Run clippy with `cargo +1.97` locally, because newer stable
toolchains report false positives on thiserror derives.
