# Live Codex Runtime and Repair Recovery, 2026-10-08

## Failure and correction

The installed Windows Developer runner used Codex CLI 0.153.4 while its shared
`models_cache.json` had been fetched by Codex 0.160.1. The advertised
`gpt-6.1-sol` model was therefore selectable, but the actual 0.153.4 invocation
returned HTTP 400: the model was not supported with a ChatGPT account. Repeated
planning retries reused the same incompatible executable.

The same Windows login and model succeeded with the official stable
`@openai/codex@0.160.1` package. A second probe used Assemblywright's then-current
bounded Windows argv, strict configuration and a bounded JSON output schema;
it exited zero and returned the required structured result. That probe established
model/schema compatibility, not an empty tool catalog. This isolates the
failure to the runtime pairing rather than generated website code or validation.

The verified executable SHA-256 is
`9e7c59c05cc1ce5677b1f94e835b2ac038ca3be14504e78d558eacdb0ea3f55d`.
The canonical owner-only `runtime.json` now points at the versioned 0.160.1
executable under the existing Windows `tools` directory. A private backup retains
the prior configuration. After confirming all execution surfaces idle, one
authenticated shutdown let the existing connection supervisor restart the runner.
The observed Windows process command line contains the new executable path.
The configuration correction directly rewrote no queue database, feature counter,
review receipt, validation command, credentials, protected-service configuration,
or security rule. Subsequent public API operations persist normal planning state.

Independent review found a related compatibility defect: the Windows argument
profile omitted four explicit disables to support Codex 0.148.0, while modern
Windows CLI 0.160.1 enables those features by default. The common profile now
explicitly disables `sleep_tool`, `in_app_chat`, `in_app_dictation`, and
`in_app_local_automation` on every platform. Strict configuration remains; an
older executable that does not recognize required flags becomes unavailable
rather than receiving a broader tool catalog. The deployed compatibility baseline
is the verified 0.160.1 executable, not a promise of support for arbitrary older
or newer versions. Existing image attachment handling is unchanged.

Focused Rust tests cover the same explicit disables on both platforms, exact
occurrence and absence of true overrides, and image CLI arguments. The real
loopback catalog harness now consumes the common profile and resolves the fixed
model alias from `developer_settings.rs`; its assertion remains an empty tool
catalog and no real model call. Native Windows 0.160.1 and the rebuilt installed
runner must be checked separately from a local CLI fixture.

The first rebuild exposed source/runtime drift: the production checkout was at
`873b660`, while the live database already retained the newer valid
`abandoned` publication state. That old runner correctly rejected a state it did
not understand. The checkout was fast-forwarded to the published main commit
`a3f42306a76671c53f7af7076d1fbf9b2aaf1cb5`, which preserves the exact abandoned
publication and removed-feature tombstone contract, then this compatibility patch
was reapplied. The unrelated dirty Python cache was preserved byte for byte.
No database migration, state rewrite, validation relaxation or counter reset was
used. When the failed runner could not answer a shutdown request, maintenance
first proved that the exact Windows executable was absent before unloading the
connection supervisor. The ordinary production build then resumed.

That compatible rebuild was healthy: the installed Windows Codex 0.160.1
loopback probe observed one request with zero tools, and a genuine
`gpt-6.1-sol`/medium current-argv probe returned the exact structured result.
All 21 retained queue fingerprints matched before and after restart. The deployed
runner and source digests and actual process arguments are retained in the
session's `native-runtime-proof.json`.

The first real website run then exposed a previously tested routing correction
missing from published source. Codex correctly rejected inaccurate game details
and weak test coverage. Ordinary repair tried to change protected tests, was
rejected, and entered another ordinary attempt. Fresh protected review blockers now
enter the existing staged automatic lane using validated exact v2 review
evidence, preserving ordinary repair counters. Clean, effect-free held proposals
can recover the same exact evidence; ambiguous applied effects remain quarantined.
The complete correction received independent frontier approval after its
clean-hold recovery, bounded feedback and non-Windows proposal paths were fixed.

