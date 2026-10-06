# Developer pipeline permission and empty-project recovery

## Observed failure

On October 5, 2026, `aw-fft-demo2` failed at `tool_effects_quarantined`.
The retained OpenCode session read the empty project, then requested permission
for `python --version 2>&1 || echo "Python not found"`. The Developer action
record retained the OpenCode permission identifier at access revision 1. The
session expired with the permission unanswered; this was not evidence that the
Python process ran or hung. Global Full access revision 5 was saved afterward.
Changing access does not answer an already issued permission request.

The latest project-chat diagnosis correctly reported an empty project. Preparing
a manual proposal reproduced `Repair context request names a non-text or
unadmitted path`. No proposal bytes were applied by that failed preparation.

Authenticated status separately confirmed `aw-metalgear-demo` succeeded at
`publication_merged`. That result establishes an existing completed pipeline
instance, not acceptance of the failed FFT feature or this source change.

## Correction and proof boundaries

Permission-wait failures must identify the unanswered permission without
auto-approving, replaying commands, or changing existing tool access. Invalid
model context requests may receive one bounded correction against the same
inventory. An empty inventory requires proposing new files instead of retrieving
absent files. Unadmitted paths are never read. Application remains a separately
bound operation followed by the original validation and independent review.

The owner requested working unattended Developer execution and authorized
recovery. Full access remains the explicit persisted Developer setting. The
protected production runtime and GitHub publication contracts are separate.
Existing unrelated checkout changes and feature evidence must be preserved.

## Source validation and installation

The exact installed source was baseline `25cc47f`; the primary checkout was older.
The narrow fix lives in the isolated `pipeline-recovery/Assemblywright` checkout.
The primary checkout's pre-task source bytes were restored without discarding
its unrelated dirty changes.

All 356 Developer binary tests passed. Focused tests also covered empty-inventory
correction, a second retrieval rejection, manifest drift, omitted admitted text,
exact permission bindings, ordinary timeout fallback, cancellation, global Full
access, and effect-free staged Resume. Both native macOS runner E2E scripts passed:
`developer-runner-escalation-e2e.py` and `developer-runner-auto-repair-e2e.py`.
Independent complete-diff high-risk review approved after resolving a staged
Resume diagnostic-compatibility regression and two production dead-code warnings.
Developer Clippy with warnings denied, formatting, and diff checks passed.

The guarded `developer-build.py --build --no-open` rebuild installed the Windows
runner and reconnected on October 5, 2026. Installed source hashes:

- `developer_main.rs`: `8b421430c344aa0f832aa5a1660739563f3a857b0d004d9adcc433e350fdce45`
- `developer_tools.rs`: `40664c115ce3eccfc467c8c1379917d6f1960e306b888bb23f733514a9fbef5e`
- Windows executable: `c2c475ca58ef57dbf5812a0b4e56f6420fae0727d5fa37c91accb82afcbb0955`

The complete FFT feature record hash remained
`0e7c37758731f622253fa01aa94a9e6b9fcbf8ecb0216cb235e6b6f655abf666`
across installation, and its API projection was identical. Global access remained
`full`, revision 5. The prior database and executable were backed up before
installation. The Mac bundle was rebuilt with local ad hoc signing; this is not
Developer ID signing or notarization. The existing app continued to use the
authenticated runner; no new app window was opened.

The settled full `release-local.sh` rerun passed, including the Rust workspace,
native process workflows, packaging checks, and partitioned Swift suites. The
first gate invocation caught the production dead-code warnings described above;
the complete rerun used the corrected source. Local validation receipts are saved
under `target/pipeline-recovery-proof/` in the isolated checkout.

## Live feature recovery

The fixed runner prepared a 16-file proposal for the empty project at escalation
4. The parent inspected every proposed file and applied it through the exact
bound manual repair endpoint under the owner's recovery request. The original
validation ran 15 tests; two correctly failed on missing spoiler warnings. The
parent's bound Resume reactivated the existing Auto AI policy with all counters
preserved. The runner prepared the first ordinary repair in a disposable copy,
applied its captured candidate, passed immutable validation, and entered actual
Codex review. Review rejected eight content/rendering/coverage findings; the
runner automatically reserved and began ordinary repair 2 without another
permission prompt or parent model/file write.

