#!/usr/bin/env bash
# Branch rules as code. The JSON files in rulesets/ are the desired state;
# this script shows (plan) or applies (apply) the difference on GitHub.
#
#   ./rulesets.sh plan            # read-only diff for all three repos
#   ./rulesets.sh apply           # create/update to match the files (admin)
#   ./rulesets.sh plan gitops-api # one repo
#
# A ruleset is matched by its "name". Change the file through a reviewed PR
# first, then apply: the file, not the GitHub UI, is the record.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"
OWNER=Dapravith
declare -A REPO=(
  [gitops-api]=gitops-api
  [gitops-config]=gitops-config
  [umbrella]=GitOps-Multi-Deployment
)

mode=${1:-plan}
[[ $mode == plan || $mode == apply ]] || { echo "usage: $0 plan|apply [file-stem]" >&2; exit 2; }
targets=("${@:2}"); [[ ${#targets[@]} -gt 0 ]] || targets=(gitops-api gitops-config umbrella)

# Fields GitHub returns that are not part of the desired state.
normalize='del(.id, .source, .source_type, .node_id, ._links, .created_at, .updated_at, .current_user_can_bypass)
  | .rules |= sort_by(.type)'

drift=0
for stem in "${targets[@]}"; do
  repo="$OWNER/${REPO[$stem]}"
  file="rulesets/$stem.json"
  name=$(jq -r .name "$file")
  id=$(gh api "repos/$repo/rulesets" --jq ".[] | select(.name == \"$name\") | .id" | head -1)

  if [[ -z $id ]]; then
    echo "## $repo: ruleset \"$name\" does not exist -> create"
    drift=1
    [[ $mode == apply ]] && gh api -X POST "repos/$repo/rulesets" --input "$file" --jq '"created id \(.id)"'
    continue
  fi

  live=$(gh api "repos/$repo/rulesets/$id" | jq -S "$normalize")
  want=$(jq -S "$normalize" "$file")
  # Compare only keys the file sets, so GitHub-added defaults don't show as drift.
  live_subset=$(jq -S --argjson want "$want" '
    def pick($w): if ($w|type) == "object" and (.|type) == "object"
                  then with_entries(.key as $k | select($w | has($k)) | .value |= pick($w[$k]))
                  elif ($w|type) == "array" and (.|type) == "array" then [range(0; length) as $i | .[$i] | pick($w[$i] // .)]
                  else . end;
    pick($want)' <<<"$live")

  if diff -u <(echo "$live_subset") <(echo "$want") --label "live $repo #$id" --label "$file" >/tmp/ruleset.diff; then
    echo "## $repo: \"$name\" (#$id) matches $file"
  else
    echo "## $repo: \"$name\" (#$id) differs -> update"; cat /tmp/ruleset.diff
    drift=1
    [[ $mode == apply ]] && gh api -X PUT "repos/$repo/rulesets/$id" --input "$file" --jq '"updated id \(.id)"'
  fi
done
rm -f /tmp/ruleset.diff
[[ $mode == plan ]] && exit $drift || exit 0