The deployed routing source matches the reviewed source. All 421 Developer runner
tests passed. A native Windows process/HTTP fixture forced a protected-test review
rejection, skipped ordinary repair, applied an automatic JSON candidate, ran fresh
validation and obtained a new approval. That fixture establishes Windows routing
and the JSON proposal fallback, not real OpenCode staged-tool execution. The
installed runner SHA-256 is
`e834e4709dc0f6108916df1caad950f9a799321986b0f806f44c9d701c784e93`.
The installed Codex catalog and genuine model/schema probes passed again; all 22
retained feature fingerprints matched across this final restart. Evidence is in
`native-windows-protected-route-proof.json` and
`native-runtime-proof-after-routing.json` in the session evidence directory.
The complete `./scripts/release-local.sh` gate subsequently exited zero, including
all 153 Swift bridge tests. Its earlier run had one timing-sensitive shell-fixture
failure; the focused recheck and this final canonical run passed without changing
the production timeout or weakening assertions.

Stopping the looping attempt raced applied files and pending review, so the
runner correctly quarantined it. No exact staged recovery binding was available;
ordinary Resume was not attempted. The failed feature was removed through the
public API with its history and files retained. A fresh planner retains the
same requested website scope and immutable Windows validation command.

## Live product check

The exact planning session from the failed screenshot,
`e3eb1290-4ee1-446f-9f61-08e8dff4d8b7` in `aw-fft-demo4`, recovered through its
public authenticated Retry route. It retained `gpt-6.1-sol` and medium reasoning,
cleared the provider error, and produced a real planning question and understanding.
The owner authorized routine choices and completion; the selected scope is one
useful responsive, spoiler-light page with local CSS and standard-library tests.

The fresh feature `a77d90dc-9b99-451a-afaa-b3e2eb07ac44` started with the same
instruction and immutable validation command. Initial implementation passed 37
Windows tests; Codex returned five blockers, including a protected test. The
application entered the real Windows OpenCode staged lane without an ordinary
repair attempt, authorized and applied the candidate through the enabled policy,
then ran new Windows validation and a second independently bound review.

That second review found two remaining source-only factual errors. The following
transition hit a separate fully-applied staged-validation lineage error and held
the feature, preserving both review decisions and the staged apply evidence. This
real chain exposes a boundary not exercised by the JSON proposal fixture. The
held attempt is not a success. The lineage correction preserves the terminal
staged proposal as an immutable ancestor only after an exact ordinary successor
is bound to its source-only rejection. It received complete-diff frontier review,
including a correction that keeps manual staged and JSON fallback rejections on
their existing routes. All 426 runner tests passed. The native pinned-OpenCode
fixture passed on macOS and on the installed Windows runner: one escalation,
one ordinary repair, and three separately bound reviews through final approval.
This fixture uses deterministic model responses; the real model run is separate.

The installed runner SHA-256 is now
`4fe6147e0f193181ddc7cffc3550e60e0cde49dcdd533e3575e3760d70ad842f`.
Its main source matches independently approved SHA-256
`27e4cef5c718b3fb45de8a5cd75ee0a4d9db137b8bb5be0af54d2484bcad70bb`.
All 23 retained feature fingerprints matched across installation. Public Resume
of the specific held lineage then passed its effect-gated project-state digest
check and reserved the ordinary successor without rewriting prior evidence.
The recovered real-model run passed 38 Windows tests and obtained approval on
review four after one ordinary repair and two real staged escalations. Publication
then stopped at `prepare_candidate`: the earlier failed run had left a local
`README` whose original bytes never existed on the selected remote base. The
approved candidate remained unpublished; this recovered run is not a success proof.

