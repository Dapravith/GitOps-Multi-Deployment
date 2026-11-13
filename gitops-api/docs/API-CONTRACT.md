# gitops-api: HTTP contract

Consumed by Kubernetes probes and promotion scripts in `gitops-config`.
Pinned by `tests/api_contract.rs`. Changing anything here is a breaking change.

## Endpoints

| Method + path | 200 body | Non-200 |
|---|---|---|
| `GET /health` | `{"status":"ok"}` | none |
| `GET /ready` | `{"status":"ready"}` | 503 `{"status":"not_ready"}` while starting or draining |
| `GET /version` | `{"name","version","git_sha","environment"}` | none |

`/health`, `/ready` and `/version` are public. Everything under `/items` requires a key.

## Items (authenticated)

Send `Authorization: Bearer <key>`, where the key is one of those in `API_KEYS`.
Missing, malformed and wrong keys all get the same
`401 {"status":"unauthorized"}` and `WWW-Authenticate: Bearer realm="gitops-api"`.

| Method + path | Body | Success |
|---|---|---|
| `POST /items` | `{"name", "description"?}` | `201` + `Location: /items/{id}` + item |
| `GET /items?limit=&offset=` | none | `200 {"items":[...], "total", "limit", "offset"}` |
| `GET /items/{id}` | none | `200` item |
| `PUT /items/{id}` | `{"name", "description"?}` (full replace) | `200` item |
| `DELETE /items/{id}` | none | `204` |

Item: `{"id": "<uuid>", "name": "...", "description": "..."}`.

Rules:

- `id` is a server-generated UUID; a client cannot set it.
- `name`: trimmed, 1–100 characters, no control characters, unique (case-insensitive).
- `description`: 0–1000 characters; newline and tab are allowed, other control characters are not. Defaults to `""`.
- Unknown body or query fields are rejected.
- `limit`: 1–100, default 20. `offset`: default 0. Results are ordered by `id`.
- Storage is in-memory, per replica, and is lost on restart.

## Errors

All errors use one shape: `{"status":"<code>", "message"?: "<detail>"}`.
`message` appears only where it helps the client fix the request. It is
always written by the service and never contains internal detail.

| Code | HTTP | When |
|---|---|---|
| `bad_request` | 400 | body is not JSON; query string can't be decoded |
| `unauthorized` | 401 | no or wrong API key |
| `not_found` | 404 | unknown route or item |
| `method_not_allowed` | 405 | |
| `conflict` | 409 | name already used |
| `payload_too_large` | 413 | body over `MAX_REQUEST_BODY_BYTES` (read by the JSON extractor) |
| `unsupported_media_type` | 415 | body sent without `Content-Type: application/json` |
| `validation_failed` | 422 | domain rule broken, wrong field types, unknown fields, bad id, bad `limit` |
| `internal_error` | 500 | storage failure (cause logged with `request_id`) |
| `not_ready` | 503 | `/ready` while starting or draining |
| `insufficient_storage` | 507 | item store full (10 000) |

Middleware rejections (`413` body too large, `503` timeout) carry the right
status and headers but no JSON body.

## Headers on every response

- `x-request-id`: echoed if the client sent one valid value, otherwise a new UUID v4
- `x-content-type-options`, `x-frame-options`, `content-security-policy`,
  `referrer-policy`, `cache-control`, `cross-origin-resource-policy` (see `SECURITY.md`)
