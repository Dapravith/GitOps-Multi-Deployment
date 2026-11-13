#!/usr/bin/env bash
# Validate every overlay WITHOUT touching a cluster:
#   1. render with Kustomize (kubectl's built-in)
#   2. schema-validate with kubeconform (strict, K8s 1.36 schemas)
#   3. enforce project policy (scripts/policy_check.py)
#   4. misconfiguration scan (trivy config, exceptions in .trivyignore)
#   5. schema + guard rails for argocd/ Applications and AppProjects
#   6. secret scan of the working tree and full git history (gitleaks)
# Tools other than kubectl/python3/docker run as digest-pinned containers.
# Rendered output is written to ${OUT_DIR:-.rendered}/<env>.yaml (gitignored).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="${OUT_DIR:-${root}/.rendered}"
envs=("${@:-dev staging prod}")
read -r -a envs <<< "${envs[*]}"

KUBECONFORM_IMAGE="ghcr.io/yannh/kubeconform:v0.8.0@sha256:faffaf43f95aa6425306e1ab8d6fcad72acb9049158f38e574c085ea1ec0f64e"
YQ_IMAGE="mikefarah/yq:4.54.1@sha256:4b3d9475d65571d28cbb19544d3820ec2945e4c8b2f18279394282b8dc3a592e"
GITLEAKS_IMAGE="ghcr.io/gitleaks/gitleaks:v8.30.1@sha256:c00b6bd0aeb3071cbcb79009cb16a60dd9e0a7c60e2be9ab65d25e6bc8abbb7f"
TRIVY_IMAGE="aquasec/trivy:0.76.1@sha256:790fd66d1697395d8693d1acaa1ea702406a20f109b6fc7bac8db623eca6c06c"
K8S_VERSION="1.36.0"
# CRD JSON schemas (Argo CD Application/AppProject), pinned to a catalog commit.
CRD_SCHEMAS="https://raw.githubusercontent.com/datreeio/CRDs-catalog/89c736bdde38b5fecd0f8436ec5f3aa3679a3b86/{{.Group}}/{{.ResourceKind}}_{{.ResourceAPIVersion}}.json"

mkdir -p "${out}"
fail=0
for env in "${envs[@]}"; do
  echo "== ${env}"
  file="${out}/${env}.yaml"
  if ! kubectl kustomize "${root}/apps/gitops-api/overlays/${env}" > "${file}"; then
    echo "  RENDER FAIL [${env}]"; fail=1; continue
  fi
  echo "  rendered $(grep -c '^kind:' "${file}") objects -> ${file#"${root}"/}"

  if ! docker run --rm -i "${KUBECONFORM_IMAGE}" \
        -strict -summary -kubernetes-version "${K8S_VERSION}" \
        -schema-location default -schema-location "${CRD_SCHEMAS}" - < "${file}" | sed 's/^/  /'; then
    echo "  SCHEMA FAIL [${env}]"; fail=1
  fi

  if ! docker run --rm -i "${YQ_IMAGE}" -o=json -I=0 'select(. != null)' - < "${file}" \
        | python3 -c 'import sys,json; print(json.dumps([json.loads(l) for l in sys.stdin if l.strip()]))' \
        | python3 "${root}/scripts/policy_check.py" "${env}"; then
    fail=1
  fi
done

echo "== platform/sealed-secrets"
if kubectl kustomize "${root}/platform/sealed-secrets" > "${out}/sealed-secrets.yaml"; then
  # kubeconform has no JSON schema for CustomResourceDefinition itself; skip it.
  if ! docker run --rm -i "${KUBECONFORM_IMAGE}" -strict -summary -kubernetes-version "${K8S_VERSION}" \
        -skip CustomResourceDefinition -schema-location default - < "${out}/sealed-secrets.yaml" | sed 's/^/  /'; then
    echo "  SCHEMA FAIL [sealed-secrets]"; fail=1
  fi
  want="5973b18ef77ae91e464bac62542a12c48844a08da69d80e45a8fc7f0d67fe205"
  got="$(shasum -a 256 "${root}/platform/sealed-secrets/upstream/controller-v0.41.2.yaml" | awk '{print $1}')"
  [[ "${got}" == "${want}" ]] && echo "  vendored upstream sha256 OK" \
    || { echo "  INTEGRITY FAIL: vendored controller.yaml sha256 ${got}"; fail=1; }
else
  echo "  RENDER FAIL [sealed-secrets]"; fail=1
fi

echo "== misconfiguration scan (trivy config; exceptions in .trivyignore)"
if docker run --rm -v "${out}:/m:ro" -v "${root}/.trivyignore:/.trivyignore:ro" \
      -v gitops-trivy-cache:/root/.cache/trivy "${TRIVY_IMAGE}" \
      config --quiet --ignorefile /.trivyignore --exit-code 1 /m > "${out}/trivy.txt" 2>&1; then
  echo "  trivy config OK"
else
  sed 's/^/  /' "${out}/trivy.txt" | tail -30; echo "  MISCONFIG FAIL"; fail=1
fi

echo "== argocd/ (AppProjects + Applications)"
if ! docker run --rm -v "${root}/argocd:/argocd:ro" "${KUBECONFORM_IMAGE}" \
      -strict -summary -kubernetes-version "${K8S_VERSION}" \
      -schema-location default -schema-location "${CRD_SCHEMAS}" /argocd | sed 's/^/  /'; then
  echo "  SCHEMA FAIL [argocd]"; fail=1
fi
# Guard rails on the Application objects themselves.
prod_app="${root}/argocd/applications/gitops-api-prod.yaml"
# staging and prod are manual approval gates: a merge alone must never deploy.
for e in staging prod; do
  if grep -qE '^\s+automated:' "${root}/argocd/applications/gitops-api-${e}.yaml"; then
    echo "  POLICY FAIL [argocd] gitops-api-${e} must NOT have automated sync (manual gate)"; fail=1
  fi
done
if grep -q 'resources-finalizer' "${prod_app}"; then
  echo "  POLICY FAIL [argocd] gitops-api-prod must not cascade-delete (no resources finalizer)"; fail=1
fi
grep -qE '^\s+selfHeal: true' "${root}/argocd/applications/gitops-api-dev.yaml" \
  || { echo "  POLICY FAIL [argocd] gitops-api-dev must auto-sync with selfHeal"; fail=1; }
[[ ${fail} -eq 0 ]] && echo "  argocd policy OK"

echo "== secret scan (gitleaks: working tree + git history)"
leaks=0
docker run --rm -v "${root}:/repo:ro" "${GITLEAKS_IMAGE}" dir /repo --config /repo/.gitleaks.toml --no-banner --redact --verbose --no-color \
  > "${out}/gitleaks-dir.txt" 2>&1 || leaks=1
if [[ -d "${root}/.git" ]]; then
  docker run --rm -v "${root}:/repo:ro" "${GITLEAKS_IMAGE}" git /repo --config /repo/.gitleaks.toml --no-banner --redact --verbose --no-color \
    > "${out}/gitleaks-git.txt" 2>&1 || leaks=1
fi
if [[ ${leaks} -eq 0 ]]; then echo "  no leaks found"; else
  grep -hE 'RuleID:|File:|Commit:|leaks found' "${out}"/gitleaks-*.txt | sed 's/^/  /' | head -20; echo "  SECRET SCAN FAIL"; fail=1; fi

if [[ ${fail} -ne 0 ]]; then echo "VALIDATION FAILED"; exit 1; fi
echo "ALL OVERLAYS VALID"
