# Implementation Guide: step by step, with proof at every step

Companion to [HLD.md](HLD.md) and [LLD.md](LLD.md). Each step has the same four parts:

- **Do**: the commands or changes.
- **Verify**: the check that must pass. A step is only done when this passes.
- **Capture**: what to paste into `docs/evidence/` (output, not paraphrase).
- **Status**: as of 2026-11-13, with an evidence link.

Rules that apply to every step:

1. Use the lab kubeconfig: `export KUBECONFIG="$HOME/.kube/gitops-lab.kubeconfig"`. Never change the default context.
2. After bootstrap, no `kubectl apply` to app namespaces. Changes go through Git; Argo CD applies them.
3. **Gates that need the owner's explicit OK each time:** any change on GitHub (repo settings, rulesets, pushes, branch deletes), and **every prod sync**.
4. Record failures as they happened. A ❌ with output is evidence. A ✅ without output is not.

| Phase | Step | Status |
|---|---|---|
| 0 | Workstation | ✅ |
| 1 | Application | ✅ |
| 2 | Container image | ✅ |
| 3 | Cluster | ✅ (currently stopped, B1) |
| 4 | Desired state (Kustomize) | ✅ |
| 5 | Argo CD | ✅ (lab Git) · ⬜ (GitHub source) |
| 6 | Secrets | ✅ dev/staging · ⬜ prod |
| 7 | Promotion, rollback, failure drills | ✅ lab |
| 8 | GitHub CI/CD | 🟡 CI ✅ · image-updater ❌ · review gate ❌ |
| 9 | Hardening to target | ⬜ |
| 10 | Runbooks + audit report | ⬜ |

---

## Phase 0: Workstation

**Do.** Install docker (buildx), kind 0.33, kubectl 1.36, gh, git. Rust, Trivy, gitleaks, kubeconform, yq,
hadolint and the argocd CLI run as digest-pinned containers, so they don't need installing.

**Verify.**
```bash
docker buildx ls | grep -E 'linux/(arm64|amd64)'
kind version && kubectl version --client && gh auth status
gitops-api/scripts/cargo.sh --version      # rustc 1.99.0 in container
```

