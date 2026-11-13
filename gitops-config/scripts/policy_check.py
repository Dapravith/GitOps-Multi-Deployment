#!/usr/bin/env python3
"""Policy gate for rendered Kustomize output (JSON list of manifests on stdin).

Usage: yq -o=json ... rendered.yaml | python3 policy_check.py <env>

Rules are deliberately few and each maps to a requirement of this project.
Exit 1 with a list of violations if any rule fails.
"""

from __future__ import annotations

import json
import re
import sys

DIGEST_RE = re.compile(r"^ghcr\.io/[a-z0-9-]+/[a-z0-9._-]+@sha256:[0-9a-f]{64}$")
FORBIDDEN_KINDS = {"Secret"}  # only SealedSecret may carry secret material
EXPECTED_NS = {"dev": "gitops-dev", "staging": "gitops-staging", "prod": "gitops-prod"}
MIN_REPLICAS = {"dev": 1, "staging": 2, "prod": 3}


def pod_specs(obj: dict):
    if obj.get("kind") in {"Deployment", "StatefulSet", "DaemonSet", "Job"}:
        yield obj["spec"]["template"]["spec"]
    elif obj.get("kind") == "Pod":
        yield obj["spec"]


def check_app_config(env: str, ref: str, data: dict) -> list[str]:
    """Mirror gitops-api's own config validation (src/config.rs) so a value the
    app would reject at startup fails here, before merge, instead of crash-looping
    in the cluster (found in phase 13 with PORT=0)."""
    errs: list[str] = []
    port = data.get("PORT", "8080")
    if not (port.isdigit() and 1 <= int(port) <= 65535):
        errs.append(f"{ref}: PORT must be an integer 1-65535, got {port!r}")
    app_env = data.get("APP_ENV")
    if app_env != env:
        errs.append(f"{ref}: APP_ENV must be {env!r} in the {env} overlay, got {app_env!r}")
    log_format = data.get("LOG_FORMAT", "json").strip().lower()
    if log_format not in {"json", "pretty", "text"}:
        errs.append(f"{ref}: LOG_FORMAT must be json or pretty, got {data.get('LOG_FORMAT')!r}")
    return errs


def is_sealed_secrets_binding(o: dict) -> bool:
    return (
        o["metadata"]["name"] == "sealed-secrets-controller"
        and o.get("roleRef") == {
            "apiGroup": "rbac.authorization.k8s.io",
            "kind": "ClusterRole",
            "name": "secrets-unsealer",
        }
        and o.get("subjects") == [{
            "kind": "ServiceAccount",
            "name": "sealed-secrets-controller",
            "namespace": "sealed-secrets",
        }]
    )


SEALED_VALUE_RE = re.compile(r"^[A-Za-z0-9+/]{100,}={0,2}$")


def check_sealed_secret(ref: str, o: dict) -> list[str]:
    errs: list[str] = []
    annotations = o["metadata"].get("annotations") or {}
    # Strict scope only: the ciphertext is bound to this name AND namespace.
    for key in ("sealedsecrets.bitnami.com/cluster-wide", "sealedsecrets.bitnami.com/namespace-wide"):
        if annotations.get(key) == "true":
            errs.append(f"{ref}: {key} weakens scope; use the default strict scope")
    spec = o.get("spec", {})
    encrypted = spec.get("encryptedData") or {}
    if not encrypted:
        errs.append(f"{ref}: spec.encryptedData is empty")
    for key, value in encrypted.items():
        if not SEALED_VALUE_RE.match(str(value)):
            errs.append(f"{ref}: encryptedData[{key!r}] does not look like kubeseal ciphertext")
    template = spec.get("template") or {}
    for field in ("data", "stringData"):
        if template.get(field):
            errs.append(f"{ref}: spec.template.{field} would commit plaintext")
    return errs


