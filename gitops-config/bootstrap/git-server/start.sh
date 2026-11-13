#!/usr/bin/env bash
# Start the lab Git server and publish gitops-config to it.
#
#   Host bare repo : ${GIT_LAB_ROOT:-$HOME/.gitops-lab/repos}/gitops-config.git
#   Git remote     : `lab` in this repo  (git push lab main)
#   In-cluster URL : http://git.lab-infra.svc.cluster.local/git/gitops-config.git
#   Host URL       : http://127.0.0.1:18081/git/gitops-config.git
#
# The container mounts the bare repos READ-ONLY and refuses HTTP pushes, so the
# only write path is `git push lab` from this machine.
#
# Pods cannot resolve Docker container names (CoreDNS forwards to Docker
# Desktop's host DNS), so a selector-less Service `lab-infra/git` plus an
# EndpointSlice pointing at the container's kind-network IP is (re)written on
# every run. Re-run this script after the container restarts.
set -euo pipefail

NAME="gitops-lab-git"
IMAGE="gitops-lab-git:local"
HOST_PORT="18081"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${here}/../.." && pwd)"
root="${GIT_LAB_ROOT:-$HOME/.gitops-lab/repos}"
bare="${root}/gitops-config.git"
export KUBECONFIG="${KUBECONFIG:-$HOME/.kube/gitops-lab.kubeconfig}"

mkdir -p "${root}"
if [[ ! -d "${bare}" ]]; then
  git init -q --bare -b main "${bare}"
  echo "created bare repo ${bare}"
fi
git -C "${repo_root}" remote get-url lab >/dev/null 2>&1 \
  || git -C "${repo_root}" remote add lab "${bare}"
git -C "${repo_root}" push -q lab main
echo "published $(git -C "${repo_root}" rev-parse --short HEAD) to lab remote"

docker build -q -t "${IMAGE}" "${here}" >/dev/null
if [[ "$(docker inspect -f '{{.State.Running}}' "${NAME}" 2>/dev/null || true)" != "true" ]]; then
  docker rm -f "${NAME}" >/dev/null 2>&1 || true
  docker run -d --restart=unless-stopped --name "${NAME}" --network kind \
    --read-only --tmpfs /tmp --cap-drop ALL --security-opt no-new-privileges \
    -p "127.0.0.1:${HOST_PORT}:8080" -v "${root}:/srv/git:ro" "${IMAGE}" >/dev/null
  echo "started ${NAME}"
fi

ip="$(docker inspect -f '{{(index .NetworkSettings.Networks "kind").IPAddress}}' "${NAME}")"
sed "s/__GIT_IP__/${ip}/" "${here}/lab-infra.yaml" | kubectl apply -f - >/dev/null
echo "lab-infra/git -> ${ip}:8080"

git ls-remote "http://127.0.0.1:${HOST_PORT}/git/gitops-config.git" main
