# gitops-api: service architecture

Scope: the inside of the Rust service. For the cluster, Argo CD and promotion
flow, see [`../../docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md).

## Layers

```
main.rs ──► bootstrap ──► app ──► modules ──► app::{state, error}
               │            │         │
               └────────────┴─────────┴──────► platform   (depends on nothing)
```

| Directory | Responsibility | Must not |
|---|---|---|
| `src/bootstrap/` | Read env → `Config`; build dependencies (`dependencies.rs` is the composition root); bind, serve, drain | contain request logic |
| `src/app/` | Router assembly, middleware stack, `AppState`, `ApiError`, shared bodies | know any feature's business rules |
| `src/modules/<name>/` | One feature / bounded context. Exposes only `routes()` (taking `&AppState` if it needs state for a `route_layer`) | read env, install global layers, build its own infrastructure |
| `src/platform/` | Telemetry, signal handling, build metadata | import anything else from the crate |

Rules that keep it maintainable:

1. **One way in per layer.** `main` calls `bootstrap::{init, run}`. Inside
   `app`, only `router` (mounts routes) and `state` (holds module services)
   name `modules`. A module is reached only through its `routes()`.
2. **Errors are typed per layer.** `bootstrap::Error` is a startup failure
   (stderr / exit code). `app::ApiError` is an HTTP response. A module's
   domain error converts into `ApiError` with `From`; it never builds a
   response itself.
3. **Dependencies are injected, not constructed.** Handlers get them from
   `AppState`, which `bootstrap::dependencies` fills. That is the seam tests
   use to substitute fakes.
4. **Cross-cutting behaviour lives in `app::middleware`**, applied once. Tests
   reuse the exact stack through `app::middleware::apply`.

## Request path

```
TCP ─► sensitive-header marking ─► x-request-id (sanitise / generate / echo)
    ─► trace span {method, path, request_id}
    ─► security headers ─► timeout (503) ─► body limit (413)
    ─► router ─► module handler ─► Result<T, ApiError>
```

## Module shapes

`system` is stateless, so it is just `routes.rs` + `handlers.rs`. `items` is
the reference for a stateful module
([ADR 0001](ADR/0001-modular-layered-architecture.md)):

```
modules/items/
├── mod.rs                     pub use transport::routes;
├── domain/                    Item, ItemId, ItemName, Description, ValidationError (no I/O, no axum)
├── application/
│   ├── ports.rs               ItemRepository trait, Page, RepositoryError
│   ├── service.rs             ItemService: create / get / list / replace / delete
│   └── error.rs               ItemError (use-case failures)
├── transport/                 routes (+ auth route_layer), handlers, DTOs, From<ItemError> for ApiError
└── infrastructure/
    └── memory_repository.rs   InMemoryItemRepository (implements ItemRepository)
```

Dependencies point inward: `transport` and `infrastructure` depend on
`application`, which depends on `domain`. Swapping storage means writing a
new `ItemRepository` and changing one line in `bootstrap/dependencies.rs`.

To add another module: copy the shape, add its service to `AppState` with a
`FromRef` impl, construct its adapters in `dependencies.rs`, and merge its
`routes()` in `modules/mod.rs`.

## Configuration

| Var | Default | Range |
|---|---|---|
| `PORT` | `8080` | 1–65535 |
| `APP_ENV` | `local` | `[a-z0-9-]+` |
| `LOG_FORMAT` | `json` | `json` \| `pretty` |
| `RUST_LOG` | `info` | `EnvFilter` syntax (invalid → `info` + warning) |
| `REQUEST_TIMEOUT_SECS` | `10` | 1–60 |
| `MAX_REQUEST_BODY_BYTES` | `65536` | 1–16777216 |
| `API_KEYS` | unset: protected routes return 401 | 1–8 comma-separated keys, each 32–256 printable ASCII characters. **Secret** |

A set-but-invalid value stops the process at start-up; it never falls back
silently.

## Tests

| File | Covers |
|---|---|
| unit tests in `src/` | config, log format, request-id and bearer parsing, API-key verification and redaction, `ApiError` bodies, items domain rules, service use cases, repository pagination |
| `tests/items_api.rs` | `/items` over HTTP: auth matrix, CRUD lifecycle, pagination, conflicts, every 4xx path, 500 via a broken-storage fake, 507 |
| `tests/api_contract.rs` | the `/health`, `/ready`, `/version`, 404, 405 contract that `gitops-config` depends on |
| `tests/security.rs` | security headers, request-id handling, body limit, timeout |
| `tests/server_lifecycle.rs` | real TCP: serve, readiness on/off, graceful drain |
