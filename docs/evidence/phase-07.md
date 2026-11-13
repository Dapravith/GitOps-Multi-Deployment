# Evidence — Phase 7: Argo CD + declarative Applications

Captured 2026-11-12. Argo CD v3.5.7 on `gitops-lab` (K8s v1.36.4). You approved: a local Git server
(no GitHub changes yet) and local commits.

## Commits

| Repo | Commit | Content |
|---|---|---|
| gitops-api | `9432470` | phases 1–4. Before committing, re-verified the restructured `src/` (fmt ✓, clippy `-D warnings` ✓, 15 tests ✓) |
| gitops-config | `2c6682b` | phases 5–6 (kind bootstrap, overlays, validator) |
| gitops-config | `e78bb12` | phase 7 (Git server, Argo CD bootstrap, projects, Applications) |

Git had no global identity. I set a **repo-local** identity: `Dapravith <90898700+Dapravith@users.noreply.github.com>`
(GitHub no-reply address, so no personal email goes into history).

## Local Git server (`bootstrap/git-server/`)

- `alpine:3.24.4@sha256:f59abe65…`, plus `lighttpd` running `git http-backend` (smart HTTP), user 100:101,
  container started `--read-only --cap-drop ALL --security-opt no-new-privileges`, bare repos mounted `:ro`.
- Source of truth for Argo CD is `~/.gitops-lab/repos/gitops-config.git`, published with `git push lab main`.
- Pods can't resolve Docker container names (CoreDNS forwards to Docker Desktop's host DNS: `nameserver 192.168.65.254`),
  so `lab-infra/git` is a selector-less Service with an EndpointSlice pointing at the container IP (`172.18.0.6`).
- The first attempt failed because host port 8081 was already used by another process on your machine. Moved to 18081.

```
git ls-remote http://127.0.0.1:18081/git/gitops-config.git main  -> e78bb120… refs/heads/main
git push over HTTP                                               -> error: 403   (read-only, as intended)
pod: wget http://git.lab-infra.svc.cluster.local/git/gitops-config.git/info/refs?service=git-upload-pack
     -> "# service=git-upload-pack ... 2c6682b… HEAD"
hadolint                                                          -> ok
```

## Argo CD install (`bootstrap/argocd/`)

- Upstream `install.yaml` fetched by tag. `install.sh` verifies
  **sha256 `09bfbb9041b4a8503c6e4fa22cf41c403fa62be17526de353648a4551f5beb77`** before applying.
- Images re-pinned by digest: `quay.io/argoproj/argocd:v3.5.7@sha256:16df57da…` (7 containers),
  `redis:8.2.3-alpine@sha256:08ad0b1d…`.
- Dex Deployment, Service and NetworkPolicy removed (no SSO). `timeout.reconciliation: 60s`, anonymous off,
  `policy.default: role:readonly`, `role:release-manager` (sync on `gitops-prod/*`) bound to group `release-managers`.
- Server-side apply (the CRDs are too large for client-side). Install took 104 s; 6 pods Running, 6 upstream NetworkPolicies active.
- PSA: `kubectl label --dry-run=server ns argocd …enforce=restricted` reported **no violations**, so the namespace now
  **enforces `restricted`**. Recreated pods are still admitted.

## Declarative objects (`argocd/`)

| Object | Purpose |
|---|---|
| `AppProject/default` | locked: no repos, no destinations |
| `AppProject/platform` | root app only: `Application`/`AppProject` in `argocd` |
| `AppProject/gitops-nonprod` | dest `gitops-dev`, `gitops-staging`; allow-list: Namespace, ConfigMap, Service, SA, Deployment, NetworkPolicy, PDB, SealedSecret |
| `AppProject/gitops-prod` | dest `gitops-prod`; same allow-list |
| `Application/root` | app-of-apps over `argocd/` (recurse), automated prune + selfHeal, manages itself |
| `Application/gitops-api-dev`, `-staging` | automated prune + selfHeal; resources finalizer (cascade delete) |
| `Application/gitops-api-prod` | **no `automated`**; **no resources finalizer**, so deleting the Application can't delete prod |

