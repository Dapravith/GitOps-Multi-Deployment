# Kubernetes command reference

Commands for building, inspecting, releasing and debugging `gitops-api` on the lab cluster, in the order you would use them.
Run them from `gitops-config/` unless a section says otherwise. Each block marks whether it is **read-only**, **dev or staging only**,
or **needs the owner's OK**.

Background: [README → Kubernetes implementation guide](../README.md#kubernetes-implementation-guide) ·
[Implementation Guide](design/IMPLEMENTATION-GUIDE.md) · [PROMOTION.md](../gitops-config/docs/PROMOTION.md) ·
[SECRETS.md](../gitops-config/docs/SECRETS.md)

## Best practices

1. **Git is the only write path.** After bootstrap, never `kubectl apply`, `edit`, `patch`, `scale` or `set image` an Argo CD-managed
   object in `gitops-*`. In dev and staging, self-heal reverts it within seconds. In prod, it leaves the app `OutOfSync` until the next sync.
   The only exceptions are the drift drills in §9.
2. **Name the namespace in every command** (`-n gitops-dev`). Never rely on the context's default namespace.
3. **Read before you write.** Run `argocd app diff` before every sync, and `kubectl diff` or `--dry-run=server` before any manual change.
4. **Prod is read-only.** The only command that writes to prod is `scripts/argocd.sh app sync gitops-api-prod`, and it needs the owner's OK every time.
5. **Use digests, not tags.** This applies to every image you add, including test pods.
6. **Save evidence.** Pipe important output into `docs/evidence/` with `| tee`. Output that wasn't captured doesn't count as proof.

---

## 1. Session setup (every new terminal)

```bash
export KUBECONFIG="$HOME/.kube/gitops-lab.kubeconfig"   # lab only; your default kubeconfig is never touched
kubectl config current-context                          # expect: kind-gitops-lab
kubectl cluster-info                                    # API server on 127.0.0.1
kubectl version                                         # client and server both v1.36.x (no version skew)
```

Optional safety guard for your shell profile. It refuses to run if the context is wrong:

```bash
k() {
  [[ "$(kubectl config current-context)" == "kind-gitops-lab" ]] || { echo "wrong context" >&2; return 1; }
  kubectl "$@"
}
```

## 2. Cluster lifecycle

```bash
bootstrap/kind/create-cluster.sh          # idempotent: creates the cluster and registry, or just re-exports the kubeconfig
bootstrap/git-server/start.sh             # lab Git server (used until phase 8.5)
kubectl get nodes -o wide                 # expect: Ready, v1.36.4
kubectl get --raw='/readyz?verbose' | tail -1    # expect: readyz check passed
kubectl get pods -A                       # every system pod Running/Completed
```

Check the Pod Security baseline once. This uses a throwaway namespace, which you delete afterwards:

```bash
kubectl create ns psa-probe
kubectl label ns psa-probe pod-security.kubernetes.io/enforce=restricted
kubectl -n psa-probe run bad --image=registry.k8s.io/pause:3.10 --dry-run=server -o name
# expect: Error ... violates PodSecurity "restricted:latest"
kubectl delete ns psa-probe --wait
```

Check every environment namespace against `restricted` (**read-only**: server-side dry run):

```bash
for ns in gitops-dev gitops-staging gitops-prod; do
  kubectl label --dry-run=server --overwrite ns "$ns" pod-security.kubernetes.io/enforce=restricted
done                                      # no warnings means every running pod complies
```

## 3. Validate offline before you commit (no cluster needed)

```bash
scripts/validate.sh                                        # expect: ALL OVERLAYS VALID
kubectl kustomize apps/gitops-api/overlays/dev             # render one environment
diff <(kubectl kustomize apps/gitops-api/overlays/staging) \
     <(kubectl kustomize apps/gitops-api/overlays/prod)    # differences between environments
for e in dev staging prod; do                              # digest currently in each environment
  printf '%-8s ' "$e"; grep 'digest:' apps/gitops-api/overlays/$e/kustomization.yaml
done
```

Compare the rendered manifests with the live cluster (**read-only**). Exit code 1 means there are differences:

```bash
kubectl kustomize apps/gitops-api/overlays/dev | kubectl diff --server-side -f -
```

## 4. Argo CD (read-only unless marked)

`scripts/argocd.sh` runs the pinned CLI in `--core` mode. It needs no password, token or port-forward.

```bash
scripts/argocd.sh app list                                 # sync and health status of every app
scripts/argocd.sh app get gitops-api-dev                   # resources, sync policy, conditions
scripts/argocd.sh app get gitops-api-dev --refresh         # re-read Git now instead of waiting ~60 s
scripts/argocd.sh app diff gitops-api-prod                 # what a prod sync WOULD change
scripts/argocd.sh app history gitops-api-prod              # past syncs and their revisions
scripts/argocd.sh app manifests gitops-api-dev             # what Argo CD rendered
scripts/argocd.sh app wait gitops-api-staging --sync --health --timeout 300

kubectl -n argocd get applications,appprojects             # the same objects, seen as CRs
kubectl -n argocd get application gitops-api-dev \
  -o jsonpath='{.status.sync.revision}{"\n"}'              # Git commit currently applied
```

**Needs the owner's OK, every time:**

```bash
scripts/argocd.sh app diff gitops-api-prod                 # always run diff first
scripts/argocd.sh app sync gitops-api-prod
scripts/argocd.sh app wait gitops-api-prod --sync --health --timeout 600
```

## 5. Inspect workloads (read-only, safe in every environment)

```bash
NS=gitops-dev     # or gitops-staging / gitops-prod
SEL='app.kubernetes.io/name=gitops-api'

kubectl -n $NS get all,cm,networkpolicy,pdb,sa -l $SEL -o wide
kubectl -n $NS get networkpolicy                           # default-deny-all must be present
kubectl -n $NS rollout status deploy/gitops-api --timeout=120s
kubectl -n $NS describe deploy gitops-api
kubectl -n $NS get pods -l $SEL -o wide
kubectl -n $NS get endpointslices -l kubernetes.io/service-name=gitops-api
kubectl -n $NS get events --sort-by=.lastTimestamp | tail -20
```

Prove that the running digest matches Git. `promote.sh` runs this same check:

```bash
grep 'digest:' apps/gitops-api/overlays/${NS#gitops-}/kustomization.yaml
kubectl -n $NS get pods -l $SEL \
  -o jsonpath='{range .items[*]}{.metadata.name}{"\t"}{.status.containerStatuses[0].imageID}{"\n"}{end}'
```

Logs (JSON lines; pipe through `jq` to read them):

```bash
kubectl -n $NS logs deploy/gitops-api --tail=50
kubectl -n $NS logs -l $SEL --prefix --since=10m           # all replicas
kubectl -n $NS logs <pod> --previous                       # the container's last crash
kubectl -n $NS logs deploy/gitops-api -f | jq -c '{ts:.timestamp, lvl:.level, msg:.fields.message}'
```

Check the hardening on the live pod:

```bash
kubectl -n $NS get pod -l $SEL -o jsonpath='{.items[0].spec.securityContext}{"\n"}{.items[0].spec.containers[0].securityContext}{"\n"}'
kubectl -n $NS get pod -l $SEL -o jsonpath='{.items[0].spec.automountServiceAccountToken}{"\n"}'   # false
kubectl auth can-i --list -n $NS --as=system:serviceaccount:$NS:gitops-api                        # minimal permissions
```

## 6. Reach the service

**Port-forward** (dev or staging; this goes through the API server, not the Service network):

```bash
kubectl -n gitops-dev port-forward svc/gitops-api 8080:80
curl -s localhost:8080/health; curl -s localhost:8080/ready; curl -s localhost:8080/version
```

**In-cluster client** (dev or staging test fixture). This is the only route the NetworkPolicy allows. The pod must meet PSA `restricted`,
and it stays up for the whole test (short-lived `kubectl run --rm` pods get past kindnet filtering for their first moments).
First pin the image by digest:

```bash
docker buildx imagetools inspect curlimages/curl:8.17.0 --format '{{json .Manifest.Digest}}'
```

```bash
kubectl -n gitops-dev apply -f - <<'EOF'
apiVersion: v1
kind: Pod
metadata:
  name: api-client
  labels: { app.kubernetes.io/role: api-client }
spec:
  automountServiceAccountToken: false
  securityContext:
    runAsNonRoot: true
    runAsUser: 100
    seccompProfile: { type: RuntimeDefault }
  containers:
    - name: curl
      image: curlimages/curl:8.17.0@sha256:<digest-from-above>
      command: ["sleep", "infinity"]
      resources: { requests: { cpu: 10m, memory: 16Mi }, limits: { memory: 32Mi } }
      securityContext:
        allowPrivilegeEscalation: false
        readOnlyRootFilesystem: true
        capabilities: { drop: ["ALL"] }
EOF
kubectl -n gitops-dev wait pod/api-client --for=condition=Ready --timeout=60s
kubectl -n gitops-dev exec api-client -- curl -s http://gitops-api/version
```

Load loop for drills (one request every 100 ms; reports failures only):

```bash
kubectl -n gitops-dev exec api-client -- sh -c \
  'while true; do curl -sf -o /dev/null --max-time 2 http://gitops-api/version || echo "FAIL $(date +%T)"; sleep 0.1; done'
```

Check the NetworkPolicy: a pod **without** the label must be refused:

```bash
kubectl -n gitops-dev label pod api-client app.kubernetes.io/role- --overwrite
kubectl -n gitops-dev exec api-client -- curl -s --max-time 3 http://gitops-api/health; echo "exit=$?"   # expect: non-zero (timeout)
kubectl -n gitops-dev label pod api-client app.kubernetes.io/role=api-client
```

Clean up: `kubectl -n gitops-dev delete pod api-client`

## 7. Debug a failing pod

Work through these in order and stop as soon as you find the cause:

```bash
kubectl -n $NS get pods -l $SEL                            # status: Pending, CrashLoopBackOff, ImagePullBackOff…
kubectl -n $NS describe pod <pod>                          # Events section: scheduling, pull, probe and admission errors
kubectl -n $NS logs <pod> --previous                       # e.g. "configuration error: PORT must be..."
kubectl -n $NS get events --field-selector involvedObject.name=<pod>
kubectl -n $NS get rs -l $SEL                              # a stuck new ReplicaSet next to the old one?
scripts/argocd.sh app get gitops-api-${NS#gitops-}         # sync errors, e.g. admission refused
```

The image is distroless, so `kubectl exec ... sh` cannot work. Attach an ephemeral debug container instead
(**dev or staging only**; it is not in Git, but Argo CD does not remove it):

```bash
kubectl -n gitops-dev debug -it <pod> --target=api --profile=restricted \
  --image=busybox:1.37@sha256:<pinned-digest> -- sh
```

| Symptom | Usual cause | Fix (in Git) |
|---|---|---|
| `ImagePullBackOff` | digest is not in GHCR or the mirror | correct the digest in the overlay; revert |
| `CrashLoopBackOff`, exit 1 | invalid `PORT`/`APP_ENV`/`LOG_FORMAT` | fix the `configMapGenerator` literal |
| Pod `Running`, not `Ready` | `/ready` returns 503 (starting or draining) | wait; check logs |
| Admission `violates PodSecurity` | a new field breaks `restricted` | fix the manifest; `validate.sh` should have caught it |
| App `OutOfSync` in prod | expected after a merge, or manual drift | `app diff`, then a sync with the owner's OK |

## 8. Release: promote and roll back

Run these from a clean `main`. The scripts check their own preconditions (see [PROMOTION.md](../gitops-config/docs/PROMOTION.md)).

```bash
scripts/promote.sh dev staging          # branch promote/staging-<digest12>, one-line diff
git show --stat HEAD                    # review: exactly one digest line changed
# Phase 8+: open a PR and let `validate` pass. Lab: git switch main && git merge --no-ff promote/staging-<digest12>
scripts/argocd.sh app wait gitops-api-staging --sync --health --timeout 300

scripts/promote.sh staging prod         # merging does NOT deploy prod
scripts/argocd.sh app diff gitops-api-prod
scripts/argocd.sh app sync gitops-api-prod     # owner's OK first
```

Rollback means reverting the promotion merge in Git:

```bash
git log --merges --oneline -5
git revert -m 1 <promotion-merge-sha>   # then push/PR as usual
# dev/staging: heal automatically. prod: app diff, then app sync (owner's OK)
```

Don't use these on dev or staging: `kubectl rollout undo`, `argocd app rollback`, `kubectl set image`. Self-heal reverts them.

## 9. Drills (dev only, with the load loop from §6 running)

These are the only deliberate out-of-Git changes. Self-heal should revert each one. Time how long it takes:

```bash
kubectl -n gitops-dev scale deploy gitops-api --replicas=5; date +%T
kubectl -n gitops-dev get deploy gitops-api -w             # watch it return to 1; note the time

kubectl -n gitops-dev delete networkpolicy default-deny-all
kubectl -n gitops-dev get networkpolicy -w                 # watch it come back
```

Prod drift (**read-only** check after a deliberate scale, with the owner's OK): it must stay `OutOfSync`, not be reverted:

```bash
scripts/argocd.sh app get gitops-api-prod | grep -E 'Sync Status|Health'
```

Controller outage (dev keeps serving; self-heal pauses, then resumes):

```bash
kubectl -n argocd scale statefulset argocd-application-controller --replicas=0
kubectl -n argocd scale statefulset argocd-application-controller --replicas=1
kubectl -n argocd rollout status statefulset argocd-application-controller
```

## 10. Secrets

```bash
scripts/seal.sh --check-cert                                         # committed cert == cluster's active cert
printf '%s' "$VALUE" | scripts/seal.sh dev <secret-name> <key>       # plaintext via stdin only
git add apps/gitops-api/overlays/dev/sealedsecret-<secret-name>.yaml # commit; Argo CD applies it

kubectl -n gitops-dev get sealedsecrets
kubectl -n gitops-dev get sealedsecret <secret-name> -o jsonpath='{.status.conditions}{"\n"}'   # Synced=True
kubectl -n gitops-dev get secret <secret-name> -o jsonpath='{.data}' | jq 'keys'                # key names only, never values

scripts/sealed-secrets-key.sh status
scripts/sealed-secrets-key.sh backup        # after setup and after every 30-day key renewal; keep it outside Git
scripts/sealed-secrets-key.sh restore <file>
```

## 11. Never do this

| Command | Why |
|---|---|
| `kubectl apply/edit/patch` on `gitops-*` objects | bypasses review; reverted (dev/staging) or causes drift (prod) |
| `kubectl delete ns gitops-*` | prune and the finalizer handle removal through Git |
| `kubectl get secret -o yaml` and pasting it anywhere | leaks plaintext; check key names only |
| `kubectl create secret` in Git or committing a `Secret` | `validate.sh` rejects it; use `seal.sh` |
| any write against `gitops-prod` without the owner's OK | prod changes go through Git plus a manual sync only |
| images by tag (`:latest`) | not reproducible; policy rejects it |
| `kubectl config use-context` on your default kubeconfig | the lab uses its own kubeconfig file |

## 12. Tear down (lab only)

```bash
scripts/sealed-secrets-key.sh backup        # FIRST: otherwise every SealedSecret in Git can never be decrypted
kind delete cluster --name gitops-lab
docker rm -f gitops-lab-registry            # optional: also deletes the local mirror's images
```
