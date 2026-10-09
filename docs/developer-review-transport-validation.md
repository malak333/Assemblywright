# Developer pipeline recovery, 2026-10-08

The retained `aw-fft-demo5` feature reached `succeeded` at
`review_18_approved`. Its original Windows validation command passed all 54
tests, and real `gpt-6.1-sol`/medium review approved the exact cumulative files.
The application completed the final repair cycles without another owner Resume
or parent edits to the generated website. The final repository and native Windows gates passed, the reviewed follow-up
runner is installed, and the completed feature survived its verified restart.

The live `aw-fft-demo5` feature stopped at `review_4_unavailable` after three
applied automatic staged repairs and 45 passing Windows tests. The retained
error was `Codex batch decision binding mismatch`. The old provider schema
accepted digest-shaped strings and manifest entries without fixing their exact
values; the provider had to copy host identity metadata correctly. Its generic
failure mapping also hid whether identity or semantic validation failed.

The batched provider schemas now accept only reviewer-owned judgments: decision,
blocking/nonblocking findings, and batch summary/interfaces. Windows constructs
the existing version-2 receipt's candidate, provider/model, validation, ordered
manifest, and batch receipt digests from the canonical input of that completed
call. The final aggregate cannot approve a rejected batch or omit a batch
blocker's exact path/message. Unknown response fields, invalid findings,
contradictory judgments, and bounded/redaction failures remain unavailable.
The legacy version-1 transport and all persisted receipt shapes are unchanged.

This changes the transport contract, not the independent approval requirement.
No validation command, model selection, counter, candidate bytes, credentials,
permissions, cancellation rule, or publication gate was relaxed.

## Initial transport verification

The isolated reviewed source is based on Git commit
`4f853777277c53ba35fcd395e7a41009d9df2c69`. Its reviewed
`developer_review.rs` SHA-256 is
`882f658b9b3a17aca42b5d6998f7a170cae8933664957bcdb0189593fd81dd69`;
this is a file digest, not a Git commit. This runtime evidence was recorded
before the implementation commit and GitHub publication.

- `cargo test -p assemblywright-master developer_review::tests -- --nocapture`:
  31 passed. Tests cover strict semantic parsing, metadata injection rejection,
  path/decision/summary bounds, host-owned deletion/image manifests, and aggregate
  rejection/blocker preservation.
- Native Mac review and scalable-repair HTTP/process fixtures passed, including
  rejected review through automatic repair and approval, malformed output through
  Resume without generation/write replay, cancellation, drift, restart, image
  attachments, and 126 entries across four batches.
- Native Windows Developer runner tests: 432 passed, one explicitly ignored.
  Native Windows review HTTP/process E2E passed all seven recovery assertions.
- Native Windows scalable HTTP/process E2E passed all assertions: 126 candidate
  entries, four review batches, typed text/PNG/deletion changes, partial-apply
  restart, exact owner adoption, provider-unavailable Resume, immutable validation,
  and drift quarantine without a second write. Its 120 generated files remained
  bound to the reviewed candidate.
- Independent high-risk complete-diff review: approved, no actionable findings.
- `./scripts/release-local.sh`: passed (exit 0), completed at 23:20 UTC. This
  includes workspace format/clippy/tests/build, native Developer recovery E2E,
  connection/build tests, package/evidence checks, and partitioned Swift tests
  plus the Swift build. Dedicated final documentation drift/diff checks are run
  again after recording the live result below.
- Format, diff, Codex workflow, and documentation drift checks passed before
  closeout; final documentation results are recorded after the live result.

## Installation and live feature

An owner-private SQLite backup passed integrity checking before maintenance.
An authenticated graceful shutdown and stopped-process check preceded replacing
only the runner executable. All 25 retained queue entries matched byte for byte
across installation. SQLite integrity remained `ok`.

Installed optimized Windows runner SHA-256:
`ee9fe6e1aac02d5d140df1c0b6dc25c95d15f8255b4200b6ac63bfb25c6397d9`.
The installed Windows review source matches the reviewed file digest above.
The app-owned connection supervisor restarted normally and authenticated to
`MIKE-PC`. The accepted patch was integrated into the existing production build
checkout, preserving its unrelated dirty Python cache.

Public Resume of feature `80464d91-a552-4505-9699-7ddaf9965e01` retained the
immutable `python -m unittest discover -s tests -v` command, passed 45 tests,
and admitted fresh real `gpt-6.1-sol`/medium review attempt five. No implementation
or staged file writes were replayed by this resume. The terminal outcome is recorded below; the retained feature was not replaced
with a simpler proof project.

