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
- **No authentication** exists because no endpoint needs it. See ADR 0001 for
  the required shape when one does.
- Consider adding `cargo deny` (licences, duplicate and yanked crates) to CI.
