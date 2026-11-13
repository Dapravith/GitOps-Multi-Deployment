# Promotion: dev → staging → prod

A release is **one immutable image digest** moving through three overlays. Nothing is rebuilt
between environments, and nobody runs `kubectl apply`.

| Step | Who | What changes | How it reaches the cluster |
|---|---|---|---|
| 1. Build | CI | new image `ghcr.io/dapravith/gitops-api@sha256:…` | n/a |
| 2. dev | CI | `overlays/dev` digest, via PR | Argo CD **auto-sync** (+ self-heal) |
| 3. staging | engineer + reviewer | `overlays/staging` digest, via reviewed PR | Argo CD **auto-sync** after merge |
| 4. prod | release manager + reviewer | `overlays/prod` digest, via reviewed PR | **manual** `argocd app sync` after merge |

> Lab note: until the repos are on GitHub (phase 8), a "PR" is a `promote/…` branch merged with
> `git merge --no-ff`, and "publish" is `git push lab main`. The commands below are otherwise identical.

## 0. Prerequisites

```bash
export KUBECONFIG="$HOME/.kube/gitops-lab.kubeconfig"
```

```bash
scripts/argocd.sh app list
```

## 1–2. New image into dev (CI does this)

```bash
git switch -c ci/dev-<short-sha>
```

```bash
scripts/set-image.sh dev sha256:<digest>
```

```bash
scripts/validate.sh
```

Commit, open a PR, merge. Argo CD syncs dev within ~60 s of the merge. Verify:

```bash
scripts/argocd.sh app wait gitops-api-dev --sync --health --timeout 300
```

## 3. Promote dev → staging

```bash
scripts/promote.sh dev staging
```

The script refuses unless **all** of these hold, and prints which gate failed:

1. the hop is adjacent (`dev→staging` or `staging→prod`; `dev→prod` is rejected)
2. you are on a clean `main`
3. `gitops-api-dev` is `Synced` + `Healthy` at the current `main` commit
4. dev's Deployment is fully rolled out and every running pod's `imageID` equals the digest in Git
5. `scripts/validate.sh` passes with the change

It creates `promote/staging-<digest12>` containing **one changed line** (the digest). Review that
diff, then merge:

```bash
git merge --no-ff promote/staging-<digest12>
```

```bash
git push lab main
```

```bash
scripts/argocd.sh app wait gitops-api-staging --sync --health --timeout 300
```

## 4. Promote staging → prod

```bash
scripts/promote.sh staging prod
```

Review, merge and push as above. **Merging does not deploy.** Argo CD marks `gitops-api-prod`
`OutOfSync` and waits. The release manager then reviews and syncs:

```bash
scripts/argocd.sh app diff gitops-api-prod
```

```bash
scripts/argocd.sh app sync gitops-api-prod
```

```bash
scripts/argocd.sh app wait gitops-api-prod --sync --health --timeout 300
```

## 5. Verify the release

Every environment must report the same `git_sha`, and the same digest in Git and live:

```bash
for e in dev staging prod; do grep digest: apps/gitops-api/overlays/$e/kustomization.yaml; done
```

```bash
kubectl get pods -A -l app.kubernetes.io/name=gitops-api -o custom-columns='NS:.metadata.namespace,IMAGE:.status.containerStatuses[0].imageID'
```

## Rollback

Rollback is a promotion in reverse: **revert the promotion merge commit** for that environment.

```bash
git revert -m 1 <merge-commit>
```

```bash
git push lab main
```

dev and staging roll back automatically. prod needs `scripts/argocd.sh app sync gitops-api-prod`.
Do not use `argocd app rollback` or `kubectl rollout undo` for dev/staging: self-heal reverts them
to Git within seconds. In prod they would work but leave the cluster `OutOfSync` from Git.

## Drift

| Environment | Manual change in the cluster | Result |
|---|---|---|
| dev, staging | reverted automatically (seconds) | Git wins |
| prod | **not** reverted; app shows `OutOfSync`, `app diff` shows the change | operator decides: sync, or fix Git |