Ordinary repair 2 applied a candidate but immutable validation failed with 15
errors, including an undefined Python variable and invalid job data. Ordinary
repair 3 passed all 15 tests; it reintroduced a fabricated Chapter 5 to satisfy
one of the existing tests. Exact independent review rejected that candidate with
16 factual, link, schema, discovery, and coverage findings. The runner then
automatically reserved escalation 5, which can propose changes to the incorrect
existing test rather than forcing ordinary repair to preserve it. Neither a
passing test command nor active automatic repair establishes feature acceptance.

Live FFT acceptance remains unproven. At 03:06:53 UTC on October 6, the owner
requested a one-hour investigation bound, a durable summary, and GitHub push and
merge of the reviewed application fixes. The deadline is 04:06:53 UTC. The FFT
project remains local-only; application-source publication does not establish
FFT completion.

At the bounded investigation snapshot, **03:31:55 UTC on October 6**, revision
984 retained feature `835329ad-4bc2-4dc8-80e0-3f7df4993d54` at
`escalation_5_preparing`, with 3 ordinary repair attempts, 5 escalations, the
latest review rejected with 16 findings, and Auto AI lifecycle `running` at
epoch 2. Global Full access remained revision 5; there was no pending tool
approval. The exact durable feature-record SHA-256 at that instant was
`f132af3b1e78e6bd9184a1ac635713541582d54cb3addfa99ce84b86e9fd4eb5`.
This is a timestamped observation, not a terminal state or Resume authorization.
The parent ended investigation to reserve time for publication; the autonomous
runner was left active without a counter reset, fabricated approval, project-file
edit, or Stop request.

The installed Windows Developer source directory is a transferred tree, not a
Git checkout. All 146 tracked runtime inputs (`Cargo.toml`, `Cargo.lock`,
`rust-toolchain.toml`, and `crates/`) matched the reviewed source byte for byte.
The canonical input hash-manifest digest was
`9a3eab829799b8fd2d90ca3261408896b63cd3c6e2737dfad2fb5e97bd36aac5`.
Subsequent closeout edits are documentation only, so they require no binary
replacement or interruption of the active feature. Protected-service executable
inputs were unchanged; protected SCM deployment is outside this Developer slice.

Application publication is [PR #432](https://github.com/malak333/Assemblywright/pull/432).
Its final documentation commit must pass all three hosted gates on its own SHA
before the normal merge path. GitHub retains exact-head check and merge receipts;
local source validation and installed source parity remain distinct proof layers.

## Closeout verdicts

- Documentation and safety: the bounded correction and exact permission diagnostic
  preserve Windows authority, planning/action separation, cancellation, evidence,
  and current access policy. Design and operator contracts were updated.
- Durable knowledge: permission-wait timing, access revisions, and empty-project
  retrieval behavior were added to the project knowledge base.
- Unit coverage: focused behavior tests and all 356 Developer binary tests passed;
  the unit-testing workflow was applied to success, rejection, binding, drift,
  cancellation, and staged recovery. No coverage percentage was measured.
- E2E coverage: native Rust/Swift/process workflows and installed Windows/API
  boundaries were exercised. The e2e-testing workflow selected native tests; a
  browser matrix is inapplicable to this runner change. Native computer-use failed
  with a closed pipe, so visual UI acceptance is not claimed.
- Canonical validation: the complete corrected local release gate passed.
- Deployment: Windows runner rebuilt and reconnected with exact source hashes and
  preserved feature state; Mac bundle rebuilt with ad hoc signing. Developer ID
  signing, notarization, and protected-service release readiness are unverified.
- Publication: requested through PR #432, with exact-head hosted gates required
  before merge. The final publication result is recorded by GitHub and the
  parent closeout. Live feature acceptance remains unproven at the timestamped
  snapshot and is separate from application-source publication.


## Subsequent receipt and publication

All three required hosted gates passed on final head
`9b788a4d69e4671a22d2727c56c06b082f252145`. PR #432 was merged normally on
October 6 at 10:53:44 UTC; merge commit
`de42e35a869bff6860547820e91a1fc206609e21` contains that reviewed head.
The next authenticated observation at 11:13:40 UTC retained FFT at
`escalation_5_unavailable`, with all prior counters and history. The full private
feature-record SHA-256 was
`55694d4cdf139667508471c5f4ad96345e42cb54ddcfd8d21fbafcde03041e2a`.
The failed staged proposal had removed obsolete HTML pages that the write/image
candidate contract could not admit. That subsequent boundary is addressed in
[the generated-output recovery phase](developer-generated-output-recovery-validation.md).
The published permission fix does not establish FFT acceptance.
