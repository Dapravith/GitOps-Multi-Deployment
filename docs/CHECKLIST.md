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
| 11 | Staging reviewed promotion | 🟡 | [evidence/phase-10-12.md](evidence/phase-10-12.md) — gated promotion, 1-line diff, 1905 requests / 0 failed during rollout. Mandatory reviewer approval **not** enforced: GitHub ruleset requires PR + `validate` but 0 approvals (B6); a single maintainer can't self-approve |
| 12 | Prod manual sync | ✅ | [evidence/phase-10-12.md](evidence/phase-10-12.md) — no sync for 4 min after merge; you authorized; manual sync 3 s; same digest in all 3 envs; prod drift reported, not reverted |
| 13 | Drift, rollback, security, failure tests | ✅ | [evidence/phase-13.md](evidence/phase-13.md) — 13 tests; 4 gaps found and fixed (namespace allow-list, unpublished digest, invalid config, manifest scan); rollback 20 s / 0 failed; workloads unaffected by Git and Argo CD outages. Not tested: prod failures |
| 14 | Runbooks, PROMOTION.md, audit report | ⬜ | |

## Known open items

- [x] Sealed Secrets installed and verified for dev + staging ([evidence/sealed-secrets.md](evidence/sealed-secrets.md)); prod not enabled yet (needs a manual prod sync)
- [ ] Seal the first real credential (GHCR pull secret / Argo CD repo creds) in phase 8. You create and seal it
- [ ] Re-run `scripts/sealed-secrets-key.sh backup` and `scripts/seal.sh --fetch-cert` after each 30-day key renewal
- [ ] `argocd-initial-admin-secret` still present (unused; `--core` mode)
- [ ] Shared Docker VM is CPU-starved by other workloads (load 20–80 on 8 vCPU)
- [ ] Fix image-updater (B5): owner decides repo setting vs GitHub App; workflow fetch-before-push fix in [IMPLEMENTATION-GUIDE 8.3](design/IMPLEMENTATION-GUIDE.md#83-fix-the-image-updater-defect-b5)
- [ ] Verify Argo CD against GitHub (PR #1) on a running cluster, then remove the `TRANSITION` lab-Git sources (guide 8.5)
- [ ] Lab stopped on 2026-11-13 (Docker daemon down, B1)

## Open prerequisites

- [ ] Install host tools (or keep using containers): `rustup` (optional — `scripts/cargo.sh` uses Docker), `kubeseal` (Trivy, hadolint, kubeconform, yq and the argocd CLI run as digest-pinned containers)
- [x] GitHub repos created: `Dapravith/gitops-api`, `Dapravith/gitops-config`, both public (B3)
- [x] Phase 7 Git source: local read-only Git server (chosen 2026-11-12); Applications' `repoURL` switched to GitHub in PR #1 (not yet verified on a cluster)
- [x] `gitops-config` is public, so Argo CD needs no repo credential