def check(env: str, objs: list[dict]) -> list[str]:
    errs: list[str] = []
    ns = EXPECTED_NS[env]
    kinds = {o["kind"] for o in objs}

    for o in objs:
        kind, name = o["kind"], o["metadata"]["name"]
        ref = f"{kind}/{name}"

        if kind == "ConfigMap" and name.startswith("gitops-api-config"):
            errs.extend(check_app_config(env, ref, o.get("data", {})))

        if kind == "SealedSecret":
            errs.extend(check_sealed_secret(ref, o))

        # AppProjects cannot pin namespaced kinds by name, so the ONLY RBAC object
        # an overlay may contain is the Sealed Secrets access binding.
        if o.get("apiVersion", "").startswith("rbac.authorization.k8s.io/"):
            if kind != "RoleBinding" or not is_sealed_secrets_binding(o):
                errs.append(f"{ref}: only components/sealed-secrets-access may add RBAC to an overlay")

        if kind in FORBIDDEN_KINDS:
            errs.append(f"{ref}: plaintext {kind} must not be committed (use SealedSecret)")

        if kind == "Namespace":
            labels = o["metadata"].get("labels", {})
            if name != ns:
                errs.append(f"{ref}: expected namespace {ns}")
            if labels.get("pod-security.kubernetes.io/enforce") != "restricted":
                errs.append(f"{ref}: must enforce Pod Security 'restricted'")
        elif o["metadata"].get("namespace") != ns:
            errs.append(f"{ref}: namespace is {o['metadata'].get('namespace')!r}, expected {ns}")

        if kind == "Deployment":
            replicas = o["spec"].get("replicas", 1)
            if replicas < MIN_REPLICAS[env]:
                errs.append(f"{ref}: replicas {replicas} < {MIN_REPLICAS[env]} for {env}")

        for spec in pod_specs(o):
            if spec.get("automountServiceAccountToken") is not False:
                errs.append(f"{ref}: automountServiceAccountToken must be false")
            psc = spec.get("securityContext", {})
            if psc.get("runAsNonRoot") is not True:
                errs.append(f"{ref}: pod runAsNonRoot must be true")
            if psc.get("seccompProfile", {}).get("type") != "RuntimeDefault":
                errs.append(f"{ref}: seccompProfile must be RuntimeDefault")
            for c in spec.get("initContainers", []) + spec["containers"]:
                cref = f"{ref}[{c['name']}]"
                if not DIGEST_RE.match(c["image"]):
                    errs.append(f"{cref}: image {c['image']!r} is not ghcr.io/...@sha256:<digest>")
                sc = c.get("securityContext", {})
                if sc.get("allowPrivilegeEscalation") is not False:
                    errs.append(f"{cref}: allowPrivilegeEscalation must be false")
                if sc.get("readOnlyRootFilesystem") is not True:
                    errs.append(f"{cref}: readOnlyRootFilesystem must be true")
                if "ALL" not in sc.get("capabilities", {}).get("drop", []):
                    errs.append(f"{cref}: capabilities.drop must include ALL")
                res = c.get("resources", {})
                for field in ("cpu", "memory"):
                    if field not in res.get("requests", {}):
                        errs.append(f"{cref}: missing requests.{field}")
                if "memory" not in res.get("limits", {}):
                    errs.append(f"{cref}: missing limits.memory")
                for probe in ("readinessProbe", "livenessProbe"):
                    if probe not in c:
                        errs.append(f"{cref}: missing {probe}")

    if "NetworkPolicy" not in kinds:
        errs.append("no NetworkPolicy rendered (default-deny required)")
    if env in {"staging", "prod"} and "PodDisruptionBudget" not in kinds:
        errs.append(f"{env}: PodDisruptionBudget required")
    return errs


def main() -> int:
    env = sys.argv[1]
    data = json.load(sys.stdin)
    objs = [d for d in (data if isinstance(data, list) else [data]) if d]
    errs = check(env, objs)
    for e in errs:
        print(f"  POLICY FAIL [{env}] {e}")
    if not errs:
        print(f"  policy OK [{env}]: {len(objs)} objects")
    return 1 if errs else 0


if __name__ == "__main__":
    sys.exit(main())
