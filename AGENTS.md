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
- **Keep the catalog boundary.** The Unity Catalog runtime (`olai-uc-client`,
  `olai-uc-storage-access`, every other UC resource) is reachable only through
  `opensharing-server`'s `unity` feature. The one exception is the admin API:
  `opensharing-admin-api` takes its models, handler traits and error from
  `olai-uc-common` and `olai-uc-handlers`, with only the `shares`,
  `recipients` and `providers` resources (plus `axum`), and the provider and
  the server's `admin` feature inherit exactly that. `opensharing-api` and
  `opensharing-engine` depend on no `olai-uc-*` crate. CI's
  `catalog-independence` job guards this.
- **Keep the layers.** The surface crates (`opensharing-api`,
  `opensharing-admin-api`) are barebones: generated traits, routers, auth and
  errors (the admin API's from Unity Catalog, see above). `opensharing-api` has no table format, object store or engine, and
  never depends on `opensharing-admin*`. `opensharing-admin-api` never depends
  on `opensharing-api`, `opensharing-common` or `opensharing-client`. Each
  surface has its own auth seam and error type. `opensharing-engine` never
  depends on the admin crates or the provider. `opensharing-provider` never
  depends on sqlx; the store crates do. The same CI job guards this. See
  `ARCHITECTURE.md` ("Crate layering").
- **Cargo.lock and sibling crates.** CI resolves the crates of sibling repos
  (trestle's `olai-*`, mangrove's `olai-uc-*` once unpinned) from crates.io. A
  lock resolved under a local `just patch-deps <upstream>` patch must not be
  committed: run `just lock-registry` first. `just check-lock`, which
  the pre-commit hook also runs (`git config core.hooksPath .githooks`), catches
  it. See "crates.io proxy" below.
- **Every change ships with tests.** Never delete a passing test to green a
  suite.

## Codegen gotchas

- Install the `trestle` CLI from the open-lakehouse Homebrew tap:
  `brew install open-lakehouse/tap/trestle`, or `brew upgrade trestle` after a
  trestle release. Trestle's release pipeline updates the tap, so a release is
  installable right away. A crates.io proxy that age-filters new versions can
  hide it for days. A stale CLI rejects `json_field_names`. The trestle
  source checkout is only needed to patch the `olai-*` library crates
  (`just configure-trestle-deps`), not for the CLI.
- The admin API protos (`unitycatalog.{shares,recipients,providers}.v1`) live in
  mangrove, not here: change them there, release `olai-uc-common` /
  `olai-uc-handlers`, then bump the pins. `proto/` holds only the protocol.
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

## Builds and disk

Dev builds keep line tables for workspace crates and no debug info for
dependencies (`[profile.dev]`). Panic locations and backtrace file:line still
work. For a debugger, use `cargo build --profile debugging` (separate
`target/debugging/`). Run `cargo clean` in a worktree before removing it.

## crates.io proxy

The local crates.io proxy hides versions for 7 days after release. That applies
to every crate a sibling repo publishes, not only trestle's: distributary
consumes trestle's `olai-*` from crates.io and mangrove's `olai-uc-*` (git pins
today). Trestle's
[`docs/crates-proxy.md`](https://github.com/open-lakehouse/trestle/blob/main/docs/crates-proxy.md)
has the full guide; trestle only hosts the tooling. In short:

- A fresh sibling release: `just patch-deps <upstream>` (`trestle`, `mangrove`;
  `configure-trestle-deps` is a shortcut), build, then `just lock-registry`
  before committing. The shim `dev/scripts/crates-proxy.sh` runs trestle's
  `scripts/crates-proxy/` and finds the checkouts next to this one (or via
  `<REPO>_ROOT`), from worktrees too.
- Chained releases: a fresh mangrove release usually needs a fresh trestle
  release too. Patch both: `just patch-deps mangrove && just patch-deps trestle`.
- The mangrove crates are git pins (`version` + `git` + `rev`), which bypass the
  proxy. Switching one to a plain `version` that is less than 7 days old is the
  case above. Otherwise just drop `git`/`rev`.

## Commits and pull requests

- **PRs are squash-merged. The PR title is the commit release-plz sees**: a
  conventional commit scoped by crate directory (`common`,
  `admin-api`, `client`, `api`, `engine`, `provider`, `postgres`, `sqlite`,
  `server`, `neon`). release-plz derives each crate's version bump and
  changelog from it, and CI lints it (`pr-title.yml`). Commits inside a PR are
  for review only.
- When work should land on `main` as separate commits (several crates or change
  types, each deserving its own changelog entry), split it into a
  [`gh stack`](https://github.com/github/gh-stack), one layer per
  `type(scope)`.
