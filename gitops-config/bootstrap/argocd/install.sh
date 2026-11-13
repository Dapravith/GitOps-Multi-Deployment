#!/usr/bin/env bash
# One-time Argo CD bootstrap for the kind lab:
#   1. verify the pinned upstream manifest's SHA-256
#   2. server-side apply bootstrap/argocd (CRDs are too large for client-side apply)
#   3. wait for every component
#   4. hand over to Git: apply the platform AppProject + root Application
set -euo pipefail

ARGOCD_VERSION="v3.5.7"
EXPECTED_SHA256="09bfbb9041b4a8503c6e4fa22cf41c403fa62be17526de353648a4551f5beb77"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${here}/../.." && pwd)"
export KUBECONFIG="${KUBECONFIG:-$HOME/.kube/gitops-lab.kubeconfig}"

url="https://raw.githubusercontent.com/argoproj/argo-cd/${ARGOCD_VERSION}/manifests/install.yaml"
actual="$(curl -fsSL "${url}" | shasum -a 256 | awk '{print $1}')"
if [[ "${actual}" != "${EXPECTED_SHA256}" ]]; then
  echo "upstream manifest checksum mismatch: got ${actual}, want ${EXPECTED_SHA256}" >&2
  exit 1
fi
echo "upstream ${ARGOCD_VERSION} install.yaml sha256 verified"

kubectl kustomize "${here}" | kubectl apply --server-side --force-conflicts -f - >/dev/null
echo "Argo CD manifests applied"

kubectl -n argocd rollout status statefulset/argocd-application-controller --timeout=300s
for d in argocd-server argocd-repo-server argocd-redis argocd-applicationset-controller argocd-notifications-controller; do
  kubectl -n argocd rollout status "deployment/${d}" --timeout=300s
done

# Hand-over: from here on Argo CD manages argocd/ (projects + applications),
# including the root Application itself.
kubectl apply -f "${repo_root}/argocd/projects/platform.yaml"
kubectl apply -f "${repo_root}/argocd/applications/root.yaml"
