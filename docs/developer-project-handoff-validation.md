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

## Completed live feature

The retained demo6 job reached `succeeded` / `review_7_approved` at runner
revision 2565. Its original Windows command passed 34 tests. The application
performed six staged preparation attempts and seven reviews, with zero ordinary
repair attempts. Five staged attempts supplied applied corrections; the fifth
preparation completed with an empty mutation ledger and no generated bytes.

That known empty result exposed a second continuation defect: it was recorded
as unavailable rather than using the existing bounded `no_op` retry path. After
checking that the candidate bytes, stage ledger, and retained edits matched, the
parent issued one public Resume. Counters were preserved. The application then
prepared staged attempt six, corrected the remaining reference and focused
skip-link color, revalidated, obtained real approval, and completed. No parent
edits were made to generated files. This proves retained real-model completion;
the original fresh start required maintenance and one Resume before the final
empty-result correction.

Read-only export verified all five delivered files against retained reviewed
edit contents and confirmed unchanged durable state before and after export:

| Path | SHA-256 |
| --- | --- |
| `docs/design.md` | `ca3b9b7010622d528c41cb11468332dd54da90063eb7357ee9d664c0031cc411` |
| `index.html` | `fe92f2a9857277c29e8d37ce3df737ec2f99e691c9dcdd3893e92dbec55cb0c3` |
| `styles.css` | `3eef352c786d0ba80383b3e9a9304db937736ea01df97a1ec625c0e76994e817` |
| `tests/__init__.py` | `53f0bfbd275333adb8217b5f191d0b1638ebe290af13beed5536f8dc642ab6e9` |
| `tests/test_site.py` | `6ef7a4eaaa64f2a665fb3fbc9381ad0f4fe58f62266fe1b5d315111d90266390` |

The final validation-log SHA-256 is
`7a26714add6afefa60569c25674771c7dba42bca2a04d8a0bf9f291a86c0ffcb`.
Review seven bound packet
`40df88cdb82293821f85f478e0e7ba020e08ac4d9470f886cb2304c245dfa902`,
validation evidence
`fda9e5fd313f01a7caf1e807cbf9a1a6a19564dbe82ea3de94eb0d0d20a2cadd`,
and approved decision
`2f9215934521607942e2e640bb23940b6da1ebf024209f6de11b0a7b0daf6c72`.
The ordered batch packet/receipt hashes are respectively
`18961c70f4f53d443318e97d4869ca27d96fb919ce8647b5a1c1cfae7390bc48`
and `f2006c0cc4f8d9c8183444cc3f8eb17cf10a23afc3bf80fde15edc7912fb9a51`.

The reviewer retained two nonblocking limitations: the bounded CSS resolver does
not fully implement specificity, source order, or comma-separated selectors and
can skip unsupported syntax; the focus-outline assertion checks a color token
without calculating contrast, width, or style. The delivered palette resolves
correctly and actual outlines are visible 3px solid burgundy. These observations
remain part of the approval evidence. They do not establish exhaustive browser
or accessibility acceptance.

## Clean empty-stage continuation

A successfully completed, exactly bound stage with no raw mutations and zero
mutation/text/asset counts uses the existing terminal `no_op` outcome. It consumes
one capped attempt, retains the same reviewed candidate, has no applied effects,
and prepares a new attempt under current policy and lifecycle authority. Provider
failure, unknown completion, malformed bindings, nonempty mutations that reduce
to no edits, failed cleanup or compaction, tool attention, drift, and cancellation
keep their held recovery behavior. A terminal effect-free `not_run` receipt is
excluded from successor
reservation only after its exact no-op provenance is verified; actual review
receipts retain their uniqueness checks.

The native `--empty-staged-retry-only` mode covers this separate completion and
retry boundary. All four staged modes are included in the canonical Cargo
Developer workflow E2E. The regression does not substitute fixture inference for
the real-model demo6 result above.

The focused zero-ledger, provenance-drift, and both-route cleaned-state recovery
tests passed. The cleaned-state test reconstructs a durable pre-compaction row in
SQLite; it does not kill and restart the runner process. The native empty
stage mode passed with zero ordinary repairs, two staged attempts, two
fixture-backed review decisions, and no Resume. Its effect-free `not_run` receipt
remains audit history and does not count as a review call.

