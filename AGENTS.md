# Agent Instructions

Instructions for AI coding agents working in this repository. See
[`README.md`](README.md) for the layout and [`ARCHITECTURE.md`](ARCHITECTURE.md)
for the design.

## Rules

- **Never commit directly to `main`.** Work on a `feat/`, `fix/`, `chore/`,
  `refactor/` or `test/` branch.
- **Never hand-edit generated files**: anything under `crates/**/models/_gen/**`
  or `crates/**/codegen/**`, or any file marked `@generated`. Change `proto/` and
  run `just generate`, then commit the output in the same commit as the source
  change.
- **Keep the catalog boundary.** No crate except `opensharing-server` depends
  on a catalog crate, and the server depends on `olai-uc-*` only behind its
  `unity` feature. CI's `catalog-independence` job guards this.
- **Keep the layers.** The surface crates (`opensharing-api`,
  `opensharing-admin-api`) are barebones: generated traits, routers, auth and
  errors. `opensharing-api` has no table format, object store or engine, and
  never depends on `opensharing-admin*`. `opensharing-admin-api` never depends
  on `opensharing-api`, `opensharing-common` or `opensharing-client`. Each
  surface has its own auth seam and error type. `opensharing-engine` never
  depends on the admin crates or the provider. `opensharing-provider` never
  depends on sqlx; the store crates do. The same CI job guards this. See
  `ARCHITECTURE.md` ("Crate layering").
- **Cargo.lock and trestle.** CI resolves the `olai-*` trestle crates from
  crates.io. A lock resolved under the local `just configure-trestle-deps` patch
  must not be committed. Check with `dev/scripts/check-cargo-lock-trestle.sh`,
  which the pre-commit hook also runs (`git config core.hooksPath .githooks`).
- **Every change ships with tests.** Never delete a passing test to green a
  suite.

## Codegen gotchas

- Install the `trestle` CLI from the open-lakehouse Homebrew tap:
  `brew install open-lakehouse/tap/trestle`, or `brew upgrade trestle` after a
  trestle release. Trestle's release pipeline updates the tap, so a release is
  installable right away. A crates.io proxy that age-filters new versions can
  hide it for days. A stale CLI rejects `json_field_names`. The `../trestle`
  source checkout is only needed to patch the `olai-*` library crates
  (`just configure-trestle-deps`), not for the CLI.
- The admin protos (`proto/unitycatalog/{shares,recipients,providers}`) have their
  own pipeline (`buf.gen.admin.yaml`, `trestle.admin.yaml`, `crates/admin`). They
  share message names with the protocol (`Share`, `ListSharesRequest`, …), and a
  single trestle run over both cross-wires the generated protocol client.
  The admin run emits the models into `crates/admin` and the handler traits and
  route functions into `crates/admin-api/src/codegen`.
- Run `buf generate` **before** `trestle generate`. Raw buffa output carries
  camelCase JSON names, and trestle rewrites them to the snake_case wire names.
  `just generate` runs both, in order.
- The NDJSON table query RPCs (`version`, `metadata`, `query`) are not in the
  proto services. They are hand-written in
  `crates/common/src/query_extractors.rs` and `crates/api`.

## Toolchain

CI and `rust-toolchain.toml` pin Rust 1.99, the same as the other
open-lakehouse repos (mangrove, headwaters). The minimum supported Rust is the
workspace `rust-version` (1.95.0, set by the dependency tree). It is separate
from the pin, so bumping the pin never raises it. With 1.99, keep
`async-trait` ≥ 0.1.92 and `thiserror` ≥ 2.0.21: older versions of their
macros trip clippy's `double_must_use` and `redundant_field_names`.

## Pre-push check (mimics CI)

```bash
cargo fmt --all --check \
  && cargo clippy --workspace --all-targets --all-features -- -D warnings \
  && cargo nextest run --workspace --all-features \
  && cargo test --workspace --all-features --doc
```

## Commits and pull requests

Conventional commits, scoped by crate directory (`common`, `admin`,
`admin-api`, `client`, `api`, `engine`, `provider`, `postgres`, `sqlite`,
`server`, `neon`): release-plz derives each crate's version bump and
changelog from them. PR titles follow the same format.
