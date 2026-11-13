# Evidence — Phase 13: security, deployment failures, rollback, drift, outages

Captured 2026-11-12 on the local lab. Load average on the shared Docker VM stayed at 20–40 (8 vCPU).
All failure tests ran in **dev/staging**. Prod was not touched. Request counts come from a long-lived
`api-client` pod polling `GET /version` through the Service every 100 ms.

Timing note: after each `git push`, my test helper set Argo CD's `refresh` annotation, which is what a
Git webhook would do. Times below are with that. Without it, add up to ~60 s for polling.

## Summary

| # | Test | Result | Found / fixed |
|---|---|---|---|
| S1 | Trivy misconfig scan of rendered manifests | 3 rule IDs × 3 envs, 101 checks passed per env | 3 accepted with reasons in `.trivyignore`; scan added to `validate.sh` |
| S2 | Trivy scan of `Cargo.lock` (70 crates) | 0 vulnerabilities | closes phase-4 gap "crate CVEs not scanned" |
| S3 | Trivy scan of released image `de04de91` | 0 HIGH/CRITICAL fixable | — |
| S4 | Hostile manifests via `gitops-nonprod` project | Role, ClusterRoleBinding, Secret, foreign namespace **refused** | — |
| S5 | Allowed kinds abused (new Namespace, privileged Deployment) | **Namespace `probe-evil-ns` was created**; privileged pod blocked by PSA | **fixed**: Namespace allow-list restricted by name |
| S6 | Live pod hardening + app ServiceAccount RBAC | uid 65532, RO rootfs, drop ALL, seccomp, no SA token; SA can do nothing | — |
| F1 | Non-existent image digest in dev | old pod kept serving; app `Degraded`; promotion gate refused; revert healed in 92 s | **fixed**: `set-image.sh` checks registry + both architectures |
| F2 | Config the app rejects (`PORT=0`) in dev | CrashLoopBackOff, **1964 requests / 0 failed**; revert healed in 193 s | **fixed**: `policy_check.py` mirrors app config rules |
| R1 | Rollback staging with `git revert -m 1` of the promotion merge | previous digest live in **20 s**; **2211 requests / 0 failed** | — |
| D1 | Hand-made ConfigMap in staging | `OrphanedResourceWarning`, not deleted | — |
| D2 | Hand-added annotation on Deployment | **not detected** (field not owned by Git) | limitation, documented |
| O1 | Git server down 3+ min | `ComparisonError`, **1357 requests / 0 failed**; **self-heal paused** | limitation, documented |
| O2 | Argo CD application-controller down | **1077 requests / 0 failed**; drift not reverted until controller back | expected |

## Security

### S1 — `trivy config` (Trivy 0.76.1) on `.rendered/{dev,staging,prod}.yaml`

```
dev.yaml passed 101 failed 3   (same for staging, prod)
LOW    KSV-0011  CPU not limited
MEDIUM KSV-01010 ConfigMap with sensitive content
MEDIUM KSV-0125  Restrict container images to trusted registries
```

- KSV-01010 is a **false positive**: `stores sensitive contents in key(s) or value(s) '{"PORT"}'`.
- KSV-0125: `ghcr.io` is our registry, and images are digest-pinned (enforced separately).
- KSV-0011: deliberate. CPU requests without CPU limits avoids CFS throttling. Memory limits are enforced.

All three are in [`.trivyignore`](../../gitops-config/.trivyignore) with reasons. `validate.sh` now runs the scan.
Negative test: adding `hostNetwork: true` gives `KSV-0009 (HIGH) … MISCONFIG FAIL`.

### S2/S3

```
Cargo.lock │ cargo │ 0          (70 crates)
gitops-lab-registry:5000/dapravith/gitops-api@sha256:de04de91… (debian 13.7) │ 0   (HIGH/CRITICAL, fixable)
```

### S4/S5 — Argo CD project boundaries

Temporary branch `test/sec-probe` with temporary Applications in project `gitops-nonprod`. All deleted afterwards,
and the branch was removed from the lab Git server.

Probe A, kinds the project does not allow:

```
Phase: Failed
resource :Secret is not permitted in project gitops-nonprod
namespace kube-system is not permitted in project 'gitops-nonprod'
resource rbac.authorization.k8s.io:ClusterRoleBinding is not permitted in project gitops-nonprod
resource rbac.authorization.k8s.io:Role is not permitted in project gitops-nonprod
cluster: role/clusterrolebinding/secret/foreign configmap -> all NotFound
```

Probe B, allowed kinds used maliciously:

```
Phase: Succeeded
namespace/probe-evil-ns created                                    <- REAL GAP
deployment.apps/probe-privileged created, but pods forbidden:
  violates PodSecurity "restricted:v1.36": host namespaces (hostNetwork=true), privileged ...
```

Fix (commit "fix(security): restrict project Namespace allow-list by name"): `clusterResourceWhitelist` entries now
carry `name: gitops-dev` / `gitops-staging` (nonprod) and `gitops-prod` (prod). Re-run of probe B:

```
Phase: Failed
Message: one or more synchronization tasks are not valid: resource :Namespace is not permitted in project gitops-nonprod
namespaces "probe-evil-ns" not found
```

Layered defence worked as intended: even before the fix, the privileged pod never ran, because PSA stopped it.

### S6 — runtime posture

