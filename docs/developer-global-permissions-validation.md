# Global permissions and navigation validation

Scope: existing-project selection in feature planning, Active/Succeeded Assembly
Line tabs, and one Windows-owned global permission policy. The accepted contract
is [global permissions and project navigation](developer-global-permissions-design.md).

## Coverage and review

- Rust tests cover Ask defaults, migration above legacy project/action revisions,
  cross-project access, immutable legacy evidence, atomic policy history,
  exhausted revisions, missing/rolled-back state, and staged authority revocation.
- Swift tests cover catalog-bound project choices, empty and disappeared choices,
  explicit new-project entry, retained succeeded history, global request bodies,
  exact save acknowledgements, stale receipts, and unsupported runners.
- The existing chat HTTP/process E2E covers authenticated reads and saves,
  unauthenticated/invalid/stale/legacy-save rejection, active-chat and Emergency
  Pause rejection, cross-project inheritance, and restart persistence. The
  scalable-repair E2E uses the same global policy for staged tool execution.
- Independent complete-diff review identified policy rollback, shutdown-save,
  and project-selector marker collision boundaries. Guards and typed project
  selection address them; regression tests cover each finding. A SQLite abort
  trigger verifies that failed policy-history writes roll back the policy save.

## Closeout verdicts

Documentation and safety: the design, safety rules, workflow guide, chat-history
contract, and canonical command guide describe the new global authority and
preserved review/publication boundaries. Durable knowledge was added to the
repository knowledge base.

The `unit-testing-test-generate` workflow targets behavior and failure modes; no
coverage percentage is claimed. The `e2e-testing` workflow uses native Swift,
Rust, HTTP, SQLite, filesystem, and process boundaries. The product has no browser
surface requiring Playwright or cross-browser checks.

Final canonical validation: **PASS**. `./scripts/release-local.sh` completed on
the settled implementation, including format, strict workspace Clippy, workspace
tests and ignored tests, registered native Developer process E2E, package checks,
release-evidence self-tests, the partitioned Swift suites, and Swift build. The
Developer binary suite passed all 350 tests. The new audit rollback, revoked
policy restart, and shutdown-handler tests passed inside that canonical run.
`git diff --check` and the documentation drift check also pass.

Independent complete-diff review: **APPROVE**, with no remaining blocking
findings after the rollback, shutdown, and typed-selector fixes. All implementation
bytes remained unchanged through the settled-source canonical run.

Publication requires hosted macOS/Windows checks on the PR candidate and final
merged SHA. Those immutable results are retained with the GitHub pull request
and workflow runs; a local pass alone does not establish hosted publication.

Deployment and installed UI: the installed owner runner had active feature,
chat, and tool work during this slice. It was not stopped or rebuilt. The source
and fixture evidence does not establish installed Windows source/binary parity,
rendered live UI, real-provider execution, signing, notarization, or distribution
readiness. A guarded rebuild must wait for idle owner work.
