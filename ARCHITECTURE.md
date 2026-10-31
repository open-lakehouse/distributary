# OpenSharing server architecture

Status: accepted (2026-10-29). Delivered as one stacked PR series. The
layers are listed under [Delivery](#delivery).

This record was written and delivered in
[mangrove](https://github.com/open-lakehouse/mangrove) (PRs #237–#243, tracking
issue #228). The crates moved to this repository on 2026-10-30. Paths and PR
numbers below refer to mangrove at that time.

## Context

This crate already has the beginnings of an OpenSharing / Delta Sharing
surface: a `SharingBackend` port, blanket discovery and query handlers, and an
axum router. Nothing mounts or implements it, though. The only backend is the
in-memory `testing` one, and the query path has gaps:

- It goes through DataFusion and `olai-uc-datafusion`'s `ReconciledLogProvider`.
  That pulls DataFusion and the git-pinned `deltalake-core` into a crate whose
  real job is listing files.
- `/query` returns bare `file` lines. They carry raw storage paths: no
  presigned URL, no protocol or metadata lines, and no stats. Version,
  timestamp, predicate hints and limit hints are all ignored.
- The server never parses `delta-sharing-capabilities`, and
  `DeltaSharingCapabilities::parse_header` panics on malformed input.
- Only `GET /shares` passes pagination to the backend. Schemas are derived from
  tables only.
- There is no recipient authentication and no share store.

The upstream reference implementation,
[OpenSharing-IO/OpenSharing](https://github.com/OpenSharing-IO/OpenSharing), is
Java on Armeria with delta-kernel-java, licensed Apache-2.0. It defines the
OpenSharing spec (`/api/1.0/opensharing`, Share → Schema → Asset, with volumes,
agent skills and models as asset kinds). Its implementation is narrower than
the spec:

| Capability | Spec (OpenSharing ⊇ Delta Sharing) | Upstream (Java) | Here, before this work |
|---|---|---|---|
| List/get shares, schemas, tables, all-tables (paged) | ✓ | ✓ | ✓ (only shares paged) |
| Table version (`startingTimestamp`) | ✓ | ✓ | version only |
| Table metadata (`version`/`timestamp`) | ✓ | ✓ | latest only |
| `POST …/query` (presigned file URLs, hints) | ✓ | ✗ | raw paths, no hints |
| `GET …/changes` (CDF) | ✓ | ✗ | 501 |
| `responseformat=delta` | ✓ | ✓ (negotiated) | ✗ |
| Async query (`/queries/{id}`) | ✓ | ✗ | 501 |
| `temporary-table-credentials` (vending) | ✓ | ✓ | ✗ |
| Volumes / agent skills | ✓ | ✗ | discovery + vend via port |
| Iceberg REST view of tables | ✓ | ✗ | ✗ |
| Recipient bearer tokens + activation | ✓ | ✓ | ✗ |
| Provider admin API (recipients, shares, permissions) | — | ✓ | UC-shaped CRUD in `uc-server` only |
| Pluggable catalog | — | one `CatalogConnector`, YAML only | `SharingBackend` (unimplemented) |

Upstream has no `/query`. Classic `delta-sharing` clients therefore can't
read table data from it: they get metadata and must use vended credentials
themselves. We want both models.

## Decision

Build a **composable sharing server**:

- It maps shares onto assets that already exist in a **pluggable source
  catalog**.
- It serves classic Delta Sharing `/query` with **presigned URLs** and also the
  OpenSharing **credential-vending** endpoints, from one vending implementation.
- It reads Delta through `delta_kernel` directly, with **no DataFusion**.
- It owns its share, recipient and token state in Postgres, and seals secrets
  at rest behind a key-rotation seam.

### Ports

`SharingBackend` is split into narrow ports. They stay generic over the
request context `Cx`, which becomes the authenticated recipient.

| Port | Responsibility | First implementations | Later |
|---|---|---|---|
| `BearerAuthenticator<Cx>` | bearer token → `Cx` (the caller) | the token store, sha256-hashed tokens | OIDC / JWT |
| `SharingPolicy<Cx>` | may the caller list shares / read this share? | the store's grants | — |
| Store traits (`ShareStore: ShareReader`, `RecipientStore: TokenReader`, `CredentialStore`) | shares (UC `shares/v1` `Share` / `DataObject`) and grants, recipients and tokens, sealed credentials | Postgres and SQLite (sqlx), memory | — |
| `CatalogSource` | resolve a `DataObject` (`catalog.schema.table`; `SCHEMA` objects expand) to a `SourceAsset { url, format, id, … }`, paged | UC REST (`olai-uc-client`: our UC or Databricks), static | Iceberg REST, UC-Delta API, in-process UC store |
| `StorageAccess` | the server's own log reads, `presign(url, ttl)`, `vend(url, op)` | delegated (catalog-vended), local (own credentials) | — |
| `TableFormat` | version / metadata / query / changes as protocol actions | Delta (kernel) | Iceberg |
| `KeyProvider` (`olai-store`) | wrap/unwrap data keys for sealed secrets | `LocalKeyProvider` | cloud KMS / Key Vault |

The recipient-facing ports live in `opensharing-engine`: `ShareCatalog`, which
covers discovery and resolution; `SharingPolicy`, which authorizes every
action; `StorageAccess`; and `TableFormat`. The provider implements
`ShareCatalog` on top of its `ShareStore` and a `CatalogSource`, and
`SharingPolicy` on top of its grants, and `opensharing-api`'s bearer layer puts
a `BearerAuthenticator` in front of the router. See
[Crate layering](#crate-layering) for where each piece lives.

The wire types (generated request/response models, the NDJSON query lines and
the capabilities header) live in `opensharing-common`, which both this crate
and `opensharing-client` depend on. The server side never depends on the
client, so the client can take on heavier dependencies (DataFrame libraries and
the like) without the server inheriting them.

A `SharingService` holds one implementation per port, plus a format registry.
It implements the existing handler traits, so discovery, query and asset
serving stay identical whatever backs them. The router mounts the same
handlers twice behind a bearer-auth layer: under `/api/1.0/opensharing` (the
spec, including assets and credential endpoints) and under the Delta Sharing
prefix the `opensharing-client` targets.

### Discovery maps shares onto a source catalog

A share is a list of share objects naming assets in a source catalog. The
entries are tables, whole schemas, volumes and agent skills. The sharing server
stores the share (as `ShareObject`s, which use the field names of UC's
`DataObject`). It resolves each object through `CatalogSource` at request time:
storage location, format and identity come from the catalog, which stays the
source of truth. Fields we honor:

- `shared_as` renames the object for recipients.
- `start_version` clamps time travel and CDF.
- `history_data_sharing_status` gates version/timestamp reads.
- `partitions` restricts which partitions are shared. This is a follow-up.

### Storage access: delegated or local

Every mode serves the same recipient surface: `/query` with presigned URLs and
the `temporary-{table,volume,skill}-credentials` endpoints.

- **Delegated.** This is the default whenever the source catalog can vend.
  The server stores **no storage secrets**. Per request it asks the catalog
  for short-lived, downscoped credentials, using UC
  `temporary-table-credentials` / `temporary-volume-credentials` (later
  Iceberg REST `loadCredentials`). It caches them for a short TTL and uses
  them for three things:
  - reading the Delta log;
  - presigning file URLs (S3 SigV4 with the session token, or the Azure SAS
    appended to the blob URL);
  - answering the recipient credential endpoints, by passing the credential
    through.

  The URLs and credentials we hand out never outlive what the catalog
  issued. GCS is the exception for presigning: an OAuth access token cannot
  sign URLs, so GCS tables are served via vending only, or the catalog must
  vend signed URLs.
- **Local.** This mode is for standalone deployments, static catalogs, or
  catalogs that can't vend. The server holds storage credentials itself, in
  UC's model: a storage `Credential` (IAM role, Azure service principal or
  managed identity, storage key) plus external locations matched by longest
  URL prefix. It presigns and vends with the **same** code `uc-server` uses,
  the trestle `olai-credential-vending` minters and `olai-vended-store`
  presigning, so the workspace has exactly one vending implementation.

Each catalog source chooses its mode (`access = delegated | local`).

### Secrets at rest

Storage credentials (local mode) and catalog-source client secrets (PATs,
OAuth client secrets) are sealed with `olai-store`'s `EnvelopeEncryptor`.
This is the same envelope encryption `uc-server` uses. Recipient tokens are
stored only as hashes.

`KeyProvider` is async and keyed by KEK id, so an external key store slots in
without format changes. Rotation is **lazy**. When a read opens a blob sealed
under a retired KEK, the server `rewrap`s it under the active KEK and writes it
back. A `rotate-keys` admin command re-wraps everything eagerly. The first
release ships `LocalKeyProvider` (config-supplied KEKs) and a
`key_provider = local | …` config seam. A cloud KMS provider is a follow-up, and
it is the expected choice for hosted (Neon) deployments: the KEK stays in KMS,
and DEK unwraps are cached in-process.

### Delta without DataFusion

The Delta `TableFormat` uses only `delta_kernel` and its default engine:

- **Version and timestamp.** `Snapshot::builder_for(..).at_version(v)` serves a
  version. Timestamps resolve through
  `history_manager::latest_version_as_of` (`HistoryCommitType::Recreatable`).
- **Files.** A `Scan` with a best-effort kernel `Predicate` translated from
  `jsonPredicateHints` prunes files. Unsupported hint shapes are dropped:
  hints are advisory, and the recipient still filters. Then `scan_metadata` →
  `visit_scan_files` yields `ScanFile { path, size, stats, partition_values, dv_info }`.
- **Response.** The response streams protocol, metaData, then `file` lines.
  Each `file` line has a presigned `url`, a stable `id` (a hash of the path),
  `partitionValues`, `size`, `stats` and `expirationTimestamp`. `limitHint` is
  applied cumulatively over `numRecords` from stats.
- **Capabilities.** `delta-sharing-capabilities` is parsed leniently. The first
  release serves `responseformat=parquet`, and `responseformat=delta` and CDF
  (`table_changes`) are follow-ups.

`ReconciledLogProvider` stays in `olai-uc-datafusion` for the log explorer.
This crate drops `datafusion` and `olai-uc-datafusion`.

### Hosts

`opensharing-server` is a library plus binary, structured like Headwaters'
service crate:

- **Startup.** `embed::Service::start` returns a socket-less `Router`.
- **Binary subcommands.** `serve`, `migrate`, `healthcheck` and `rotate-keys`.
- **Configuration.** Config is layered: file, then env, then `DATABASE_URL`.
- **Schema.** sqlx migrations are embedded in the binary. They run only via
  `migrate`, against the unpooled URL, and `serve` refuses to start on schema
  drift.
- **Admin API** (feature `admin`, on by default). It reuses the UC `shares`,
  `recipients` and `credentials` protos under `/api/2.1/unity-catalog/…`,
  gated by a provider token. It is mounted only when provider tokens are
  configured. Recipient creation issues a token and an activation URL, and
  `GET /api/1.0/opensharing/activations/{code}` returns the profile. The
  activation route belongs to the protocol router, so tokens provisioned
  straight through the `RecipientStore` activate too.

The admin API is built the same way as the protocol. Trestle generates the
`ShareHandler<Cx>`, `RecipientHandler<Cx>` and `ProviderHandler<Cx>` traits and
their route functions into `opensharing-admin-api` (from
`trestle.admin.yaml`). The hand-written routers there (`shares_router`,
`recipients_router`, `providers_router`, `admin_router`) bind them, and the
request context `Cx` is extracted per request. `opensharing-provider`
implements the share and recipient traits in a custom `ProviderService<Cx>`,
not on a generic resource store, because issuing tokens, activation links and
sealing need more than CRUD. Each handler first calls the service's
`AdminPolicy<Cx>` with an `AdminAction`, the admin counterpart of
`SharingPolicy`. The standalone server uses `Provider` (from
`require_provider` over the provider tokens) as the context and allows every
authenticated caller.

A Unity Catalog server can therefore mount the admin API into its own router
in either of two ways:

- Nest `admin_router::<ProviderService<Cx>, Cx>` under its
  `/api/2.1/unity-catalog` with its own request context as `Cx`, and plug its
  permission model in as the `AdminPolicy`.
- Implement the generated traits on its own store.

`opensharing-neon` hosts the same router as a Neon Function. It mirrors
Headwaters (`headwaters-neon`): a napi `cdylib`, a thin dependency-free
TypeScript fetch handler, and a `release-neon` profile. Configuration comes from
the environment (`SHARING_SERVER_CONFIG_YAML`, plus Neon's `DATABASE_URL` /
`DATABASE_URL_UNPOOLED`). Migrations run out of band (`just neon
migrate`).

Size is the binding constraint: Neon allows 10 MiB zipped and 64 MiB unzipped.
All builds below are for `aarch64-unknown-linux-gnu.2.28`, with fat LTO, one
codegen unit and stripped symbols. Measured on 2026-10-29 with the standalone
server binary, which links the same dependency graph as the addon (kernel,
arrow/parquet, `object_store` with AWS/Azure/GCS, sqlx, rustls):

| `opt-level` | `panic` | unzipped | zipped |
|---|---|---|---|
| 3 | unwind | 27.9 MiB | 11.1 MiB (over the limit) |
| `"z"` | abort | 15.3 MiB | 6.7 MiB |
| `"z"` (`release-neon`, the actual addon bundle) | unwind | 17.5 MiB | 7.8 MiB |

The bundle loads and passes the smoke suite on `node:24-bookworm-slim`
(linux-arm64) via `just neon test-arm64`. It has not been deployed to a
Neon branch yet.

`release-neon` therefore uses `opt-level = "z"`. It keeps `panic = "unwind"`,
so a panic fails the request instead of the isolate. `just neon build`
gates the real bundle on both limits. If it gets tight, the next levers are:

1. `panic = "abort"`;
2. per-cloud `object_store` features;
3. trimmed parquet codecs.

### Crate layering

Added 2026-10-31. It replaces the earlier "Protocol and admin API separation"
section. Migrating the code exposed friction in the split between the protocol
and the admin API:

- `opensharing-api` mixed the bare spec surface with our implementation of it:
  the ports, `SharingService` and the Delta format, plus a hard dependency on
  `delta_kernel` and `object_store`. Even the wire crate embedded kernel's
  `Protocol` type.
- Recipient identity (`Recipient`, `RecipientAuthenticator`) lived in the
  protocol, although recipients and tokens are provider resources.
- Authorization was a method on the discovery port.
- The provider domain was locked inside the batteries-included server.
- The bridge types were hard-coded to the server's own context.

The crates are now layered:

```
wire        opensharing-common            opensharing-admin
surface     opensharing-api               opensharing-admin-api   generated traits + routers, barebones
engine      opensharing-engine                                    the protocol over ports, formats, vending
domain      opensharing-provider                                  share mapping, recipients, tokens, grants
stores      opensharing-postgres, opensharing-sqlite              the provider's store traits over sqlx
batteries   opensharing-server  ->  opensharing-neon              config, store selection, binary, `unity`
```

| Layer | Holds | Depends on |
|---|---|---|
| Surface | handler traits per spec service (generated, plus the NDJSON `SharingQueryHandler`), routers, `BearerAuthenticator<Cx>` + `require_bearer`, the error contract | wire models only |
| Engine | `ShareCatalog`, `SharingPolicy`, `StorageAccess`, `TableFormat`; `SharingService`, which implements every surface trait; the Delta format; `CatalogSource` and `SourceStorageAccess<R: CredentialResolver>` (feature `vending`) | surface, kernel, object_store, trestle vending |
| Provider | the store traits and in-memory store, recipients and activation, secrets, `ProviderService` (feature `admin`), and the bridge: `Recipient`, `RecipientIdentity`, `StoreCatalog`, `GrantPolicy`, `StoreAuthenticator`, `StoreCredentialResolver`, `sharing_service` | engine, both surfaces |
| Stores | Postgres / SQLite store implementations and migrations | provider |
| Batteries | config, `embed`, binary, UC source and UC credential routes | everything |

Both surfaces follow one pattern: handler traits generic over the request
context `Cx`, generated where the protos define the RPCs and hand-written where
they don't (the NDJSON query path). Routers extract `Cx` per request and pass
it to the handler. The two kinds of caller authenticate in unrelated ways, so
each surface owns its seam and its 401 semantics. The surface fixes the
mechanism (a bearer token); the host decides who the token belongs to.

From mangrove we took the separate `Policy<Cx>` (here `SharingPolicy`) and the
authentication layer that puts the caller's context in the request extensions.
We did not take its blanket handler impls over capability traits:
`SharingService` stays a struct of port trait objects, so a host can mix ports
from different crates at runtime, such as a UC catalog with our grants.

The provider's share mapping is not a complete discovery implementation on its
own. Expanding `SCHEMA` entries and resolving locations and formats need the
source catalog. The provider therefore implements the engine's `ShareCatalog`
port over the mapping composed with a `CatalogSource`, rather than the
generated traits directly. `ShareCatalog::list_shares` still returns only the
caller's shares, because a listing is paged where it is filtered.

A catalog server such as mangrove can take the sharing slot at any layer:

1. **Barebones.** Implement the `opensharing-api` traits on its own type and
   mount `sharing_router` (`crates/api/tests/barebones.rs`).
2. **Engine.** Implement `ShareCatalog`, `SharingPolicy` and `StorageAccess`
   over its own catalog with its own `RequestContext`. The protocol semantics,
   Delta serving and vending come with it (`crates/engine/tests/policy.rs`).
3. **Engine and provider.** Use the provider's share, recipient and token state
   and admin API (mounted with its own `Cx` and `AdminPolicy`), implement
   `RecipientIdentity` for its context, and supply its own `CatalogSource`. A
   catalog that keeps shares and recipients in its own store implements only
   `ShareReader` and `TokenReader` (`crates/provider/tests/host_store.rs`).

Either way, the admin API is mounted separately or not at all.

### Stores

The server's state is split into targeted traits instead of a generic resource
store, because the set of resources is small and fixed:

- `ShareStore` holds shares and grants.
- `RecipientStore` holds recipients and tokens.
- `CredentialStore` holds sealed storage credentials and external locations.
- `SharingStore` bundles all three.

The read-only views `ShareReader` and `TokenReader` are all the protocol needs.
The traits and the memory implementation live in `opensharing-provider`. The
Postgres and SQLite implementations are their own crates
(`opensharing-postgres`, `opensharing-sqlite`), each with its own SQL and
embedded migrations. All of them run the provider's conformance suite
(`testing::round_trips`). SQLite serves single-node deployments that should
survive a restart without a database server. Postgres (Neon) stays the hosted
backend.

### Catalog independence

The sharing crates live in their own repository, and any catalog should be able
to host them. The layering:

- `opensharing-common`, `opensharing-api` (the surface) and
  `opensharing-engine` (ports, formats, sources, storage access) depend on no
  catalog crate. `opensharing_api::Error` owns its `{ "errorCode", "message" }`
  envelope.
- `opensharing-provider` (store, recipients, secrets, `StoreCatalog`,
  `GrantPolicy`) and the store crates use catalog-independent types:
  `ShareObject` for share entries and the trestle crates'
  `RegisteredCredential` / `VendedCredential` for vending.
- The provider admin API for shares, permissions and recipients needs no
  catalog, and it is optional (feature `admin`). Its traits and routers are
  generated into `opensharing-admin-api`, and implemented by
  `opensharing-provider`. Its models come from `opensharing-admin`
  (`proto/opensharing/{shares,recipients,providers}`,
  moved from mangrove's `unitycatalog.*` packages with an unchanged wire format,
  2026-10-30).
- Unity Catalog comes in only through `opensharing-server`'s `unity` feature
  (`src/unity/`): the UC
  REST `CatalogSource` and the UC-shaped `credentials` / `external-locations`
  admin routes. It converts UC models at that boundary (`olai-uc-common`'s
  `credential-vending` feature for credentials).

Credential vending, the object-store primitives and presigning come from the
catalog-independent trestle crates (`olai-storage-credentials`,
`olai-vended-store`, `olai-credential-vending`), which own their credential
shapes and depend on no catalog crate.

## Delivery

One GitHub stack, one PR per layer:

1. This document.
2. Extract `olai-uc-credentials` (vending, storage locations, presign) out of
   `uc-server`, with no behavior change.
3. `sharing-api` ports split and kernel Delta format. DataFusion is dropped,
   pagination and capability parsing are fixed.
4. Storage access on the recipient surface: presigned `/query`, credential
   endpoints, the dual-prefix router and bearer auth.
5. `opensharing-server`: Postgres store, sealed secrets with lazy rewrap,
   UC REST catalog source, delegated and local access, admin API, binary,
   end-to-end tests.
6. The Neon Function host (`opensharing-neon`), with the size measurement
   above.

## Deferred

Tracked as follow-up issues:

- Change data feed.
- `responseformat=delta`.
- Async query.
- `partitions` filtering.
- Iceberg REST and UC-Delta catalog sources.
- An Iceberg REST view of shared tables.
- OIDC recipients.
- The upstream-shaped `/provider/*` admin API.
- A cloud KMS `KeyProvider`.