Real review five completed and rejected five remaining website/content/test
defects. The application automatically prepared and applied staged repair four,
validated it, and completed real review six. Only the stylesheet-derived contrast
test remained blocking. No parent edits were made to the generated website.

## Local model output bound

Automatic preparation five used the OpenCode disposable-stage tool lane, not the
direct-HTTP JSON lane. It recorded zero admitted stage mutations and held at
`escalation_5_unavailable`. The selected local server completed exactly 32,000
output tokens. Unrestricted reasoning was a likely contributor; the retained
empty mutation ledger does not establish the provider's precise finish reason.
Direct-HTTP calls already set `enable_thinking=false`, but OpenCode does not
inherit that setting.

The installed llama.cpp `b10901-28ff09582` explicitly supports a reasoning token
budget. The server now uses `--reasoning-budget 4096` with the same
`qwen3.6-35b-a3b` alias, 262,144-token context, executable, weights, loopback
address, and other generation/server arguments. The unpinned Hugging Face model
selector was replaced with the exact existing local snapshot path. Its
20,419,565,568-byte GGUF SHA-256 is
`671e47e0ec53c665d048b98c3ecbfd5236b5ca9c3e02ed19fc8f81f7b85140c7`;
the llama-server executable SHA-256 is
`cff88bca0c9a023e8deb0c1a68b87604a530f1edfabd3c4067f62b8b523bdfcc`.

Before restarting, the runner/chat/tools and model slot were idle; TCP inspection
confirmed one loopback listener owned by the recorded process and no established
clients. The owner-controlled argument-array launcher verifies both hashes.
An initial SSH-child launch did not persist after SSH closed; launching its
wrapper through `Win32_Process.Create` resolved that process-lifetime boundary.
Separate subsequent SSH/API calls confirmed persistent health and the budget.
No permissions, network exposure, credentials, or reviewer settings changed.

