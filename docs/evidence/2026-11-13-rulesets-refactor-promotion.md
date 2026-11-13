# 2026-11-13: refactor PR, image-updater App, rulesets applied

## Rust refactor (task 1): gitops-api#1

Branch `refactor/modular-layout` (`52daddf`). CI on the PR: `test`, `scan-source`, `image`, GitGuardian all pass.

Black-box comparison: `main` and the branch built with the same `GIT_SHA`, run with `APP_ENV=staging`, same requests:

| Request | main | branch |
|---|---|---|
| `GET /health` | 200 `{"status":"ok"}` | identical |
| `GET /ready` | 200 `{"status":"ready"}` | identical |
| `GET /version` | 200 `{"name":"gitops-api","version":"0.1.0","git_sha":"same","environment":"staging"}` | identical |
| `GET /nope`, `GET /version/` | 404 `{"status":"not_found"}` | identical |
| `GET /health?x=1`, `HEAD /health` | 200 | identical |
| `POST /health`, `PUT /ready`, `DELETE /version` | 405, empty body, `Allow: GET,HEAD` | 405, `{"status":"method_not_allowed"}`, `Allow: GET,HEAD` |

Additive only: security headers and `x-request-id` on every response. `/items` and auth are in the separate draft gitops-api#2.

## Image-updater (task 2): gitops-config#5 on top of #4

- Last 3 runs failed with `! [rejected] ci/dev-28ae24b (stale info)` (B5b, fixed by #4). The repo token also cannot create PRs: `can_approve_pull_request_reviews: false` (B5a).
- #5 mints a GitHub App token per run (`actions/create-github-app-token@64ab729` v3.3.0, `client-id` input), scoped to gitops-config with contents + pull-requests write. `GITHUB_TOKEN` drops to `contents: read`. The manual `validate` dispatch is removed. If the App is missing, the run fails instead of falling back.
- `actionlint` 1.7.7 passes; `validate` passes on #4 and #5.
- **Open:** the owner creates the App and sets `IMAGE_UPDATER_CLIENT_ID` + `IMAGE_UPDATER_APP_KEY`; then a real run must be verified.

## Rulesets (task 3): applied with `docs/governance/rulesets.sh apply`

```
## Dapravith/gitops-api: ruleset "main: PR + ci" does not exist -> create
created id 27191039
## Dapravith/gitops-config: "main: PR + validate" (#27097700) differs -> update
updated id 27097700
## Dapravith/GitOps-Multi-Deployment: ruleset "main: PR" does not exist -> create
created id 27191040
```

`rulesets.sh plan` right after: all three `matches`, exit 0. Effective rules on `main` (`GET /rules/branches/main`):

```
gitops-api: deletion non_fast_forward pull_request(approvals=0) required_status_checks(test,scan-source,image; strict=true)
gitops-config: deletion non_fast_forward pull_request(approvals=0) required_status_checks(validate; strict=true)
GitOps-Multi-Deployment: deletion non_fast_forward pull_request(approvals=0)
```

No bypass actors. 0 approvals by owner decision (one account). gitops-api#1, gitops-config#4 and #5 all report `MERGEABLE CLEAN` under the new rules.

Not yet tested (SEC-009): negative tests (a direct push to `main` is rejected; a PR with a failing check can't merge).
