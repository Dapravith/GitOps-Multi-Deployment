# Evidence — Sealed Secrets

Captured 2026-11-12 on the local lab. Prod was not touched: the controller does not watch `gitops-prod`
yet, because enabling it needs a prod sync. The demo secret is a random lab value (`lab-only-` + 48 hex chars).
It existed only in a shell variable and was never printed. Values were compared by SHA-256 prefix.

## Install (Argo CD-managed, no kubectl apply)

| Item | Value |
|---|---|
| Version | v0.41.2 (released 2026-10-26) |
| Manifest | `platform/sealed-secrets/upstream/controller-v0.41.2.yaml`, vendored unmodified, sha256 `5973b18e…fe205`, checked by `validate.sh`. Upstream's signed checksums cover kubeseal binaries only |
| Image | `docker.io/bitnami/sealed-secrets-controller:0.41.2@sha256:e63ec6ec…01d0b` (amd64 + arm64) |
| kubeseal | `docker.io/bitnami/sealed-secrets-kubeseal:0.41.2@sha256:6697225a…bc84f` |
| Namespace | `sealed-secrets`, PSA `restricted` enforced; pod UID 65532 |
| Argo CD | Application `sealed-secrets` (wave −1) in new AppProject `cluster-addons`; every cluster-scoped object allowed **by name** |

Hardening versus upstream defaults:

| Upstream | Here | Why |
|---|---|---|
| `kube-system` | own namespace, restricted PSA | isolate the key |
| `secrets-unsealer` bound **cluster-wide** | bound per namespace (own + opted-in envs) | Trivy KSV-0041 CRITICAL, least privilege |
| watches all namespaces | `--all-namespaces=false --additional-namespaces=gitops-dev,gitops-staging` | decrypt only where approved |
| `system:authenticated` may proxy to the controller | removed | sealing is offline against the committed cert |
| runAsUser 1001 | 65532 | Trivy KSV-0020/0021 |
| — | `Prune=false,Delete=false` on Namespace + CRD; Application without cascade finalizer | never delete the key or all SealedSecrets |

Effective controller permissions (`kubectl auth can-i … --as=…:sealed-secrets-controller`):

```
sealed-secrets  get secrets: yes      gitops-prod  get secrets: no
gitops-dev      get secrets: yes      kube-system  get secrets: no
gitops-staging  get secrets: yes      argocd       get secrets: no
                                      default      get secrets: no
logs: Starting informer namespace=sealed-secrets | gitops-staging | gitops-dev   (and no others)
```

### Failures hit during install (fixed, committed)

1. `validate.sh`: `could not find schema for CustomResourceDefinition` (kubeconform has none). The CRD is now skipped explicitly.
2. Trivy on the controller: KSV-0041 (CRITICAL), KSV-0113, KSV-0020/0021. Fixed 0041 by scoping RBAC (exception kept with a reason,
   since the ClusterRole still *defines* Secret access), fixed 0020/0021 with the UID, and accepted 0113 (key storage in its own namespace).
3. **CrashLoopBackOff** after scoping: `panic: secrets "sealed-secrets-key" is forbidden … cannot get resource "secrets"`.
   Upstream's key-admin Role only grants create/list, and `get` came from the cluster-wide binding I removed.
   Fixed with a RoleBinding to `secrets-unsealer` in the controller's own namespace. Then `Synced/Healthy`, new key `sealed-secrets-key2kvhc`.

### Overlay RBAC guard

AppProjects can't pin namespaced kinds by name, so `policy_check.py` allows exactly one RoleBinding shape
(`sealed-secrets-controller` → ClusterRole `secrets-unsealer` → SA `sealed-secrets/sealed-secrets-controller`).
Negative tests: roleRef `admin`, or a binding renamed to `gitops-api`, give `POLICY FAIL … only components/sealed-secrets-access may add RBAC`.

## Sealing workflow

- `scripts/seal.sh <env> <name> <key>` reads the value **from stdin only**, renders the Secret with
  `kubectl --dry-run=client` (no cluster), and encrypts with `kubeseal --scope strict` against `sealed-secrets/pub-cert.pem`.