The disposable `aw-review-budget-smoke-20261008` project exercised the actual
public project-chat/OpenCode path. Request
`0c42fa30-258f-4791-af3d-57b5abc202c8` completed write/read tool actions, finished
with no error and workspace revision one, and produced exactly
`BOUNDED_MODEL_TOOL_OK\n` (SHA-256
`e2513a5afccca5ee925e5972813c87e25c07fe0192754ef944900520bcc049f7`).
Only after that terminal tool proof did public exact Resume admit epoch three.
Fresh real review seven retained the one blocker and automatically started
staged repair six. That candidate changed the contrast test, passed 50 immutable
tests, and reached real review eight. The application then automatically fixed
a remaining incorrect character title. Reviews nine and ten identified regional
publisher attribution and insufficiently scoped factual assertions. An ordinary
repair that attempted the requested protected test change was rejected without
applying candidate files; the bounded loop reached combined staged repair seven.
That candidate passed 52 immutable tests. Real review eleven rejected an
incomplete sentence introduced by the preceding edit and another contrast-test
coverage gap; automatic staged repair eight started without owner intervention.
Repairs eight through ten corrected the incomplete sentence, scoped factual
assertions, a spoiler-boundary problem, the reference citation, and omitted
approved-design sections. Repair ten independently checked the [archived primary magazine scan](https://archive.org/stream/NextGeneration40Apr1998/Next_Generation_40_Apr_1998_djvu.txt)
rather than copying an inaccurate issue number suggested by review.
Its original immutable validation passed 53 tests. Review fourteen requested
coverage of the actual print CSS cascade. Repair eleven added a print-contrast
test and passed 54 tests, but review fifteen rejected its hardcoded colors and
incorrect inherited-background assumptions. Automatic repair twelve started
without another owner Resume. Repairs twelve through fourteen finished the stylesheet-derived print checks,
including spoiler-note and inherited character-card paragraph colors. The final
original immutable command passed 54 tests and real review eighteen approved
the cumulative candidate with zero blocking findings. Counters remain three
ordinary repair attempts, fourteen escalation attempts, and eighteen review
attempts; they were never reset.

The approval retained two nonblocking observations: some documented print
ratios were inaccurate despite the implemented colors passing, and an Alma
test's name/documentation overstated its assertions. These are recorded rather
than silently rewritten by the parent after approval.

## Windows native-command guidance

The pinned OpenCode PowerShell adapter also reported successful Python test
commands as failed: Windows PowerShell 5.1 promotes ordinary native stderr into
`NativeCommandError` under `ErrorActionPreference=Stop`. This is separate from
the runner's immutable validation command, which correctly passed the tests.

Staged Windows prompt guidance now supplies a child-scope wrapper that uses
`Continue`, converts merged output records into strings, captures the native
exit code immediately, rejects a missing code, and propagates a nonzero code.
The surrounding error preference is restored when the child scope finishes.
The host's tool-event parser and validation execution are unchanged.

An independent probe on the actual Windows PowerShell host reproduced shell exit
one for a native exit-zero process writing stderr under `Stop`. The wrapper
returned shell zero for native zero and shell seven for native seven, with both
output streams preserved as ordinary text. A nonexistent native program returned
shell one, and successful scope exit restored the outer `Stop` preference.
Independent source review approved
this guidance delta. These probes establish shell behavior; they do not show
that a model will always choose the supplied wrapper.

## Protected-input rejection routing

The ordinary repair path can receive a source-file finding that also asks for
a regression test. The source path alone does not predict a protected test
change. The existing tool boundary correctly rejects that staged candidate
without applying its bytes, but previously spent another ordinary repair
attempt before combined staging.

The follow-up routes an exact typed staged-candidate rejection to combined
staging using its completed rejected-review predecessor. Admission checks the
retained cumulative edits against the prior repair evidence, validates current
live bytes, and recomputes the exact review packet and ordered batch digests.
A rejected candidate can retain cumulative edits from earlier accepted work;
requiring an empty edit list incorrectly excludes this real recovery state.
Missing, stale, duplicated, or mismatched review links and candidate drift hold.
A validation-only predecessor retains the established bounded route, and the
older quarantine route still requires protected-path review feedback.

This route preserves repair history and budgets, fresh project/protected-input
snapshots, cancellation checks, immutable validation, and fresh independent
review. It applies none of the rejected candidate bytes. The stable complete 14-file diff received independent high-risk approval with
no P0-P3 findings. Three focused routing/live-byte tests passed, as did the
existing protected-review preservation tests. The actual native HTTP/process
candidate-rejection scenario passed on both Mac and Windows with one ordinary
attempt, one escalation, two reviews, nonempty retained edits, exact review
feedback, and a malformed-linkage hold without another model call or write.
The default lineage scenario also passed on both hosts. Canonical
`developer_workflow_e2e` now invokes both modes.

Final Windows Developer unit verification passed 436 tests with one explicitly
ignored fixture helper. The reviewed source hashes are:
`developer_main.rs`
`940c92da06c54970215f5b0eaa5377ca82927d24eb51d5d989c434dd93a4b204`;
`developer_review.rs`
`882f658b9b3a17aca42b5d6998f7a170cae8933664957bcdb0189593fd81dd69`.
The seven explicit Rust/fixture files were integrated into the Windows checkout
only after independent approval, with exact before/after digest checks and
preserved prior source. The final Windows native gate passed all six commands: Developer unit tests,
native runner/reviewer-fixture build, both staged modes, all seven review recovery
assertions, and scalable repair. The scalable case retained 120 generated files,
126 candidate entries, 127 recovery entries, four batches, 27 OpenCode calls,
26 tool actions, and 71 incremental apply events (177.0 seconds).

The optimized Windows build passed with candidate executable SHA-256
`1ae78acb38e1d3573be45c1d038c1b88cb6bae442b3b7eb5eaa0bbd73985d651`.
The final full repository gate passed (exit 0) at 00:57:23 UTC on 2026-10-09
(October 8 locally). It includes both canonical staged modes, workspace
format/clippy/normal and ignored tests/build, connection tests, package and
release-evidence checks, partitioned Swift tests, and the Swift build. The seven
reviewed source/fixture digests remained unchanged. Final documentation drift,
Codex workflow, and diff checks passed afterward. Final installation and restart verification passed as recorded below.

## Exact live completion

Feature ID: `80464d91-a552-4505-9699-7ddaf9965e01`.
Terminal status/checkpoint: `succeeded` / `review_18_approved`.
Immutable command: `python -m unittest discover -s tests -v`.

- Review packet SHA-256:
  `28f3484e84c8660ffd1ab4616eaaf490c0684f7d200ecd9da24b8f66f97a351d`.
- Validation evidence SHA-256:
  `4ec9f8671c68be8ee872c9d1b63551e7da314dc28d0c5487d1bd7b783d6008ea`.
- Review decision SHA-256:
  `14c5db40caf619ac9f54be7f815b7f8207aab7d45c4d2c143ecf6a0058a8e5c7`.
- Ordered batch packet/receipt digests:
  `8dc710f507c422b5d6685db48a9f0b4b49286223f65be8752e83f09c1085f962` /
  `07503f31ad618946c1767eff7bb85c1607a153d302003dd2b40b534540fa02b5`.
- Final `index.html` SHA-256:
  `f3bb2ebb61a7de472da87bdf699a193c486b827a429236693b7afc2373c0ca92`.
- Final `tests/test_site.py` SHA-256:
  `34d21260dc6767163ddf5210a9a4e362ae6b12773cf16e3a08a708ea207f4eab`.

SQLite integrity was `ok`; every other retained queue entry remained byte-for-byte
unchanged against the original pre-maintenance backup. Public status revision
2457 showed no active run, repair, or escalation. Full review history, original
validation log, all six approved project files, source manifests, browser reports,
screenshots, and mutation results are retained in private session evidence.

## Website acceptance and limits

Read-only checks against the final approved website bytes used actual Windows
Chrome 154.0.8037.98 with JavaScript disabled. The receipt records the executable,
headless mode, and disabled-JavaScript context configuration; an inline-script
canary did not execute in each of the three viewport contexts. The exact launch
script and its SHA-256 are retained with the browser evidence. Desktop 1280px and mobile 375px
passed five-section presence, no horizontal overflow, first-Tab skip-link focus
and visibility, skip target, fragment navigation, and enlarged-text reflow.
A separate computed-style check evaluated 57 rendered text/background pairs
in each of screen and print mode; all met their font-size/weight contrast
threshold. The before/after browser source manifest exactly matched the approved
completion manifest. The copied final project independently passed all 54 tests
on Mac. In-memory-only mutations of the screen body, print body, and spoiler-note
color each caused the relevant contrast assertion to fail; the live project
and final captured files were not modified.
An additional 320px enlarged-text check found horizontal overflow; it is recorded
as a limitation. These checks did not modify the project. Browser screenshots
and the machine-readable report are retained in the private session evidence.

The generated project's own browser-report provenance was not established by
these independent checks. The macOS app computer-use pipe was unavailable;
API/process evidence does not establish a visible installed-app walkthrough.
The generated feature is local-only because it had no GitHub connection at its
original start. Its local completion does not establish generated-feature
publication, signing, notarization, protected-service activation, or universal
future-error-free behavior.


## Final installation and closeout verdicts

After the full gate passed, a fresh private SQLite backup passed integrity
checking. Public status showed all run, repair, escalation, chat, planning,
tools, setup, and publication activity idle. An authenticated graceful stop
completed, and native process inspection proved zero runner processes before
replacement.

Installed optimized runner SHA-256:
`1ae78acb38e1d3573be45c1d038c1b88cb6bae442b3b7eb5eaa0bbd73985d651`.
The original runner and database backups remain private and recoverable.

The normal app-owned connection supervisor restarted and authenticated to
`MIKE-PC`. All 25 queue entries matched the fresh pre-maintenance snapshot
byte for byte after restart. SQLite integrity remained `ok`. Public revision
2458 retained `succeeded` / `review_18_approved` with no active run or repair
and no emergency pause. The local model's health endpoint returned `ok`.
The installed Windows source hashes match the reviewed `developer_main.rs` and
`developer_review.rs` hashes recorded above.

- Documentation and safety compliance: PASS. Design, safety, build commands,
  recovery contracts, and this evidence record are consistent. Validation,
  independent approval, redaction, cancellation, and publication requirements
  remain intact.
- Conversation-derived repository knowledge: ADDED. The knowledge base records
  host-owned review metadata, Windows native stderr handling, model reasoning
  bounds, exact rejected-candidate routing, and retained-job proof limits.
- `unit-testing-test-generate` coverage: PASS. Strict semantic/metadata rejection,
  binding/digest/order errors, live-byte drift, deletion recreation, asset
  metadata, missing/pending/stale links, and protected-route preservation were
  tested. Native PowerShell probes preserve zero/nonzero/missing exit behavior.
- `e2e-testing` coverage: PASS at the native HTTP/process/SQLite/OpenCode and
  real installed Developer boundaries. Canonical Cargo E2E includes both staged
  modes; real retained-feature completion supplements fixture proofs. Chrome
  acceptance is separate and limited to the recorded browser/viewport states.
- Canonical validation: PASS. Final full `release-local.sh` exit 0; native Windows
  six-command gate exit 0; documentation drift, Codex workflow, and diff checks
  pass.
- Publication and hosted gates: OWNER REQUESTED after runtime closeout. Publish
  these reviewed application changes through a feature branch and normal pull
  request, wait for all hosted gates on the exact final candidate, and verify
  the merged source on both hosts. The generated feature remains local-only
  under its original frozen selection.
- Deployment: PASS for the reviewed Windows Developer runner and verified
  connection/queue/database restart. The production Mac build checkout receives
  the reviewed source/docs while preserving its unrelated tracked cache.
  Swift wire contracts were unchanged, so no Mac UI rebuild was required.
  Protected `AssemblywrightMaster` service activation, signing, notarization,
  clean-profile installation, and a visible app walkthrough remain unproved.
