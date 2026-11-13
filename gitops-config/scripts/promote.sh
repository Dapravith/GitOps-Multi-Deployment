#!/usr/bin/env bash
# Promote the EXACT image digest running in <from> to <to>, as a reviewable branch.
#
#   scripts/promote.sh dev staging
#   scripts/promote.sh staging prod
#
# Gates (any failure aborts, nothing is committed):
#   1. only adjacent promotions: dev->staging, staging->prod
#   2. working tree clean and on main
#   3. <from>'s Argo CD Application is Synced + Healthy at the current main commit
#   4. <from> is fully rolled out and every running pod's imageID equals the digest in Git
#   5. scripts/validate.sh passes with the change applied
#
# Output: branch promote/<to>-<digest12> with one commit. Review the diff, then
# merge it to main (GitHub PR in phase 8+; locally `git merge --no-ff`).
set -euo pipefail

from="${1:?usage: promote.sh <from> <to>}"
to="${2:?usage: promote.sh <from> <to>}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export KUBECONFIG="${KUBECONFIG:-$HOME/.kube/gitops-lab.kubeconfig}"
cd "${root}"

case "${from}->${to}" in
  dev-\>staging|staging-\>prod) ;;
  *) echo "refusing ${from}->${to}: only dev->staging and staging->prod are allowed" >&2; exit 2 ;;
esac

[[ -z "$(git status --porcelain)" ]] || { echo "working tree not clean" >&2; exit 1; }
[[ "$(git rev-parse --abbrev-ref HEAD)" == "main" ]] || { echo "must run on main" >&2; exit 1; }

digest_of() { sed -nE 's/^    digest: (sha256:[0-9a-f]{64})$/\1/p' "apps/gitops-api/overlays/$1/kustomization.yaml"; }
src_digest="$(digest_of "${from}")"
dst_digest="$(digest_of "${to}")"
[[ "${src_digest}" != "${dst_digest}" ]] || { echo "${to} already runs ${src_digest}; nothing to promote"; exit 0; }

app="gitops-api-${from}"
status="$(kubectl -n argocd get application "${app}" \
  -o jsonpath='{.status.sync.status} {.status.health.status} {.status.sync.revision}')" \
  || { echo "gate: cannot read Argo CD Application ${app}" >&2; exit 1; }
# here-string adds the trailing newline jsonpath omits (read would fail under set -e).
read -r sync health rev <<< "${status}"
head_rev="$(git rev-parse HEAD)"
[[ "${sync}" == "Synced" && "${health}" == "Healthy" ]] \
  || { echo "gate: ${app} is ${sync}/${health}, must be Synced/Healthy" >&2; exit 1; }
[[ "${rev}" == "${head_rev}" ]] \
  || { echo "gate: ${app} is at ${rev:0:7}, main is ${head_rev:0:7}; wait for sync" >&2; exit 1; }

# What is ACTUALLY running: rollout complete and every pod resolved to the Git digest.
ns="gitops-${from}"
read -r want updated ready <<< "$(kubectl -n "${ns}" get deployment gitops-api \
  -o jsonpath='{.spec.replicas} {.status.updatedReplicas} {.status.readyReplicas}')"
[[ "${want}" == "${updated}" && "${want}" == "${ready}" ]] \
  || { echo "gate: ${ns} rollout incomplete (want ${want}, updated ${updated:-0}, ready ${ready:-0})" >&2; exit 1; }
# One line per pod: "<deletionTimestamp>|<imageID>". Terminating pods have a
# timestamp before the bar and are ignored; the rest must match the Git digest.
running="$(kubectl -n "${ns}" get pods -l app.kubernetes.io/name=gitops-api \
  -o jsonpath='{range .items[*]}{.metadata.deletionTimestamp}|{.status.containerStatuses[0].imageID}{"\n"}{end}' \
  | sed -n 's/^|//p')"
[[ -n "${running}" ]] || { echo "gate: no running pods found in ${ns}" >&2; exit 1; }
while IFS= read -r image_id; do
  [[ "${image_id}" == *"@${src_digest}" ]] \
    || { echo "gate: ${ns} runs ${image_id}, Git says @${src_digest}" >&2; exit 1; }
done <<< "${running}"
echo "gates passed: ${app} Synced/Healthy at ${rev:0:7}, running @${src_digest:7:12}"

branch="promote/${to}-${src_digest:7:12}"
git switch -q -c "${branch}"
trap 'git switch -q main 2>/dev/null || true' ERR
scripts/set-image.sh "${to}" "${src_digest}"
if ! scripts/validate.sh "${to}" >/dev/null 2>&1; then
  git checkout -q -- . && git switch -q main && git branch -q -D "${branch}"
  echo "validation failed; promotion aborted" >&2; exit 1
fi
git add "apps/gitops-api/overlays/${to}/kustomization.yaml"
git commit -q -F - <<EOF
promote(${to}): gitops-api@${src_digest:0:19} from ${from}

Source: ${app} Synced/Healthy at ${rev:0:12}
Image:  ghcr.io/dapravith/gitops-api@${src_digest}
Previous ${to} digest: ${dst_digest}
EOF
git switch -q main
echo "created ${branch}:"
git --no-pager diff "main...${branch}" --stat
git --no-pager diff "main...${branch}" -U1 -- "apps/gitops-api/overlays/${to}/kustomization.yaml" | grep -E '^[+-] '
