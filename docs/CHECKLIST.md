# Implementation Checklist

Status legend: ✅ done and verified (evidence linked) · 🟡 partial · ⬜ not started.
An item is only ✅ when a real command output backs it up.

| # | Phase | Status | Evidence / notes |
|---|---|---|---|
| — | Sealed Secrets (brief: "for sensitive configuration") | ✅ | [evidence/sealed-secrets.md](evidence/sealed-secrets.md) — Argo CD-managed v0.41.2, namespace-scoped RBAC, offline sealing, scope/tamper tests fail closed, key backup + restore drill passed; gitleaks in validate.sh. Prod not enabled |
| 1 | Inspect environment, define architecture | ✅ | [evidence/phase-01-03.md](evidence/phase-01-03.md), [ARCHITECTURE.md](ARCHITECTURE.md). Current design: [design/HLD.md](design/HLD.md), [design/LLD.md](design/LLD.md), [design/IMPLEMENTATION-GUIDE.md](design/IMPLEMENTATION-GUIDE.md) |
| 2 | Rust Axum API: `/health`, `/ready`, `/version` | ✅ | curl smoke test in evidence |
| 3 | Unit tests, structured logging, graceful shutdown | ✅ | 15 tests pass at `9432470` (11 in phase 3); SIGTERM → exit 0 |
| 4 | Secure multi-stage Dockerfile, linux/arm64 + linux/amd64 | ✅ | [evidence/phase-04.md](evidence/phase-04.md) — both arches run; Trivy gate clean; crate CVE scan still pending (phase 8) |
| 5 | kind cluster | ✅ | [evidence/phase-05.md](evidence/phase-05.md) — `gitops-lab` v1.36.4, single node; PSA restricted verified; kubeconfig `~/.kube/gitops-lab.kubeconfig`. Recreated in phase 6 with a local registry acting as ghcr.io mirror |
| 6 | Kustomize base + dev/staging/prod overlays | ✅ | [evidence/phase-06.md](evidence/phase-06.md) — validate.sh (render+schema+policy) passes, 4 negative tests fail as intended; deployed, NetworkPolicy verified, 0/980 failed requests during rollout; then torn down |
| 7 | Argo CD install + declarative Applications/AppProject | ✅ | [evidence/phase-07.md](evidence/phase-07.md) — v3.5.7 (checksum + digests), PSA restricted, local Git server, app-of-apps; dev/staging Synced+Healthy, prod OutOfSync awaiting manual sync |
| 8 | GitHub Actions: test, scan, build, push immutable images | 🟡 | [evidence/2026-11-13-design-baseline.md](evidence/2026-11-13-design-baseline.md) — both repos public; `ci` green on `28ae24b`; digest `sha256:910d9b7b…` published with verified provenance (B3, B4). **image-updater failing** since 2026-11-12 16:46Z (B5): repo setting blocks PR creation, then a stale `ci/dev-28ae24b` branch breaks `--force-with-lease`. `gitops-api` main unprotected (B6) |
| 9 | PR-based image promotion | 🟡 | `set-image.sh` + gated `promote.sh` verified locally with branch+merge as a simulated PR ([evidence/phase-10-12.md](evidence/phase-10-12.md)); real GitHub PRs blocked by the image-updater defect (B5) |
| 10 | Dev auto-sync | ✅ | [evidence/phase-10-12.md](evidence/phase-10-12.md) — commit→sync 69 s detect + 5 s apply; 4 drift types self-healed in 1.4–24.8 s |
| 11 | Staging reviewed promotion | 🟡 | [evidence/phase-10-12.md](evidence/phase-10-12.md) — gated promotion, 1-line diff, 1905 requests / 0 failed during rollout. Manual-sync staging proposed in draft [gitops-config#3](https://github.com/Dapravith/gitops-config/pull/3), **on hold**: conflicts with SEC-023 (approved PR → auto-sync). Live staging still auto-syncs |
| 12 | Prod manual sync | ✅ | [evidence/phase-10-12.md](evidence/phase-10-12.md) — no sync for 4 min after merge; you authorized; manual sync 3 s; same digest in all 3 envs; prod drift reported, not reverted |
| 13 | Drift, rollback, security, failure tests | ✅ | [evidence/phase-13.md](evidence/phase-13.md) — 13 tests; 4 gaps found and fixed (namespace allow-list, unpublished digest, invalid config, manifest scan); rollback 20 s / 0 failed; workloads unaffected by Git and Argo CD outages. Not tested: prod failures |
| 14 | Runbooks, PROMOTION.md, audit report | 🟡 | GitHub-flow PROMOTION.md drafted in #3 (on hold with the staging policy). Runbooks + audit = SEC-029/032 |

## Known open items

- [x] Sealed Secrets installed and verified for dev + staging ([evidence/sealed-secrets.md](evidence/sealed-secrets.md)); prod not enabled yet (needs a manual prod sync)
- [ ] Seal the first real credential (GHCR pull secret / Argo CD repo creds) in phase 8. You create and seal it
- [ ] Re-run `scripts/sealed-secrets-key.sh backup` and `scripts/seal.sh --fetch-cert` after each 30-day key renewal
- [ ] `argocd-initial-admin-secret` still present (unused; `--core` mode)
- [ ] Shared Docker VM is CPU-starved by other workloads (load 20–80 on 8 vCPU)
- [ ] Rust refactor in review: [gitops-api#1](https://github.com/Dapravith/gitops-api/pull/1) (contract unchanged except a JSON body on 405); optional `/items` in draft [#2](https://github.com/Dapravith/gitops-api/pull/2)
- [ ] Fix image-updater (B5): (b) fetch-before-push fix in [gitops-config#4](https://github.com/Dapravith/gitops-config/pull/4) (open); (a) owner still decides repo setting vs GitHub App ([IMPLEMENTATION-GUIDE 8.3](design/IMPLEMENTATION-GUIDE.md#83-fix-the-image-updater-defect-b5))
- [ ] Verify Argo CD against GitHub (PR #1) on a running cluster, then remove the `TRANSITION` lab-Git sources (guide 8.5)
- [x] Lab running again (2026-11-13): all 5 Applications Synced/Healthy from GitHub `gitops-config@3b269a3`
- [ ] Umbrella repo `GitOps-Multi-Deployment` (commit `48a786d`) is docs/workspace only. CI and Argo CD stay on the two repos `gitops-api` + `gitops-config` (decided 2026-11-13); keep the folders in sync with them

## Open prerequisites

- [ ] Install host tools (or keep using containers): `rustup` (optional — `scripts/cargo.sh` uses Docker), `kubeseal` (Trivy, hadolint, kubeconform, yq and the argocd CLI run as digest-pinned containers)
- [x] GitHub repos created: `Dapravith/gitops-api`, `Dapravith/gitops-config`, both public (B3)
- [x] Phase 7 Git source: local read-only Git server (chosen 2026-11-12); Applications' `repoURL` switched to GitHub in PR #1 (not yet verified on a cluster)
- [x] `gitops-config` is public, so Argo CD needs no repo credential

## Security remediation (SEC-001 … SEC-032)

From the security assessment of 2026-11-13. ✅ only with evidence.

| ID | Item | Status | Evidence / blocker |
|---|---|---|---|
| SEC-001 | Repository ownership | ✅ | [sec-phase1-baseline](evidence/2026-11-13-sec-phase1-baseline.md#sec-001-repository-ownership); umbrella resynced to `gitops-api@28ae24b` / `gitops-config@3b269a3` |
| SEC-002 | Last known-good state | ✅ | [baseline](evidence/2026-11-13-sec-phase1-baseline.md#sec-002-last-known-good-state); finding: running digest `de04de91…` is not in GHCR |
| SEC-003 | Review PR #3 | ✅ | split: fix → [#4](https://github.com/Dapravith/gitops-config/pull/4), policy PR [#3](https://github.com/Dapravith/gitops-config/pull/3) closed after the owner chose approved PR → auto-sync for staging; nothing merged |
| SEC-004 | Preserve production safety | ✅ | prod manual, enforced by `validate`; stays manual until SEC-006/009/016/022/026 pass |
| SEC-005 | Protect main branches | 🟡 | applied to all 3 repos as code (`governance/rulesets.sh`), no bypass: [evidence/2026-11-13-rulesets-refactor-promotion.md](evidence/2026-11-13-rulesets-refactor-promotion.md). 0 approvals until a 2nd reviewer exists; negative tests = SEC-009 |
| SEC-006 | Independent PR reviews | ⬜ | waiting for the owner to add a second reviewer account; CODEOWNERS drafted |
| SEC-007 | Required CI checks | ✅ | gitops-api: `test`, `scan-source`, `image`; gitops-config: `validate`; integration-pinned, strict: [evidence/2026-11-13-rulesets-refactor-promotion.md](evidence/2026-11-13-rulesets-refactor-promotion.md) |
| SEC-008 | Restrict automation | ⬜ | |
| SEC-009 | Verify governance controls | ⬜ | needs SEC-005–008 |
| SEC-010 | Fix image-updater | 🟡 | branch handling in #4; GitHub App wiring in [gitops-config#5](https://github.com/Dapravith/gitops-config/pull/5); owner creates App, then verify a real run |
| SEC-011 | Scoped automation credentials | 🟡 | workflow side done in #5 (`GITHUB_TOKEN` read-only, App token scoped to gitops-config); owner creates the App ([settings](governance/README.md#3-github-app-for-the-image-updater-sec-010-sec-011)) |
| SEC-012 | Enforce PR review policy | ⬜ | bot auto-merge still on |
| SEC-013 | Restrict image publication | ⬜ | finding: `workflow_dispatch` on any branch publishes + signs ([governance §5](governance/README.md#5-related-finding-for-phase-3-sec-013)) |
| SEC-014 | Branch-independent promotion validation | ⬜ | |
| SEC-015 | End-to-end dev pipeline | ⬜ | needs SEC-010/011 |
| SEC-016 | Prod Sealed Secrets | ⬜ | needs a prod sync (ask at the time) |
| SEC-017 | Off-host key backups | ⬜ | |
| SEC-018 | Prod Secret protection | ⬜ | |
| SEC-019 | Image admission verification | ⬜ | |
| SEC-020 | Argo CD authorization | ⬜ | |
| SEC-021 | Remove obsolete permissions | ⬜ | |
| SEC-022 | Real image availability | ⬜ | all envs on lab-only `de04de91…` |
| SEC-023 | Staging promotion test | ⬜ | model decided: approved PR → auto-sync; needs SEC-006 |
| SEC-024 | Safe prod auto-sync | ⬜ | last; after all safeguards |
| SEC-025 | Critical resource deletion | ⬜ | |
| SEC-026 | Git-based rollback | ⬜ | |
| SEC-027 | Resilient prod infrastructure | ⬜ | plan only |
| SEC-028 | Monitoring and alerting | ⬜ | |
| SEC-029 | Runbooks | ⬜ | |
| SEC-030 | Failure-injection tests | ⬜ | |
| SEC-031 | Upgrade + vulnerability review | ⬜ | |
| SEC-032 | Evidence-based audit | ⬜ | |
