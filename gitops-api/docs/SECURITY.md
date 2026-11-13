# gitops-api: security controls

What the service itself enforces, what it leaves to the platform, and what is
still open. Platform and pipeline controls (SEC-001..) are tracked in
[`../../docs/evidence/`](../../docs/evidence/).

## Enforced in the service

| Control | Where | Test |
|---|---|---|
| No `unsafe` code | `Cargo.toml` `[lints.rust] unsafe_code = "forbid"` | compile |
| Config validated at start-up, fail closed | `bootstrap/config.rs` | unit |
| Errors never expose internals: 500 body is `{"status":"internal_error"}`, cause is logged | `app/error.rs` | `internal_error_never_leaks_its_cause` |
| Untrusted `x-request-id` validated (`[A-Za-z0-9._-]{1,128}`, exactly one) or replaced | `app/middleware/request_id.rs` | `tests/security.rs` |
| Logs record path but not query string or headers; `Authorization`, `Cookie`, `Set-Cookie`, `Proxy-Authorization` marked sensitive | `app/middleware/mod.rs` | review |
| Security headers on every response, errors included: `nosniff`, `X-Frame-Options: DENY`, `CSP: default-src 'none'`, `Referrer-Policy: no-referrer`, `Cache-Control: no-store`, `CORP: same-origin` | `app/middleware/security.rs` | `tests/security.rs` |
| Request body limit (default 64 KiB) → 413 | `app/middleware/mod.rs` | `oversized_body_is_rejected_with_413` |
| Handler timeout (default 10 s) → 503; also bounds drain time | `app/middleware/mod.rs` | `slow_handler_is_cut_off_with_503` |
| Protected routes need `Authorization: Bearer <key>`. Keys are kept only as SHA-256 digests and compared in constant time against every configured key. They are never logged or echoed (`Debug` is redacted; config errors don't print the value). Several keys can be configured for rotation | `platform/security/api_key.rs`, `app/middleware/authentication.rs` | unit + `tests/items_api.rs` |
| Fails closed: if `API_KEYS` is unset, every protected route returns 401; if it is set but invalid, the process won't start | `bootstrap/config.rs`, `ApiKeys::none` | `no_configured_keys_means_nothing_gets_in` |
| Auth runs before any extractor (`route_layer`), so an unauthenticated body is never parsed | `modules/items/transport/routes.rs` | `unauthenticated_body_is_never_parsed` |
| Missing, malformed and wrong credentials get the same 401 | `authentication.rs` | `every_items_route_requires_a_valid_key` |
| Strict input: `deny_unknown_fields` (no mass-assignment of `id`), length and character limits on every field, server-generated ids | `items/transport/dto.rs`, `items/domain` | `schema_violations_are_422`, domain unit tests |
| Bounded memory: the item store is capped (10 000 items) and returns 507 instead of growing until OOM | `items/infrastructure/memory_repository.rs` | `full_store_is_507` |
| Storage errors are 500 with no detail, even when the cause contains credentials | `items/transport/error.rs`, `app/error.rs` | `storage_failures_are_500_without_leaking_details` |
| Graceful drain: `/ready` → 503 on SIGTERM before connections close | `bootstrap/server.rs` | `tests/server_lifecycle.rs` |

## Left to the platform (by design)

| Concern | Owner |
|---|---|
| TLS, HSTS | ingress / gateway |
| Rate limiting, connection limits, slow-client (slowloris) protection | ingress / gateway |
| Network reachability | `NetworkPolicy` in `gitops-config` |
| Non-root, read-only rootfs, no capabilities, seccomp | Pod `securityContext` + distroless image |
| Secret delivery | Sealed Secrets → env, never baked into the image |
| Dependency CVEs, secret scanning, image signing | CI (`trivy`, `gitleaks`, build provenance attestation) |

## Known gaps / notes

- **Panics abort the process** (`panic = "abort"` in release). A panicking
  handler kills the pod and Kubernetes restarts it; there is no
  `CatchPanicLayer` because it would have no effect under `abort`.
- **413 and timeout 503 bodies are not JSON.** They come from `tower-http`
  with an empty / plain body. Status codes and security headers are correct.
- **One trust level.** Any valid key can do anything on `/items`. Add scopes
  (e.g. read vs write) before giving keys to clients with different trust.
- **No rate limiting in the service**, so brute-forcing keys is limited only by
  the ingress. With 32+ character keys that is not practical, but add ingress
  rate limits for any public exposure.
- **The in-memory store is per pod.** With more than one replica, each pod has
  different data. Fine for a demo; replace it with a database adapter before
  relying on the data.
- Consider adding `cargo deny` (licences, duplicate and yanked crates) to CI.
