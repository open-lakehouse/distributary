# Task runner for distributary. Run `just` to list the recipes.
#
# Code generation needs the `trestle` CLI (built from a `../trestle` source
# checkout, see README), `buf`, `uv` and `bunx`. The fast edit/check loop
# (`cargo check`, `cargo test`) needs none of this tooling.

set dotenv-load

# the Open Sharing server as a Neon Function
mod neon 'crates/neon/justfile'

# show the available recipes
_default:
    @just --list

# run the full code-generation pipeline (proto models + openapi + trestle code)
[group('codegen')]
generate: generate-proto generate-code
    just fix

# generate the buffa models and the OpenAPI spec from proto/
[group('codegen')]
generate-proto:
    buf generate proto
    just generate-openapi

# generate the OpenAPI spec from proto/ and merge the hand-written NDJSON paths
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

# patch the trestle crates (olai-*) from ../trestle (once per clone behind a crates.io proxy)
[group('dev')]
configure-trestle-deps:
    dev/scripts/configure-trestle-deps.sh

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
    cargo run -p olai-sharing-server --features bin -- migrate
