# gitops-api

Minimal Rust/Axum service deployed through the `gitops-config` repository by Argo CD.
See [`../docs/ARCHITECTURE.md`](../docs/ARCHITECTURE.md) for the full picture.

## Endpoints

| Path | 200 when | Other |
|---|---|---|
| `GET /health` | process is serving | — |
| `GET /ready` | accepting traffic | 503 while starting/draining |
| `GET /version` | always | JSON: name, version, git_sha, environment |

## Configuration

| Var | Default | Notes |
|---|---|---|
| `PORT` | `8080` | 1–65535 |
| `APP_ENV` | `local` | `[a-z0-9-]+` |
| `LOG_FORMAT` | `json` | `json` or `pretty` |
| `RUST_LOG` | `info` | standard `EnvFilter` syntax |
| `REQUEST_TIMEOUT_SECS` | `10` | 1–60; slower handlers get 503 |
| `MAX_REQUEST_BODY_BYTES` | `65536` | 1–16 MiB; larger bodies get 413 |

See [`.env.example`](.env.example).

## Source layout

```
src/
├── main.rs                 calls bootstrap::init, then bootstrap::run
├── lib.rs                  layer map and public re-exports
├── bootstrap/              config → wiring → bind / serve / drain
│   ├── config.rs           environment variables → validated Config
│   ├── dependencies.rs     composition root: Config → AppState
│   ├── server.rs           bind, serve, readiness on/off, drain
│   └── error.rs            startup / fatal Error
├── app/                    HTTP kernel shared by every module
│   ├── router.rs           mounts modules, JSON 404/405 fallbacks, middleware
│   ├── state.rs            AppState (dependency container)
│   ├── error.rs            ApiError → status + {"status": code}
│   ├── response.rs         StatusBody
│   └── middleware/         request id, security headers, timeout, body limit, trace
├── modules/                one directory per feature
│   └── system/             /health, /ready, /version
└── platform/               no deps on the rest of the crate
    ├── telemetry.rs        tracing subscriber, LogFormat
    ├── shutdown.rs         SIGTERM / SIGINT
    └── build_info.rs       compile-time name / version / git_sha
```

To add a feature, create `modules/<name>/` exposing `routes()` and merge it in `modules/mod.rs`.
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) explains the layers and how a stateful module grows;
[`docs/ADR/0001`](docs/ADR/0001-modular-layered-architecture.md) records why, and what was left out on purpose.
Security controls: [`docs/SECURITY.md`](docs/SECURITY.md). HTTP contract: [`docs/API-CONTRACT.md`](docs/API-CONTRACT.md).

## Develop

Toolchain is pinned in `rust-toolchain.toml` (1.99.0). With rustup installed, use `cargo` directly.
Without it, `scripts/cargo.sh` runs the same toolchain in Docker:

```bash
scripts/cargo.sh fmt --all -- --check
```

```bash
scripts/cargo.sh clippy --all-targets --locked -- -D warnings
```

```bash
scripts/cargo.sh test --locked
```

```bash
PUBLISH=1 LOG_FORMAT=pretty scripts/cargo.sh run
```

## Container image

Multi-stage, cross-compiled (no QEMU for rustc), distroless, non-root (65532). Base images are digest-pinned in the `Dockerfile`.

```bash
docker buildx build --platform linux/arm64,linux/amd64 --build-arg GIT_SHA="$(git rev-parse HEAD 2>/dev/null || echo unknown)" -t gitops-api:dev --load .
```

```bash
docker run --rm --read-only --cap-drop ALL -p 127.0.0.1:8080:8080 -e APP_ENV=local gitops-api:dev
```
