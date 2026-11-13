# Evidence: design baseline (2026-11-13)

Captured 2026-11-13 ~10:05–10:30 (UTC+7) for the HLD/LLD in [`docs/design/`](../design/).
All commands are read-only. Nothing was pushed, synced or changed on GitHub or in the cluster.

## B1. Lab runtime is currently down

```
$ kubectl --kubeconfig ~/.kube/gitops-lab.kubeconfig get nodes
The connection to the server 127.0.0.1:62986 was refused
$ docker info
Cannot connect to the Docker daemon at unix:///Users/rothadapravith/.docker/run/docker.sock
```

Docker Desktop is stopped, so the kind cluster, lab registry and lab Git server are all down.
**Every live-cluster claim in the design docs therefore cites the dated phase evidence from
2026-11-12, not a run from today.** Container-based tools (`cargo.sh`, kubeconform, Trivy, gitleaks)
could not be re-run either.

## B2. Offline validation re-run today (no Docker needed)

kubectl v1.36.1 (Kustomize v5.8.1) rendered each overlay. `scripts/policy_check.py` ran on the output,
with PyYAML in a throwaway venv standing in for the yq container:

```
policy OK [dev]: 10 objects        exit=0
policy OK [staging]: 10 objects    exit=0
policy OK [prod]: 9 objects        exit=0
negative (dev image -> :latest):
  POLICY FAIL [dev] Deployment/gitops-api[api]: image 'ghcr.io/dapravith/gitops-api:latest' is not ghcr.io/...@sha256:<digest>
  exit=1
```

Rendered images (all three environments):
`ghcr.io/dapravith/gitops-api@sha256:de04de911d746bd9a3388b7f450931febe819e8c8ca723d660a67052f0af2482`
(the lab build of `gitops-api@9432470`, not yet a GHCR-published digest).

## B3. GitHub state (phase 8 has started)

Both repositories exist and are **public**: `Dapravith/gitops-api` and `Dapravith/gitops-config`.
Local `main` equals `origin/main` in both.

`gitops-api` CI (`.github/workflows/ci.yml`):

```
2026-11-12T08:06:21Z  ci  push  main  success  28ae24b   jobs: test ✓ scan-source ✓ image ✓
2026-11-12T07:52:14Z  ci  workflow_dispatch  failure  ee9b787   (job promote-dev, removed in 28ae24b)
```

`gitops-config` workflows:

```
validate  pull_request feat/image-updater   success  2480733
validate  push main                         success  3b269a3
image-updater schedule  failure  2026-11-12T16:46:51Z
image-updater schedule  failure  2026-11-12T22:27:39Z
image-updater schedule  failure  2026-11-13T02:49:23Z
```

Merged PRs in `gitops-config`: #1 (Argo CD reads GitHub), #2 (pull-based image updater). Both
authored and merged by the repository owner.

## B4. Published image + provenance verified

```
$ gh attestation verify oci://ghcr.io/dapravith/gitops-api@sha256:910d9b7b358d1c65cd069e3c5a4a2a5b89b48a6fe4711b1fd5ecd15add10a503 \
    --repo Dapravith/gitops-api --signer-workflow Dapravith/gitops-api/.github/workflows/ci.yml
exit=0
predicate: commit=28ae24bf523c4aa620e23131066116df1270276b ref=refs/heads/main
```

So `ci.yml` on `main` built that digest from commit `28ae24b`, and Sigstore signed it. That digest has
**not** reached any overlay yet (see B5).

## B5. Defect: image-updater cannot open the dev PR

First run (2026-11-12T16:46Z):

```
Switched to a new branch 'ci/dev-28ae24b'
 * [new branch]      ci/dev-28ae24b -> ci/dev-28ae24b
pull request create failed: GraphQL: GitHub Actions is not permitted to create or approve pull requests (createPullRequest)
```

Repository setting: `GET /repos/Dapravith/gitops-config/actions/permissions/workflow` returns
`{"default_workflow_permissions":"read","can_approve_pull_request_reviews":false}`.

Every later run:

```
 ! [rejected]        ci/dev-28ae24b -> ci/dev-28ae24b (stale info)
```

The first run left the branch on GitHub (`branches: ci/dev-28ae24b, main`) with no PR. `actions/checkout`
fetches only `main`, so `git push --force-with-lease` has no remote-tracking ref for the branch and refuses.

**Two causes, both need fixing:** (1) the repository setting (a remote change that needs the owner's OK);
(2) the workflow must handle an existing bot branch. See
[IMPLEMENTATION-GUIDE.md, step 8.3](../design/IMPLEMENTATION-GUIDE.md#83-fix-the-image-updater-defect-b5).

## B6. Branch rules: review is not enforced

`gitops-config` ruleset 27097700 "main: PR + validate", active, on the default branch, with no bypass actors:

```
deletion, non_fast_forward,
pull_request: required_approving_review_count=0, allowed_merge_methods=[merge],
              dismiss_stale_reviews_on_push=true, required_review_thread_resolution=true
required_status_checks: validate
```

`gitops-api`: no rulesets (`[]`), and `branches/main/protection` returns 404 (not protected).

Result: changes to `gitops-config` must go through a PR and must pass `validate`, but **nobody has to approve them**.
Staging and prod promotions are not human-gated on GitHub. With a single maintainer account, GitHub
cannot require approval at all, because authors can't approve their own PRs. That limits the design,
and it is recorded as such in the HLD.

## B7. Version currency (GitHub `releases/latest`, read today)

| Component | Lab pins | Latest upstream | Gap |
|---|---|---|---|
| Argo CD | v3.5.7 | v3.6.1 (2026-11-04) | one minor behind (3.5 still in support window) |
| Kubernetes (kind node) | v1.36.4 | v1.37.2 (2026-10-28) | one minor behind |
| kind | v0.33.0 | v0.33.0 (2026-08-26) | current |
| Sealed Secrets | v0.41.2 | v0.41.2 (2026-10-26) | current |
| Rust toolchain | 1.99.0 | 1.100.0 (2026-11-12) | released yesterday |
| Axum | 0.8 (semver range) | 0.8.9 (2026-04-14) | current line |
| Kustomize | v5.8.1 (in kubectl 1.36.1) | v5.8.3 (2026-10-08) | patch |
| Trivy | 0.76.1 | v0.76.2 (2026-11-05) | patch |
| gitleaks | v8.30.1 | v8.30.1 | current |
| kubeconform | v0.8.0 | v0.8.0 | current |
