#!/usr/bin/env bash
# Run cargo inside the pinned Rust toolchain container, so the host needs only Docker.
# Usage: scripts/cargo.sh test | scripts/cargo.sh clippy --all-targets -- -D warnings
# Set PUBLISH=1 to expose port 8080 (for `scripts/cargo.sh run`).
set -euo pipefail

RUST_IMAGE="${RUST_IMAGE:-rust:1.99.0-slim-trixie}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

port_args=()
if [[ "${PUBLISH:-0}" == "1" ]]; then
  port_args=(-p 127.0.0.1:8080:8080)
fi

exec docker run --rm ${TTY:+-it} \
  "${port_args[@]+"${port_args[@]}"}" \
  -v "${repo_root}:/work" -w /work \
  -v gitops-api-rustup:/usr/local/rustup \
  -v gitops-api-cargo-registry:/usr/local/cargo/registry \
  -v gitops-api-target:/target \
  -e CARGO_TARGET_DIR=/target \
  -e RUST_LOG -e LOG_FORMAT -e APP_ENV -e PORT -e REQUEST_TIMEOUT_SECS -e MAX_REQUEST_BODY_BYTES \
  "${RUST_IMAGE}" cargo "$@"
