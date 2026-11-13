# Evidence — Phase 4: Secure multi-arch container image

Captured 2026-11-12 on macOS arm64, Docker 29.9.2 (containerd image store), buildx builder `desktop-linux`.

## Design

[`gitops-api/Dockerfile`](../../gitops-api/Dockerfile)

| Control | Implementation |
|---|---|
| Pinned inputs | Dockerfile frontend, `rust:1.99.0-slim-trixie` and `distroless/cc-debian13:nonroot` all pinned by **index digest**; `--locked` cargo build |
| No emulated compile | Builder runs on `$BUILDPLATFORM` and cross-compiles (`x86_64-linux-gnu-gcc` / `aarch64-linux-gnu-gcc`); cross packages installed only when `TARGETARCH != BUILDARCH` |
| Minimal runtime | distroless: glibc + libgcc + ca-certs, **no shell, no package manager** |
| Non-root | `USER 65532:65532` (numeric, so `runAsNonRoot` can verify) |
| Small context | `.dockerignore` is an allow-list: `Cargo.toml`, `Cargo.lock`, `src/` only |
| Traceability | `GIT_SHA` build-arg → compiled into `/version` and `org.opencontainers.image.revision` label |
| Signals | Binary is PID 1 (exec-form `ENTRYPOINT`) and handles SIGTERM itself |

## Lint

```
hadolint v2.15.1 (digest-pinned)  => exit 0
```

Two findings fixed/justified on the first run: SC2086 (now a bash array) and DL3008 (apt
version pinning — suppressed with an inline justification: builder-only packages on a
digest-pinned base; Debian drops old point versions from mirrors).

## Build

```
docker buildx build --platform linux/arm64,linux/amd64 \
  --build-arg GIT_SHA=local-phase4 --build-arg VERSION=0.1.0 -t gitops-api:phase4 --load .
=> exit 0, 155 s cold (cargo release build ~53 s per arch)
```

Build log shows the cross stage `[linux/arm64->amd64 builder ...]`, confirming amd64 was
cross-compiled natively, not run under QEMU.

```
IMAGE              ID             CONTENT SIZE
gitops-api:phase4  4bfc7038e9ec   23.3MB   (manifest list sha256:4bfc7038e9ec…fbea8f)
├─ linux/arm64     fb177331e1d4   11.8MB
└─ linux/amd64     a4495b0e9c60   11.5MB
```

Extracted binaries:

```
arm64: ELF 64-bit LSB pie executable, ARM aarch64 ... size=1.6M
amd64: ELF 64-bit LSB pie executable, x86-64 ...      size=1.7M
```

## Runtime verification (both architectures)

amd64 ran under Docker Desktop emulation; arm64 natively. Identical results:

```
GET /health  -> {"status":"ok"} [200]
GET /ready   -> {"status":"ready"} [200]
GET /version -> {"name":"gitops-api","version":"0.1.0","git_sha":"local-phase4","environment":"dev"} [200]
docker exec ... /bin/sh -> stat /bin/sh: no such file or directory   (no shell, as intended)
Config.User = 65532:65532
docker stop (SIGTERM) -> exit code 0, logs "shutdown signal received ... SIGTERM" then "shutdown complete"
```

Kubernetes-equivalent hardening, simulated with Docker:

```
docker run --read-only --cap-drop ALL --security-opt no-new-privileges ...
GET /ready -> {"status":"ready"} [200]; stop -> exit 0
```

## Vulnerability scan

Trivy 0.76.1 (digest-pinned), DB updated 2026-11-12 01:03 UTC, per-platform image tarballs:

```
gate: --severity HIGH,CRITICAL --ignore-unfixed --exit-code 1
  arm64 => exit 0 (0 findings)
  amd64 => exit 0 (0 findings)

all severities, including unfixed (both arches identical):
  MEDIUM 34, LOW 12, UNKNOWN 7 — every one has NO upstream fix
  packages: libc6, libgcc-s1, gcc-14-base, libstdc++6, libgomp1, libssl3t64 (from the distroless base)
```

## Known gaps (honest)

- **Rust dependency CVEs are not covered by this scan.** Trivy only saw the Debian layer;
  the binary is not built with `cargo-auditable`, so crates are invisible to it. Plan:
  `cargo audit` / `cargo deny` in CI (phase 8).
- The base carries libssl/libstdc++ the app does not use. Moving to a static musl binary on
  `distroless/static` would remove all 53 base findings — a possible hardening step.
- Images exist only locally; nothing pushed to GHCR yet (phase 8, needs authorization).
- No SBOM/provenance attestation was pushed (buildx produced local attestation manifests only); to add in CI.
