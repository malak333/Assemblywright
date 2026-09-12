# Developer GitHub publication

The owner's subsequent [GitHub setup extension](developer-github-setup-design.md)
adds sign-in, discovery, and explicit creation of repositories. It supersedes the
existing-repositories-only setup restriction in this design while preserving the
reviewed-feature publication, protected-check, and merge boundaries below.

## Accepted scope

The owner confirmed that each Developer assembly line feature should use its own
branch, automatically commit and push independently reviewed changes, open a pull
request, and merge automatically after the required GitHub checks pass. The owner
then requested this document and implementation. This is a Developer-only
extension of the supervised Windows runner, not activation of the production
publication adapter or proof of production readiness.

## Requirements and assumptions

- One owner uses the existing Mac UI and Windows execution account. Windows owns
  repository bindings, candidate bytes, publication state, and recovery decisions.
- Each project has an explicitly saved existing GitHub repository destination and
  base branch. Connecting the destination enables automatic publication for future
  runs. Unconnected projects retain local operation with an explicit unpublished
  label; historical completed features are not retroactively uploaded.
- A feature branch is derived from the immutable feature ID. Only the exact
  validated, independently approved candidate may enter its commit and PR.
- Required checks must exist, belong to the exact candidate commit, and all pass.
  Normal protected merge is used; no administrator bypass, force push, protection
  changes, automatic conflict resolution, or implicit public repository creation.
- Success for a connected project includes verified merge and remote base
  reconciliation. Later features cannot advance while publication is unresolved.
- Network and command work runs asynchronously, with bounded output, deadlines,
  cancellation, visible progress, and durable intent before each external effect.
- Authentication stays in the Windows owner's Git/GitHub CLI configuration. Cloud
  planners, reviewers, model prompts, and tool packets receive no publication
  credentials or ability to approve publication.

## Design

### Repository connection and compatibility

Provide a GitHub connection sheet in the Developer app. The owner selects a project,
enters its GitHub repository URL and base branch, and saves while developer work is
idle. The authenticated Windows API validates and persists the binding with the
observed state revision. Show the effective destination and automatic merge policy
in the Start confirmation. Missing tools, authentication, repository access, base,
or required-check configuration produce actionable errors before publication.

The connection selects an existing repository. Repository creation and importing an
existing repository into a local project are separate operations; the initial slice
does not silently create repositories or overwrite existing project contents.

### Candidate and publication authority

Use a dedicated Developer publication module in the Windows master package.
Adapt the production ordering and evidence principles without coupling Developer
projects to production's fixed `malak333/Assemblywright` provisioning policy.
Use a runner-owned publication checkout, never execute credentialed Git from a
model-writable directory. Freeze the exact reviewed candidate and bind the selected
repository, base SHA, feature branch, candidate SHA, and PR identity. Reject
unreviewed local content, conflicting remote changes, and repository/head drift.
Git hooks, inherited repository command configuration, and model-selected remote
URLs cannot determine credentialed operations.
Execution remains under the existing Windows owner account. Separate publication
directories and process controls are not an OS boundary against hostile code
running as that same account; production containment evidence remains separate.

Build the commit from a clean runner-owned checkout at the frozen base SHA. Overlay
only the review packet's cumulative approved edits, checking every original
`before_sha256` (including expected absence) against the base. Reject any unmentioned
difference and record the resulting tree and commit identities. The bounded,
potentially truncated `project_context` helper is never a publication manifest.

Publication order is prepare candidate, commit, push feature branch, create or
reconcile the exact PR, wait for required checks, verify the PR head and base,
request normal merge, and verify the resulting remote base. State and receipts
distinguish each step. Never call a pending merge request or a successful push
complete. Preserve PR URL and verified commit identifiers in the feature result.
Derive the required-check set from GitHub's protected-branch policy and reject an
empty set. Bind successful checks to the exact candidate commit. The merge request
itself uses an expected-head guard (`--match-head-commit`), not merely a preceding
head check. Verify merged state, merge SHA, and remote base before recording success.

### Cancellation and recovery

Stop and Emergency Pause remain available throughout publication. They prevent new
effects and cancel active commands; they cannot undo completed pushes or merges.
Persist intent before effect and receipt after verification. Lost acknowledgements,
restart, timeouts, changed heads, and ambiguous failures block the queue. Ordinary
Resume must never rerun code or blindly replay a publication operation. An explicit
reconciliation action may inspect the existing branch, PR, checks, and merge to
recover only the same immutable operation; uncertain state remains attention.
Publication failure cannot consume a model-repair attempt or authorize new code.

### User interface

Show connection state per project, publication stage, error/recovery guidance, and
a validated GitHub PR link. Distinguish local validation/review completion from
GitHub publication and merge completion. Disable repair, removal, reviewer changes,
and new starts when they could bypass unresolved publication. Keep polling and
controls responsive while checks are pending.

### Developer HTTP contract

The authenticated snapshot advertises `github_publication_supported`,
`github_publication_running`, `github_publication_unresolved`,
`can_manage_github_connections`, and `github_connections`. The snapshot reports
runtime readiness separately as `github_publication_available` and
`github_publication_message`; missing Git/gh does not disable local-only work.
Each connection carries
`project`, `repository_url`, `base_branch`, and `automatic_merge`. Feature fields
include `publication_status`, `publication_stage`, destination, branch, commit,
PR URL, merged SHA, message, and `can_reconcile_publication`. Optional absence
preserves compatibility with older local results; it is never a merged receipt.

