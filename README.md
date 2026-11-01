# distributary

An [OpenSharing](https://github.com/OpenSharing-IO/OpenSharing) /
[Delta Sharing](https://github.com/delta-io/delta-sharing) implementation in
Rust: the protocol models, a client, the bare protocol surface, an engine that
implements it over pluggable ports, and a
composable sharing server that maps shares onto any catalog.

| Crate | Package | What |
|---|---|---|
| [`crates/common`](crates/common) | `opensharing-common` | Wire models, NDJSON query lines, capabilities, axum extractors |
| [`crates/client`](crates/client) | `opensharing-client` | HTTP client for the sharing protocol |
| [`crates/api`](crates/api) | `opensharing-api` | The protocol surface: generated handler traits, the NDJSON query trait, routers, bearer auth |
| [`crates/engine`](crates/engine) | `opensharing-engine` | The protocol implemented over ports (`ShareCatalog`, `StorageAccess`, `TableFormat`): `SharingService` and the kernel-backed Delta format |
| [`crates/admin-api`](crates/admin-api) | `opensharing-admin-api` | Provider admin API server surface: Unity Catalog's share / recipient / provider models and handler traits, routers, provider auth |
| [`crates/provider`](crates/provider) | `opensharing-provider` | The provider domain: share, recipient, token and credential stores (memory), grants, sealed secrets, the admin API service, and the catalog, policy and context they plug into the protocol |
| [`crates/postgres`](crates/postgres) | `opensharing-postgres` | The Postgres store (sqlx, embedded migrations) |
| [`crates/sqlite`](crates/sqlite) | `opensharing-sqlite` | The SQLite store (sqlx, embedded migrations), for single-node deployments |
| [`crates/server`](crates/server) | `opensharing-server` | The batteries-included server: store selection, configuration, Unity Catalog integration (feature `unity`), `sharing-server` binary |
| [`crates/neon`](crates/neon) | `opensharing-neon` | The server as a Neon Function (deployment artifact, not published) |

The protocol is defined in [`proto/opensharing/v1`](proto/opensharing/v1),
and its OpenAPI document is [`openapi/sharing.yaml`](openapi/sharing.yaml).
The provider admin API (wire-compatible with the Databricks Delta Sharing REST
API) is Unity Catalog's `unitycatalog.{shares,recipients,providers}.v1`, defined
in [mangrove](https://github.com/open-lakehouse/mangrove). The design is in
[`ARCHITECTURE.md`](ARCHITECTURE.md).

The crates moved here from
[mangrove](https://github.com/open-lakehouse/mangrove), a Unity Catalog
implementation. Unity Catalog stays an optional integration: the server's
`unity` feature adds a Unity Catalog REST catalog source and local storage
access through `olai-uc-storage-access` (storage credentials and external
locations, with their UC admin routes). Without it, the only Unity Catalog
crates here are the admin API's models and handler traits (`olai-uc-common`
and `olai-uc-handlers`, shares, recipients and providers only) and the sharing
store (`olai-uc-sharing`: shares, recipients, tokens and grants, over
catalog-independent records). CI's `catalog-independence` job enforces this.

## Development

```sh
cargo check --workspace --all-features
cargo nextest run --workspace --all-features
```

Behind a crates.io proxy that hides freshly published versions, patch the
sibling repo's crates from its checkout next to this one (writes a gitignored
`.cargo/config.toml`). That means trestle's `olai-*` or mangrove's `olai-uc-*`;
the tooling comes from a [`trestle`](https://github.com/open-lakehouse/trestle)
checkout. Then restore the registry entries in `Cargo.lock` before committing:

```sh
just patch-deps trestle   # or mangrove, or both for chained releases
just lock-registry        # before committing a Cargo.lock change
```

Never commit a `Cargo.lock` resolved under that patch. The pre-commit hook
(`git config core.hooksPath .githooks`) runs the same check as `just check-lock`
to catch it. See trestle's
[`docs/crates-proxy.md`](https://github.com/open-lakehouse/trestle/blob/main/docs/crates-proxy.md).

### Code generation

Models, the client, and the handler traits are generated from `proto/` by `buf`
and the [`trestle`](https://github.com/open-lakehouse/trestle) CLI. Run
`just generate` after changing a proto, and commit the generated output with the
change. Never hand-edit files under `**/models/_gen/**` or `**/codegen/**`.

Install the CLI from the open-lakehouse Homebrew tap. It is updated on every
trestle release:

```sh
brew install open-lakehouse/tap/trestle   # later: brew upgrade trestle
```

## License

Apache-2.0
