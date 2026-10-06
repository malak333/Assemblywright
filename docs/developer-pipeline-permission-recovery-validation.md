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

Live FFT acceptance remains in progress. At 03:06:53 UTC on October 6, the owner
requested a one-hour investigation bound, a durable summary, and GitHub push and
merge of the reviewed application fixes. The deadline is 04:06:53 UTC. The FFT
project remains local-only; application-source publication does not establish
FFT completion.

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
- Publication: requested; hosted checks and merge evidence will be recorded at
  closeout. Live feature acceptance remains a separate outcome.
