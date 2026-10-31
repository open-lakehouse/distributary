# distributary

An [OpenSharing](https://github.com/OpenSharing-IO/OpenSharing) /
[Delta Sharing](https://github.com/delta-io/delta-sharing) implementation in
Rust: the protocol models, a client, a portable server-side protocol layer, and a
composable sharing server that maps shares onto any catalog.

| Crate | Package | What |
|---|---|---|
| [`crates/common`](crates/common) | `opensharing-common` | Wire models, NDJSON query lines, capabilities, axum extractors |
| [`crates/client`](crates/client) | `opensharing-client` | HTTP client for the sharing protocol |
| [`crates/api`](crates/api) | `opensharing-api` | Protocol ports (`ShareCatalog`, `StorageAccess`, `TableFormat`), handlers, router and recipient auth |
| [`crates/admin`](crates/admin) | `opensharing-admin` | Provider admin API models: shares, permissions, recipients, providers |
| [`crates/admin-api`](crates/admin-api) | `opensharing-admin-api` | Provider admin API server surface: generated handler traits, routers, provider auth |
| [`crates/server`](crates/server) | `opensharing-server` | A complete server: share, recipient and credential stores (memory, Postgres, SQLite), sealed secrets, catalog sources, admin API (feature `admin`), `sharing-server` binary |
| [`crates/neon`](crates/neon) | `opensharing-neon` | The server as a Neon Function (deployment artifact, not published) |

The protocol is defined in [`proto/opensharing/v1`](proto/opensharing/v1),
and the provider admin API (wire-compatible with the Databricks Delta Sharing
REST API) in [`proto/opensharing/{shares,recipients,providers}`](proto/opensharing).
Their OpenAPI documents are [`openapi/sharing.yaml`](openapi/sharing.yaml) and
[`openapi/admin.yaml`](openapi/admin.yaml). The design is in
[`ARCHITECTURE.md`](ARCHITECTURE.md).

The crates moved here from
[mangrove](https://github.com/open-lakehouse/mangrove), a Unity Catalog
implementation, together with the shares, recipients and providers APIs.
Unity Catalog stays an optional integration: the server's `unity` feature adds a
Unity Catalog REST catalog source and the UC-shaped storage credential /
external location admin routes. Without it, no crate here depends on a
catalog's models (CI's `catalog-independence` job enforces this).

## Development

```sh
cargo check --workspace --all-features
cargo nextest run --workspace --all-features
```

Behind a crates.io proxy that hides freshly published versions, patch the
`olai-*` crates from a [`trestle`](https://github.com/open-lakehouse/trestle)
checkout at `../trestle` (writes a gitignored `.cargo/config.toml`):

```sh
just configure-trestle-deps
```

Never commit a `Cargo.lock` resolved under that patch. The pre-commit hook
(`git config core.hooksPath .githooks`) runs
`dev/scripts/check-cargo-lock-trestle.sh` to catch it.

### Code generation

Models, the client, and the handler traits are generated from `proto/` by `buf`
and the [`trestle`](https://github.com/open-lakehouse/trestle) CLI. Run
`just generate` after changing a proto, and commit the generated output with the
change. Never hand-edit files under `**/models/_gen/**` or `**/codegen/**`.

## License

Apache-2.0
