# Task runner for distributary. Run `just` to list the recipes.
#
# Code generation needs the `trestle` CLI (`brew install
# open-lakehouse/tap/trestle`), `buf`, `uv` and `bunx`. The fast edit/check loop
# (`cargo check`, `cargo test`) needs none of this tooling.

set dotenv-load

# the OpenSharing server as a Neon Function
mod neon 'crates/neon/justfile'

# show the available recipes
_default:
    @just --list

# run the full code-generation pipeline (proto models + openapi + trestle code)
[group('codegen')]
generate: generate-proto generate-code
    just fix

# generate the buffa models and the OpenAPI specs from proto/
[group('codegen')]
generate-proto:
    buf generate proto
    just generate-openapi

# generate the OpenAPI spec of the protocol, merged with the hand-written NDJSON paths
[group('codegen')]
generate-openapi:
    mkdir -p openapi/sharing-gen
    buf generate proto --template buf.gen.openapi.yaml
    uv run --with pyyaml python3 dev/scripts/merge_sharing_openapi.py \
      openapi/sharing-gen/openapi.yaml \
      openapi/sharing-query-paths.yaml \
      openapi/sharing.yaml
    rm -rf openapi/sharing-gen
    bunx @redocly/cli bundle openapi/sharing.yaml > /dev/null

# The NDJSON table query RPCs are intentionally excluded from the proto service
# and implemented by hand (see `crates/common/src/query_extractors.rs`).
#
# generate server/client/extractor code from proto/ (run after `buf generate`)
[group('codegen')]
generate-code:
    buf build --output descriptors.bin proto
    trestle generate --config trestle.yaml --descriptors descriptors.bin
    rm descriptors.bin
    just fmt

# patch a sibling's crates (e.g. `trestle`, `mangrove`) from its checkout while the crates.io proxy hides a fresh release
[group('dev')]
patch-deps upstream:
    dev/scripts/crates-proxy.sh patch {{ upstream }}

# drop a sibling's local patch again
[group('dev')]
unpatch-deps upstream:
    dev/scripts/crates-proxy.sh patch --remove {{ upstream }}

# shortcut for `patch-deps trestle`
[group('dev')]
configure-trestle-deps: (patch-deps "trestle")

# restore crates.io source/checksum lines in Cargo.lock after building under the patch
[group('dev')]
lock-registry:
    dev/scripts/crates-proxy.sh lock-registry

# fail if Cargo.lock still resolves a crate from a local [patch]
[group('dev')]
check-lock:
    dev/scripts/crates-proxy.sh check-lock

# format all code (Rust, protobuf, Python)
[group('meta')]
fmt:
    cargo fmt
    buf format proto/ --write
    uvx ruff format dev

# auto-fix Rust and Python lint issues, then format
[group('meta')]
fix:
    cargo clippy --fix --workspace --all-features --allow-dirty --allow-staged
    uvx ruff check --fix dev
    just fmt

# apply the sharing-server migrations to DATABASE_URL
[group('dev')]
migrate:
    cargo run -p opensharing-server --features bin -- migrate

# Set CRATES_PROXY (a sparse registry URL) to build without direct crates.io access.
#
# build the server Docker image (distributary); context = repo root
[group('build')]
build-docker:
    docker build -f crates/server/Dockerfile --build-arg CRATES_PROXY -t distributary:dev .
