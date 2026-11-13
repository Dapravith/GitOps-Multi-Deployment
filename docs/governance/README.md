# GitHub governance: Phase 2 proposal (SEC-005 … SEC-009, SEC-011)

**Status: proposal for owner review. Nothing in this folder has been applied to GitHub.**

Decisions (2026-11-13): staging becomes "approved PR → Argo CD auto-sync"; a GitHub App
replaces the default token for the image-updater. **Reviews (revised 2026-11-13):** one account
for now, so rules are applied as code with a required PR + checks and **0 required approvals**;
the owner reviews and merges each PR. Raise to 1 approval + code owners when a second reviewer
exists (SEC-006 stays open until then).

Apply / check drift: `./rulesets.sh plan` (read-only, exit 1 on drift) and `./rulesets.sh apply`.

## 1. What changes

| | gitops-api | gitops-config | umbrella |
|---|---|---|---|
| Today | nothing | PR + `validate`, **0 approvals** | nothing |
| PR required | yes | yes | yes |
| Approvals | **0** for now (owner reviews + merges); later 1, code owner | same | same |
| Stale approval dismissed on push | yes | yes | yes |
| Required checks (GitHub Actions only) | `test`, `scan-source`, `image` | `validate` | none (no CI) |
| Branch must be up to date | yes | yes | n/a |
| Merge methods | merge, squash | **merge only** (rollback = `git revert -m 1`) | merge, squash |
| Force push / delete `main` | blocked | blocked | blocked |
| Bypass actors | **none**, admins included | none | none |

Files: [rulesets/gitops-api.json](rulesets/gitops-api.json), [rulesets/gitops-config.json](rulesets/gitops-config.json)
(replaces ruleset `27097700`), [rulesets/umbrella.json](rulesets/umbrella.json), and
`CODEOWNERS.<repo>` (each goes to `.github/CODEOWNERS` in that repo).

Why these settings:

- **Code-owner review + `require_last_push_approval`.** The approval must come from a human owner
  who did not push the last commit. A GitHub App can submit an approving review, but it can't be a
  code owner, so a bot approval never satisfies the rule. That closes the
  "bot approves its own PR" path (SEC-008).
- **No bypass actors.** As admin you could otherwise merge around every rule. With none, emergency changes
  also go through a PR. To break glass, you would edit the ruleset itself, which leaves an audit-log entry.
- **Integration-pinned checks (`integration_id: 15368`, GitHub Actions).** A status named `validate`
  posted by some other app or token does not count.
- **Strict up-to-date.** For config, two PRs that each pass alone can still conflict once combined. This setting re-runs
  `validate` on the merged result.

## 2. Order of operations (avoids locking yourself out)

1. **You:** invite the second account: `gh api -X PUT repos/Dapravith/<repo>/collaborators/<reviewer> -f permission=push`
   for all three repos, then accept the invitation from that account. Tell me the handle; I replace `@REVIEWER`.
2. **Me (PRs, under today's rules):** add `.github/CODEOWNERS` to each repo. In gitops-config the PR
   still merges with 0 approvals; in the other two there are no rules yet.
3. **Me (after your OK):** apply the rulesets:
   ```bash
   gh api -X POST repos/Dapravith/gitops-api/rulesets --input rulesets/gitops-api.json
   gh api -X PUT  repos/Dapravith/gitops-config/rulesets/27097700 --input rulesets/gitops-config.json
   gh api -X POST repos/Dapravith/GitOps-Multi-Deployment/rulesets --input rulesets/umbrella.json
   ```
4. **Me + reviewer:** SEC-009 negative tests (section 4). From here on, every PR (mine, yours, the bot's)
   needs the reviewer's approval.

If step 3 runs before step 1 finishes, nothing can merge. Recovery: `gh api -X PUT …/rulesets/<id> -f enforcement=disabled`.

## 3. GitHub App for the image-updater (SEC-010, SEC-011)

Create it under Settings → Developer settings → GitHub Apps → New GitHub App:

| Field | Value |
|---|---|
| Name | `gitops-config-image-updater` (any unique name) |
| Homepage URL | `https://github.com/Dapravith/gitops-config` |
| Webhook | **unchecked** (Active off) |
| Repository permissions | **Contents: Read and write**, **Pull requests: Read and write**, Metadata: Read-only (forced). Everything else: No access |
| Organization / account permissions | none |
| Where can it be installed | Only on this account |

Then: generate a private key (downloads a `.pem`), click **Install App**, choose **Only select
repositories → gitops-config**, and store:

```bash
# Client ID is shown at the top of the App settings page (starts with "Iv").
gh variable set IMAGE_UPDATER_CLIENT_ID -R Dapravith/gitops-config --body <Client ID>
gh secret set IMAGE_UPDATER_APP_KEY -R Dapravith/gitops-config < ~/Downloads/<app>.private-key.pem
```

Delete the local `.pem` afterwards. GitHub keeps the only copy you need. The workflow change
(I make it once the secret exists):

- mint a token per run with `actions/create-github-app-token` (pinned by SHA), scoped to `gitops-config`;
- push the branch and open the PR with that token. App-made PRs trigger `pull_request`, so the manual
  `gh workflow run validate.yml` dispatch and the `actions: write` permission go away;
- keep `gh pr merge --auto`: with the ruleset above it only fires **after** the reviewer approves
  and `validate` passes, so no config change merges unapproved (SEC-012). The bot can't approve.

What the App cannot do: approve PRs (it isn't a code owner), push to `main` (ruleset), touch
any repo except gitops-config, or reach the cluster. If its key leaks, the worst case is an
unapproved PR on gitops-config.

## 4. SEC-009 verification plan (run after step 3)

| # | Attempt | Expected |
|---|---|---|
| 1 | `git push origin HEAD:main` with a commit, as admin | rejected: `GH013 … Changes must be made through a pull request` |
| 2 | `git push --force origin <old-sha>:main` | rejected (non_fast_forward) |
| 3 | open a PR, approve it with the author's account | GitHub refuses ("can't approve your own PR"); merge stays blocked |
| 4 | PR where the reviewer approves, then the author pushes another commit | approval dismissed; merge blocked until re-approved |
| 5 | gitops-config PR that adds `automated:` to `gitops-api-prod` | `validate` fails → merge blocked even with approval |
| 6 | gitops-api PR with a failing `cargo fmt` | `test` fails → merge blocked |
| 7 | `gh pr merge --admin` on a blocked PR | rejected (no bypass actors) |
| 8 | delete `main` via API | rejected |

Each result (command, output, timestamp) goes to `docs/evidence/sec-phase2-governance.md`.

## 5. Related finding for Phase 3 (SEC-013)

`gitops-api` `ci.yml` pushes and signs an image whenever `github.event_name != 'pull_request'`.
That includes `workflow_dispatch` **on any branch**. The image-updater only proposes `main`-signed
attestations, so an unreviewed branch image never reaches dev. It is still published to GHCR and
signed. Fix (separate PR): publish only when `github.event_name == 'push' && github.ref == 'refs/heads/main'`.
