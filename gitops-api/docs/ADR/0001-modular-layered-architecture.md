# ADR 0001: Modular, layered service layout

- Status: accepted
- Date: 2026-11-13

## Context

`gitops-api` began as a flat crate (`config.rs`, `server.rs`, `routes/`). That
was right for three endpoints, but it gave no answer to "where does a new
feature, its storage, or its auth go?" and every new concern would have
landed in `routes/` or `server.rs`.

We want a layout that:

- lets a feature be added, read and deleted as one directory;
- keeps HTTP, domain rules and infrastructure separable, so domain logic is
  unit-testable without a server or a database;
- applies security behaviour once, for every route, by default;
- does not add abstraction the current code does not use.

## Decision

Four top-level layers (`bootstrap`, `app`, `modules`, `platform`) with
one-directional dependencies, as described in `docs/ARCHITECTURE.md`.

Feature modules start minimal (`routes.rs` + `handlers.rs`) and adopt the
`domain / application / transport / infrastructure` split only when they gain
state or business rules. Ports (traits) live in `application/ports.rs` and are
implemented in `infrastructure/`. Adapters are wired only in
`bootstrap/dependencies.rs`.

## Deliberately not built yet

These appeared in the proposed tree but have no caller today. Each would add
dependencies, attack surface and code to maintain with nothing exercising it:

| Item | Add it when | Notes for whoever adds it |
|---|---|---|
| `modules/users/`, `migrations/` | a real user feature is specified | Use `sqlx` with compile-time-checked queries; run migrations as a separate Job/init step, not on every pod start |
| `platform/persistence/pool.rs` | the first module needs a DB | Pool built in `dependencies.rs`; `/ready` must then check it; credentials via SealedSecret, never env defaults |
| `app/middleware/authentication.rs`, `platform/security/{jwt,identity,authorization}.rs` | there is an identity provider and a protected route | Verify JWTs against the IdP's JWKS (pin `alg`, check `iss`/`aud`/`exp`); expose an `Identity` extractor; apply with `route_layer` on protected routers only. `/health`, `/ready`, `/version` stay public |
| `shared/identifiers.rs` | two modules share an id type | Newtypes over `Uuid` with `serde(transparent)` |
| `benches/` | there is a hot path worth measuring | `criterion` |

## Consequences

- Positive: each concern has one obvious home; middleware cannot be forgotten
  on a new route; the public API (`/health`, `/ready`, `/version`, error
  bodies, env vars) is pinned by contract tests.
- Negative: more files than the flat layout for the same three endpoints.
  `app::router` and `modules` reference each other (router mounts modules,
  modules use `app::state`). That is accepted and confined to one function.
- The HTTP contract is unchanged except that `405` responses now have the same
  JSON body shape as other errors, and every response carries
  `x-request-id` and the security headers.
