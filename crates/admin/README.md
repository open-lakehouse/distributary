# opensharing-admin

Models for the OpenSharing provider admin API: shares, share permissions,
recipients and providers. The JSON wire format matches the Databricks Delta
Sharing REST API (`/api/2.1/unity-catalog/{shares,recipients,providers}`).

Generated from `proto/opensharing/{shares,recipients,providers}` (`just
generate`). The handler traits and routers that serve these models are in
[`opensharing-admin-api`](../admin-api). The recipient-facing protocol
models are in [`opensharing-common`](../common).
