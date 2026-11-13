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

## Source layout

```
src/
├── main.rs         entry point: load config, init logging, run, map to exit code
├── lib.rs          module map and public re-exports
├── config.rs       environment variables → validated Config
├── telemetry.rs    global tracing subscriber
├── server.rs       bind, serve, readiness on/off, drain
├── shutdown.rs     wait for SIGTERM / SIGINT
├── state.rs        AppState shared by handlers
├── build_info.rs   compile-time name / version / git_sha
├── error.rs        top-level Error returned to main
└── routes/
    ├── mod.rs      router assembly, trace layer, 404 fallback
    ├── health.rs   /health, /ready
    ├── version.rs  /version
    └── response.rs shared response bodies
```

To add an endpoint, create `routes/<resource>.rs` with a `router()` function and merge it in `routes/mod.rs`.

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
