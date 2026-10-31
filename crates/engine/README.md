# opensharing-engine

An **OpenSharing** / **Delta Sharing** implementation over pluggable ports.

[`opensharing-api`](../api) defines the protocol surface. This crate owns the
sharing *semantics* behind it — share / schema / asset discovery,
the table query path (version / metadata / query) and the storage-backed asset
surface (volumes, agent skills) — and expresses them over three narrow ports:

- `ShareCatalog`: which shares and objects a recipient sees, and where a shared
  object lives (backed by any source catalog);
- `StorageAccess`: the server's own reads of a shared asset, plus presigned file
  URLs and temporary credentials for recipients;
- `TableFormat`: serving one table format. Delta is built in (feature `delta`)
  and reads tables through `delta_kernel` directly, so no query engine is
  involved.

`SharingService` composes one implementation of each port and implements every
`opensharing-api` handler trait. `opensharing_api::sharing_router` serves it
under `/api/v1/delta-sharing` (Delta Sharing) and `/api/1.0/opensharing`
(OpenSharing, adding volumes, agent skills and temporary credentials), and
`opensharing_api::auth` puts a bearer-token authenticator in front.

## Integrating

The crate depends on no catalog (no Unity Catalog crates). To serve sharing from
an existing catalog, implement `ShareCatalog`, `StorageAccess` and a
`RecipientAuthenticator` over the catalog's own discovery, grants and credential
vending, then mount `opensharing_api::sharing_router` behind
`opensharing_api::auth::require_recipient`. Map your own errors onto
`opensharing_api::Error`, which renders the `{ "errorCode", "message" }`
envelope. [`opensharing-server`](../server) is a complete host built this way.

A host that wants none of this implements the `opensharing-api` handler traits
directly instead.

## Admin API

Like `opensharing-api`, this crate carries only the recipient-facing protocol.
The provider admin API (shares, permissions, recipients) is a separate surface,
[`opensharing-admin-api`](../admin-api), with its own authentication and error
contract.

## Status

- **Discovery** (shares, schemas, tables, volumes, skills) is implemented and
  paginated.
- **Query** (`version`, `metadata`, `query`) serves Delta tables in the parquet
  response format, with version / timestamp time travel, predicate-hint file
  skipping and limit hints. Tables that need reader features (column mapping,
  deletion vectors, …) require `responseformat=delta` and are rejected for now.
- **Credentials**: `temporary-table-credentials` (OpenSharing) and the volume /
  skill credential endpoints vend read-only credentials through `StorageAccess`.
- **Not implemented** (`501`): change data feed (`/changes`), asynchronous
  queries (`POST /queries/{id}`) and `responseformat=delta` serving.

## Architecture

See [`ARCHITECTURE.md`](../../ARCHITECTURE.md) for the pluggable server design
(ports, source-catalog share mapping, delegated vs. local storage access,
secrets at rest) and the gap assessment against the upstream OpenSharing
reference implementation.
