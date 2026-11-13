# SEC Phase 1: safe baseline (2026-11-13)

Captured from the live lab (`~/.kube/gitops-lab.kubeconfig`) and the GitHub API. Every value
below is command output, not a plan.

## SEC-001 Repository ownership

| Repository | Owns | Deploys? |
|---|---|---|
| `Dapravith/gitops-api` | application source, Dockerfile, `ci` workflow (test, scan, image, provenance) | publishes images to GHCR only |
| `Dapravith/gitops-config` | desired state: Kustomize base/overlays, Argo CD Applications/AppProjects, Sealed Secrets, `validate` + `image-updater` workflows | **yes**: the only repo Argo CD reads |
| `Dapravith/GitOps-Multi-Deployment` (umbrella) | documentation (`docs/`) and a read-only snapshot of the two repos | **no**: workflows under `*/.github/` do not run here, nothing reads it |

Rule: changes to code or deployed state are made in `gitops-api` / `gitops-config` by PR. The
umbrella folders are re-synced from those repos' `main` (`rsync --delete`), never edited first.
Snapshot at this baseline: `gitops-api@28ae24b`, `gitops-config@3b269a3` (verified with `diff -rq`).

Correction made: umbrella commit `d80daf8` had copied the unmerged PR #3 policy change into
`gitops-config/`. It was reset to `gitops-config@3b269a3` so the umbrella never shows undeployed state.

## SEC-002 Last known-good state

| Item | Value |
|---|---|
| `gitops-api` main | `28ae24bf523c4aa620e23131066116df1270276b`: checks `test`, `scan-source`, `image` = success |
| GHCR image for that commit | `ghcr.io/dapravith/gitops-api:sha-28ae24b…` → `sha256:910d9b7b358d1c65cd069e3c5a4a2a5b89b48a6fe4711b1fd5ecd15add10a503` (OCI index) |
| `gitops-config` main | `3b269a3893bc0de1accb881c21c6b453bb2b3394` |
| umbrella main | `d80daf885dc952ee280e2a1772ff5b78c239d6cc` (before the correction above) |
| Kubernetes | kind `gitops-lab`, server `v1.36.4`, single node |
| Argo CD | `v3.5.7@sha256:16df57da…` (all components), redis `8.2.3-alpine@sha256:08ad0b1d…` |

Argo CD Applications (all at revision `3b269a3`):

| Application | Sync | Health | Automated |
|---|---|---|---|
| root | Synced | Healthy | prune + selfHeal |
| sealed-secrets | Synced | Healthy | prune + selfHeal |
| gitops-api-dev | Synced | Healthy | prune + selfHeal |
| gitops-api-staging | Synced | Healthy | prune + selfHeal |
| gitops-api-prod | Synced | Healthy | **none (manual)** |

Running workloads: dev 1/1, staging 2/2, prod 3/3 pods ready, every pod on
`ghcr.io/dapravith/gitops-api@sha256:de04de911d746bd9a3388b7f450931febe819e8c8ca723d660a67052f0af2482`.

**Finding (feeds SEC-022):** `de04de91…` is the phase-4 local build. `docker buildx imagetools
inspect ghcr.io/dapravith/gitops-api@sha256:de04de91…` returns `not found`. It exists only in the
kind node's local ghcr.io mirror. If that registry is lost, no environment can reschedule a pod.

Rollback reference: the known-good config revision is `gitops-config@3b269a3`. Git-based
rollback was verified in phase 13 (revert PR → 20 s, 0 failed requests,
[phase-13.md](phase-13.md)); SEC-026 re-tests it under the new review rules.

## SEC-003 PR #3 review

- The image-updater repair moved to its own PR,
  [gitops-config#4](https://github.com/Dapravith/gitops-config/pull/4) (one file, +6 lines).
- [gitops-config#3](https://github.com/Dapravith/gitops-config/pull/3) now holds only the
  staging sync-policy change and PROMOTION.md. It is a **draft**, titled "DO NOT MERGE: policy pending":
  it conflicts with the target model in SEC-023/024 (approved PR → auto-sync), and no
  independent reviewer exists yet.
- Nothing was merged. Cluster state unchanged.

## SEC-004 Production safety

- `gitops-api-prod` has no `automated` block (live: `AUTO <none>` above) and no resources finalizer.
- `scripts/validate.sh` (required `validate` check) fails any PR that adds `automated` to prod.
- Prod stays manual until SEC-006/009 (enforced independent review), SEC-016 (prod Sealed
  Secrets) and SEC-022/026 (real digests, rollback test) pass. Every prod sync is asked for at
  the moment it is due.

## GitHub governance at baseline (input for Phase 2)

| Repo | Ruleset / protection | Collaborators | Actions may create/approve PRs |
|---|---|---|---|
| gitops-api | **none** | Dapravith (admin) | no |
| gitops-config | `main: PR + validate`: PR required, 0 approvals, `validate` required, no force-push/delete, no bypass actors | Dapravith (admin) | no |
| umbrella | **none** | Dapravith (admin) | no |

There is one human identity. GitHub does not count an author's own approval, so "≥1 independent
approval" cannot be enforced until a second reviewer (account) exists.
