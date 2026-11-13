#!/usr/bin/env bash
# Run the pinned argocd CLI in --core mode against the kind lab.
#
# --core talks to the Kubernetes API directly, using your kubeconfig's RBAC.
# No Argo CD password, token or port-forward is involved.
#
#   scripts/argocd.sh app list
#   scripts/argocd.sh app diff gitops-api-prod
#   scripts/argocd.sh app sync gitops-api-prod     # prod release (needs approval)
set -euo pipefail

IMAGE="quay.io/argoproj/argocd:v3.5.7@sha256:16df57da6f223808d204cd94c67c929b931fa01670b91b0b3a2298c4647de0db"
CLUSTER="gitops-lab"
kc="${HOME}/.kube/${CLUSTER}-internal.kubeconfig"

# Kubeconfig addressed for the kind Docker network (not 127.0.0.1); kept outside Git.
kind get kubeconfig --internal --name "${CLUSTER}" > "${kc}"
chmod 600 "${kc}"
KUBECONFIG="${kc}" kubectl config set-context --current --namespace=argocd >/dev/null

tty_flag=()
[[ -t 0 && -t 1 ]] && tty_flag=(-it)

exec docker run --rm "${tty_flag[@]+"${tty_flag[@]}"}" --network kind \
  -v "${kc}:/kubeconfig:ro" -e KUBECONFIG=/kubeconfig -e HOME=/tmp \
  "${IMAGE}" argocd --core "$@"
