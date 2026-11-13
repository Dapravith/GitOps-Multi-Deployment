# Evidence — Phase 5: kind cluster

Captured 2026-11-12. kind v0.33.0, kubectl v1.36.1, Docker Desktop (8 CPU, 7.75 GiB shared VM).

## Decisions

| Decision | Reason |
|---|---|
| Single node (`gitops-lab-control-plane`, schedulable) | The Docker VM is shared with two other kind clusters and a Kafka/Postgres stack (~4.7 GiB in use before this cluster). Argo CD + Sealed Secrets + three environments fit on one node; workers can be added in [`kind-config.yaml`](../../gitops-config/bootstrap/kind/kind-config.yaml). |
| `kindest/node:v1.36.4@sha256:099e0493…` | Digest from kind v0.33.0 release notes; matches kubectl 1.36 client (zero skew). kind's default would be 1.37.0. |
| API server bound to `127.0.0.1` | Not reachable from the LAN. |
| Separate kubeconfig `~/.kube/gitops-lab.kubeconfig` (mode 600) | Your default `~/.kube/config` current-context stayed `kind-ai-ops-lab`. File lives outside every Git repo, so it can never be committed. |

Bootstrap: [`gitops-config/bootstrap/kind/create-cluster.sh`](../../gitops-config/bootstrap/kind/create-cluster.sh) (idempotent; re-running exports the kubeconfig instead of recreating).

## Cluster health

```
kind create cluster ... => "Ready after 19s"
gitops-lab-control-plane  Ready  control-plane  v1.36.4  Debian 13 (trixie)  7.0.14-linuxkit (arm64)  containerd://2.3.4
Client Version: v1.36.1 / Server Version: v1.36.4
/readyz?verbose => readyz check passed
default kubeconfig current-context after creation: kind-ai-ops-lab   (unchanged)
```

All system pods `1/1 Running`: coredns ×2, etcd, kindnet, kube-apiserver, kube-controller-manager,
kube-proxy, kube-scheduler, local-path-provisioner.

DNS from a non-root pod: `kubernetes.default.svc.cluster.local -> 10.96.0.1`.

Node container memory after boot: ~791 MiB.

## Image availability

```
kind load docker-image gitops-api:phase4 --name gitops-lab   => loaded (multi-arch index)
crictl images: docker.io/library/gitops-api  phase4  ebf9d4d0f185a  11.8MB   (arm64 variant)
```

This local load is only for cluster validation. Real deployments will pull
`ghcr.io/...@sha256:` from the registry (phase 8).

## Pod Security Admission `restricted` (throwaway namespace `smoke-phase5`, deleted afterwards)

1. Pod with default security context → **rejected**:

```
Error from server (Forbidden): pods "bad" is forbidden: violates PodSecurity "restricted:v1.36":
allowPrivilegeEscalation != false, unrestricted capabilities, runAsNonRoot != true, seccompProfile ...
```

2. Hardened pod (runAsNonRoot 65532, seccomp RuntimeDefault, drop ALL, readOnlyRootFilesystem,
   no SA token, requests/limits, HTTP probes) → **admitted, Ready**:

```
GET /health  -> {"status":"ok"} [200]
GET /ready   -> {"status":"ready"} [200]
GET /version -> {"name":"gitops-api","version":"0.1.0","git_sha":"local-phase4","environment":"smoke"} [200]
```

3. `kubectl delete pod` (SIGTERM via kubelet) → pod gone in **1.2 s** of the 30 s grace period:

```
{"message":"shutdown signal received, draining connections","signal":"SIGTERM",...}
{"message":"shutdown complete",...}
```

These smoke objects were created imperatively **only to validate the cluster**; namespace
`smoke-phase5` was deleted (`NotFound` confirmed). From phase 6 on, everything goes through Git + Argo CD.

## Findings carried forward

- The app exits ~1 ms after SIGTERM. Behind a Service, endpoint removal is asynchronous, so a
  few requests could hit a terminating pod. Phase 6 will add `lifecycle.preStop.sleep` (native
  in K8s ≥1.30, no shell needed in distroless) before SIGTERM.
- NetworkPolicy enforcement by kindnet is **not yet verified**; to test in phase 6/13 with a
  default-deny policy.
- Memory headroom on the shared Docker VM is limited (~2.2 GiB free before Argo CD). If Argo CD
  pods get OOM-killed or stay Pending, stopping unrelated containers is the fix — that's your call.
