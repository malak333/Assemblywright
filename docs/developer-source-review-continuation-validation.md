# Source review continuation validation

The October 10 `aw-fft-demo8` failure exposed a missing automatic-repair route.
Feature `4fe41d3f-2289-48df-9455-c35ae1e999a7` had three ordinary attempts, seven
escalations out of its snapshotted 100, and a complete version-2 review rejection
at `review_10_rejected`. Its only current finding targeted `index.html`.
The runner was idle while the retained automatic lifecycle remained `running`.

The terminal staged-review continuation helper admitted protected-input findings
but stopped source-only findings after the ordinary limit. The change admits the
next bounded escalation after the exact terminal application, current candidate,
validation, and review chain is verified. Remaining ordinary attempts retain their
existing route. No counter, validation command, reviewer, or publication binding
is changed, and cancellation, operational holds, and ambiguous effects retain
their existing behavior.

## Regression and independent review

The `unit-testing-test-generate` workflow covers verified terminal routes,
remaining versus exhausted ordinary attempts, disabled policy, held lifecycle,
cancellation, and missing evidence. Existing lineage tests cover malformed,
stale, or spliced stage and review evidence.

The `e2e-testing` workflow uses the native Rust runner, pinned OpenCode process,
HTTP model/reviewer fixtures, SQLite, and real files. The dedicated
`--exhausted-source-staged-retry-only` scenario starts with a source rejection,
consumes all three ordinary repairs, rejects a staged candidate on another
source-only finding, and requires a second staged attempt to reach fresh approval
without Resume. The canonical native workflow wrapper includes this fifth staged
mode. The longer sequence has its own bounded 480-second fixture deadline.

An independent reviewer ran this scenario against published baseline `a26502c`.
It stopped at `review_5_rejected` with three ordinary attempts, one escalation,
five reviews, `running=false`, automatic policy enabled, and the retained
lifecycle still `running`. The candidate runner reached approval in the same
scenario. Review also identified that restart reservation needed the same
terminal lineage verification as live continuation; that boundary is included
in the final change and regression coverage.

The final runner unit suite passed all 444 tests. Focused exhausted-route and
terminal-stage lineage cases cover current packet/receipt/byte/history drift,
ordinary descendants following validation failures, and restarted reservation.
The corrected recovered-review fixture contains real sequential ordinary
history and complete version-2 review evidence; a status-only legacy fixture
cannot authorize a new staged attempt.

The exhausted native scenario passed on both macOS and Windows, retaining three
ordinary attempts, two escalations, and six reviews. The canonical workflow
also exercised remaining ordinary routing, candidate rejection, protected
rejection, and completed empty-stage retry. Windows fixture success is emitted
after authenticated graceful runner shutdown, exit-code verification, server
teardown, and temporary-directory cleanup. Forced termination is failure cleanup,
not successful proof.

Independent high-risk review approved the complete behavior and fixture changes
with no P0-P3 findings. The final runner source SHA-256 is
`ea4fe35a5fd48bd8df8ba6045397368d92607ec29c935dd77ca2d28a6f82a3b7`;
the staged native fixture SHA-256 is
`5899b56ed00f2ac7beb113f0b17199ac5bc418c64b528e721baac301f5dbcded`.

The full gate also exposed two test-fixture compatibility issues: a malformed
positive recovered-review fixture, and Python 3.10's lack of
`BaseException.add_note` in GitHub setup/publication rejection diagnostics.
The latter fixes preserve bounded readable HTTP errors, exact rejection codes,
and all authentication, redaction, and publication assertions. Both focused
native fixtures passed and their complete diffs received independent approval.
The final full canonical run and exact-source installed rebuild both passed;
their distinct results are recorded below.

## Installed run and preservation

Before changes, a consistent SQLite backup, complete project archive and hashes,
and the prior runner executable were retained at
`C:\a\aw-developer-20260905\state\pipeline-completion-demo8-backups\20261010T124725Z`.
The source change is developed in an isolated checkout based on published commit
`a26502c`; unrelated edits in the original checkout are preserved.

The retained original feature reached `succeeded` / `review_18_approved` at the
October 11 capture, after its unchanged Windows command
`python -m unittest discover -s tests -v` passed 99 tests. Its three ordinary
attempts, fourteen escalation attempts, and eighteen reviews remain in durable
history. The app generated and applied every website repair; the parent made no
website edits, changed no model/reviewer/validation setting, and used no review
override. All 28 other queue entries match the initial capture.

The installed correction automatically advanced the original seventh escalation
through subsequent repairs. Review 16 returned no structured provider decision
and entered the existing operational hold. One exact-bound parent Resume on
October 11 retried the retained candidate; fresh review 17 then rejected two
content/test issues, the app automatically prepared escalation 14, and review 18
approved the new validated bytes. This proves automatic repair continuation and
retained recovery, while preserving the external reviewer interruption as a
limitation of the original run. It does not claim uninterrupted completion.

The completion capture verifies every current project file against the exact
cumulative edit bytes. The final review packet digest is
`722580d1429c0f522b6940feae262ab7aed3cf571a9fcf9e96435191e019f271`;
the decision digest is
`e22cf5cacf5db8e1941b3da3a332c9b5a97f4e867a6f66af983ec295cbabf2a2`.
The approved review retained one nonblocking observation: some factual regex
checks depend on line wrapping and do not distinguish all negated claims.
Future editorial validation can improve that coverage; the approved bytes are
preserved as the completion proof.