All use `ServerSideApply`, `PruneLast`, `FailOnSharedResource`, and retry 3× with backoff.

`validate.sh` now also schema-checks `argocd/` (kubeconform + CRDs-catalog pinned to commit `89c736bd…`):
`8 resources … Valid: 8`. It enforces the prod guard rails. Negative test: adding `automated` plus a finalizer to
prod gives two `POLICY FAIL` lines, exit 1.

## Result after hand-over (~85 s after `root` was applied)

```
NAME                 PROJECT          SYNC        HEALTH    REVISION
gitops-api-dev       gitops-nonprod   Synced      Healthy   e78bb120cc6e…
gitops-api-prod      gitops-prod      OutOfSync   Missing   e78bb120cc6e…
gitops-api-staging   gitops-nonprod   Synced      Healthy   e78bb120cc6e…
root                 platform         Synced      Healthy   e78bb120cc6e…

gitops-dev      gitops-api-…-j96j7  1/1 Running
gitops-staging  gitops-api-…-dmbfl  1/1 Running
gitops-staging  gitops-api-…-z7vrn  1/1 Running
```

`argocd --core app get gitops-api-prod` (via `scripts/argocd.sh`, which uses kube RBAC, no password): `Sync Policy: Manual`,
`OutOfSync from main (e78bb12)`, and all 9 objects `OutOfSync / Missing`. **Prod was deliberately not synced**: that's
the phase-12 gate, and it needs your authorization.

Locked default project, probe Application (deleted afterwards):

```
InvalidSpecError: application destination … namespace 'gitops-dev' do not match any of the allowed destinations in project 'default'
InvalidSpecError: application repo http://git.lab-infra.svc.cluster.local/git/gitops-config.git is not permitted in project 'default'
```

Memory: node container 1.32 GiB (was ~0.78 GiB before Argo CD); git server 1.5 MiB; registry 12 MiB.

## Incident during verification: host CPU starvation

After commit `1513239` was picked up (in ~20 s), `gitops-api-dev` briefly showed `Synced / Progressing`.
Its manifests hadn't changed. The cause:

```
pod/gitops-api-…-j96j7  Liveness probe failed: Get "http://10.244.0.46:8080/health": context deadline exceeded
pod/gitops-api-…-j96j7  Readiness probe failed: … context deadline exceeded
kube-controller-manager / kube-scheduler: Liveness probe failed … Back-off restarting  (restarts=5 each)
node /proc/loadavg: 41.05 → 80.43 (8 vCPU)
docker stats: k8sops-dev-control-plane 211% CPU, gitops-lab 125%, dev-kafka-ui 96%, ai-ops-lab 95%
```

Argo CD reported this correctly: a Deployment with 0 available replicas is `Progressing`, not `Healthy`. Everything
recovered without intervention (`Synced/Healthy`, pod restarts 0). The root cause is that other workloads saturate the
shared Docker VM. **I did not stop them.** That's your decision: stop the other kind clusters or the Kafka stack, or give
Docker Desktop more CPUs. Until then, expect flaky probes and slow syncs, and treat probe timings in phase 13 with that in mind.
The app's probes use the default `timeoutSeconds: 1`. Raising it would hide this kind of contention instead of fixing it, so I left it.

## Known gaps

- The `bootstrap/` files (Argo CD install, namespace, lab-infra) are applied by script, not managed by Argo CD.
  Argo CD self-management is a possible later step.
- No webhook from the Git server, so changes are picked up by polling (≤60 s plus Git cache). Fine for the lab.
- `release-managers` is a placeholder group. Without SSO, only `admin` (not used) and `--core` kube-RBAC users can sync.
- I haven't tested whether a Namespace *name* outside a project's destinations is refused. That's planned for phase 13.
