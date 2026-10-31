# opensharing-provider

The OpenSharing **provider** domain, as a library. A provider maps shares onto
assets in source catalogs, issues recipients their bearer tokens and grants them
shares. This crate holds that state and its logic:

- `store`: shares (lists of `ShareObject`s naming source-catalog assets),
  recipients, tokens, grants and sealed storage credentials, behind targeted
  per-resource traits, with an in-memory implementation;
- `secrets`: envelope encryption with lazy key rotation;
- `recipients`: recipient tokens with one-time activation links;
- `admin` (feature `admin`, on by default): `ProviderService`, which
  implements the share and recipient handler traits of
  [`opensharing-admin-api`](../admin-api), authorized by an `AdminPolicy`.

## Plugging into the protocol

The provider owns the recipients, so it supplies what the recipient surface of
[`opensharing-engine`](../engine) needs from them, generic over the host's
request context:

| Piece | Implements | Over |
|---|---|---|
| `recipients::Recipient` | the context of the standalone server | |
| `RecipientIdentity` | implemented by a host for its own context | |
| `recipients::StoreAuthenticator` | `BearerAuthenticator<Recipient>` | `TokenReader` |
| `catalog::StoreCatalog` | `ShareCatalog<Cx>` | `ShareReader` + `SourceRegistry` |
| `catalog::GrantPolicy` | `SharingPolicy<Cx>` | `ShareReader` grants |
| `credentials::StoreCredentialResolver` | trestle `CredentialResolver` | `CredentialStore` + `Secrets` |

`sharing_service` wires them into a `SharingService`. A catalog that keeps
shares and recipients in its own store implements only the read-only
`ShareReader` and `TokenReader` views and reuses the rest
(`tests/host_store.rs`, which also uses its own context type).

## Stores

The state is split into targeted traits rather than one generic resource store:

- `ShareStore` holds shares and grants; its read-only view is `ShareReader`.
- `RecipientStore` holds recipients and tokens; its read-only view is
  `TokenReader`.
- `CredentialStore` holds sealed storage credentials and external locations.
- `SharingStore` bundles all three.

Every implementation runs the conformance suite `testing::round_trips`
(feature `testing`). The SQL implementations live outside this crate, so it
does not depend on sqlx.