The public API lacked recovery for this exact failure before remote publication
began. A reviewed correction permits owner-bound abandonment only for the single
canonical preparation intent with no durably recorded author/base/tree/commit/PR/
merge field, receipt, or remote-effect intent. An unrecorded local commit may remain
in the disposable checkout; this does not authorize a push. The action revalidates
the frozen candidate and approval, retains all evidence and counters, appends its
distinct receipt, and tombstones the feature without a Git or GitHub call. Its
projection shares the action's activity, setup, pause, and shutdown gates. Existing
closed-unmerged PR recovery remains intact. Independent frontier review approved
the complete diff; 429 runner tests, ten Swift publication tests and a native
HTTP/process/Git fixture passed independently.

The installed Windows runner now has SHA-256
`3db34c5d14bc4ecc539b59722f13ee79c890544626b889b86425993df82958e2`.
Its reviewed main/publication source digests are
`ce57d40dff1809baf505b3770f2151a6b3a5acd03999c13b72a480221d97b176` and
`7a6422809c6c748414b46804070496d7a213d6c786d6055021154bf720d6c3ef`.
All retained feature fingerprints matched across installation. The actual Windows
public abandonment preserved every field outside its declared terminal updates,
retained the publication ledger, cleared the barrier, and left remote main at
`ba7ec916732c16629d413b2e58c1753b272a6533`. A public shutdown and supervisor restart
loaded the exact terminal feature unchanged.

The complete plain project was archived outside the project root, with every file
digest preserved, and replaced atomically with all verified blobs from that exact
remote commit. No database rewrite or broad Git reset was used. The earlier
refinement plan was cancelled before enqueue. Fresh feature
`01b6e979-a6ed-45d2-8030-6f55f65989ec` plans the complete site from this truthful
baseline, changing only `index.html`, `styles.css`, `tests/test_site.py`, and
`README.md`. It retains section-scoped facts, ancestry-aware semantic tests,
immutable Windows validation, independent review, and normal automatic publication.
The first start stopped before any model call: the Windows context inventory
incorrectly reused the file-writing validator for the published `.gitattributes`
and `.github/workflows/validate.yml` inputs. The project remained exactly at its
published baseline and no edits, repair attempt, or review was recorded.

The independently reviewed inventory correction keeps ordinary writing, applying
and bounded context retrieval on the strict path validator. Benign dot metadata
is read only for the complete manifest, with exact path/content digests and
`path=None`; names and contents never enter inventory-selected model portions.
Existing Full Access project tools retain their separate owner-configured reading
authority; this correction grants no new tool permission. Sensitive material stays redacted, and directory holds, symlink/reparse, containment,
hardlink, cancellation and scan budgets remain in effect. A regression verifies
that metadata changes alter the manifest while attempted metadata writes fail.

All 430 runner unit tests passed. Parent-independent local process/HTTP proof
passed, and the rebuilt installed Windows executable passed the same native
proof with its native reviewer fixture: model admission, exact metadata hashes,
no metadata names or bytes in inventory-selected portions, immutable metadata
bytes, validation and review approval. The
installed main source is `03b2b449c77512248cbac9c1c3d688e1c90e720ad026cd785fd618bf04109aa9`
and runner is `cf0661db5683678984ba08272150b2cca8464230b2b7ac611391f6c8cf17d985`.
All retained feature hashes survived installation unchanged. Public Resume of
the untouched initial checkpoint then restarted the approved feature with zero
repair/review/escalation attempts.

## Autonomous completion and exact publication

Feature `01b6e979-a6ed-45d2-8030-6f55f65989ec` completed the real Windows
Qwen/OpenCode implementation lane and the selected `gpt-5.6-sol` medium-review
lane. After public Resume at 13:28:52 UTC, the application handled four staged
repairs, one ordinary repair, six fresh bound reviews, immutable Windows
validation, required GitHub checks, normal merge, and remote verification. The
parent made no control, settings, runtime, queue, counter, candidate, or
publication mutation during that sequence. Earlier installation maintenance and
the first pre-model failure remain part of the record; this is autonomous
completion after the corrected restart.

