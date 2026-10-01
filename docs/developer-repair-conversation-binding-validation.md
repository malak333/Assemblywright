# Repair conversation binding validation

## Fix and safety verdict

The repair response omitted the proposal's durable `chat_id`. A discarded
nonlegacy proposal therefore retried its saved request in the legacy conversation
and failed ownership validation. The response now retains that conversation, and
Swift selects conversation ID, request ID, and diagnosis digest together. Partial
selected identities cannot borrow fields from an older proposal. Windows keeps
its existing ownership, provenance, digest, revision, checkpoint, and execution
gates. Independent high-risk review approved the narrow change with no blockers.

Documentation and safety compliance: the accepted chat-repair design records the
complete identity and distinguishes successful preparation from validation and
review. The knowledge base records the failure and operator-facing consequence.
The explicitly requested `unit-testing-test-generate` and `e2e-testing` workflows
were read and applied during publication closeout. The test scope is native Swift,
Rust, HTTP, persistence, and processes; browser automation, visual baselines, and
a cross-browser matrix do not apply to this repair dialog. Native tests cover the
actual product boundaries. Browser validation of the separate FFT site is outside
this fix and remains separate evidence.

## Repository proof

- Swift `DeveloperRepairEscalationTests`: 20 discovered in the isolated
  publication worktree; 19 passed and one environment-gated live test was skipped.
  Coverage includes exact ready approval, partial identity
  rejection, selected-diagnosis precedence, and prevention of mixed identities.
- Rust `escalation_snapshot_projects_exact_manual_conversation_binding`: passed.
- Native `developer-runner-escalation-e2e.py`: passed, including nonlegacy
  diagnosis, cancellation, retained conversation projection, wrong-conversation
  rejection after creating another chat, and exact-conversation preparation.
- Rust format, Python syntax, documentation drift, and diff checks passed.
- Initial primary-checkout `release-local.sh`: failed in the workspace test stage because a reused native
  workflow test artifact had embedded another worktree's fixture path. Its stale
  bytecode fixture expected behavior outside the current working source. Rebuilding
  that harness without changing source bytes and rerunning
  `cargo test -p assemblywright-master --test developer_workflow_e2e -- --nocapture`
  passed all 13 native workflow scripts, including the repair regression and the
  existing scalable-repair suite. The full gate was not rerun, so its subsequent
  ignored-test, packaging, evidence, and complete Swift stages are unverified for
  this slice. The gate's format and workspace Clippy stages passed before failure.

## Workflow coverage and CI

The unit-generation workflow was applied by analyzing the two diagnosis-source
branches, each required identity field, the digest boundary, and the approval
gates. Tests assert exact accepted values and specific rejected states. They do
not mock Windows ownership validation or claim that a non-null result is enough.
The added malformed-digest tests exposed a length-only UI approval check; it now
uses the same lowercase hexadecimal digest validator as preparation.

| Boundary | Scenarios | Evidence |
| --- | --- | --- |
| Swift selected diagnosis | Complete tuple wins over retained proposal; each missing or empty field fails without fallback; uppercase digest rejected | Focused unit suite |
| Swift retained diagnosis | Complete cancelled tuple remains usable; each missing or empty field and invalid digest rejected | Focused unit suite |
| Swift approval | Missing chat/request, digest lengths 0/63/65, nonhexadecimal and uppercase 64-character digests rejected; exact ready binding accepted | Focused unit suite |
| Windows proposal response | Cancelled proposal retains its exact saved chat and request | Rust unit regression |
| Native process/API recovery | Nonlegacy diagnosis prepares; GET/cancel retain chat; newer wrong chat rejected; exact saved chat retries without project writes | Escalation E2E |

The E2E workflow uses disposable projects, SQLite state, HTTP model fixtures,
bounded waits, real native runner processes, and positive/negative assertions.
It covers persistence, cancellation, stale state, wrong-conversation rejection,
and unchanged input bytes. It does not send an owner's project to a fixture or
grant an external model application or publication authority.

