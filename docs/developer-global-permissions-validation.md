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

Publication: [PR #430](https://github.com/malak333/Assemblywright/pull/430)
merged after all three hosted candidate gates passed on
`889643249ec5685810214ccb28ebfe91f1d32ab3`. Published main
`860d1344326e1f14d02475312dfba21f5bb066b7` contains that reviewed commit and has
the same tree. The isolated local checkout was fast-forwarded to published main;
the unrelated dirty checkout was preserved.

Deployment on 2026-10-05: owner work initially prevented rebuilding. After work
finished, `python3 -B scripts/production-build.py --build --no-open` verified
authenticated idle state, rebuilt the release Mac app and Windows Developer
runner, and reconnected to MIKE-PC. This updates the supervised owner-account
Developer runtime, not the separate protected-service installation.

Post-rebuild authenticated status reported `supervised_developer`, revision 911,
all seven activity guards false, Emergency Pause false, and planning/review still
required. Authenticated `GET /permissions` reported available Windows execution,
mode `ask`, revision 4. No permission save or feature execution was performed.
The first global policy intentionally starts in Ask; legacy project Full access
does not authorize global Full access.

All 146 tracked Cargo/toolchain/crate build inputs on Windows matched published
main by SHA-256. The installed Windows runner matched its release build output,
SHA-256 `7cb858ff6db582323171ec1074894b7267bf20cd22a879d6e78eb71e5ef0d41f`.
The Mac app's 36 compiled Mach-O sections and build UUID
`F903CC76-C504-3CE0-9557-CA94B6254B02` matched its release build. Section parity
excludes code-signing metadata, which changes when the bundle is signed. The
rebuilt bundle executable SHA-256 was
`747ce22fd2d046018b349be4c14e76839c7cfe41ddd6b022bb70402d8a1d0147`;
`codesign --verify --strict --deep` passed for the local ad hoc bundle signature.

Rendered live UI remains unverified because the Mac was locked. The rebuilt app
was prepared with `--no-open`. No real-provider execution, Developer feature
acceptance, Developer ID signing, notarization, or distribution readiness is
claimed. The protected-service schema/service lifecycle is outside this slice.
