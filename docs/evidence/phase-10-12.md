# Evidence — Phases 10–12: auto-sync (dev), reviewed promotion (staging), manual sync (prod)

Captured 2026-11-12 on the local lab. **PRs are simulated**: a branch, `validate.sh`, a reviewable diff, and
`git merge --no-ff`, published with `git push lab main`. Real GitHub PRs and branch protection arrive with phase 8.
The shared Docker VM was still heavily loaded (load average 20–30 on 8 vCPU), which inflates rollout times.

## The release artifact

Built from a clean worktree of `gitops-api@9432470` (multi-arch, `GIT_SHA` baked in), pushed to the lab registry:

```
localhost:5001/dapravith/gitops-api:sha-9432470
Digest: sha256:de04de911d746bd9a3388b7f450931febe819e8c8ca723d660a67052f0af2482   (linux/arm64, linux/amd64)
```

Previous digest everywhere: `sha256:4bfc7038…` (`git_sha: local-phase4`).

## Tooling added (`gitops-config/scripts/`)

- `set-image.sh <env> sha256:<64hex>`: the one-line edit. Rejects `qa`, `sha256:abc`, `latest`.
- `promote.sh <from> <to>`: gated promotion producing branch `promote/<to>-<digest12>`.

Bugs found and fixed while testing `promote.sh` (both committed):

| Commit | Bug | How it showed |
|---|---|---|
| `16450ce` | `read < <(kubectl -o jsonpath)`: jsonpath prints no trailing newline, so `read` returned non-zero and `set -e` killed the script **silently** | script printed nothing, no branch |
| `ff84272` | gate relied on `status.summary.images`, which is `{}` outside sync operations | gate could never pass; now checks rollout completeness + every live pod's `imageID` |

Gates observed firing for real:

```
refusing dev->prod: only dev->staging and staging->prod are allowed
working tree not clean
gate: gitops-api-dev is at 7821cab, main is 16450ce; wait for sync
gate: gitops-api-dev is Synced/Progressing, must be Synced/Healthy      (0 branches created)
```

## Phase 10 — dev auto-sync

Commit→deploy (`ci/dev-9432470` merged as `7821cab`, pushed 02:59:27Z):

```
Argo CD sync started  03:00:36Z   (+69 s: polling, no webhook)
Argo CD sync finished 03:00:41Z   (5 s)
dev Synced+Healthy on de04de91… +197 s total (rolling update on a CPU-starved host)
/version -> {"git_sha":"9432470e5463d7ce3c5a9a0b22838b34b63315f6","environment":"dev"}   (staging still local-phase4)
history: id=1 automated=true
```

Self-heal (manual drift in `gitops-dev`):

| Drift | Reverted after |
|---|---|
| `kubectl scale --replicas=3` (Git: 1) | 1.4 s |
| `kubectl set image` → old digest | 24.8 s |
| `kubectl delete networkpolicy default-deny-all` | 2.1 s |
| `kubectl delete service gitops-api` | 18.1 s |

(The first image-drift measurement printed "NOT reverted": that was a zsh globbing bug in my check, not Argo CD.
Re-measured with correct quoting: reverted, 24.8 s.)

## Phase 11 — reviewed promotion dev → staging

```
gates passed: gitops-api-dev Synced/Healthy at ff84272, running @de04de911d74
created promote/staging-de04de911d74:  1 file changed, 1 insertion(+), 1 deletion(-)
-    digest: sha256:4bfc7038e9ecfb9503c113d039dc14be0e4cd60e6a8ef11b2984c60059fbea8f
+    digest: sha256:de04de911d746bd9a3388b7f450931febe819e8c8ca723d660a67052f0af2482
```

Merged as `ed6b167`. Staging auto-synced; rollout complete after 96 s. Continuous traffic through the Service for 270 s:

```
requests ok=1905 failed=0 served_by_old=497 served_by_new=1408
```

Prod at that point: `OutOfSync/Missing`, 0 pods.

**Not verified locally:** that a merge *requires* a second person's approval. That needs GitHub branch protection (phase 8/11 on GitHub).

## Phase 12 — prod requires a manual sync

Promotion `staging → prod` merged as `f5dd137` (pushed 03:15:32Z). Argo CD compared prod to the new commit after 60 s. After a further 180 s:

```
gitops-api-prod   OutOfSync   Missing   f5dd137…
namespaces "gitops-prod" not found
operationState: ''   history: empty        => zero sync operations
```

You then authorized the sync (answered "Yes, sync prod now"). `scripts/argocd.sh app sync gitops-api-prod`:

```
Sync Policy: Manual     Phase: Succeeded     Start 03:20:17Z  Finished 03:20:20Z  (3 s)
Synced+Healthy 53 s after the command.   history: id=0 rev=f5dd137 automated=<empty>
```

Same artifact everywhere, in Git and live:

```
dev      git=sha256:de04de911d74  live pods: 1 × sha256:de04de911d74
staging  git=sha256:de04de911d74  live pods: 2 × sha256:de04de911d74
prod     git=sha256:de04de911d74  live pods: 3 × sha256:de04de911d74
/version in all three: git_sha 9432470e5463d7ce3c5a9a0b22838b34b63315f6
```

Prod drift is reported, not reverted (`kubectl scale --replicas=4`, injected 03:21:32Z):

```
t+15s  replicas=4 OutOfSync/Progressing syncs=1
t+45s  replicas=4 OutOfSync/Healthy     syncs=1
t+90s  replicas=4 OutOfSync/Healthy     syncs=1
t+150s replicas=4 OutOfSync/Healthy     syncs=1
argocd app diff:  < replicas: 4   > replicas: 3
manual sync -> replicas=3 Synced/Healthy
```

## Known gaps

- Review enforcement and CI-opened PRs are simulated; they need GitHub (phase 8/9).
- Who may sync prod is defined in `argocd-rbac-cm` (`role:release-manager`) but not exercised: `--core` uses kube RBAC
  and there is no SSO. In the lab, anyone with the kubeconfig can sync prod.
- Detection latency is polling-bound (~60 s). A Git webhook would make it near-instant.
- Rollback was documented in `PROMOTION.md` but not yet executed; that is phase 13.