- `pub-cert.pem` is committed: certificate only (`PRIVATE KEY blocks: 0`), fingerprint `70:0B:54:10:…:CF:CC`, valid until 2036-11-09.
- `seal.sh --check-cert` detects a stale cert after key renewal (proven in the drill below).
- Guards tested: env `qa`, name `Bad_Name` rejected. The "stdin is a terminal" guard was **not** exercised (no TTY here).

## End-to-end proof (dev)

```
seal.sh dev lab-demo-secret token  -> 784-char ciphertext, plaintext occurrences in file: 0
validate.sh dev: policy OK [dev]: 10 objects (incl. SealedSecret + RoleBinding)
Argo CD: gitops-api-dev Synced/Healthy after 3s
decrypted value sha256 prefix: e0c696181cb73a80   expected: e0c696181cb73a80   match: YES
SealedSecret status: Synced=True; Secret owner: SealedSecret/lab-demo-secret, type Opaque
```

## Scope and tamper tests (temporary objects, deleted afterwards)

| Test | Result |
|---|---|
| T1 dev ciphertext copied to `gitops-staging` | `ErrUnsealFailed: no key could decrypt secret (token)`, no Secret |
| T2 same namespace, renamed | `ErrUnsealFailed`, no Secret |
| T3 one ciphertext character flipped | `ErrUnsealFailed`, no Secret |
| T4 namespace the controller doesn't watch | never processed (no status), no Secret |
| T5 who can read the decrypted Secret | app SA: **no**; `default` SA: **no**; `argocd-server` SA: **yes** (upstream `*/* get,patch,delete` for UI actions; Argo CD masks Secret data. Accepted, not changed) |

## Secret scanning (gitleaks v8.30.1, digest-pinned)

- Both repos' full history: `no leaks found` (gitops-api: 1 commit; gitops-config: 24 commits with changes).
- Negatives: a generated RSA key (`private-key`) and a `ghp_…` token (`github-pat`) are caught and named.
- **False positive found:** the SealedSecret ciphertext tripped `generic-api-key`.
  - My first fix (a path-based allowlist) was **wrong**: a real `ghp_` token pasted into a SealedSecret file was no longer flagged.
  - Final fix: allow only *values* shaped like kubeseal RSA-4096 output (`^AgA[A-Za-z0-9+/]{300,}`). Re-test: a `ghp_` token and a random
    base64 password pasted into the SealedSecret file are **both flagged**; the real repo and history are clean.
- **Process slip:** commit `f69e107` (the dev SealedSecret) was made while `validate.sh` was failing on that false positive, because my
  test script didn't gate the commit on the validator. The commit contains only ciphertext. History was not rewritten. Later commits were gated.

## Key backup and restore drill

```
backup -> ~/.gitops-lab/sealed-secrets-keys/sealed-secrets-keys-20261112T070605Z.yaml
          file -rw------- (7006 bytes), dir drwx------, inside a git repo: no, contains tls.key: True
disaster: delete key Secret + decrypted Secret, restart controller
          -> controller generates NEW key sealed-secrets-key9lxk4 (fp E1:D3:…)
          -> SealedSecret: Synced=False no key could decrypt secret (token)
          -> seal.sh --check-cert: committed cert is NOT the newest active key
restore:  apply backup, restart, delete the post-disaster key
          -> keys: sealed-secrets-key2kvhc active 70:0B:… (original), check-cert: matches
```

**Drill flaw found and re-tested:** after the "disaster", the decrypted Secret still existed. The old controller pod (key still in
memory) recreated it before shutting down, so the first "hash matches after restore" proved nothing. Re-test with only the restored-key
pod running: deleted the Secret (uid `769a71cd`), and the controller recreated it in **2 s** (uid `92cdf41f`) with sha256 prefix
`e0c696181cb73a80` (original). `SealedSecret Synced=True`; log `Updating key=gitops-dev/lab-demo-secret`.

## Not done

- Prod not enabled (needs the component, the controller flag, and a **manual prod sync** — your call).
- No real credential sealed yet. The first real ones will be the GHCR pull secret and/or Argo CD repo credentials (phase 8, which you
  create and seal yourself; I won't handle real tokens).
- The key backup sits on this Mac's disk. For anything real, it belongs in a password manager / vault.
- Automatic key renewal is enabled (30 days). Nothing reminds you to re-run `backup` and `--fetch-cert` afterwards.
