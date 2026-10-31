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
- **Keep the catalog boundary.** `opensharing-common`, `opensharing-admin`,
  `opensharing-admin-api`, `opensharing-client` and `opensharing-api` never
  depend on a catalog crate.
  `opensharing-server` may depend on `olai-uc-*` only behind its `unity`
  feature. CI's `catalog-independence` job guards this.
- **Keep the protocol and the admin API apart.** `opensharing-api` never
  depends on `opensharing-admin*`, and `opensharing-admin-api` never depends on
  `opensharing-api`, `opensharing-common` or `opensharing-client`. Each has its
  own auth seam and error type. The same CI job guards this.
- **Cargo.lock and trestle.** CI resolves the `olai-*` trestle crates from
  crates.io. A lock resolved under the local `just configure-trestle-deps` patch
  must not be committed. Check with `dev/scripts/check-cargo-lock-trestle.sh`,
  which the pre-commit hook also runs (`git config core.hooksPath .githooks`).
- **Every change ships with tests.** Never delete a passing test to green a
  suite.

## Pre-push check (mimics CI)

```bash
cargo fmt --all --check \
  && cargo clippy --workspace --all-targets --all-features -- -D warnings \
  && cargo nextest run --workspace --all-features \
  && cargo test --workspace --all-features --doc
```

## Commits and pull requests

Conventional commits, scoped by crate directory (`common`, `admin`, `admin-api`, `client`,
`api`, `server`, `neon`): release-plz derives each crate's version bump and
changelog from them. PR titles follow the same format.
