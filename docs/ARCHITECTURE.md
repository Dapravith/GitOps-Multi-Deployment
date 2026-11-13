# Architecture — GitOps Multi-Environment Deployment

## Goal

No human or CI job runs `kubectl apply` against the cluster. Every change to what runs
in dev, staging or production is a Git commit in the **config repo**, reviewed through a
pull request, and pulled into the cluster by **Argo CD**.

## Components

| Component | Choice | Why |
|---|---|---|
| Application | Rust 2024, Axum 0.8, Tokio | Small, fast, single static-ish binary; no DB or extra services (not needed to exercise GitOps) |
| Toolchain | `rust-toolchain.toml` → **1.99.0** | Reproducible builds; CI and local use the same compiler |
| Image | Multi-stage build → distroless `cc-debian13:nonroot` | No shell/package manager, runs as non-root, small attack surface |
| Registry | GitHub Container Registry (`ghcr.io`) | Lives next to the repos; CI pushes with the workflow's `GITHUB_TOKEN` |
| Cluster | kind (local) | Free, disposable, multi-node capable |
| Config mgmt | Kustomize base + overlays | Native to kubectl/Argo CD, patch-based, no templating language |
| CD | Argo CD (pull-based) | Cluster pulls from Git; **no cluster credentials in CI** |
| Secrets | Bitnami Sealed Secrets | Asymmetric encryption: only the in-cluster controller can decrypt |

## Repositories

```
GitOps-Multi-Deployment/          (workspace, not a repo)
├── docs/                         architecture, checklist, evidence
├── gitops-api/                   REPO 1: application source  → ghcr.io/<owner>/gitops-api
│   ├── src/  tests/  Cargo.toml  rust-toolchain.toml
│   ├── Dockerfile                (phase 4)
│   └── .github/workflows/        (phase 8) test → scan → build → push → open PR in config repo
└── gitops-config/                REPO 2: desired cluster state (Argo CD watches this)
    ├── apps/gitops-api/
    │   ├── base/                 Deployment, Service, ConfigMap, PDB, NetworkPolicy
    │   └── overlays/{dev,staging,prod}/   replicas, resources, config, image digest
    ├── argocd/
    │   ├── projects/             AppProject (allowed repos/namespaces)
    │   └── applications/         one Application per environment (sync policy differs)
    ├── sealed-secrets/           controller install reference (public cert only)
    └── docs/PROMOTION.md
```

Separation matters: CI has write access to the **app** repo's packages and can only
*propose* changes to the config repo via PR. Argo CD has read-only access to the config repo.

## Delivery flow

```mermaid
flowchart LR
  dev[Developer] -->|PR| app[gitops-api repo]
  app -->|merge to main| ci[GitHub Actions<br/>fmt · clippy · test · scan · build]
  ci -->|push multi-arch image| ghcr[(ghcr.io<br/>image@sha256)]
  ci -->|PR: bump dev digest| cfg[gitops-config repo]
  cfg -->|auto-sync| dev_ns[ns: gitops-dev]
  cfg -->|reviewed PR, auto-sync| stg_ns[ns: gitops-staging]
  cfg -->|reviewed PR + manual Sync| prd_ns[ns: gitops-prod]
  argo[Argo CD in cluster] -. pulls .-> cfg
  argo -. pulls image .-> ghcr
```

## Environment policy

| | dev | staging | prod |
|---|---|---|---|
| Image change arrives via | CI-opened PR (may auto-merge) | Promotion PR copying the dev digest, requires review | Promotion PR copying the staging digest, requires review |
| Argo CD sync | automated, `prune`, `selfHeal` | automated, `prune`, `selfHeal` | **manual** (operator clicks/`argocd app sync`) |
| Replicas | 1 | 2 | 3 + PDB |
| Drift | auto-reverted | auto-reverted | reported as OutOfSync, fixed by manual sync |

**Immutability rule:** overlays reference images **by digest** (`image@sha256:…`), never by
mutable tag. Promotion = copying the exact digest string from one overlay to the next, so
staging and prod run the byte-identical artifact tested in dev.

## Application contract

| Endpoint | Purpose | Kubernetes use |
|---|---|---|
| `GET /health` | 200 while the process serves HTTP | `livenessProbe` |
| `GET /ready` | 200 when accepting traffic, **503 during startup and drain** | `readinessProbe` |
| `GET /version` | `{name, version, git_sha, environment}` | proves which build runs where |

Config via env: `PORT` (8080), `APP_ENV` (`[a-z0-9-]+`), `LOG_FORMAT` (`json`|`pretty`),
`RUST_LOG`. Invalid config fails fast with exit 1. Logs are one-line JSON.

Shutdown: on SIGTERM the app flips `/ready` to 503, stops accepting, drains in-flight
requests, exits 0. The binary is PID 1 in the container and handles signals itself.

## Security baseline (applies in phases 4–7)

- Non-root UID, `readOnlyRootFilesystem`, `allowPrivilegeEscalation: false`, drop ALL caps, `seccompProfile: RuntimeDefault`
- Pod Security Admission `restricted` on all three namespaces
- Resource requests/limits on every container; default-deny NetworkPolicy
- No plaintext secrets or kubeconfigs in either repo; CI never holds cluster credentials
- Image scanning (Trivy) blocks HIGH/CRITICAL; GitHub Actions pinned by commit SHA
- Argo CD `AppProject` restricts source repo and destination namespaces
