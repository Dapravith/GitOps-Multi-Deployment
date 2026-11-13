#!/usr/bin/env bash
# Encrypt ONE secret value for ONE environment, offline, against the committed
# public certificate. Plaintext is read from stdin only: never an argument
# (shell history, `ps`), never a file on disk.
#
#   printf '%s' "$VALUE" | scripts/seal.sh <env> <secret-name> <key>   # write SealedSecret
#   scripts/seal.sh --fetch-cert                                        # refresh sealed-secrets/pub-cert.pem
#   scripts/seal.sh --check-cert                                        # committed cert == cluster's active cert?
#
# Output: apps/gitops-api/overlays/<env>/sealedsecret-<secret-name>.yaml, strict
# scope (decryptable only as <secret-name> in gitops-<env>). Adding more keys to
# an existing secret merges into that file.
set -euo pipefail

KUBESEAL_IMAGE="docker.io/bitnami/sealed-secrets-kubeseal:0.41.2@sha256:6697225a70537d94061b56e6a138ba9ca26307a702f827dac0a77b0a007bc84f"
CONTROLLER_NS="sealed-secrets"
CONTROLLER_NAME="sealed-secrets-controller"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cert="${root}/sealed-secrets/pub-cert.pem"

kubeseal() {
  docker run --rm -i -v "${root}/sealed-secrets:/certs:ro" "${KUBESEAL_IMAGE}" "$@"
}

active_cert() {
  # Public half only. Uses the admin kubeconfig: the cluster-wide proxy grant
  # kubeseal --fetch-cert normally relies on was removed on purpose.
  KUBECONFIG="${KUBECONFIG:-$HOME/.kube/gitops-lab.kubeconfig}" kubectl -n "${CONTROLLER_NS}" get secret \
    -l sealedsecrets.bitnami.com/sealed-secrets-key=active \
    --sort-by=.metadata.creationTimestamp -o jsonpath='{.items[-1:].data.tls\.crt}' | base64 -d
}

case "${1:-}" in
  --fetch-cert)
    mkdir -p "$(dirname "${cert}")"
    active_cert > "${cert}"
    openssl x509 -in "${cert}" -noout -subject -enddate -fingerprint -sha256
    exit 0 ;;
  --check-cert)
    if [[ "$(active_cert | openssl x509 -noout -fingerprint -sha256)" == "$(openssl x509 -in "${cert}" -noout -fingerprint -sha256)" ]]; then
      echo "committed cert matches the cluster's newest active sealing key"; exit 0
    fi
    echo "committed cert is NOT the newest active key: run scripts/seal.sh --fetch-cert (old keys still decrypt)" >&2
    exit 1 ;;
esac

env="${1:?usage: printf %s VALUE | seal.sh <env> <secret-name> <key>}"
name="${2:?secret name required}"
key="${3:?key required}"
[[ "${env}" =~ ^(dev|staging|prod)$ ]] || { echo "unknown environment: ${env}" >&2; exit 2; }
[[ "${name}" =~ ^[a-z0-9]([-a-z0-9]*[a-z0-9])?$ ]] || { echo "invalid secret name: ${name}" >&2; exit 2; }
[[ "${key}" =~ ^[-._a-zA-Z0-9]+$ ]] || { echo "invalid key: ${key}" >&2; exit 2; }
[[ -t 0 ]] && { echo "refusing: pipe the secret value on stdin" >&2; exit 2; }
[[ -s "${cert}" ]] || { echo "missing ${cert}; run scripts/seal.sh --fetch-cert" >&2; exit 1; }

ns="gitops-${env}"
out="${root}/apps/gitops-api/overlays/${env}/sealedsecret-${name}.yaml"

# The plaintext Secret exists only inside this pipe: kubectl renders it from
# stdin, kubeseal encrypts it, and only ciphertext reaches the file system.
sealed="$(KUBECONFIG=/dev/null kubectl create secret generic "${name}" -n "${ns}" \
            --from-file="${key}=/dev/stdin" --dry-run=client -o json \
          | kubeseal --cert /certs/pub-cert.pem --scope strict -o yaml)"

if [[ -f "${out}" ]]; then
  # Add/replace this key's ciphertext in the existing SealedSecret; other keys
  # are untouched (their plaintext is never needed). Ciphertext arrives on stdin.
  printf '%s\n' "${sealed}" | python3 -c '
import re, sys
path, key = sys.argv[1], sys.argv[2]
line = re.compile(r"^    " + re.escape(key) + r": (\S+)$", re.M)
new = line.search(sys.stdin.read()).group(1)
s = open(path).read()
if line.search(s):
    s = line.sub(lambda m: "    " + key + ": " + new, s)
else:
    s = s.replace("  encryptedData:\n", "  encryptedData:\n    " + key + ": " + new + "\n", 1)
open(path, "w").write(s)
' "${out}" "${key}"
else
  printf '%s\n' "${sealed}" > "${out}"
fi
echo "wrote ${out#"${root}"/} (strict scope: ${name} in ${ns}); add it to the overlay's resources if new"
