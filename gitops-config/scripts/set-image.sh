#!/usr/bin/env bash
# Set the gitops-api image digest of ONE environment overlay. This is the only
# edit a promotion makes. Used by CI (dev) and by scripts/promote.sh.
#
#   scripts/set-image.sh dev sha256:<64 hex>
set -euo pipefail

env="${1:?usage: set-image.sh <dev|staging|prod> sha256:<digest>}"
digest="${2:?usage: set-image.sh <dev|staging|prod> sha256:<digest>}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
file="${root}/apps/gitops-api/overlays/${env}/kustomization.yaml"

[[ "${env}" =~ ^(dev|staging|prod)$ ]] || { echo "unknown environment: ${env}" >&2; exit 2; }
[[ "${digest}" =~ ^sha256:[0-9a-f]{64}$ ]] || { echo "not a sha256 digest: ${digest}" >&2; exit 2; }
[[ "$(grep -cE '^    digest: sha256:[0-9a-f]{64}$' "${file}")" == "1" ]] \
  || { echo "${file}: expected exactly one digest line" >&2; exit 1; }

# The digest must exist in the registry and be a multi-arch index. A well-formed
# but unpublished digest otherwise passes every offline check and only fails at
# pull time (found in phase 13). IMAGE_REGISTRY: ghcr.io in CI; the lab registry
# (which the kind node uses as its ghcr.io mirror) by default.
registry="${IMAGE_REGISTRY:-localhost:5001}"
ref="${registry}/dapravith/gitops-api@${digest}"
if [[ "${SKIP_REGISTRY_CHECK:-0}" != "1" ]]; then
  manifest="$(docker buildx imagetools inspect --raw "${ref}" 2>/dev/null)" \
    || { echo "digest not found in registry: ${ref}" >&2; exit 1; }
  for platform in '"architecture":"amd64"' '"architecture":"arm64"'; do
    [[ "${manifest//[[:space:]]/}" == *"${platform}"* ]] \
      || { echo "${ref} is missing platform ${platform}" >&2; exit 1; }
  done
fi

sed -i.bak -E "s|^    digest: sha256:[0-9a-f]{64}$|    digest: ${digest}|" "${file}" && rm -f "${file}.bak"
echo "${env}: image -> ghcr.io/dapravith/gitops-api@${digest}"
