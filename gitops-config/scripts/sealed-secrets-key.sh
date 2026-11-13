#!/usr/bin/env bash
# Back up / restore / inspect the Sealed Secrets private sealing keys.
#
# WITHOUT A BACKUP, losing the cluster (e.g. `kind delete cluster`) makes every
# SealedSecret in Git permanently undecryptable.
#
#   scripts/sealed-secrets-key.sh status
#   scripts/sealed-secrets-key.sh backup              # -> $BACKUP_DIR/sealed-secrets-keys-<UTC>.yaml (0600)
#   scripts/sealed-secrets-key.sh restore <file>      # apply keys, restart controller
#
# Backups contain PRIVATE KEYS. They are written outside any Git repository
# (default ~/.gitops-lab/sealed-secrets-keys, mode 0700/0600) and must never be
# committed. In a real environment, store them in a password manager / KMS-backed vault.
set -euo pipefail

NS="sealed-secrets"
LABEL="sealedsecrets.bitnami.com/sealed-secrets-key"
BACKUP_DIR="${BACKUP_DIR:-$HOME/.gitops-lab/sealed-secrets-keys}"
export KUBECONFIG="${KUBECONFIG:-$HOME/.kube/gitops-lab.kubeconfig}"

fingerprints() {  # name, state, cert sha256 fingerprint, one line per key
  kubectl -n "${NS}" get secret -l "${LABEL}" --sort-by=.metadata.creationTimestamp \
    -o jsonpath='{range .items[*]}{.metadata.name} {.metadata.labels.sealedsecrets\.bitnami\.com/sealed-secrets-key} {.data.tls\.crt}{"\n"}{end}' \
  | while read -r name state crt; do
      printf '%s %s %s\n' "${name}" "${state}" \
        "$(printf '%s' "${crt}" | base64 -d | openssl x509 -noout -fingerprint -sha256 | cut -d= -f2)"
    done
}

case "${1:-}" in
  status)
    fingerprints ;;
  backup)
    git_root="$(git -C "${BACKUP_DIR%/*}" rev-parse --show-toplevel 2>/dev/null || true)"
    [[ -z "${git_root}" ]] || { echo "refusing: ${BACKUP_DIR} is inside git repo ${git_root}" >&2; exit 1; }
    umask 077; mkdir -p "${BACKUP_DIR}"; chmod 700 "${BACKUP_DIR}"
    file="${BACKUP_DIR}/sealed-secrets-keys-$(date -u +%Y%m%dT%H%M%SZ).yaml"
    # Strip cluster-specific metadata so the file restores cleanly into a new cluster.
    kubectl -n "${NS}" get secret -l "${LABEL}" -o json \
      | python3 -c 'import json,sys
d=json.load(sys.stdin)
for i in d["items"]:
    m=i["metadata"]
    for k in ("uid","resourceVersion","creationTimestamp","managedFields","ownerReferences"):
        m.pop(k,None)
print(json.dumps(d))' > "${file}"
    chmod 600 "${file}"
    echo "backed up $(python3 -c 'import json,sys;print(len(json.load(open(sys.argv[1]))["items"]))' "${file}") key(s) -> ${file}"
    fingerprints ;;
  restore)
    file="${2:?usage: sealed-secrets-key.sh restore <backup-file>}"
    [[ -f "${file}" ]] || { echo "no such file: ${file}" >&2; exit 1; }
    kubectl apply -f "${file}"
    # The controller loads keys at startup.
    kubectl -n "${NS}" rollout restart deployment/sealed-secrets-controller
    kubectl -n "${NS}" rollout status deployment/sealed-secrets-controller --timeout=180s
    fingerprints ;;
  *)
    echo "usage: $0 status | backup | restore <file>" >&2; exit 2 ;;
esac
