# Developer pipeline completion recovery

The owner requested a working autonomous end-to-end Developer pipeline on
2026-10-06, with a three-hour hard limit including summary and GitHub closeout.
This run began at 2026-10-07 03:19:30 UTC; its deadline is 06:19:30 UTC
(02:19:30 EDT). Work uses an isolated checkout based on published main
`0bf80d62d7908c7b8188d1e0d21ccc4c26a2402c`; the unrelated dirty development
checkout is preserved.

## Observed failures

The installed `aw-pipeline-proof` marker feature
`f11b2d0d-9d32-4544-b64b-8d3554dc0f82` reported `failed /
review_2_approved` while its publication record reported `succeeded / complete`.
Its exact reviewed commit `0b59160344133e56b511332220014a89c4ec14e5` had already
merged through [proof PR #2](https://github.com/malak333/aw-pipeline-proof/pull/2)
at `c1a935b2237a0953ff2994c7b70fbd39966ce7db`. A repeated reviewed-completion
pass rejected the existing frozen publication as already prepared, then marked
the feature failed. The normal reconciliation control accepted only publication
attention records, so the completed receipt could not use that control.

The owner's failed manual repair preparation also appended a provider-free
`not_run` review entry after the real approved review. A first installed recovery
candidate exposed that chronology: ordinary completion required the last history
entry to be approved and stopped at `review_binding_changed`. The actual approval,
frozen candidate and complete remote receipt were retained. Recovery must bind
any skipped marker to its terminal unapplied proposal and exact authorization/
application-not-run lineage; it must reject a later substantive review decision.
This first installed attempt is a held run, not end-to-end completion evidence.

The selected `gpt-6.1-sol` planning model also failed through the installed
Windows Codex CLI 0.153.4 with an account/model support rejection. A separate
tool-free `gpt-5.6-sol` probe succeeded. The owner-authorized settings route
restored planning and review to `gpt-5.6-sol` with medium reasoning, preserving
the saved selection history. No candidate approval was fabricated, and no
provider fallback was added to the product.

The first fresh live artifact was implemented and validated on Windows, approved
by Codex, and published as [proof PR #6](https://github.com/malak333/aw-pipeline-proof/pull/6)
at `69d12489cd9813d8363650fa9ec241b29857da07`, with exactly one added file. It
held at `wait_required_checks` with a generic CLI exit 1 while GitHub showed the
exact commit's `Validate` check starting; that check subsequently passed. The
command helper discarded stderr, so the precise CLI diagnostic was unavailable.
The check-wait path must tolerate valid absent or partial registration within its
bounded observation deadline and preserve exact head, required producer identity,
strict branch policy, and failure rejection. This held run is not claimed as an
autonomous completion. A second fresh proof is required after the correction.

Planning for the final proof exposed a separate terminal-state regression:
later project tool revisions reopened previously completed features as
`paused / review_tool_workspace_changed`, invalidating their displayed review
state while retaining their approved history and completed publication receipts.
The persisted candidate validation then prevented runner startup. A completed
publication must remain a durable historical result when later project work
occurs; an interrupted completed receipt still needs exact approval rebinding
and remote observation before recovery.

Independent review also reproduced Remove followed by restart for an interrupted
completed receipt: the removed tombstone lost access to its retained approval
for startup validation. The correction validates that exact frozen evidence
without making removed records eligible for execution or reverification. The
regression test preserves the serialized tombstone byte-for-byte and rejects a
later substantive review outcome.

The previous application PR's Windows native scalable-repair test failed because
its observer did not capture a durable partial application before its deadline.
The old failure did not distinguish a missed brief checkpoint from slow model
probes. The fixture now enlarges the actual candidate within existing production
bounds and reports the last observed proposal state on failure. Production
application timing and policy are unchanged.

## Live proof scope

The first fresh artifact `pipeline-autonomy.txt` was generated, validated and
reviewed by the application. After its held check-wait run, explicit product
reconciliation merged proof PR #6 at
`f2945e2d8cd6dabcadf58db08757ec6e78b22dff`. This recovered publication is not the
final autonomous proof.

The final fresh proof requests exactly one new `pipeline-end-to-end.txt`
containing the 19 UTF-8 bytes `Pipeline complete.` followed by LF. It preserves
existing files. Its immutable local command reads exact bytes independently of
model output. The connected repository's hosted `Validate` check independently
checks the same bytes, prepared through
[infrastructure PR #7](https://github.com/malak333/aw-pipeline-proof/pull/7),
merged at `eff26277bb5b31a5a1127d223a2e8f9de88fd6ef`. That infrastructure PR
contains no artifact implementation. The approved real planning session is
`5fb891d9-0272-4d50-908a-e7fb3f9663f0`, plan SHA-256
`be69b3aa4b15024e140900e4ece20206bab852d40dd943edd4eab7fdb4bbab20`.
The application must generate the candidate, validate it on Windows, obtain
independent review, publish its own PR, merge normally, verify the remote base,
and record `succeeded / publication_merged` without parent reconciliation before
this run claims autonomous completion.

The native computer-use adapter returned `Sky Computer Use native pipe closed
before response`. Authenticated product API/process evidence is used for control
and observation; visual UI acceptance is a separate unverified boundary.

## Live completion evidence

One revision-bound Start request on the final installed runner recovered the two
legacy receipts and automatically advanced into the new approved feature. The
new feature finished at status revision `1480` as `succeeded /
publication_merged`, with zero repair attempts and exactly one real independent
review attempt. Its saved approval binds validation evidence
`e2ecbf9ba575584cad27ceebe4967e378193744e28ccc624ea00344ba17adc70` and aggregate
review packet `8d6555f7226a52ed26a594dd80e8f3a96f57b7e9a4a71ee432507fc2d722d85b`.
The parent did not author the candidate or reconcile this publication.

The application created and normally merged
[proof PR #8](https://github.com/malak333/aw-pipeline-proof/pull/8), containing
exactly one added file and no other changed path. `Validate` from GitHub Actions
app `15368` passed on exact head
`34383e7a12f4139d89555adc6da729fc4973006b`. The normal merge is
`3ab11b507f9f6dc11c1e47a6c1a56be3ed223691`; independent GitHub API observation
confirmed both that merge and the remote main reference. Windows workspace and
remote merged file both contain the exact 19 bytes, SHA-256
`1ca856ae5e03d7b63bf7ab92abcb065f340623d76738452f3c48e012b60d575c`.
The durable publication ledger includes candidate preparation, push, PR creation,
required-check receipt, normal merge request and verified-complete receipt.

All five non-removed queue features are succeeded with `publication_merged` and the
runner is idle. The original marker's approval history is structurally identical
to the retained pre-recovery history; its two review attempts, zero repair
attempts and original merge SHA remain unchanged. The deadline watcher exited
on success, so it will not issue a delayed Stop after this run ends.

The final installed Windows binary and its release build both hash to
`1ae9d1017b60d8bc49dde2330d499dd16495c90a6e6ffeef3c9e7bfa486ac4e4`.
All 146 tracked Cargo/toolchain/runtime source inputs match the reviewed worktree.
The supported production launcher rebuilt and restarted the Developer runner
without replacing its projects or queue. The Mac production bundle passed
`codesign --verify --deep --strict` with its local ad-hoc signature.

## Hosted validation correction

The first source PR Mac gate exposed a test-harness race in the existing
brainstorming adapter rejection fixture: the adapter correctly rejected a linked
private directory and exited before the fixture finished writing stdin. The
fixture panicked on `BrokenPipe` before checking the rejection result. Only the
expected early-rejection helper now accepts that transport result; successful
request paths still reject every write error. The negative fixture still requires
exit code 11, empty output and no Codex invocation. All three adapter E2E tests
pass independently. This correction changes test infrastructure only; the installed
runtime and autonomous proof binary remain unchanged. Canonical validation and
all exact-head hosted gates are repeated for the corrected source before merge.

## Closeout verdicts

- Documentation and safety: design, publication contract and safety rules describe
  exact retained-approval recovery, cancellation dominance, terminal receipts and
  bounded required-check observation. No publication authorization is delegated
  to a model, and no required check is bypassed.
- Durable knowledge: the knowledge base records the observed completion, repair
  marker, late tool revision and check registration behavior, plus the installed
  account/model compatibility observation.
- Unit coverage: all 408 Developer runner unit tests pass. Focused negatives cover
  changed approval/candidate binding, forged or later review lineage, incomplete
  publication, non-descendant or changed candidate bytes, stopped reverification,
  removed tombstone resurrection, and failed/malformed/wrong-head/wrong-app checks.
- Native E2E: real Git/CLI/process fixtures cover completed-receipt restart,
  descendant reverification without effect replay, delayed check registration,
  later tool revisions, next-feature execution and actual partial application
  crash/restart. Installed Windows execution and real GitHub publication are
  separately demonstrated by proof PR #8 above.
- Requirements: the deliberately bounded inert artifact completed autonomously
  through implementation, immutable validation, fresh review and automatic
  publication. The FFT website and earlier parked demonstration candidates are
  not claimed complete.
- Independent review: high-risk review approved the full runtime correction after
  resolving the Stop race, interrupted approval, terminal mutation and removed
  tombstone findings. Generated Python cache artifacts are excluded from source
  publication.
- Canonical validation: `./scripts/release-local.sh` passed, including all 408
  runner unit tests and native workflow/recovery E2E. Fresh formatting and docs
  drift contracts passed after the final source and report edits. All three
  exact-source-SHA hosted workflows are required before normal merge of the
  Assemblywright fix; their final receipts accompany the source PR.
- External boundaries: authenticated API/process proof is confirmed. Visual UI
  acceptance remains unverified because the native computer-use pipe failed.
  Ad-hoc signing does not establish Developer ID signing, notarization, stapling,
  clean-profile installation or protected-service live-device release readiness.
