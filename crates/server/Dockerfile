# syntax=docker/dockerfile:1
#
# Build for the `sharing-server` binary: the deployable OpenSharing / Delta
# Sharing server. Co-located with the crate it builds; the build context is the
# REPO ROOT (`docker build -f crates/server/Dockerfile .`, or `just
# build-docker`), since the build needs the whole workspace and its Cargo.lock.
# The git-pinned Unity Catalog crates resolve from GitHub, so the build host
# needs outbound git access. cargo-chef caches the dependency graph as a
# separate layer so source edits don't trigger a full dependency rebuild.
#
# Base images are pinned by digest for reproducible, tamper-evident builds.
# Refresh a digest with: docker buildx imagetools inspect <image:tag>

# rust:1.99-bookworm (matches rust-toolchain.toml, which .dockerignore keeps out
# of the context so rustup does not fetch a second toolchain)
FROM rust@sha256:9181b56c2a21ab1d5e73cffe04fc762426d30aa7fac3c91141dd033f49a7fc8b AS chef

RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies first: this is the cached Docker layer that survives
# source-only changes. Scope to the server package + `bin` feature so a bare
# workspace build doesn't unify features across all members (which would pull
# in the napi addon of crates/neon).
RUN cargo chef cook --release --locked --recipe-path recipe.json \
    -p opensharing-server --features bin --bin sharing-server
# Build the application.
COPY . .
RUN cargo build --release --locked -p opensharing-server --features bin --bin sharing-server

# Minimal runtime: distroless cc (glibc + CA certs) for the dynamically-linked
# binary, nonroot. No shell/package manager, so the healthcheck below can't
# shell out to curl/wget: it runs the binary's own `healthcheck` subcommand.
# gcr.io/distroless/cc-debian12:nonroot
FROM gcr.io/distroless/cc-debian12@sha256:b0ae8e989418b458e0f25489bc3be523718938a2b70864cc0f6a00af1ddbd985 AS runtime

LABEL org.opencontainers.image.title="distributary" \
      org.opencontainers.image.description="Distributary: an OpenSharing / Delta Sharing server." \
      org.opencontainers.image.source="https://github.com/open-lakehouse/distributary" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.vendor="open-lakehouse"

COPY --from=builder /app/target/release/sharing-server /usr/local/bin/sharing-server
WORKDIR /app

# The server binds 0.0.0.0:8080 by default (`serve`). Without a config it runs
# on an in-memory store with an insecure development key. Mount a YAML config
# and point SHARING_SERVER_CONFIG at it, and/or set DATABASE_URL (Postgres, or
# a `sqlite:` URL) and SHARING_PROVIDER_TOKEN (see crates/server/README.md).
EXPOSE 8080

# Self-probe: the binary GETs its own /health and exits 0/1. Exec-form (JSON
# array) is REQUIRED: distroless has no shell for the string form. The probe
# reads the same config/env the server does, so it targets the right port.
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD ["/usr/local/bin/sharing-server", "healthcheck"]

USER nonroot

# ENTRYPOINT is just the binary; CMD supplies the default subcommand. Split this
# way so a CMD-less `docker run` starts the server (`serve`) while
# `docker run … migrate` / `… rotate-keys` (or a Compose `command:`) *replaces*
# CMD to pick the subcommand. NOTE: `serve` does NOT migrate a durable backend:
# run `sharing-server migrate` once before the first `serve` against a new
# database.
ENTRYPOINT ["/usr/local/bin/sharing-server"]
CMD ["serve"]
