# syntax=docker/dockerfile:1@sha256:4edf897a3ffa55b89f906fc8cc78afdb3f1834cc9c7083565e611a8a7d5fe99e
#
# Multi-stage, multi-arch build for gitops-api.
#
# * The builder always runs on the *build host's* native platform and
#   cross-compiles to TARGETARCH, so an amd64 image built on an arm64 Mac (or
#   vice versa) does not run rustc under QEMU emulation.
# * Base images are pinned by digest; bump them deliberately via PR.
# * Runtime is distroless (glibc + libgcc, no shell, no package manager) and
#   runs as the distroless `nonroot` user (65532).
#
# Build:  docker buildx build --platform linux/arm64,linux/amd64 \
#           --build-arg GIT_SHA=$(git rev-parse HEAD) -t gitops-api:dev .

ARG RUST_IMAGE=rust:1.99.0-slim-trixie@sha256:2752b332db73fdbb7dc576f06c82ed1f312005784ef913d7e04a28f5f55dc581
ARG RUNTIME_IMAGE=gcr.io/distroless/cc-debian13:nonroot@sha256:cc66d222f0a7ec996b78a99fe76a969bc80b539a5016ad866c14d1949bc6d0ab

# ---------------------------------------------------------------------------
FROM --platform=$BUILDPLATFORM ${RUST_IMAGE} AS builder

ARG TARGETARCH
ARG BUILDARCH

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# Map Docker arch -> Rust target triple; install a cross linker + target libc
# only when actually cross-compiling. apt versions are not pinned (DL3008): the
# base image is digest-pinned, Debian drops old point versions from mirrors, and
# these packages exist only in the builder stage, never in the shipped image.
# hadolint ignore=DL3008
RUN case "${TARGETARCH}" in \
      amd64) triple=x86_64-unknown-linux-gnu;  cross_pkgs=(gcc-x86-64-linux-gnu libc6-dev-amd64-cross) ;; \
      arm64) triple=aarch64-unknown-linux-gnu; cross_pkgs=(gcc-aarch64-linux-gnu libc6-dev-arm64-cross) ;; \
      *) echo "unsupported TARGETARCH=${TARGETARCH}" >&2; exit 1 ;; \
    esac \
 && echo "${triple}" > /rust-target \
 && if [ "${TARGETARCH}" != "${BUILDARCH}" ]; then \
      apt-get update \
      && apt-get install -y --no-install-recommends "${cross_pkgs[@]}" \
      && rm -rf /var/lib/apt/lists/*; \
    fi \
 && rustup target add "${triple}"

# Debian ships triplet-prefixed gcc names both natively and for cross toolchains.
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CARGO_TERM_COLOR=never

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Baked into /version via option_env!("GIT_SHA").
ARG GIT_SHA=unknown

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/build/target,id=gitops-api-target-${TARGETARCH},sharing=locked \
    triple="$(cat /rust-target)" \
 && GIT_SHA="${GIT_SHA}" cargo build --release --locked --target "${triple}" \
 && install -D -m 0555 "target/${triple}/release/gitops-api" /out/gitops-api

# ---------------------------------------------------------------------------
FROM ${RUNTIME_IMAGE} AS runtime

ARG GIT_SHA=unknown
ARG VERSION=0.1.0
LABEL org.opencontainers.image.title="gitops-api" \
      org.opencontainers.image.description="Minimal Axum API for a multi-environment GitOps workflow" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${GIT_SHA}" \
      org.opencontainers.image.licenses="MIT"

COPY --from=builder /out/gitops-api /usr/local/bin/gitops-api

ENV LOG_FORMAT=json \
    PORT=8080

# Numeric UID so Kubernetes `runAsNonRoot` can verify it without /etc/passwd lookups.
USER 65532:65532
EXPOSE 8080

# No HEALTHCHECK: there is no shell/curl in distroless; Kubernetes probes
# (/health, /ready) are the health mechanism.
ENTRYPOINT ["/usr/local/bin/gitops-api"]