```
prod pod: uid=65532 nonRoot=true seccomp=RuntimeDefault roFS=true privEsc=false caps=["ALL"] saToken=false volumes=<none>
ServiceAccount gitops-prod:gitops-api: only selfsubject*reviews; can-i get pods/secrets, list configmaps,
  create deployments, get nodes -> no
Argo CD Services: all ClusterIP (nothing exposed outside the cluster)
```

Observations, not fixed:

- `argocd-initial-admin-secret` still exists. Argo CD's guidance is to delete it once the admin password is changed or
  SSO is set up. The lab uses `--core`, so the admin login is unused.
- `argocd-application-controller` has `*/*/*` on the cluster (upstream default). It's mitigated by the AppProject
  allow-lists tested above. A tighter, namespace-scoped install is possible but not done.

## Deployment failures

### F1 — digest that does not exist

```
validate.sh on the bad digest: PASSES                      <- gap
t+20s  Synced/Progressing  pods: Running(1/1) ContainerCreating(0/1)
t+50s  Synced/Progressing  pods: Running(1/1) ImagePullBackOff(0/1)
t+140s Synced/Degraded     pods: Running(1/1) ErrImagePull(0/1)
Deployment: Available=True MinimumReplicasAvailable; Progressing=False ProgressDeadlineExceeded
event: Failed to pull image "ghcr.io/dapravith/gitops-api@sha256:deadbeef…": rpc error: code = NotFound
promote.sh dev staging -> gate: gitops-api-dev is Synced/Degraded, must be Synced/Healthy
git revert -> gitops-api-dev Synced/Healthy after 92s
```

The old pod stayed Ready throughout (`maxUnavailable: 0`). **No request-level numbers** for this test: creating the
load-client pod failed with `Unable to connect to the server: net/http: TLS handshake timeout` (API server starved).
The test helper now retries.

Fix: `set-image.sh` resolves the digest in the registry (`IMAGE_REGISTRY`, the lab registry by default, `ghcr.io` in CI)
and requires both `amd64` and `arm64`:

```
digest not found in registry: localhost:5001/dapravith/gitops-api@sha256:deadbeef…   exit=1
…@sha256:d98b8f67… (arm64-only child manifest) is missing platform "architecture":"amd64"   exit=1
good digest -> exit 0
```

### F2 — config the app rejects

```
validate.sh on PORT=0: PASSES                              <- gap
t+90s  OutOfSync/Progressing  CrashLoopBackOff(0/1,restarts=2) Running(1/1,restarts=0)
logs --previous: configuration error: PORT must be an integer between 1 and 65535, got "0"
lastState: Error exitCode=1
revert -> Synced/Healthy after 193s
traffic: requests ok=1964 failed=0 (all served by the old pod)
```

Fix: `policy_check.py` validates `PORT` 1–65535, `APP_ENV` equal to the overlay name, and `LOG_FORMAT`. Negative tests
for `PORT=0`, `APP_ENV=prod` in dev, and `LOG_FORMAT=xml` all give `POLICY FAIL`.

Observation: during F2 the app showed `OutOfSync/Progressing` rather than `Synced/Progressing`. I didn't
investigate this further. A likely cause is the sync operation retrying while the host was CPU-starved.

## Rollback (R1)

```
git revert -m 1 ed6b167        (the dev->staging promotion merge)
-    digest: sha256:de04de911d746bd9a3388b7f450931febe819e8c8ca723d660a67052f0af2482
+    digest: sha256:4bfc7038e9ecfb9503c113d039dc14be0e4cd60e6a8ef11b2984c60059fbea8f
push -> Synced/Healthy 20s; push -> every pod on previous digest 20s
traffic: requests ok=2211 failed=0 (98 on new build before the switch, 2113 on previous)
/version after: git_sha local-phase4
roll-forward (revert of the revert): Synced/Healthy after 20s, git_sha 9432470…
```

## Drift and outages

```
D1 hand-made ConfigMap: Synced/Healthy + OrphanedResourceWarning: Application has 1 orphaned resources; not deleted
D2 hand-added annotation lab/hand-edit=yes: app stays Synced, annotation kept
O1 git server stopped 04:28:27:
   ComparisonError: … failed to list refs: Get "http://git.lab-infra.svc.cluster.local/…"
   staging pods Running(1/1) ×2; traffic ok=1357 failed=0
   kubectl scale --replicas=5 while Git down -> still 5 after 30 s (self-heal paused)
   start.sh -> lab-infra/git -> 172.18.0.6:8080 (same IP); Synced/Healthy, replicas back to 2
O2 application-controller scaled to 0 at 04:30:57:
   kubectl scale --replicas=4 -> still 4 after 30 s
   controller Ready 04:31:40 -> replicas already back to 2 at first check
   traffic ok=1077 failed=0
```

Takeaways: pull-based CD keeps serving when the control plane (Git or Argo CD) is down, but it also **stops
correcting** drift until the control plane returns. Alerting on `ComparisonError` and on the controller being down
matters as much as alerting on the app.

## Not covered in phase 13

- **Sealed Secrets is not installed yet.** The "no plaintext secrets" rule is enforced in three places: the
  `policy_check.py` Secret ban, the AppProject allow-list (S4 refused a `Secret`), and `.gitignore`. But the positive
  path (encrypt → commit → controller decrypts) does not exist yet. It's required by the brief.
- No failure test in prod, and no prod rollback. Both need your authorization for each prod sync.
- NetworkPolicy was tested in phase 6, not repeated here.