Authenticated `GET /publication` observes the snapshot. Strict `POST /publication`
accepts `action: save_connection` with `expected_revision`, `project`,
`repository_url`, and `base_branch`; `action: disconnect` with revision/project;
or `action: reconcile` with revision, `feature_id`, and `expected_checkpoint`.
Reconciliation returns an accepted snapshot and proceeds asynchronously.
Stages cover `prepare_candidate`, `push_branch`, `open_pull_request`,
`wait_required_checks`, `merge_pull_request`, `verify_remote_base`, and `complete`.
An uncertain effect produces `failed`/`publication_attention`; verified publication
produces `succeeded`/`publication_merged`. Local-only results retain their review
checkpoint and are identified as `local_only`.

## Decision log

| Decision | Alternatives | Rationale |
| --- | --- | --- |
| Automatic PR and normal merge after checks | PR-only review; direct base push | Owner explicitly selected automatic merge with a feature branch. |
| Dedicated Developer adapter in Windows master package | Activate production adapter; let the model run Git | Supports project destinations while retaining Windows authority and production separation. |
| Explicit project connection, existing repository | Infer destination; silently create a public repository | Avoids uploading project code to an unintended destination. |
| Frozen candidate and durable publication stages | Push whatever is in the project after review | Prevents stale review and duplicate or ambiguous effects. |
| Reconcile interrupted publication explicitly | Automatic retry; regenerate the feature | Preserves existing PR and merge evidence without replaying uncertain effects. |
| Base plus exact cumulative edit overlay | Copy a bounded project-context snapshot | Design-review objection accepted: a truncated context is not a complete publication manifest; original hashes and resulting tree must bind the commit. |
| Protected required checks and expected-head merge guard | Check head only before an unbound merge | Design-review objection accepted: guards the merge request against a concurrent PR head change and rejects repositories without required checks. |

## Implementation plan

1. Independently review this design and record resolved objections.
2. Implement the publication adapter, persisted connection/feature state, authenticated
   owner controls, completion gating, cancellation, and restart reconciliation.
3. Add native Swift connection, publication status, PR link, and recovery controls.
4. Cover input validation, candidate binding, checks/head drift, legacy state,
   cancellation, recovery, and queue blocking with focused Rust and Swift tests.
5. Exercise real local Git and runner HTTP/process boundaries with a controlled
   GitHub fixture. Real GitHub credentials, branch protection, and Windows execution
   require separately reported live evidence.
6. Run focused tests, documentation drift, diff checks, and the canonical local
   gate; update operator instructions, build commands, and the knowledge base.

## Validation and review evidence

The evidence in this section was captured for the original September 8
implementation. It is retained as recovery provenance and does not establish that
the newly restored source or its eventual commit has passed the same boundaries.

Independent high-risk design review covered skeptic, constraint guardian, user
advocate, and arbiter roles. The arbiter required the two binding corrections now
recorded above and otherwise found the design sufficient for implementation. Both
objections are resolved; final design disposition is **APPROVED**.

The Swift implementation received independent **APPROVE** after fixes for stale
mutation acknowledgements, canonical `.git` URL acknowledgement, complete merge
evidence, and explicit local-only presentation. Focused native tests passed:
`DeveloperGitHubTests` (7) and `DeveloperRunnerTests` (8); the optional installed
runner observation was not enabled in this focused run.

The canonical `release-local.sh` run passed its prerequisite contracts, formatting,
and workspace clippy, then failed in the existing Mac agent relay suite:
`agent_sigterm_reaps_active_mlx_process_group_before_exit` did not observe its
backend process marker (9 relay tests passed; 1 failed). This is not a green full
release gate. Its log is retained at
`target/developer-github-publication/release-local.log`. The final adapter's focused
validation, installation, and live GitHub evidence are recorded separately below.

Independent integration review is **APPROVED** after correcting atomic destination
freezing, canonical Swift wire fields, and Stop/Emergency precedence at final
completion. Native runner, model-selection, and escalation E2Es passed on macOS
and Windows. Planning, review, repair, and settings E2Es passed on macOS. The
Developer build/connection unit suites each passed 21 tests, and the connection
process E2E passed. These results cover Developer behavior; they do not replace
the failed full gate or prove a live GitHub account operation.

Final adapter/process review is **APPROVED**, with no remaining P0–P3 findings.
The final source passed:

- macOS Developer runner tests: **93 passed**; Swift publication/runner tests:
  **15 passed**.
- Windows Developer runner tests: **95 passed**, with one child-process fixture
  intentionally ignored as a standalone test. Both suspended-start/failed-setup
  containment tests passed.
- Native publication E2Es on macOS and Windows: real bare Git transport,
  canonical project connection, rejection of unbound/non-strict checks, exact
  commit/app verification, Stop-to-attention, explicit reconciliation and merge,
  exact merged bytes, and unconnected local-only behavior.
- Final macOS workspace clippy, Windows Developer clippy, format, documentation
  drift, and diff checks passed. Six critical backend/fixture source hashes matched
  the native Windows checkout.

Logs are under `target/developer-github-publication/`: `local-unit-final.log`,
`swift-tests.log`, `local-publication-final.log`, `windows-final.log`, and
`windows-source-hashes.json`. Fixture tests used no live GitHub credentials or
external repository. Real account authentication, hosted checks, actual GitHub
merge, production signing, and production readiness are not claimed.

CLI behavior references: [GitHub CLI merge](https://cli.github.com/manual/gh_pr_merge)
and [required checks](https://cli.github.com/manual/gh_pr_checks).
