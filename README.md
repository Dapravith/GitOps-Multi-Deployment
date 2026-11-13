# GitOps Multi-Environment Deployment (Rust + Argo CD)

Workspace holding two independent Git repositories plus shared docs:

- [`gitops-api/`](gitops-api/) — Rust 2024 / Axum application source, CI, Dockerfile
- [`gitops-config/`](gitops-config/) — Kustomize overlays and Argo CD Applications
- [`docs/design/`](docs/design/) — HLD, LLD and step-by-step implementation guide with evidence links
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — original phase-1 design intent
- [`docs/CHECKLIST.md`](docs/CHECKLIST.md) — phase-by-phase status with evidence links
- [`docs/evidence/`](docs/evidence/) — captured command output proving each phase
