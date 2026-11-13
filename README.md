# GitOps Multi-Environment Deployment

> A Rust service delivered to **dev → staging → prod** with Argo CD. Every change is a Git commit, every
> environment runs the same signed image digest, and every claim is backed by captured evidence.

![Rust](https://img.shields.io/badge/Rust-1.99.0-orange)
![Argo CD](https://img.shields.io/badge/Argo%20CD-v3.5.7-blue)
![Kubernetes](https://img.shields.io/badge/Kubernetes-v1.36.4-326ce5)
![Sealed Secrets](https://img.shields.io/badge/Sealed%20Secrets-v0.41.2-green)

---

## Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Repository layout](#repository-layout)
- [Environments](#environments)
- [Getting started](#getting-started)
- [Release workflow](#release-workflow)
- [Security model](#security-model)
- [Project status](#project-status)
- [Documentation](#documentation)

## Overview

| | |
|---|---|
| **Application** | `gitops-api`: a minimal Rust 2024 / Axum service exposing `/health`, `/ready` and `/version` |
| **Delivery** | Pull-based GitOps. Argo CD reconciles the cluster from the config repository. CI never holds cluster credentials |
| **Artifact** | Multi-arch image (`linux/amd64`, `linux/arm64`) on GHCR, referenced only by `@sha256` digest, with SBOM and Sigstore provenance |
| **Promotion** | Copy one digest line from one overlay to the next, through a pull request. Nothing is rebuilt between environments |
| **Runtime** | Local [kind](https://kind.sigs.k8s.io/) cluster (lab). GitHub and GHCR are the remote side |

## Architecture

```mermaid
flowchart LR
  devUser(["Developer"])
  apiRepo["gitops-api repo"]
  ghcr[("GHCR: image by digest")]
  updater["image-updater"]
  cfgRepo["gitops-config repo"]
  argo["Argo CD (in-cluster)"]
  devNs["gitops-dev"]
  stgNs["gitops-staging"]
  prdNs["gitops-prod"]

  devUser -->|"pull request"| apiRepo
  apiRepo -->|"CI: test, scan, build, sign"| ghcr
  ghcr -->|"verify CI result and provenance"| updater
  updater -->|"dev PR"| cfgRepo
  argo -.->|"polls"| cfgRepo
  argo -->|"auto-sync"| devNs
  argo -->|"auto-sync after reviewed PR"| stgNs
  argo -->|"manual sync after reviewed PR"| prdNs
  devNs -.->|"pull by digest"| ghcr
  stgNs -.->|"pull by digest"| ghcr
  prdNs -.->|"pull by digest"| ghcr
```

Full design: [High-Level Design](docs/design/HLD.md) · [Low-Level Design](docs/design/LLD.md).

## Repository layout

This workspace holds **two independent Git repositories** plus shared documentation.

```
GitOps-Multi-Deployment/
├── gitops-api/              # Repo 1: application source → ghcr.io/dapravith/gitops-api
│   ├── src/  tests/         #   Axum service, unit and integration tests
│   ├── Dockerfile           #   digest-pinned, cross-compiled, distroless, non-root
│   └── .github/workflows/   #   ci.yml: test → scan → build → push → attest
├── gitops-config/           # Repo 2: desired cluster state (watched by Argo CD)
│   ├── apps/gitops-api/     #   Kustomize base, components, overlays/{dev,staging,prod}
│   ├── argocd/              #   AppProjects and Applications (app-of-apps)
│   ├── platform/            #   cluster add-ons (Sealed Secrets)
│   ├── bootstrap/           #   kind cluster, Argo CD install, lab Git server
│   ├── scripts/             #   validate, set-image, promote, seal, argocd wrapper
│   └── .github/workflows/   #   validate.yml (required check), image-updater.yml
└── docs/
    ├── design/              # HLD, LLD, implementation guide
    ├── evidence/            # captured command output per phase
    ├── CHECKLIST.md         # phase-by-phase status
    └── ARCHITECTURE.md      # original phase-1 design intent
```

| Repository | Remote | Who writes |
|---|---|---|
| `gitops-api` | [github.com/Dapravith/gitops-api](https://github.com/Dapravith/gitops-api) | developers; CI publishes images |
| `gitops-config` | [github.com/Dapravith/gitops-config](https://github.com/Dapravith/gitops-config) | pull requests only (`validate` required); Argo CD reads |

## Environments

| | dev | staging | prod |
|---|---|---|---|
| Namespace | `gitops-dev` | `gitops-staging` | `gitops-prod` |
| New image arrives via | bot PR, auto-merged after `validate` | `promote.sh dev staging` PR | `promote.sh staging prod` PR |
| Argo CD sync | automated, prune, self-heal | automated, prune, self-heal | **manual** |
| Manual drift | reverted in seconds | reverted in seconds | reported as `OutOfSync` |
| Replicas | 1 | 2 + PDB | 3 + PDB + topology spread |
| Pod Security | `restricted` | `restricted` | `restricted` |

## Getting started

**Prerequisites:** Docker (with buildx), kind 0.33, kubectl 1.36, `gh`. The Rust toolchain and the
scanners (Trivy, gitleaks, kubeconform, hadolint, yq, argocd CLI) run as digest-pinned containers.

```bash
# The lab uses its own kubeconfig; your default context is never changed.
export KUBECONFIG="$HOME/.kube/gitops-lab.kubeconfig"
```

**1. Build and test the application**

```bash
cd gitops-api
scripts/cargo.sh test --locked
docker buildx build --platform linux/arm64,linux/amd64 \
  --build-arg GIT_SHA="$(git rev-parse HEAD)" -t gitops-api:local .
```

**2. Create the cluster and install Argo CD**

```bash
cd gitops-config
bootstrap/kind/create-cluster.sh        # idempotent: kind node + ghcr.io registry mirror
bootstrap/argocd/install.sh             # verifies the upstream checksum, then hands over to Git
scripts/argocd.sh app list
```

**3. Validate desired state offline**

```bash
scripts/validate.sh                     # render → schema → policy → misconfig → secret scan
```

Step-by-step instructions, with the expected output for each step: [Implementation Guide](docs/design/IMPLEMENTATION-GUIDE.md).

## Release workflow

```bash
scripts/promote.sh dev staging          # gated: source Synced/Healthy, live pods match Git digest
scripts/promote.sh staging prod         # creates promote/<env>-<digest12> with a one-line diff
scripts/argocd.sh app sync gitops-api-prod   # prod only: explicit release step after merge
```

**Rollback** is a `git revert -m 1 <promotion-merge>`. dev and staging roll back automatically;
prod needs a manual sync. Full procedure: [PROMOTION.md](gitops-config/docs/PROMOTION.md).

## Security model

| Control | Implementation |
|---|---|
| No cluster credentials outside the cluster | Pull-based Argo CD; CI only pushes images |
| Immutable, verified artifacts | Digest-only images; SLSA provenance signed with Sigstore and verified before promotion |
| Pinned supply chain | Actions pinned by commit SHA; base images and tools pinned by digest |
| Scanning gates | Trivy (crates, image, manifests) blocks fixable HIGH/CRITICAL; gitleaks scans the full history |
| Workload hardening | Non-root UID 65532, read-only root FS, all capabilities dropped, seccomp, no SA token, default-deny NetworkPolicy |
| Blast-radius limits | Locked `default` AppProject; per-environment allow-lists for kinds and Namespaces by name |
| Secrets | [Sealed Secrets](gitops-config/docs/SECRETS.md): offline sealing, strict scope, per-namespace controller RBAC |

## Project status

As of **2026-11-13**. Details and evidence: [CHECKLIST.md](docs/CHECKLIST.md).

| Area | Status |
|---|---|
| Application, image, cluster, Kustomize overlays | ✅ verified |
| Argo CD, dev auto-sync, prod manual sync, drift and rollback drills | ✅ verified in the lab |
| Sealed Secrets (dev, staging) | ✅ verified · prod not enabled |
| GitHub CI: build, scan, publish, provenance | ✅ verified |
| Automated dev promotion (image-updater) | ❌ failing: see [Implementation Guide §8.3](docs/design/IMPLEMENTATION-GUIDE.md#83-fix-the-image-updater-defect-b5) |
| Enforced PR approval for staging/prod | ❌ not enforced (single maintainer) |
| Argo CD sourcing from GitHub | ⬜ merged, not yet verified on a cluster |
| Runbooks and audit report | ⬜ not started |

**Legend:** ✅ verified by captured output · 🟡 partial · ⬜ not done or not verified · ❌ known defect

## Documentation

| Document | Purpose |
|---|---|
| [High-Level Design](docs/design/HLD.md) | Goals, components, trust boundaries, decisions, measured results, target architecture |
| [Low-Level Design](docs/design/LLD.md) | Module, manifest, pipeline and tooling details with `file:line` references |
| [Implementation Guide](docs/design/IMPLEMENTATION-GUIDE.md) | Phase-by-phase steps: do, verify, capture |
| [Checklist](docs/CHECKLIST.md) | Current status per phase |
| [Evidence](docs/evidence/) | Raw command output behind every ✅ |
| [Promotion runbook](gitops-config/docs/PROMOTION.md) | Promote, verify, roll back, handle drift |
| [Secrets runbook](gitops-config/docs/SECRETS.md) | Seal, rotate, back up and restore |
| [Original architecture](docs/ARCHITECTURE.md) | Phase-1 design intent (superseded by the HLD) |
