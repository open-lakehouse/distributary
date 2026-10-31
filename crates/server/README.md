# opensharing-server

The batteries-included OpenSharing / Delta Sharing server. It assembles the
library crates: the protocol surface ([`opensharing-api`](../api)), its
implementation ([`opensharing-engine`](../engine)) and the provider domain
([`opensharing-provider`](../provider)). It adds the SQL stores, layered
configuration, Unity Catalog integration and the `sharing-server` binary. See
[`ARCHITECTURE.md`](../../ARCHITECTURE.md) for the design.

- **Shares map onto a source catalog.** A share is a list of share objects
  (`TABLE`, `SCHEMA`, `VOLUME`, `AGENT_SKILL`) naming assets in a catalog source:
  a static list in the config, a Unity Catalog REST server (mangrove's
  `uc-server`, UC OSS or Databricks; feature `unity`), or your own
  `CatalogSource`. Locations and formats are
  resolved from the source at request time.
- **Storage access is delegated or local.** A *delegated* source vends
  short-lived, downscoped credentials, so the server stores no storage secrets.
  A *local* source uses credentials stored in the server, sealed, and minted by
  the same `olai-credential-vending` minters the UC server uses. Either way, query
  responses carry presigned URLs that never outlive the credential, and the
  OpenSharing `temporary-*-credentials` endpoints hand the credential on.
- **Recipients use bearer tokens.** Creating a recipient issues a token with a
  one-time activation link. Opening the link returns the `.share` profile, and
  from then on the server keeps only the token's sha256 digest.
- **Secrets are sealed at rest.** Envelope encryption with pluggable
  key-encryption keys, rotated lazily on read or eagerly with `rotate-keys`.

## Surfaces

| Path | What |
|---|---|
| `/api/v1/delta-sharing/…` | Delta Sharing protocol (recipient token) |
| `/api/1.0/opensharing/…` | OpenSharing protocol: adds volumes, agent skills, temporary credentials (recipient token) |
| `/api/1.0/opensharing/activations/{code}` | One-time activation link, returns the sharing profile |
| `/api/2.1/unity-catalog/…` | Provider admin API (feature `admin`; mounted when `provider_tokens` are configured), in the Databricks Delta Sharing REST shape: `shares`, `shares/{name}/permissions`, `recipients`, `recipients/{name}/rotate-token`; with feature `unity` also `credentials`, `external-locations` (provider token) |
| `/health` | Liveness |

## Features

| Feature | Default | What |
|---|---|---|
| `postgres` | ✓ | The Postgres store (sqlx, embedded migrations) |
| `sqlite` | | The SQLite store (sqlx, embedded migrations), for single-node deployments |
| `unity` | ✓ | Unity Catalog integration: the UC REST catalog source and (with `admin`) the UC-shaped `credentials` / `external-locations` admin routes |
| `admin` | ✓ | The provider admin API: `ProviderService` implements `opensharing-admin-api`'s generated `ShareHandler` / `RecipientHandler`, authorized by an `AdminPolicy` |
| `bin` | | The `sharing-server` binary (implies `postgres`, `sqlite`, `unity` and `admin`) |

### Without Unity Catalog

With `default-features = false` (optionally plus `postgres` or `sqlite`) the
crate uses no catalog's models: the store, recipient tokens and activation,
sealed secrets, the static source and storage access remain, and `admin` adds
the share / recipient admin API. A catalog that already has discovery and an
admin surface implements `CatalogSource`, provisions shares and recipients
through the `SharingStore` (`opensharing_provider::recipients::issue_token`
issues tokens), and serves `embed::router` (`tests/custom_mount.rs`).

A host that wants less than the whole server composes the library crates
instead: see [`opensharing-provider`](../provider) for reusing the share
mapping, grants and tokens over a host's own store and request context.

### Stores

`store` re-exports the provider's store traits and in-memory store, and adds
the Postgres (`store::postgres`) and SQLite (`store::sqlite`) implementations,
with their embedded migrations. All of them pass the provider's conformance
suite (`opensharing_provider::testing::round_trips`).

## Running

```sh
cargo run -p opensharing-server --features bin -- -c config.yaml migrate
cargo run -p opensharing-server --features bin -- -c config.yaml serve
```

Without a config file the server runs on an in-memory store with an insecure
development key. Below is a configuration with a Unity Catalog source and
Postgres (for example Neon). The direct URL is used for migrations and the
startup schema check, which need a session-level advisory lock:

```yaml
public_url: https://sharing.example.com
database:
  type: postgres
  url: { env: DATABASE_URL }                 # pooled is fine
  migration_url: { env: DATABASE_URL_UNPOOLED }
encryption:
  active: { id: v1, key: { env: SHARING_KEK_V1 } }   # 32 bytes, base64
provider_tokens: [{ env: SHARING_PROVIDER_TOKEN }]
sources:
  - name: uc
    type: unity
    url: https://uc.example.com/api/2.1/unity-catalog/
    token: { env: UC_TOKEN }
    access: delegated
```

Give the server its own database: its migrator keeps its own
`_sqlx_migrations` table.

For a single node, SQLite needs no database server (feature `sqlite`, included
in `bin`). A `DATABASE_URL` starting with `sqlite:` selects it too:

```yaml
database:
  type: sqlite
  url: sqlite:///var/lib/sharing/sharing.db   # created if missing
```

### Sharing a table

```sh
H='Authorization: Bearer <provider token>'; U=https://sharing.example.com/api/2.1/unity-catalog
curl -X POST  $U/shares -H "$H" -d '{"name":"sales"}'
curl -X PATCH $U/shares/sales -H "$H" -d '{"updates":[{"action":"ADD","data_object":
  {"name":"main.sales.orders","data_object_type":"TABLE","history_data_sharing_status":"ENABLED"}}]}'
curl -X POST  $U/recipients -H "$H" -d '{"name":"acme","authentication_type":"TOKEN"}'
curl -X PATCH $U/shares/sales/permissions -H "$H" -d '{"changes":[{"principal":"acme","add":["SELECT"]}]}'
```

Send the recipient's `tokens[0].activation_url`. Opening it yields a profile
that the `delta-sharing` clients read directly.

## Testing

`cargo test -p opensharing-server` runs the end-to-end suite on the
in-memory store; `--no-default-features` runs the catalog-free host test
(`tests/custom_mount.rs`). The store conformance suite runs on SQLite (feature
`sqlite`, a temporary file), and on Postgres when
`SHARING_SERVER_TEST_DATABASE_URL` points at a disposable database (see
`tests/postgres.rs`).
