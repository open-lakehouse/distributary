# opensharing-common

**OpenSharing** / **Delta Sharing** wire models, shared by client and server.

- The generated request/response types of the discovery services
  (`models::opensharing::v1`), with snake_case JSON on the wire and camelCase
  accepted on read.
- The hand-written newline-delimited JSON lines of the table query path
  (`QueryResponseLine`, `MetadataResponse`, `ProtocolResponseData`,
  `ParquetFile`, …) and their conversions from `delta_kernel` actions.
- Parsing of the `delta-sharing-capabilities` header
  (`DeltaSharingCapabilities`).
- Behind the `axum` feature, the axum request extractors a server needs for
  these types.

[`opensharing-client`](../client) sends these types and
[`opensharing-api`](../api) serves them. Keeping them in their own crate
lets the server side build without the client's dependencies.

The generated code under `src/models/_gen/` comes from `proto/opensharing` via
`just codegen generate-proto` + `just codegen generate-code-sharing`. Never edit
it by hand.
