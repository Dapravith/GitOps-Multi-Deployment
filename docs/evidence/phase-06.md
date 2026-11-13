# Evidence — Phase 6: Kustomize base + environment overlays

Captured 2026-11-12. kubectl v1.36.1 (built-in Kustomize v5.8.1), cluster `gitops-lab` v1.36.4.

## Layout

```
gitops-config/apps/gitops-api/
├── base/                    ServiceAccount, Deployment, Service, 3× NetworkPolicy, ConfigMap generator
├── components/pdb/          PodDisruptionBudget (maxUnavailable 1), opt-in per overlay
└── overlays/
    ├── dev/                 ns gitops-dev,     1 replica,  10m/16Mi req, 64Mi limit,  RUST_LOG=info,gitops_api=debug
    ├── staging/             ns gitops-staging, 2 replicas, 25m/24Mi req, 96Mi limit,  + PDB
    └── prod/                ns gitops-prod,    3 replicas, 50m/32Mi req, 128Mi limit, + PDB + topology spread
```

Every overlay pins `ghcr.io/dapravith/gitops-api@sha256:4bfc7038…fbea8f` (the phase-4 multi-arch
index). That `images:` block is the only thing promotion will change.

Key base settings: `maxUnavailable: 0`/`maxSurge: 1`, `minReadySeconds: 5`, startup/readiness/liveness
probes, `preStop.sleep: 5s`, non-root 65532, read-only rootfs, drop ALL, seccomp RuntimeDefault,
no SA token, `enableServiceLinks: false`, hash-suffixed ConfigMap (config change ⇒ rollout).

## Cluster change: local registry as ghcr.io mirror

Problem found: a `kind load`-ed image is only known to containerd under synthetic `import-…@sha256`
names, so a pod spec with `ghcr.io/…@sha256:<index digest>` could not resolve it.

Fix (standard kind local-registry pattern, now in `bootstrap/kind/`):
- `registry:3.1.1@sha256:325b4b29…` as `gitops-lab-registry`, bound to `127.0.0.1:5001`
- `containerdConfigPatches` enabling `/etc/containerd/certs.d`; `ghcr.io/hosts.toml` lists the local
  registry first, real `https://ghcr.io` as fallback. The cluster was recreated (it was empty).

```
docker push localhost:5001/dapravith/gitops-api:bootstrap  -> digest sha256:4bfc7038…fbea8f (arm64 + amd64)
crictl pull ghcr.io/dapravith/gitops-api@sha256:4bfc7038…  -> OK
```

GHCR has no such repository yet, so the pull can only have been served by the mirror. Digests are
content addresses, so the mirror cannot substitute different bytes for a pinned digest.

## Offline validation — `gitops-config/scripts/validate.sh`

Render (kubectl kustomize) → schema (kubeconform v0.8.0, `-strict`, K8s 1.36.0) → policy
([`policy_check.py`](../../gitops-config/scripts/policy_check.py): digest-only images, no `Secret`,
namespace + PSA label, replicas ≥ 1/2/3, SA token off, non-root, seccomp, no privilege escalation,
read-only rootfs, drop ALL, CPU/memory requests, memory limit, probes, NetworkPolicy present, PDB in
staging/prod).

```
== dev      Summary: 8 resources - Valid: 8, Invalid: 0   policy OK [dev]: 8 objects
== staging  Summary: 9 resources - Valid: 9, Invalid: 0   policy OK [staging]: 9 objects
== prod     Summary: 9 resources - Valid: 9, Invalid: 0   policy OK [prod]: 9 objects
ALL OVERLAYS VALID   (exit 0)
```

Negative tests on a scratch copy — each must fail:

| Mutation | Result |
|---|---|
| dev uses `newTag: latest` | `image 'ghcr.io/dapravith/gitops-api:latest' is not ghcr.io/...@sha256:<digest>` → exit 1 |
| plaintext `Secret` in staging | `Secret/db: plaintext Secret must not be committed` → exit 1 |
| `allowPrivilegeEscalation: true` + PDB removed (prod) | both reported → exit 1 |
| `replica:` typo | kubeconform `Invalid: 1` → exit 1 |

## Runtime validation (temporary, outside Argo CD)

Rendered output applied with `kubectl apply --server-side --field-manager=phase6-validation`,
then **all three namespaces deleted** so Argo CD starts clean in phase 7.

- Apply / PSA warnings: 0 in each environment (namespaces enforce + audit + warn `restricted`).
- Rollouts: dev 1/1, staging 2/2, prod 3/3, all on `ghcr.io/dapravith/gitops-api@sha256:4bfc7038…`.
- `/version` through each environment's Service:

```
[dev]     {"name":"gitops-api","version":"0.1.0","git_sha":"local-phase4","environment":"dev"}
[staging] {"name":"gitops-api","version":"0.1.0","git_sha":"local-phase4","environment":"staging"}
[prod]    {"name":"gitops-api","version":"0.1.0","git_sha":"local-phase4","environment":"prod"}
```

### NetworkPolicy (kindnet `v20260820-69b56db7`, policy engine ready)

Design flaw found and fixed during this phase: the first draft's `default-deny-all` also denied
**egress** for callers, so the "allow from same namespace" rule was unusable. Also, Kustomize
`includeSelectors: true` was injecting app labels into NetworkPolicy selectors. Now: explicit
selectors everywhere, callers need `app.kubernetes.io/role: api-client`, and they get a matching
egress policy (API pods + kube-dns only).

Long-lived test pods in `gitops-dev`, after registration with kindnet:

```
api-client -> dev API (allow)      200-OK
api-client -> internet (deny)      BLOCKED
api-client -> prod API (deny)      BLOCKED
rogue pod  -> dev API (deny)       BLOCKED
rogue pod  -> internet (deny)      BLOCKED
rogue pod  -> DNS                  DNS-BLOCKED
```

**Test-method caveat (an actual observation, not a policy bug):** the first egress test used
`kubectl run --rm` pods that connected within milliseconds of starting, and they *reached*
1.1.1.1. kindnet filters only the IPs in its nftables set `podips-v4`, and a new pod is added
shortly *after* it starts. A long-lived pod was blocked as expected. On a real cluster with a
different CNI, re-test this. Phase 13 will reuse the long-lived pod method.

### Zero-downtime rollout (staging, 2 replicas)

60 s of continuous `GET /ready` through the Service (every 50 ms), while `rollout restart` replaced
both pods:

```
requests ok=980 failed=0
before: gitops-api-749d6b658b-krxxm, gitops-api-749d6b658b-zpbc8
after:  gitops-api-76c94c9b4c-84nlb, gitops-api-76c94c9b4c-j2rpl  (+ zpbc8 still in its 5 s preStop)
```

PDBs: staging and prod `maxUnavailable 1`, `disruptionsAllowed 1`, healthy 2 and 3.

## Known gaps

- The image digest is the local phase-4 build. CI replaces it with a GHCR-published digest (phase 8).
- With one node, prod's topology spread is `ScheduleAnyway` and gives no real HA.
- The policy gate is a project script, not an admission controller. The same rules could be
  enforced in-cluster with Kyverno/Gatekeeper (stretch goal), memory permitting.