CI already executes these tests: `developer_workflow_e2e` invokes the escalation
script in `cargo test --workspace`, the Mac `release-local.yml` workflow invokes
the complete local gate (including Swift), and `windows-protocol.yml` invokes
native Windows developer and workflow tests. No separate browser workflow or
new dependency is required. No instrumented line/branch coverage percentage was
measured; the table states scenario coverage, with native visual UI, real account
publication, signing, and the FFT candidate review kept as separate proof.

## Publication closeout

Publication is isolated from the primary checkout's unrelated changes. The
publication branch contains only the repair identity fix, its tests, and relevant
documentation/knowledge-base updates. The full canonical `release-local.sh` gate
passed from this worktree with fresh attributable build artifacts, including the
complete Rust workspace, ignored tests, native workflow E2E, packaging/evidence
checks, partitioned Swift suites, and Swift build. Hosted checks and merge are
separate publication gates and must pass before merging this candidate.

## Installed proof and limits

The installed Windows source differs from the working checkout. Installation
therefore added only the missing response field to the existing Windows source
after verifying its hash and backing up source and executable. The runner rebuilt
successfully, restarted with its existing configuration, and returned the saved
nonlegacy conversation ID for the cancelled live proposal. Its feature record was
unchanged across restart.

The Mac update was built from an isolated archive of the clean installed-source
checkout with only the repair-view and test changes. Release build and focused
tests passed. The existing app bundle was backed up, its executable replaced, its
local ad-hoc signature verified, and the app relaunched. This is local installation
evidence; it does not establish distribution signing, notarization, or full visual
QA. Native UI verification was unavailable after the computer-use pipe failed.

Live preparation with the newest saved diagnosis reached the correct conversation
and terminalized as unavailable with `Repair proposal contains no file changes`.
That diagnosis had already edited the content. The original validation command
passed all 32 tests in a disposable Windows project copy. The live project inventory
remained identical across preparation: 55 files, excluding environment and cache
directories, SHA-256
`ed3d460f0288b53c5d73411c9623ee6880c344b60e26be8e3f02630b3633cf2e`.
No candidate was approved or applied, and the live feature was not independently
reviewed or released from quarantine. Publishing this repair-flow fix does not
authorize applying the FFT candidate or clearing its quarantine.

## Final candidate installation

The publication candidate was built as optimized Windows and Mac release products
from the isolated PR branch. Both builds passed. The supervised Developer runner
was idle before shutdown; its existing configuration and durable state were
retained. After the connection supervisor's initial ready wait timed out, it
reconnected and authenticated successfully. Status reported `running: false`,
`emergency_paused: false`, and revision 678. The live FFT feature remained failed
at `tool_workspace_changed_requires_proposal`; its saved latest conversation and
request were returned with the same no-file-changes proposal outcome.

The installed Windows runner SHA-256 is
`33c0f32ef2bb0fcbf4a998c0f1d6685b80ef56dad6b278c876ab813d95501928`.
All 145 Rust build-input files in its runtime source were verified against the
publication candidate; the sorted path/content-hash inventory SHA-256 is
`dda2e0bd4092ceb3a39e2cd199696ae5577a253bb9b0b957562378710b4d3fff`.
The Mac app was backed up, updated, ad-hoc signed, verified with
`codesign --verify --deep --strict`, and relaunched. Its executable SHA-256 is
`70077e2d6997b3e1cfdbff6fb285e4ca73bf2ca4e6c181d5ee858b344cc25b4a`.

This docs-only installation closeout leaves both binaries unchanged. The
supervised Developer process owns this app workflow; no protected master-service
runtime input or protocol/schema changed, so no protected service restart or
migration was applicable. Before declaring publication complete, the exact final
PR head must pass all hosted gates and be merged normally, and the authoritative
Windows Git checkout must fast-forward to published main while preserving its
untracked evidence. Distribution signing, notarization, and native visual QA
remain separate unexecuted boundaries.
