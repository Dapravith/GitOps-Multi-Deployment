# Promotion: dev → staging → prod

A release is **one immutable image digest** moving through three overlays. Nothing is rebuilt
between environments, and nobody runs `kubectl apply`: every change is a pull request to this
repository, and Argo CD applies `main`.

| Step | Who | Git change (PR to `main`) | How it reaches the cluster |
|---|---|---|---|
| 1. Build | `gitops-api` CI | none: pushes `ghcr.io/dapravith/gitops-api@sha256:…` + signed provenance | n/a |
| 2. dev | `image-updater` workflow | `overlays/dev` digest, bot PR, auto-merge after `validate` | Argo CD **auto-sync** + self-heal |
| 3. staging | engineer | `overlays/staging` digest, PR from `scripts/promote.sh` | **manual sync** (approval gate) |
| 4. prod | release manager | `overlays/prod` digest, PR from `scripts/promote.sh` | **manual sync** (approval gate) |

Guard rails that make this the only path (all enforced by `scripts/validate.sh`, which is the
required `validate` check on every PR):

- `gitops-api-dev` must auto-sync with self-heal. `gitops-api-staging` and `gitops-api-prod` must
  **not** have `automated` sync, so merging a PR never deploys them by itself.
- `gitops-api-prod` has no resources finalizer: deleting the Application never deletes prod workloads.
- Images are referenced by digest only; the digest must exist in the registry for amd64 + arm64.
- `promote.sh` only allows adjacent hops (`dev→staging`, `staging→prod`).

## 0. Prerequisites

```bash
export KUBECONFIG="$HOME/.kube/gitops-lab.kubeconfig"
git switch main && git pull --ff-only
scripts/argocd.sh app list
```

All five Applications (`root`, `sealed-secrets`, `gitops-api-{dev,staging,prod}`) should show
`Synced` / `Healthy` before you start.

## 1. Build (automatic)

A merge to `Dapravith/gitops-api` `main` runs its `ci` workflow: test → scan → multi-arch build →
push `sha-<commit>` → Trivy scan of the pushed digest → Sigstore build-provenance attestation.
CI holds **no** credential for this repo or for the cluster.

## 2. dev (automatic)

Every 10 minutes `.github/workflows/image-updater.yml` in this repo:

1. reads `gitops-api` `main`, checks that its `image` job succeeded,
2. resolves `sha-<commit>` to a digest and verifies the attestation names that commit, `ci.yml`, `refs/heads/main`,
3. opens PR `ci/dev-<sha7>` changing one line in `overlays/dev/kustomization.yaml`, starts `validate`, enables auto-merge.

Argo CD syncs dev within ~60 s of the merge. Verify:

```bash
scripts/argocd.sh app wait gitops-api-dev --sync --health --timeout 300
```

Manual fallback (same result, if the workflow is down):

```bash
git switch -c ci/dev-<sha7>
IMAGE_REGISTRY=ghcr.io scripts/set-image.sh dev sha256:<digest>
scripts/validate.sh dev
git commit -am "ci(dev): gitops-api@sha256:<digest12> (<sha7>)"
git push -u origin ci/dev-<sha7>
gh pr create --fill --base main
```

## 3. Promote dev → staging

```bash
scripts/promote.sh dev staging
```

The script refuses unless **all** of these hold, and prints which gate failed:

1. the hop is adjacent (`dev→prod` is rejected)
2. you are on a clean `main`
3. `gitops-api-dev` is `Synced` + `Healthy` at the current `main` commit
4. dev's Deployment is fully rolled out and every running pod's `imageID` equals the digest in Git
5. `scripts/validate.sh` passes with the change

It creates a local branch `promote/staging-<digest12>` with **one changed line** (the digest).
Open the PR:

```bash
git push -u origin promote/staging-<digest12>
gh pr create --fill --base main --head promote/staging-<digest12>
```

Wait for `validate` to pass and merge the PR. **Merging does not deploy staging.** Argo CD marks
`gitops-api-staging` `OutOfSync`. Approve the change by reviewing exactly what will be applied,
then syncing:

```bash
scripts/argocd.sh app get gitops-api-staging --refresh
scripts/argocd.sh app diff gitops-api-staging
scripts/argocd.sh app sync gitops-api-staging
scripts/argocd.sh app wait gitops-api-staging --sync --health --timeout 300
```

The diff must show only the image digest change. Anything else: stop, do not sync, investigate.

## 4. Promote staging → prod

Same as step 3, with prod. `promote.sh` now gates on staging being `Synced` + `Healthy` and
running the digest from Git:

```bash
scripts/promote.sh staging prod
git push -u origin promote/prod-<digest12>
gh pr create --fill --base main --head promote/prod-<digest12>
```

After the merge, the **release manager** reviews and syncs (record who and when in the PR):

```bash
scripts/argocd.sh app get gitops-api-prod --refresh
scripts/argocd.sh app diff gitops-api-prod
scripts/argocd.sh app sync gitops-api-prod
scripts/argocd.sh app wait gitops-api-prod --sync --health --timeout 300
```

## 5. Verify the release

The same digest in Git and live in every environment:

```bash
for e in dev staging prod; do grep digest: apps/gitops-api/overlays/$e/kustomization.yaml; done
kubectl get pods -A -l app.kubernetes.io/name=gitops-api \
  -o custom-columns='NS:.metadata.namespace,IMAGE:.status.containerStatuses[0].imageID'
```

Each environment's `/version` reports the same `git_sha`.

## Rollback

Rollback is a promotion in reverse: open a PR that **reverts the promotion merge commit** for
that environment.

```bash
git switch -c revert/<env>-<digest12>
git revert -m 1 <merge-commit>
git push -u origin revert/<env>-<digest12>
gh pr create --fill --base main
```

After the merge: dev rolls back automatically; staging and prod need
`scripts/argocd.sh app sync gitops-api-<env>`.

Do not use `argocd app rollback` or `kubectl rollout undo` for dev: self-heal reverts it to Git
within seconds. In staging and prod they work as an emergency measure, but they leave the
Application `OutOfSync` from Git; follow up with the revert PR at once.

## Drift

| Environment | Manual change in the cluster | Result |
|---|---|---|
| dev | reverted automatically (seconds) | Git wins |
| staging, prod | **not** reverted; app shows `OutOfSync`, `app diff` shows the change | operator decides: sync (Git wins), or fix Git with a PR |

## Failed sync: where to look

```bash
scripts/argocd.sh app get gitops-api-<env>                 # sync + health status, failed resources
scripts/argocd.sh app diff gitops-api-<env>                # Git vs live
scripts/argocd.sh app history gitops-api-<env>             # which revision is deployed
kubectl -n gitops-<env> get events --sort-by=.lastTimestamp | tail -20
kubectl -n gitops-<env> describe deployment gitops-api
kubectl -n argocd logs deploy/argocd-repo-server --tail=100          # render errors
kubectl -n argocd logs statefulset/argocd-application-controller --tail=100   # apply errors
```
