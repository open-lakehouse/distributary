# opensharing-api

The **OpenSharing** / **Delta Sharing** REST surface, and nothing more.

This crate is the protocol as the spec defines it, generic over a request
context `Cx`:

- the generated handler traits `ShareHandler` (shares, schemas, tables),
  `VolumeHandler` and `SkillHandler`;
- the hand-written `SharingQueryHandler`: the NDJSON `version` / `metadata` /
  `query` path, temporary table credentials, change data feed and asynchronous
  queries;
- the axum routers: `get_router` (Delta Sharing), `opensharing_router`
  (OpenSharing, adding volumes, agent skills and credentials) and
  `sharing_router`, which mounts both under `/api/v1/delta-sharing` and
  `/api/1.0/opensharing`;
- bearer-token authentication in front of the routes (`auth`);
- the `{ "errorCode", "message" }` error contract (`Error`).

It holds no sharing semantics and depends on no catalog, table format or object
store. The most barebones server there is implements the traits and mounts a
router (`tests/barebones.rs`). A Delta Sharing server needs only
`ShareHandler` and `SharingQueryHandler`.

[`opensharing-engine`](../engine) implements every trait over pluggable ports
(share catalog, storage access, table formats, with a kernel-backed Delta
format) and is the usual way to serve this surface. The wire types come from
[`opensharing-common`](../common), which the client shares.

The provider admin API is a separate surface,
[`opensharing-admin-api`](../admin-api), with its own authentication and error
contract.
