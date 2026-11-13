#!/usr/bin/env bash
# Idempotently create the `gitops-lab` kind cluster plus a local OCI registry.
#
# * The kubeconfig is written to its OWN file (outside any Git repo) so it never
#   changes your default ~/.kube/config current-context and can never be committed.
# * Registry `gitops-lab-registry` listens on 127.0.0.1:5001 (host) and
#   gitops-lab-registry:5000 (kind network). Inside the node it is configured as a
#   pull mirror for ghcr.io, falling back to the real ghcr.io when it lacks a digest.
#
#   export KUBECONFIG="$HOME/.kube/gitops-lab.kubeconfig"
set -euo pipefail

CLUSTER_NAME="gitops-lab"
REG_NAME="gitops-lab-registry"
REG_PORT="5001"
REG_IMAGE="registry:3.1.1@sha256:325b4b29b041e82803abeb703e201655e4e23ab83264ec1a7c9ddb0a5b14a6e0"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export KUBECONFIG="${KUBECONFIG_PATH:-$HOME/.kube/gitops-lab.kubeconfig}"

# 1. Local registry (host-only binding).
if [[ "$(docker inspect -f '{{.State.Running}}' "${REG_NAME}" 2>/dev/null || true)" != "true" ]]; then
  docker rm -f "${REG_NAME}" >/dev/null 2>&1 || true
  docker run -d --restart=unless-stopped --name "${REG_NAME}" \
    -p "127.0.0.1:${REG_PORT}:5000" "${REG_IMAGE}" >/dev/null
  echo "started registry ${REG_NAME} on 127.0.0.1:${REG_PORT}"
fi

# 2. Cluster.
if kind get clusters 2>/dev/null | grep -qx "${CLUSTER_NAME}"; then
  echo "cluster ${CLUSTER_NAME} already exists; refreshing kubeconfig at ${KUBECONFIG}"
  kind export kubeconfig --name "${CLUSTER_NAME}" --kubeconfig "${KUBECONFIG}"
else
  kind create cluster --config "${here}/kind-config.yaml" --kubeconfig "${KUBECONFIG}" --wait 180s
fi
chmod 600 "${KUBECONFIG}"

# 3. Wire registry into the kind network and node containerd.
if [[ "$(docker inspect -f '{{json .NetworkSettings.Networks.kind}}' "${REG_NAME}")" == "null" ]]; then
  docker network connect kind "${REG_NAME}"
fi
for node in $(kind get nodes --name "${CLUSTER_NAME}"); do
  docker exec "${node}" mkdir -p "/etc/containerd/certs.d/localhost:${REG_PORT}" /etc/containerd/certs.d/ghcr.io
  docker exec -i "${node}" sh -c "cat > /etc/containerd/certs.d/localhost:${REG_PORT}/hosts.toml" <<EOF
[host."http://${REG_NAME}:5000"]
  capabilities = ["pull", "resolve"]
EOF
  docker exec -i "${node}" sh -c "cat > /etc/containerd/certs.d/ghcr.io/hosts.toml" <<EOF
server = "https://ghcr.io"

[host."http://${REG_NAME}:5000"]
  capabilities = ["pull", "resolve"]
EOF
done

# Advertise the local registry (KEP-1755).
kubectl apply -f - <<EOF
apiVersion: v1
kind: ConfigMap
metadata:
  name: local-registry-hosting
  namespace: kube-public
data:
  localRegistryHosting.v1: |
    host: "localhost:${REG_PORT}"
    help: "https://kind.sigs.k8s.io/docs/user/local-registry/"
EOF

kubectl wait --for=condition=Ready node --all --timeout=180s
kubectl -n kube-system rollout status deployment/coredns --timeout=180s
kubectl get nodes -o wide
echo
echo "Use this cluster with:  export KUBECONFIG=${KUBECONFIG}"
echo "Seed an image locally:   docker tag IMG localhost:${REG_PORT}/<owner>/<name>:<tag> && docker push localhost:${REG_PORT}/<owner>/<name>:<tag>"