The final checkpoint is `publication_merged`, status `succeeded`, with no pending
repair, escalation or review. The sixth review approved the candidate, and all
36 Windows artifact tests passed. The application published
[website PR #5](https://github.com/malak333/aw-fft-demo4/pull/5) at reviewed head
`fbbdf8f9443b5b4a48879f17cab6ca8419fb31e4`; required `Validate` succeeded on
that exact head. Normal merge completed at 13:59:50 UTC with main commit
`4166c95da840f3111471ab979b601b71eea31ffb`. Independent GitHub blob reads verified
that both the reviewed head and remote main match the exact Windows edits and
frozen publication candidate:

| Delivered file | SHA-256 |
| --- | --- |
| `index.html` | `2b1ce2c053f78c90d7000abb52e08200cd0e30a65e5c5b979f619ad57126ded6` |
| `styles.css` | `f18b50ed3c979b9d3773a08110038d90c926aaf23e74999ea730d0a0cf0372c3` |
| `tests/test_site.py` | `f66c543b357f476c071031ec010239eb4d262e00070f6c512321eba8f74f02bb` |
| `README.md` | `55086e50f9e10e220cf7ce92dd61f91da6ad9c01f8d73d604d3fd32dc105e1d2` |

The immutable `.gitattributes` and `.github/workflows/validate.yml` blobs also
retain their verified baseline hashes. The live API is idle after completion.
Full durable feature, review and publication receipts are retained in
`completion-durable-success.json`; `completion-publication-proof.json` records
the exact required check and published-byte comparison in the session evidence
directory.

Independent local artifact validation passed the same 36 tests. Disposable-copy
mutation checks rejected 26 selected structural and factual corruptions, including
broken landmarks, section/profile ancestry, fragment targets, missing CSS,
active resources, incorrect CT threshold, forbidden mechanics and commoner
identity removal. This is selected mutation coverage, not exhaustive natural
language validation. An independent post-merge inspection found that an alternate
incorrect Jump sentence can escape the loose concept regex when a separate
correct Jump/elevation phrase remains. The delivered page's Move/Jump facts are
correct; this remaining alternative-wording test gap is nonblocking future
hardening under the current review contract. The final application review likewise
retains nonblocking notes about unused resource APIs and tighter association of
foundational job names with prerequisite wording.

Rendered browser checks passed at 1280px and 390px: no horizontal overflow,
responsive navigation, working links to all four sections, a keyboard-activated
skip link that scrolls to main, visible keyboard focus, system serif typography,
and only the local stylesheet in resource-bearing HTML elements. Browser logs
contained no warning or error. Desktop/mobile screenshots and
`website-browser-proof.json` are retained alongside the final site. This verifies
the website artifact separately from the unavailable native Mac window.

The final `./scripts/release-local.sh` run exited zero, including all 430 runner
tests, 153 Swift bridge tests, the remaining canonical Rust/Swift checks and the
native process/HTTP workflow fixtures. `canonical-final-validation.json` binds
that result to the reviewed core sources and installed native Windows inventory
proof. The unrelated dirty Python cache retained its original SHA-256. Historical
runner/source hashes above describe earlier installations; the final runner is
`cf0661db5683678984ba08272150b2cca8464230b2b7ac611391f6c8cf17d985`.

## Proof boundaries and closeout

This is a supervised-runtime correction and a fixed-argument compatibility change,
with unchanged protected-service behavior. Repository unit fixtures cannot establish
live account/model/runtime compatibility. The `unit-testing-test-generate` and
`e2e-testing` workflows guide meaningful artifact assertions and the real
Windows HTTP/process/model/review/publication checks. Website browser inspection is
separate from native application UI inspection. Native Computer Use was unavailable
at the initial attempt. A later retry reported that the Mac was locked and no
inspectable native window was available; no rendered Mac UI result is claimed.

Documentation and safety verdict: the correction follows the accepted supervised
workflow and needs no security relaxation. Durable knowledge is recorded in the
project facts. Source publication was outside the initial repair request; the
owner subsequently requested push and merge of this reviewed slice. Source
publication must pass the exact-head hosted gates through the normal pull request
path. The connected website feature's normal automatic publication was part of
its authorized product workflow. Signing, notarization and protected-service
readiness are separate.