The generated website is local-only because this feature had no frozen GitHub
publication connection. Installed completion, native fixture coverage, website
browser checks, repository validation, hosted publication, and signed distribution
remain separate evidence.

## Fresh installed completion

Fresh local feature `ff826d9b-8510-40c4-ba5c-c65b576cb1de`, project
`aw-autonomous-proof-20261011`, exercised real Codex planning, approved queue
admission, the saved Windows implementation model, immutable validation, and
fresh Codex batch/aggregate review on the final installed runner. The parent
reviewed and narrowed the plan before approval, then issued one ordinary Start.
There were no parent mutations or artifact edits after Start.

The job reached `succeeded` / `review_1_approved`, with zero ordinary repairs,
zero escalations, and one review. Its only project file is the exact 31-byte
UTF-8/LF `result.txt` payload `Autonomous pipeline completed.` plus one newline.
The unchanged Python byte comparison passed. Artifact SHA-256 is
`3634c279f1b176b47bf3a49a630e9ed41c573387a5bc76e38222ea25acbd0840`;
review packet SHA-256 is
`2356b8fc47d057dd0c9950725cc5024ae3dfbd379bb00a5a26783b3837c09848`;
decision SHA-256 is
`28e125c1fcb052332f2a63555af05ff263327c47c063cba8134d9a8aa3d4fff8`.
The original website remained succeeded and the other original 28 queue entries
remained unchanged. This fresh run proves an uninterrupted bounded completion;
the original website separately proves substantial retained repair recovery.

## Final validation and deployment

- `./scripts/release-local.sh` passed in the isolated checkout with the pinned
  OpenCode 1.18.23 executable supplied through
  `ASSEMBLYWRIGHT_DEVELOPER_OPENCODE_EXECUTABLE`. This includes Rust formatting,
  Clippy, workspace tests and ignored fixtures, all five staged native workflow
  modes, connection/process tests, platform builds, unsigned package launch,
  release-evidence self-tests, and Swift tests/builds.
- `./scripts/release-docs-drift-smoke.sh` and `git diff --check` passed. The
  runtime/test source remained stable throughout the successful full gate.
- `python3 -B scripts/production-build.py --build --no-open` rebuilt and installed
  the exact reviewed Windows runner from the production checkout. Its source
  matches the final `ea4fe35...a3b7` digest above. The built release executable
  and installed executable both hash to
  `59c2dbfa507fa9d22c6aa723d16d43ded63d9eb096310a4a74bb549935f03bfd`.
  The exhausted-source Windows fixture passed again against these exact
  installed bytes, with clean post-teardown success JSON and counts 3/2/6.
- The previous production Mac app process was stopped only after verifying its
  exact executable and an idle completed queue. The canonical production
  launcher reopened the rebuilt bundle. The connected API retained
  `review_18_approved`; the fresh proof also completed on this installation.
- The exact approved website bytes passed Chromium 151 and WebKit 26.5 checks
  at 1440x1000 and 375x812, with JavaScript disabled: landmarks, section links,
  navigation, overflow, keyboard skip-link focus, no scripts, no external
  requests, and no page errors. macOS WebKit uses Option-Tab to focus links.
  Desktop/mobile screenshots were inspected. Firefox fixtures could not launch
  in this host's sandbox/framebuffer environment, so Firefox behavior is not
  asserted. Native CUA's pipe was unavailable; final native AX/screenshot QA is
  not asserted. Native process/API completion and the earlier native progress
  capture remain distinct evidence.

## Initial local closeout verdicts

- Documentation, requirements, and safety: pass. Design, repair routing, safety,
  commands, and this evidence record are current. Exact lineage, current bytes,
  immutable validation, fresh review, cancellation, holds, and shared caps are
  retained; no review or authorization override was used.
- Durable knowledge: facts added for the exhausted source route, realistic
  regression sequence, Windows teardown, Python diagnostic compatibility, and
  the original/fresh installed completion boundaries.
- `unit-testing-test-generate`: pass. Focused route/lineage/recovery cases and
  the complete 444-test runner suite passed; no coverage percentage is claimed.
- `e2e-testing`: pass for the native pipeline boundary. Native processes,
  HTTP fixtures, SQLite, files, pinned OpenCode, Windows installed execution,
  and real model/reviewer runs cover the relevant boundaries. Browser checks
  apply to the generated website; Firefox and final native visual gaps remain
  explicitly recorded above.
- Canonical validation and independent review: pass. The full local gate passed;
  independent high-risk reviews approved the complete behavior and fixture
  changes without P0-P3 findings.
- Publication/hosted gates at the initial local closeout: not requested and not
  run. The owner subsequently requested GitHub publication and merge on October
  11. That follow-up must verify the exact PR revision through all three hosted
  workflows before normal merge, then reconcile published source on the local
  and Windows hosts. The installed completion results above precede publication;
  both live proof features had no frozen GitHub connection and remain local-only.
- Deployment: pass for this installed Developer runner and Mac app. The
  AssemblywrightMaster service inputs were not changed. Developer ID signing,
  notarization, stapling, broader clean-profile/owner-recorded live-device QA,
  and production-release readiness are not established by this slice.
