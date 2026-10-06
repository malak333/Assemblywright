# Developer generated-output deletion recovery

## Failure and scope

The prior permission/empty-project fixes were published in
[PR #432](https://github.com/malak333/Assemblywright/pull/432), merged on
October 6, 2026 after all three hosted checks passed on its final head.
The FFT feature subsequently reached a different failure. Escalation 5 removed
18 obsolete generated HTML pages in a disposable stage, but the mutation ledger
could not admit deletion into a write/image-only candidate. The retained feature
remained failed at `escalation_5_unavailable`, with three ordinary repair attempts,
five escalations, and two independent reviews. Global Full access remained
revision 5; no tool approval was pending.

The owner's renewed work window starts at 10:53:24 UTC on October 6 and ends at
13:53:24 UTC (9:53:24 AM Eastern). This phase adds explicit supported staged text
deletion across capture, frozen candidate, review, recovery, and publication.
It retains the original validation command, independent review, exact access and
execution bindings, and all prior history. The unsupported escalation 5 cannot
be retrospectively admitted. A new owner-bound Resume must prepare fresh evidence.
Application-source publication and actual FFT feature acceptance remain separate.

## Contract

The accepted contract is documented in
[the scalable repair design](developer-scalable-repair-design.md#explicit-staged-text-deletion)
and [the safety rules](safety-rules.md). A deletion includes complete admitted
UTF-8 prior text, its exact hash, explicit operation identity, and expected
absence after application. Linked, binary, secret-bearing, unbound, or
unsupported deletions remain holds. Queue format version 13 prevents an older
runner from defaulting a deletion to an empty text write. Prior non-deletion
bindings retain their original digest format.

## Native defects found during validation

The disposable-stage copy excluded `dist`, so an existing generated page was
absent before the model attempted deletion. Stage population now copies admitted
text and PNG/JPEG files from `dist` under the existing bounds; sensitive files,
opaque binaries, and linked paths remain excluded. Native testing also caught
a review-binding mismatch after a file changed from original publication bytes
A to current bytes B before deletion. Review now binds B and the retained
before-text, while publication independently binds A. Auto access retains
write/image-only staged recovery; explicit deletion requires Full access.
A later native restart found that the compact terminal-manifest validator still
recognized only text and image records. The validator now admits a typed deletion with its
required prior digest and absent asset metadata. Compact-and-reload regression
coverage rejects missing prior hashes, disguised assets, and unknown kinds.

## Sequential escalation and review counters

The live escalation-7 run exposed a separate review-admission defect: terminal
escalation `not_run` evidence uses the escalation number, while actual independent
reviews use the review counter. Comparing those mixed namespaces made historical
escalation slots 3, 4, and 6 appear to conflict with real review attempt 3. The
provider was never called. The correction excludes only synthetic no-provider
markers from that counter comparison while retaining exact pending identity,
review-history hash, packet reconstruction, candidate authority, and project
snapshot checks. It does not broaden effect-free recovery eligibility. A regression
reproduces the synthetic slots 3/4/6 versus real review 3 and rejects a competing
binding-v2 provider review. The settled counter fix passed 382 Mac Developer
tests, formatting, Clippy, build, and independent high-risk review.

The first exact-snapshot recovery then exposed an epoch-floor assumption:
operational quarantine preserved the proposal epoch, but the validator required
two increments as if Stop had already advanced it. Recovery now requires a
strictly newer epoch (`proposal + 1`); Stop-origin `+2` remains valid. Same-epoch
recovery, incorrect policy/identity, and malformed application lineage still
reject. Exact owner adoption continues to enforce its snapshot digest and
history-derived upper epoch bound.

The final guarded build reconnected Windows with all 146 source inputs matching
manifest SHA-256
`dec487b29284221e605d32a3d7a44bcfdee5ea28081ca0dd8010ed4c9b175073`.
Installed binary SHA-256:
`1b741ff0ef7cc265998839c9a7f352b0051cbb7fccac23db15d78dcbd4a52b86`.
A bound validation/review-only Resume advanced the retained candidate to epoch 5
without implementation or application replay. At 12:51 UTC the unchanged
validation command passed and the live feature reached `review_4_pending` with
independent review running. The final Mac native rerun passed with 30 application progress events and
164.447 seconds. The final Windows suite passed 383 tests plus its separate
platform helper check, and the Windows native recovery fixture passed with
23 progress events and 136.219 seconds. The final complete repository gate passed; the bounded live outcome is recorded
below as review rejection followed by automatic escalation 8.

## Provider schema boundary

A real Codex CLI 0.160.1 probe reproduced HTTP 400 `invalid_json_schema` when a
new property was omitted from an object's `required` array. The provider rejected
it before returning a review response. The correction must use strict separate
legacy-write and deletion variants, preserving old receipt serialization while
binding both deletion baselines. This follows
[OpenAI's structured-output contract](https://developers.openai.com/api/docs/guides/structured-outputs).
Fixture decoding alone cannot prove provider admission.

## Evidence and closeout

Independent high-risk review approved the complete source and each subsequent
native defect fix. The settled `developer_main.rs` SHA-256 is
`3006882fb1e41cf68cf1be416c05aaacefe9560ee529e1ff37fed70b597d0feb`.
All 382 Mac Developer unit tests, formatting, Clippy with warnings denied, the binary
build, and the focused native scalable-repair harness passed. The native harness
proved 50-path application with generated-page deletion, existing-test correction,
partial restart without replay, independent review, compact-and-reload recovery,
provider-unavailable resume, and drift quarantine.

The first installation, before the counter and epoch fixes, built the runner
natively on Windows and signed the Mac
Developer bundle ad hoc, and reconnected MIKE-PC. All 146 transferred runtime
source inputs matched the reviewed checkout, manifest SHA-256
`4debf9ed858551a89b71d102779f22e65be5e8cadbd244dbaf6020e791f25380`.
That first Windows binary SHA-256:
`b331cb116f2fba3bfc35a2fe102bfcabe6f3f37f7a0a8f3936d167b1212a35cd`.
Queue migration from version 12 to 13 retained an exact revision-986 backup and
left the full FFT record and repair/escalation/review histories unchanged.
Full access stayed at revision 5. A bound Resume at 12:15 UTC reserved fresh
escalation 6, epoch 3, proposal `fa30dcaa-f4dd-4611-9187-6bcc8367d5c8`; it did
not adopt the unsupported escalation 5.

The final full `./scripts/release-local.sh` repository gate passed at 13:02 UTC
on the settled source above. An earlier pre-counter/epoch-fix gate also passed
at 12:23 UTC.
It includes Rust, Swift, native Developer process workflows, connection controls,
packaging provenance, documentation contracts, and release-evidence checks.
Earlier gate runs were cancelled after native defects were discovered;
those runs are not counted as passes. The settled final gate is retained privately
as `aw-delete-release-local-final-settled.log`. Mac ad hoc bundle verification passed.
The earlier Windows Developer suite passed (382 tests, one platform-special test
executed separately). The native Windows scalable-repair harness also passed,
including explicit generated-page deletion, existing-test correction, partial
restart, owner-bound adoption without replay, independent batched review,
provider-unavailable recovery, terminal compaction, and drift rejection. Its
application emitted 35 progress events and completed in 156 seconds. Two earlier
Windows harness launches stopped at missing fixture prerequisites; the settled
run built the native reviewer helper and exercised the complete fixture.
At 12:32 UTC the live runner had prepared, policy-authorized, and applied
escalation 6, candidate SHA-256
`33b644b77e76e505e83d050a3667c8342fd2c55b28c173dbaa802432334612ea`.
Its cumulative record contained 80 typed entries, including 16 deletions;
all 16 deleted paths were absent in the live project.
The original validation command ran and failed with exit code 1. The runner
classified this as a recoverable validation failure and automatically reserved
escalation 7 without human intervention. This proves live preparation, deletion
admission, authorization, application, validation, and automatic progression;
it does not prove final website acceptance.

At the final live checkpoint, independent review 4 completed its real bounded
batch and aggregate provider calls and rejected the exact candidate with 15
blocking findings, chiefly inaccurate/fabricated PS1 reference content. The
runner automatically reserved escalation 8 and began another staged correction.
The retained feature has three ordinary repairs, eight escalations, and four
actual review attempts. It remains unfinished; no feature success or approval
was manufactured. Final recorded state at 2026-10-06T12:59:16.618358+00:00:
`failed / escalation_8_preparing`, lifecycle
`running`, full-record SHA-256
`2e3d68c591788fd46c7b8b40780caad4cd66619f627de016c0dbb955c8b13b76`.

This bounded closeout freezes live investigation so the source can complete
its hosted checks and normal PR merge within the owner's three-hour window.
The autonomous runner remains active on escalation 8. GitHub's exact-head check
receipts and normal PR merge record provide publication proof; this document
does not assert hosted success before those receipts exist.

At 11:13:40 UTC, the unchanged full FFT record SHA-256 was
`55694d4cdf139667508471c5f4ad96345e42cb54ddcfd8d21fbafcde03041e2a`.
Its history receipts are retained privately under
`target/pipeline-recovery-proof/aw-delete-baseline-receipt.json`; no credentials or
raw project transcript belong in this document.
