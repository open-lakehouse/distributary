# opensharing-admin-api

The server side of the OpenSharing provider admin API: shares, share
permissions, recipients and providers, in the Databricks Delta Sharing REST
shape (`/api/2.1/unity-catalog/{shares,recipients,providers}`).

The crate is built the same way as the recipient-facing protocol in
[`opensharing-api`](../api), and depends on it not at all:

- The models (`shares`, `recipients`, `providers`), `ShareHandler`,
  `RecipientHandler`, `ProviderHandler` and their route functions are Unity
  Catalog's, re-exported from `olai-uc-common` and `olai-uc-handlers` (only
  these three resources). Each trait is generic over the request context `Cx`.
- `shares_router`, `recipients_router`, `providers_router` and `admin_router`
  bind the generated route functions, extracting `Cx` per request. Mount them
  under `ADMIN_PREFIX`.
- `auth::ProviderAuthenticator` and `auth::require_provider` give you
  provider bearer tokens, for hosts that want them. Any other scheme (OIDC,
  sessions, a gateway header) works as your own layer plus a `Cx` extractor.
- `Error` is Unity Catalog's handler error (`olai-uc-handlers`), rendered as
  the shared `{ "errorCode", "message" }` envelope.

The protos live in [mangrove](https://github.com/open-lakehouse/mangrove).
`opensharing-server` implements the share and recipient traits over its store.
A catalog can implement them over its own store instead.
