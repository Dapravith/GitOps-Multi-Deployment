# Secrets with Sealed Secrets

Plaintext secrets never enter Git. A value is encrypted on your machine against the
controller's **public** certificate. Only the in-cluster controller holds the private key
that can turn it back into a Kubernetes `Secret`.

```
value ──stdin──▶ scripts/seal.sh ──▶ SealedSecret (ciphertext) ──git──▶ Argo CD ──▶ controller ──▶ Secret
                 (public cert,                                                    (private key,
                  offline)                                                         in-cluster only)
```

## What protects what

| Control | Where | Stops |
|---|---|---|
| Strict scope (name + namespace bound into the ciphertext) | `seal.sh --scope strict`, `policy_check.py` | reusing dev ciphertext in staging/prod, or under another name |
| No `Secret` kind in overlays | `policy_check.py`, AppProject allow-lists | committing plaintext `Secret` manifests |
| No plaintext in `SealedSecret.spec.template` | `policy_check.py` | `data`/`stringData` smuggled into the template |
| gitleaks over working tree + full history | `validate.sh`, `.gitleaks.toml` | tokens, private keys, kubeconfigs anywhere in the repo |
| Controller watches only opted-in namespaces | `--all-namespaces=false --additional-namespaces=…` | decrypting into namespaces nobody approved |
| Controller RBAC per namespace, no cluster-wide Secret access | `components/sealed-secrets-access`, `platform/sealed-secrets/rbac.yaml` | a compromised controller reading every Secret in the cluster |
| `Prune=false,Delete=false` on the Namespace and CRD | `platform/sealed-secrets` | Argo CD deleting the sealing key or every SealedSecret |

## Add or change a secret

The controller must watch the environment, and the environment must opt in. Today that's
`gitops-dev` and `gitops-staging`. Enabling prod is a prod change, so it needs a prod sync.

```bash
read -rs VALUE
```

```bash
printf '%s' "$VALUE" | scripts/seal.sh dev my-secret my-key; unset VALUE
```

For a new secret, add `sealedsecret-my-secret.yaml` to the overlay's `resources:`. Then validate,
commit and push as usual. Argo CD applies it, and the controller creates `Secret/my-secret`.

```bash
scripts/validate.sh
```

To use it in the Deployment, reference it with `envFrom.secretRef` or a `secretKeyRef`, in a
reviewed change.

## Enable another environment (e.g. prod)

1. Add `../../components/sealed-secrets-access` to `overlays/prod/kustomization.yaml` `components:`.
2. Add `gitops-prod` to `--additional-namespaces` in `platform/sealed-secrets/kustomization.yaml`.
3. Validate, commit, merge. The controller change auto-syncs. The prod RoleBinding needs the
   **manual prod sync**.

## Key lifecycle

| When | Do |
|---|---|
| Right after install, and after every key renewal (default every 30 days) | `scripts/sealed-secrets-key.sh backup`, then store the file in a password manager / vault |
| After a renewal (`seal.sh --check-cert` fails) | `scripts/seal.sh --fetch-cert` and commit the new public cert. Old keys keep decrypting existing SealedSecrets |
| Cluster rebuilt (`kind delete cluster`) | bootstrap, then `scripts/sealed-secrets-key.sh restore <file>` **before** syncing apps |
| Key suspected leaked | renew the key, **change the underlying credentials**, re-seal every secret. Old ciphertext in Git history stays decryptable with the old key |

Backups are written to `~/.gitops-lab/sealed-secrets-keys/` (directory 0700, files 0600). The
script refuses to write inside a Git repository. **Never commit a backup.**

## Troubleshooting

```bash
kubectl -n gitops-dev get sealedsecret my-secret -o jsonpath='{.status.conditions}'
```

| Status / event | Meaning |
|---|---|
| `ErrUnsealFailed: no key could decrypt secret` | sealed for another name/namespace, ciphertext edited, or the cluster lacks the key (restore it) |
| no status at all | the controller doesn't watch that namespace (see "Enable another environment") |
| Secret not updated after re-seal | check that the controller pod is Running and that the app synced the new commit |
