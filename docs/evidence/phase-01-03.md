# Evidence — Phases 1–3

Captured 2026-11-12 on macOS (Darwin 27.2.0, arm64). All commands run for real; output trimmed only where marked.

## Phase 1 — Environment

| Tool | Result |
|---|---|
| docker | 29.9.2, engine aarch64; buildx builder `desktop-linux` supports linux/arm64 + linux/amd64 |
| kind | 0.33.0 |
| kubectl | v1.36.1 |
| gh | 2.102.1, authenticated (account `Dapravith`, ssh protocol) |
| git | 2.56.0 |
| rustc / cargo / rustup | **missing on host** → toolchain runs in `rust:1.99.0-slim-trixie` via `gitops-api/scripts/cargo.sh` |
| kustomize, argocd, kubeseal, trivy, helm | **missing** (needed from phase 6/7/8) |

Pinned toolchain inside container:

```
rustc 1.99.0 (b940084d7 2026-09-28)
cargo 1.99.0 (5f94df478 2026-08-27)
```

Image digest pulled: `rust:1.99.0-slim-trixie@sha256:2752b332db73fdbb7dc576f06c82ed1f312005784ef913d7e04a28f5f55dc581`

## Phases 2–3 — Quality gates

```
cargo fmt --all -- --check                      => exit 0
cargo clippy --all-targets --locked -- -D warnings  => exit 0   (clippy::pedantic enabled)
cargo test --locked                             => exit 0
```

Test summary:

```
unittests src/lib.rs   5 passed  (config parsing: defaults, all values, bad port/env/log format)
tests/api.rs           6 passed  (health, ready state transitions, version, JSON 404, 405,
                                  real TCP server + graceful shutdown + listener closed)
```

## Phase 2–3 — Runtime smoke test

Release binary run in `debian:trixie-slim`, `APP_ENV=dev`, port published on 127.0.0.1:18080:

```
GET /health  -> 200 {"status":"ok"}
GET /ready   -> 200 {"status":"ready"}
GET /version -> 200 {"name":"gitops-api","version":"0.1.0","git_sha":"unknown","environment":"dev"}
GET /missing -> 404 {"status":"not_found"}
```

`docker stop -t 10` (SIGTERM to PID 1) → container exit code **0**. Logs (JSON, one per line):

```
{"level":"INFO","message":"server listening","addr":"0.0.0.0:8080","environment":"dev","version":"0.1.0","git_sha":"unknown",...}
{"level":"INFO","message":"finished processing request","latency":"0 ms","status":200,"span":{"method":"GET","uri":"/health",...}}
... (3 more request lines)
{"level":"INFO","message":"shutdown signal received, draining connections","signal":"SIGTERM",...}
{"level":"INFO","message":"shutdown complete",...}
```

Fail-fast config: `PORT=0` →

```
configuration error: PORT must be an integer between 1 and 65535, got "0"
exit=1
```

## Known gaps

- `git_sha` is `unknown` locally; CI will inject it at build time (phase 4/8).
- The "draining" window under real load is not yet measured; to be tested in-cluster with
  a readiness probe + `preStop` sleep in phase 13.
