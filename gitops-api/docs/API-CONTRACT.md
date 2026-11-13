# gitops-api: HTTP contract

Consumed by Kubernetes probes and promotion scripts in `gitops-config`.
Pinned by `tests/api_contract.rs`. Changing anything here is a breaking change.

## Endpoints

| Method + path | 200 body | Non-200 |
|---|---|---|
| `GET /health` | `{"status":"ok"}` | none |
| `GET /ready` | `{"status":"ready"}` | 503 `{"status":"not_ready"}` while starting or draining |
| `GET /version` | `{"name","version","git_sha","environment"}` | none |

## Errors

All handler and routing errors use one shape: `{"status":"<code>"}`.

| Code | HTTP |
|---|---|
| `not_found` | 404 |
| `method_not_allowed` | 405 |
| `not_ready` | 503 |
| `internal_error` | 500 |

Middleware rejections (`413` body too large, `503` timeout) carry the right
status and headers but no JSON body.

## Headers on every response

- `x-request-id`: echoed if the client sent one valid value, otherwise a new UUID v4
- `x-content-type-options`, `x-frame-options`, `content-security-policy`,
  `referrer-policy`, `cache-control`, `cross-origin-resource-policy` (see `SECURITY.md`)
