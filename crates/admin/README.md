# opensharing-admin

Models for the Open Sharing provider admin API: shares, share permissions,
recipients and providers. The JSON wire format matches the Databricks Delta
Sharing REST API (`/api/2.1/unity-catalog/{shares,recipients,providers}`).

Generated from `proto/open_sharing/{shares,recipients,providers}` (`just
generate`). The recipient-facing protocol models are in
[`opensharing-common`](../common).