**Capture.** Tool versions plus the toolchain image digest.
**Status.** ✅ [phase-01-03](../evidence/phase-01-03.md#phase-1--environment).

## Phase 1: Application (LLD §1)

**Do.** Implement the modules in LLD §1.1 and the contract in §1.2–1.4.

**Verify.**
```bash
cd gitops-api
scripts/cargo.sh fmt --all -- --check
scripts/cargo.sh clippy --all-targets --locked -- -D warnings
scripts/cargo.sh test --locked            # expect 15 passed (9 unit + 6 integration)
```
Runtime smoke test: run the release binary, then `curl` `/health`, `/ready`, `/version` and `/missing`, and expect 200, 200, 200, 404.
`docker stop` should give exit 0 with a "shutdown signal received" log line. `PORT=0` should give exit 1 with the config error.

**Capture.** Test summary, curl output, exit codes, the JSON log lines.
**Status.** ✅ [phase-01-03](../evidence/phase-01-03.md), and 15 tests at commit `9432470` ([phase-07](../evidence/phase-07.md#commits)).

## Phase 2: Container image (LLD §2)

**Do.**
```bash
docker buildx build --platform linux/arm64,linux/amd64 \
  --build-arg GIT_SHA="$(git rev-parse HEAD)" -t gitops-api:local --load gitops-api
```

**Verify.** Both architectures run and give the same `/version` output. `docker exec … /bin/sh` must fail (no shell).
`Config.User` = `65532:65532`. hadolint exits 0. Trivy `--severity HIGH,CRITICAL --ignore-unfixed --exit-code 1` exits 0 for each platform.

**Capture.** Image sizes, the manifest list digest, scan summaries.
**Status.** ✅ [phase-04](../evidence/phase-04.md).

## Phase 3: Cluster

**Do.**
```bash
gitops-config/bootstrap/kind/create-cluster.sh     # idempotent; starts the registry mirror and kind node
```

**Verify.** Node `Ready`. `/readyz` passes. Default kubeconfig context unchanged. In a throwaway namespace,
PSA `restricted` rejects a default pod and admits a hardened one. Delete the namespace afterwards.

**Capture.** `kubectl get nodes -o wide`, both PSA results, `NotFound` for the deleted namespace.
**Status.** ✅ [phase-05](../evidence/phase-05.md), [phase-06 mirror](../evidence/phase-06.md#cluster-change-local-registry-as-ghcrio-mirror).
Currently stopped ([B1](../evidence/2026-11-13-design-baseline.md#b1-lab-runtime-is-currently-down)).
To resume: start Docker Desktop, then re-run `create-cluster.sh` (it exports the kubeconfig if the cluster exists)
and the git server's `start.sh`.

## Phase 4: Desired state (LLD §3)

**Do.** Base + components + overlays as in LLD §3.1. The image digest is the only per-release field.

**Verify.**
```bash
gitops-config/scripts/validate.sh            # expect: ALL OVERLAYS VALID, exit 0
```
Negative tests on a scratch copy. **Each must exit 1**: `newTag: latest`; a plaintext `Secret`;
`allowPrivilegeEscalation: true`; PDB removed in prod; `replica:` typo; `PORT=0`; `APP_ENV=prod` in dev.

**Capture.** The summary line per environment plus each negative test's failure line.
**Status.** ✅ [phase-06](../evidence/phase-06.md#offline-validation--gitops-configscriptsvalidatesh). Re-run today
(policy stage, offline) ✅ [B2](../evidence/2026-11-13-design-baseline.md#b2-offline-validation-re-run-today-no-docker-needed).

## Phase 5: Argo CD (LLD §4)

**Do.**
```bash
gitops-config/bootstrap/argocd/install.sh    # verifies upstream sha256, applies, then hands over to root
```

**Verify.**
```bash
gitops-config/scripts/argocd.sh app list
# expect: root, sealed-secrets, gitops-api-dev, gitops-api-staging = Synced/Healthy
#         gitops-api-prod = OutOfSync (never synced automatically)
```
Probe the locked `default` project with a throwaway Application. Expect `InvalidSpecError` for both the repo and the destination.

**Capture.** App list, `app get gitops-api-prod` (`Sync Policy: Manual`), the probe errors.
**Status.** ✅ with the lab Git server, [phase-07](../evidence/phase-07.md). The GitHub source is ⬜, see 8.5.

## Phase 6: Secrets (LLD §5)

**Do.** Follow [SECRETS.md](../../gitops-config/docs/SECRETS.md): seal from stdin, commit the `SealedSecret`, let Argo CD apply it.
Immediately run `scripts/sealed-secrets-key.sh backup`, and store the file outside any repo.

**Verify.** The decrypted value's SHA-256 prefix matches the original. Cross-namespace copy, rename and a one-character flip
each give `ErrUnsealFailed`. A key restore into a rebuilt cluster decrypts the existing ciphertext.

**Capture.** Hash comparison, the three failure statuses, the restore drill.
**Status.** ✅ dev and staging, [sealed-secrets](../evidence/sealed-secrets.md). Prod is ⬜ and needs a prod sync (owner's OK).
Open: seal the first *real* credential, and re-back-up the key after each 30-day renewal.

## Phase 7: Promotion, rollback, failure drills (LLD §7)

**Do / Verify**, in order:

| Drill | Command | Pass criterion |
|---|---|---|
| dev deploy | merge a digest change for dev | `app wait gitops-api-dev --sync --health`; `/version` shows the new `git_sha` |
| self-heal | `kubectl scale`, `set image`, delete NetworkPolicy, delete Service in dev | each reverted; record seconds |
| staging promote | `scripts/promote.sh dev staging` → review → merge | 1-line diff; 0 failed requests under load |
| prod promote | `scripts/promote.sh staging prod` → merge | **no** sync for ≥3 min after merge |
| prod sync | `scripts/argocd.sh app sync gitops-api-prod` | **ask the owner first, every time** |
| prod drift | `kubectl scale --replicas=4` in prod | stays `OutOfSync`, not reverted |
| rollback | `git revert -m 1 <promotion merge>` | previous digest live; 0 failed requests |
| bad digest / bad config | commit to dev | old pod keeps serving; `promote.sh` refuses; revert heals |
| Git down / controller down | stop the git server; scale the controller to 0 | 0 failed requests; self-heal paused, then resumes |

Use a **long-lived** `api-client` pod for load (100 ms `GET /version`). Short-lived `kubectl run --rm` pods bypass
kindnet filtering for their first moments ([phase-06 caveat](../evidence/phase-06.md#networkpolicy-kindnet-v20260820-69b56db7-policy-engine-ready)).

**Status.** ✅ all of the above in the lab: [phase-10-12](../evidence/phase-10-12.md), [phase-13](../evidence/phase-13.md).
Not done: a prod failure drill and a prod rollback (both need a prod sync).

---

## Phase 8: GitHub CI/CD

### 8.1 Repositories ✅

Both repos are public under `Dapravith`. Local `main` = `origin/main`
([B3](../evidence/2026-11-13-design-baseline.md#b3-github-state-phase-8-has-started)).

### 8.2 Application CI ✅

**Verify.**
```bash
gh run list -R Dapravith/gitops-api --limit 1                       # ci push main success
gh attestation verify oci://ghcr.io/dapravith/gitops-api@<digest> \
  --repo Dapravith/gitops-api \
  --signer-workflow Dapravith/gitops-api/.github/workflows/ci.yml   # exit 0
```

**Status.** ✅ run `28ae24b`. Digest `sha256:910d9b7b…a503` verified, commit and ref match
([B4](../evidence/2026-11-13-design-baseline.md#b4-published-image--provenance-verified)).

### 8.3 Fix the image-updater defect (B5)

**Status.** ❌ failing since 2026-11-12T16:46Z. Two independent causes ([B5](../evidence/2026-11-13-design-baseline.md#b5-defect-image-updater-cannot-open-the-dev-pr)).

**(a) Repository setting: owner's decision.** Pick one:

| Option | Change | Trade-off |
|---|---|---|
| A. Allow Actions to create PRs | Settings → Actions → General → "Allow GitHub Actions to create and approve pull requests", or `gh api -X PUT repos/Dapravith/gitops-config/actions/permissions/workflow -f default_workflow_permissions=read -F can_approve_pull_request_reviews=true` | simplest. The same switch also lets workflows **approve** PRs, which would undercut a future approval rule |
| B. GitHub App token (recommended for real use) | create an App with `contents` + `pull_requests` write on `gitops-config` only. Mint a token with `actions/create-github-app-token` (pin by SHA) | no approve-PR power. App-made PRs **do** trigger `pull_request` workflows, so the manual `gh workflow run validate.yml` dispatch can go |

**(b) Workflow change (local commit is fine; pushing needs the owner's OK).** In `image-updater.yml`,
before the push:

```bash
# A previous run may have left the bot branch on the remote. Fetch it so
# --force-with-lease has a remote-tracking ref to compare against.
if git ls-remote --exit-code --heads origin "${branch}" >/dev/null; then
  git fetch --no-tags origin "+refs/heads/${branch}:refs/remotes/origin/${branch}"
fi
git push --force-with-lease origin "${branch}"
```

Keep the "PR already open" check. With this change, a branch that exists without a PR is repaired, not fatal.

**Verify.**
```bash
gh workflow run image-updater.yml -R Dapravith/gitops-config
gh run watch -R Dapravith/gitops-config "$(gh run list -R Dapravith/gitops-config --workflow image-updater --limit 1 --json databaseId --jq '.[0].databaseId')"
gh pr list -R Dapravith/gitops-config --head ci/dev-28ae24b          # 1 open PR → merges after validate
```
Then confirm the dev overlay on `main` holds `sha256:910d9b7b…` and `validate` ran on the branch head.

**Capture.** The run log lines (`verified …`, `Opened https://…/pull/N`), the PR's checks, the merge commit.

### 8.4 Enforce the rules on GitHub ❌

**Do (owner's OK, remote change).**

| Repo | Rule | Why |
|---|---|---|
| `gitops-config` | keep: PR required, `validate` required, no force-push/delete, merge-commit only | already active (B6) |
| `gitops-config` | `required_approving_review_count: 1` + CODEOWNERS for `apps/gitops-api/overlays/{staging,prod}/` | makes the staging/prod review real. **Needs a second account or team**: authors can't self-approve |
| `gitops-config` | dev bot PRs: approval exempt by path (CODEOWNERS covers only staging/prod), or a ruleset bypass for the App | keeps dev fully automatic |
| `gitops-api` | ruleset on `main`: PR required, checks `test`, `scan-source`, `image` required | today `main` is unprotected (B6) |

**Verify.** On a test branch, each of these must be **refused** by GitHub:
- a PR that changes two lines in `overlays/dev/kustomization.yaml` (the `validate` one-line rule);
- a direct push to `main`;
- a merge of a prod promotion without approval.

Capture the refusal messages.

**Status.** Ruleset exists with 0 approvals; `gitops-api` unprotected ([B6](../evidence/2026-11-13-design-baseline.md#b6-branch-rules-review-is-not-enforced)).
With a single maintainer, record this as an **accepted limitation**. The manual prod sync is the remaining human gate.

### 8.5 Point Argo CD at GitHub, then retire the lab Git server ⬜

PR #1 changed every `repoURL` to `https://github.com/Dapravith/gitops-config.git`. Nothing has proved it on a cluster yet.

**Do.** Start the lab (Phase 3). Argo CD picks up the change from Git.

**Verify.**
```bash
scripts/argocd.sh app list      # REPO column = github.com; revision = origin/main HEAD
kubectl -n argocd get applications -o jsonpath='{range .items[*]}{.metadata.name} {.status.sync.revision}{"\n"}{end}'
```
Then remove the `TRANSITION` lab-Git entries from all four AppProjects in one PR. Verify all apps stay Synced.
Delete `lab-infra/git` via Git.

**Capture.** App list before and after, revision = `git -C gitops-config rev-parse origin/main`.

### 8.6 First fully-remote release ⬜

`image-updater` PR → dev auto-sync → `promote.sh dev staging` PR → `promote.sh staging prod` PR → **owner's OK** → prod sync.

**Verify.** All six pods run `sha256:910d9b7b…` (or newer), and `/version` reports `git_sha 28ae24b…` in every environment.
**Capture.** The digest table and per-pod `imageID` table, in the same shape as [phase-10-12](../evidence/phase-10-12.md#phase-12--prod-requires-a-manual-sync).

---

## Phase 9: Hardening to the target architecture ⬜

One PR each, in this order (cheapest risk reduction first). Each row's proof is the "Done when" column of HLD §10.

1. **Version bumps** (B7): Rust 1.100.0, Trivy 0.76.2, then Argo CD 3.6.x (read the upgrade notes, update the `install.sh` checksum and image digests), then a kind node at K8s 1.37.x. Re-run `validate.sh` and phase 7 drills after each.
2. **Alerting** on Argo CD `ComparisonError` / `Degraded` / controller down. Prove it by re-running O1.
3. **Admission-time signature check** (Kyverno `verifyImages` or Sigstore `policy-controller`) for `ghcr.io/dapravith/*`. Prove it by applying an unsigned digest in dev: rejected.
4. **Git webhook** (needs ingress + TLS to `argocd-server`). Prove commit→sync < 10 s.
5. **SSO + Argo CD RBAC**, so `role:release-manager` is real. Prove a non-member's sync is denied.
6. **musl + `distroless/static`**. Prove Trivy all-severity = 0.
7. **Multi-node + `DoNotSchedule`**. Prove a node drain with 0 failed requests.
8. Delete `argocd-initial-admin-secret` (unused in `--core`).

## Phase 10: Runbooks and audit report ⬜

- `gitops-config/docs/RUNBOOK.md`: start/stop the lab, restore the sealing key, common Argo CD states and what to do, rollback (copy of PROMOTION.md §Rollback).
- `docs/AUDIT.md`: requirement → control → evidence → status, generated from HLD §1 and §11. Every ❌/🟡 row lists the owner decision it waits on.
- Update [CHECKLIST.md](../CHECKLIST.md) after each step above, using the same evidence links.
