# GitHub project handoff and autonomous continuation

The October 8 owner report showed `aw-fft-demo6` in GitHub discovery but absent
from the local project picker. The two catalogs have different authorities:
GitHub lists repositories; Windows lists direct project directories. The old
selection merely populated a draft, leaving a separate local creation action.

Using a repository now requests the existing authenticated, idle, revision-bound
local creation operation, preserves the connection draft, and selects the
authoritatively acknowledged project. Exact successful newly requested remote
creation uses the same handoff. Unknown or unrelated creation receipts do not
create projects. A local failure retains a local-only retry. Publication
connection saving remains a separate explicit action.

The native client accepts the canonical Windows spelling, rejects ambiguous
case matches, and cannot replace a newer Stop/Emergency/polling snapshot with
a delayed creation acknowledgement. Saving requires current catalog membership.
Cached creation results distinguish ready projects from selected projects.

## Verification recorded before publication

- Independent high-risk review of the handoff approved after resolving delayed
  acknowledgement ordering, nonexistent-project saving, and casing findings.
- Focused Swift suites: 37 tests passed; one optional live test was skipped.
  The tests include a threaded delayed HTTP acknowledgement while a newer
  Emergency Pause snapshot arrives.
- Local project Rust unit tests: three passed. Review transport tests: 31 passed.
- Native GitHub setup HTTP/process E2E passed, including authenticated catalog
  agreement, idempotent registration, creation uncertainty/restart, and path
  rejection. Assemblywright has a native Swift/Rust surface, so browser matrices
  are inapplicable to the project-picker boundary.
- The full canonical `./scripts/release-local.sh` passed at 03:04:45 UTC on
  October 9 before the subsequent continuation correction. A final gate for
  that correction must be recorded separately.

## Live installation and retained feature

Authenticated `create_project` registered `aw-fft-demo6` at runner revision 2464.
It appeared in both status and chat project catalogs; retained queue items were
unchanged. No publication connection was saved. The fresh optimized Mac app was
installed in the production checkout, ad-hoc signed, verified, and launched. Its
executable SHA-256 is
`8ae71c8baac1af78fbfbadc0559e2d2e406be8f87f24bfc9d91ea116685a9d61`.
Two older GUI processes were gracefully closed so the active app uses this build.
Native UI automation could not establish its pipe, so no visual picker acceptance
is claimed. This local build has no Developer ID/notarization claim.

A fresh real-model feature, `777947df-8a68-4a77-a113-35a537ad1d0b`, planned a
small original PlayStation Final Fantasy Tactics fan site. Its frozen command is
`python -B -m unittest discover -s tests -v`. Windows Qwen implemented it; real
`gpt-6.1-sol`/medium review rejected deficient content/tests, and the application
automatically applied staged repair one and passed validation again. Review two
then rejected remaining protected-test and content defects.

That rejection exposed a scheduler defect: the staged-attempt error branch
cleared its in-memory repair authorization while durable lifecycle still said
`running`. It consequently stopped instead of preparing the next authorized
staged repair. Preserve this failure evidence; the initial fresh run was not
uninterrupted completion. The correction and publication snapshot are recorded below.

Before runner maintenance, an owner-private SQLite backup passed integrity
checking and retained all 26 queue entries. The previous executable was backed up.
No generated website bytes, counters, or immutable validation commands were
changed by the parent. This feature remains local-only; source publication for
Assemblywright and website publication are separate outcomes.


## Continuation correction and installation

The terminal staged review route now distinguishes ordinary source feedback
from protected findings. Complete terminal application/candidate/review/validation
checks select the route. Only current enabled policy, running durable lifecycle,
and an uncancelled attempt retain authorization to prepare the next staged
proposal. Source-only findings preserve ordinary successor repair. No terminal
application is replayed and each repair gets fresh validation and review.

- Independent complete-diff high-risk source review: APPROVED, no P0-P3 findings.
- The authority unit rejects disabled policy, held lifecycle, cancellation, and
  absent route. Existing malformed/changed-byte terminal tests passed.
- Native HTTP/process/SQLite/OpenCode regression completed initial generation,
  staged repair one, protected review rejection, staged repair two, and fresh
  approval: zero ordinary repairs, two staged attempts, three reviews, no Resume.
  Both existing staged modes passed; all three modes run in canonical Cargo E2E.
- The canonical production launcher gracefully shut down the idle runner,
  rebuilt optimized Windows and Swift artifacts, verified the local signature,
  and reconnected the existing supervisor. The new runner automatically resumed
  the retained feature at `escalation_2_preparing`, without a parent Resume.
- Installed runner SHA-256:
  `ab4773241a19f8bddf1ce4035dfa2fbc3c6420b4970a90c366aef9ce3ee49816`.
- Installed `developer_main.rs` SHA-256:
  `3d89aba2ca2e09e35ff53946d30e2af7ba3c736232568bd9356419809a380584`.
- Installed `developer_review.rs` SHA-256:
  `882f658b9b3a17aca42b5d6998f7a170cae8933664957bcdb0189593fd81dd69`.
- All other 25 retained queue entries matched the pre-maintenance backup exactly;
  the queue still contains 26 entries. SQLite integrity remained `ok`, and the
  real local model health endpoint returned `ok`.

## Canonical verification and publication snapshot

The final `./scripts/release-local.sh` passed (exit 0) at 04:02:04 UTC on
October 9. Its real Developer workflow E2E passed in 702.39 seconds, including
all three staged-repair modes. Format, workspace Clippy/tests/build, native
recovery/relay, packaging/evidence checks, and partitioned Swift tests/build
passed. The final general Swift partition passed all 154 tests. The dedicated
37-test handoff suites and independent complete-diff review also passed.

Documentation/safety compliance: PASS. Conversation-derived repository knowledge:
ADDED. Unit workflow: PASS for relevant identity, acknowledgement, ordering,
casing, cancellation, policy, and malformed-evidence cases. E2E workflow: PASS
at native HTTP/process/SQLite/OpenCode boundaries. Installed runtime: PASS for
the recorded builds, authenticated reconnect, retained queue, database, and
model health. Visual macOS UI automation remains unavailable; Developer ID and
notarization remain outside this local build.

At the source-publication snapshot, live demo6 had reached
`escalation_5_preparing` after real review five rejected remaining generated-test
coverage. Since the corrected runner restart, repeated protected rejections
continued automatically without Resume or parent website edits. This snapshot
proves the corrected continuation; it does not yet establish terminal site
approval. The earlier retained demo5 completion remains separately recorded
in [the review recovery record](developer-review-transport-validation.md).

Source publication is owner requested. All triggered hosted checks must pass
on the final PR head before normal merge. Website publication remains local-only
under this feature's original frozen selection. Final live/hosted evidence will
be recorded after it is observed.