The full gate caught a compatibility regression in the older non-staged JSON
no-op retry. Staged verification now opts in only when a staged binding exists;
the early protected route still cannot proceed without that binding. The full
Developer binary unit suite passed all 440 tests after the correction, and the
native empty-stage mode passed again on that exact source.

The final correction was built and installed through the canonical production
launcher. Windows executable SHA-256 is
`1e1878451edb5a15713614c3d333073d5ec2dca3c4d97f3a67b6729c7c3db8ed`;
`developer_main.rs` is
`c4674fc8ef405304c8ffee57840328d22665d7d3ecf5ee3d6f61724c9a3b12ea`.
The installed Mac executable retains the recorded `8ae71c8b...` digest above.
SQLite integrity was `ok`, all 27 queue entries matched the pre-install backup
(the previous 26 plus the newly queued fresh proof),
real local model health was `ok`, and demo6 retained `succeeded` /
`review_7_approved` after reconnect. The Windows directory is a nongit runtime
source archive, so these digests establish its source/binary identity; it has no
Git HEAD claim.

## Fresh uninterrupted real-model completion

After the final `c467...` installation, a separate bounded feature
`18087476-4737-418e-8e4d-a44396c8a0ca` in
`aw-autonomous-completion-20261009` implemented a usable standard-library Python
temperature conversion library. Real Codex planning produced the approved plan
`78fea7d3a451fb621b5a6ca4cdcadb00d17480d6f1f8015e68dc11b08d9ae28a`.
From the approved queued state, the parent issued one public Start. Windows Qwen
implemented the files; the original `python -B -m unittest discover -s tests -v`
passed 45 tests; real Codex review approved the exact candidate; the application
reached `succeeded` / `review_1_approved`. It used zero ordinary repairs and zero
escalations. There was no Resume, service restart, or parent file edit after Start.

Read-only export verified every delivered byte against retained reviewed edits,
and the feature state was unchanged during export. All other 26 queue entries
matched the pre-install backup. SQLite integrity, installed source/binary hashes,
and real local-model health remained valid.

| Path | SHA-256 |
| --- | --- |
| `README.md` | `d71eee75965def64ce0c1a41ef0e98729de0ad5613a30508ea52b71c70230b5e` |
| `temperatures.py` | `8f46bc67ad59592b75f4cac56b1bed8acae6e2d2eeee3d313611ea7ce9340ff7` |
| `tests/test_temperatures.py` | `8fb25f75ed41e3b18ac9ae637181a82849e9559577980e12078817671012b069` |

Validation-log SHA-256:
`aa79e87ac84f6b55006724cee881c016b6acffd9a9962ede44b2a1032cf142c7`.
The approved review packet and decision are
`014261f5e55bff72665aede958a24b0f43292f4a0254b63804bb8271d11cd23b`
and `7290c862843942429363499c262a618fa52f57ac8647b5ccccade5bbff71e4b6`.
Two nonblocking observations remain: the extreme Fahrenheit test uses `1e15`
rather than a value near the float limit, and a README fractional example shows
`25.5` rather than the approximate unrounded representation. The delivered
implementation handles large finite Fahrenheit values correctly; these selected
test/documentation limitations remain recorded. This proof is local-only under
its approved selection and separate from GitHub source publication.

## Canonical validation and publication boundary

The canonical `./scripts/release-local.sh` passed at 04:02:04 UTC on October 9
for the project-handoff and repeated protected-review correction. Its Developer
workflow E2E passed in 702.39 seconds with the then-current three staged modes;
the general Swift partition passed 154 tests. This earlier gate does not cover
the later clean-empty correction. Final-head canonical validation, independent
review, installation, and hosted results belong to the final publication record
on [PR #442](https://github.com/malak333/Assemblywright/pull/442).

Documentation and conversation-derived repository knowledge describe each
boundary. Relevant unit coverage includes identity, acknowledgement ordering,
casing, cancellation, current policy, malformed evidence, and effect-free retry.
Native E2E exercises HTTP/process/SQLite/OpenCode boundaries. Visual macOS UI
automation remains unavailable; Developer ID and notarization remain outside
this local build. The earlier retained demo5 completion is separately recorded
in [the review recovery record](developer-review-transport-validation.md).

Assemblywright source publication is owner requested. All triggered hosted
checks must pass on the final PR head before normal merge. This generated site
remains local-only under its original frozen selection; no website publication
connection was added during this proof.
