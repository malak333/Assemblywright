# Windows local workflow testing

This matrix records how the owner's `unit-testing-test-generate` and
`e2e-testing` workflows apply to the Windows GUI and same-computer pipeline.
The native application uses Rust unit/process tests and Windows UI Automation.
The generated Final Fantasy Tactics site has its own Playwright browser suite.

## Coverage and evidence

| Contract | Unit coverage | End-to-end coverage |
| --- | --- | --- |
| Job parsing, scope and local requests | Strict JSON, unknown/invalid values, loopback endpoints, allowlist traversal, reasoning budgets, candidate changes | Real native runner, fake loopback inference, real owner validator, malformed/oversized responses, forbidden edits and no-op repairs |
| ChatGPT selections and provider boundary | Authenticated catalog parsing, unsupported model/effort rejection, bounded output, runtime identity, strict response shapes | Seven explicit-feature native cases cover planning/local implementation/review, provider rejection, stop/pause, review evidence and tool-disabled contained children |
| Settings and right-pane chat | Catalog display/ID binding, effort defaults, stale revisions, legacy migration, history bounds and cloud history isolation | Native UIA saves/reloads independent roles; chat reply, stop, pause, active close and no execution/queue authority |
| Queue and process supervision | FIFO persistence, malformed/recovered state, exact terminal ownership, reentrancy, cleanup, bounded shared JSON reads and real Windows transient/persistent replacement locks | Sequential features, failure/cancel/restart, stop during preflight, interrupted recovery and failed-cancel window retention |
| Captured native responses | Windows argument quoting and stream ownership | 320 immediate real child exits cover exact stdout/stderr, newline/EOF termination, success/nonzero exit codes, missing-executable failure and handle release; production helper compiled directly, bounded runtime stabilization and a retained-process-handle negative control |
| Global permissions | Strict mode/revision acknowledgements and exact approval bindings | Ask approve/reject, Auto/Full edit and command behavior, stopped effects and stale approvals |
| GitHub setup and publication | Canonical Windows paths, repository/visibility validation, exact run/candidate/connection/receipt bindings, explicit throttling classification, bounded backoff and redaction | Native cases cover private creation, required-check polling/failure, `Retry-After`, repeated and alternating throttling, cancel/pause during backoff, ambiguous merge/reconciliation, real runner candidates and two successive clean source updates |
| Generated website | Existing pure filter/movement assertions in the owner suite | Twenty-four Chromium/Firefox/WebKit groups cover semantic content/assets, search/filter recovery, keyboard details, movement, responsive layouts, strict paired screenshots, traces and error-free assets |
| Windows GUI validation gate | Fresh unit and journey evidence, immediate child exit-code checks, extended demo completion binding | Actual gate entrypoint in disposable repositories rejects nonzero children, missing/stale/failed/malformed evidence and incomplete extended demos before downstream commands; full real native UIA remains separate |
| Windows CI scope | Eighteen actual event-handler cases preserve Windows-only selection and full-validation defaults | Four fresh native PowerShell cases cover unset/inherited exit codes, selector failure and cleanup failure with exact process outcomes and no false passing report |

The earlier installed proof retained fourteen GUI unit groups, eleven local-runner
units, six GitHub units, thirty default-feature runner E2E cases and twenty
GitHub E2E cases. Eight ignored validator helpers are child-process fixtures
invoked by their parent scenarios, not unexecuted acceptance cases. The seven
ChatGPT fixture cases require the explicit default-off Cargo feature and an
isolated target directory; a fixture-enabled runner is never installed.

The full native UIA matrix and its extended `-LiveDemo` scenarios are
fixture-only. Separate authenticated installed proof records actual settings
save/reload, selected ChatGPT chat, cancellation during real Codex execution,
ChatGPT planning, local Qwen implementation, ChatGPT review, private PR checks,
automatic merge, verified receipt and clean source parity. See the exact
run/commit/runtime hashes and retained artifacts in
[the live closeout](windows-local-pipeline.md#chatgpt-role-parity-and-live-closeout-2026-10-13).

The generated site's unchanged owner test blob is
`82370a62b5e579b4349eecb16e7b849575ef9df5`. Its candidate, merged Windows
checkout, required PR run and post-merge run each passed the 24 browser groups.
Browser fixtures, screenshots and traces remain in that separate repository's
hosted artifacts and the retained local proof directory.

## Commands and CI

[Build and test commands](build-test-commands.md#windows-local-pipeline) are
canonical. The Windows-local hosted gate builds the production-feature bundle,
runs GUI self-tests and native UIA, then tests the native runner and GitHub
adapter and the isolated ChatGPT fixture suite. CI contract smoke tests reject
removal of those steps. The existing macOS release and Windows distributed
gates remain separate. Successful hosted fixture tests do not establish live
provider entitlement or real model inference.

For the owner's current Windows-only scope, publication requires production
Windows, Windows-local and Windows distributed gates on the exact candidate.
Mac application and Swift validation are deferred; the existing Mac workflow
can run independently but is not this phase's acceptance gate.

The gate regression command is `./scripts/test-windows-gui-gate.ps1`. The
validation wrapper reserves a new GUID evidence directory for each invocation,
checks child exit codes before any later command can overwrite them, and reads
only that invocation's unit and complete journey JSON. `-LiveDemo` additionally
requires the completed extended run and twelve acceptance tests. Its hosted
artifacts include fresh unit, journey and failure evidence plus the gate
regression report. A successful job status alone is insufficient if the native
journey has no corresponding fresh completion artifact.

## Gap assessment

The pre-publication audit found hosted CI gaps for the WinForms/ChatGPT suites;
the Windows-local gate closes those gaps. Its integrated local run passed
fourteen GUI groups, twelve local-runner units, six GitHub units, thirty runner
E2E cases, twenty GitHub E2E cases and the seven isolated ChatGPT cases. The
additional unit exercises actual Windows handles: a transient lock permits an
atomic replacement after release, while a persistent lock exhausts the bounded
retry and preserves both old authority and new temporary bytes. GUI reads also
cover UTF-8 BOM compatibility, invalid encoding, oversized JSON and delete
sharing. Existing unit tests already assert
meaningful successful, rejected, malformed, oversized, stale, cancelled,
concurrent and recovered behavior; no implementation-mirroring tests were added
to inflate the count. This is a scenario coverage report, not an instrumented
line/branch percentage claim. No Rust/C# combined coverage percentage was
measured. macOS execution, remote control, signing, notarization, clean-profile
installation and general production readiness remain separate proof boundaries.

The first hosted Windows-local run exposed a gap in the extended demo case: its
launcher used hard-coded port 8080 and model defaults, so the earlier local
extended pass depended on this workstation's live inference server. That case
did not establish isolated fixture coverage. The corrected journey uses saved
role settings and a nondefault ephemeral loopback fixture, and checks the
submitted job plus all twelve unchanged slugify acceptance cases. Keep this
correction separate from the earlier authenticated private-site pipeline proof.

Required-check retry tests exercise real adapter processes. They honor an
explicit `Retry-After`, succeed after a temporary throttle, stop on the fifth
HTTP 429 even with pending responses between throttles, and cancel or pause
without merging. Authentication/authorization/server errors still stop after
one read. Production polls every fifteen seconds and uses sixty-second
exponential fallback delays; the validated fixture mode alone accelerates those
timings. The existing twenty-minute deadline bounds both waits and commands.
The alternating throttled/pending retry-cap case has a separate thirty-second
fixture check budget and forty-five-second overall wall bound. Nine real check
reads each inject 800 ms, so the test crosses the old ten-second observation
budget while still proving exactly four retries, nine reads, attention and no
merge. This isolates retry-cap semantics from hosted process-start contention;
production and the distinct short pending-check deadline test are unchanged.

Fast-exit response capture is a separate native cross-process regression in
`test-windows-process-capture.ps1`, invoked by the full GUI gate. Both readers
start before exit notification is enabled; the returned task completes after
stream drainage and process disposal. This exercises the actual helper instead
of mocking the race or adding another nominal GUI self-test group.
The fixture primes its own ThreadPool and requires two full-workload windows
that establish no new handle/thread peak, with a bounded recent range, before
recording its actual last sample as the measurement baseline. It keeps the original
32-handle bound across the 320 measured children, deliberately opens 33 process
handles to prove rejection, then proves recovery after disposal. Schema v2
evidence retains warm-up, baseline/final and negative-control samples even when
the harness fails; an incomplete failed run does not claim passed assertions.
This adjusts test measurement only; the installed production helper is unchanged.
The current fixture primes at least sixty-four workers for sixteen concurrent
captures and bounds stabilization to eight full windows and ninety seconds,
with the remaining deadline enforced within each batch wait. Shared decision
regressions reject the hosted growing prefix and alternating continued growth,
then accept its later plateau. The earlier four-window checkpoint and its
failed hosted evidence remain separately recorded in the pipeline closeout.

The current canonical native journey is
`1d006c47caaf49969ceacbf11d3bbf13`; its submitted demo job and twelve-test
completion are recorded in the pipeline closeout. Current focused totals are
fourteen GUI groups, thirteen local-runner units, eight GitHub units, thirty-one
default-feature pipeline cases, twenty-five GitHub cases, seven isolated
ChatGPT cases and the separate 320-child capture regression. Earlier totals
above are historical checkpoints. The production GUI digest is
`e1cee4fcee83c5b91db505456a8d9bbc0dbdb56a991601b1bb1b773cacdfc124`.
The earlier authenticated automatic demo used GUI digest
`f6c4d697b0f8bd0932131b582e299ade1488f58bdbc47ee3a9ba50d426c9fb0d`;
keep its live proof separate from the subsequent display-projection correction.
The current default Windows pipeline suite has nine ignored child-validator
helpers; their parent cases invoke them. The Windows model-path regression rejects
rooted/drive/traversal/duplicate aliases without effects, then completes using a
relative backslash representation through exact canonical allowlist checks.

The existing distributed publication fixture now waits for the full projected
abandonment readiness, including idle controls and exact attention/stage state.
It retains every negative assertion and the original timeout. Seventeen native
publication assertions passed in the parent rerun; the production distributed
recovery rules remain unchanged.

The two Job Object parent cases, their three child fixtures, Windows model-path
fixture and `tasklist` helpers are Windows-only. The portable Mac test inventory
therefore has twenty-eight default cases and five ignored child helpers; it
does not establish Unix descendant containment. The Windows rerun still passed
all thirty-one default cases, including both Job Object scenarios.

All child-validator helpers require an exact individual name plus `--ignored`
and a remote-free candidate checkout before executing. A separate broad
`--ignored` native rerun passed all nine Windows helpers without creating files,
spawning descendants or changing repository status. These nine safe no-ops are
fixture-selection proof, not nine additional E2E acceptance cases. The real
parent-process scenarios continue to exercise their effectful child invocations.

The GUI queue restart journey waits for restored terminal status and an enabled
Start control, then proves admission of the exact pending entry after a single
Start invocation. It separately checks completion at the unchanged deadline,
preserves the first stopped run's identity, and confirms the restored queue
does not advance before the owner acts. Bounded sanitized queue, report and UI
state is retained if readiness, admission or completion fails. This replaces a
fixed startup sleep and closes a diagnostic gap without adding nominal test
counts or claiming an unproven product root cause.

Terminal queue persistence is also separated from display refresh. The existing
FIFO/persistence unit group uses real Windows Save/Load with an injected
projection exception for both local-only and connected completion; the saved
Completed result, run/publication fields and next frozen Pending feature stay
intact. A separate injected persistence failure propagates, never projects and
leaves the old bytes unchanged. The GUI stops queue advancement on a display
failure and retains the authoritative result. Full native UIA still exercises
normal sequential completion through this production boundary.

The complete Windows wrapper passed on source `b0e98cc`, rebuilding the current
production bundle and consuming fresh unit/journey evidence from that invocation.
The independently inspected installed bundle matches all four tested runtime
digests, exposes all five expected control IDs and retains the Completed display
and all five pre-install owner JSON hashes. Inspection used PowerShell 7.6.5;
Windows PowerShell 5.1 exposed only fallback pane/handle nodes in this installed
session, so its earlier readiness failures remain failures. This observation
does not replace the separate full canonical GUI journey or establish a startup
timing guarantee. The original automatic demo still belongs to its earlier bytes.
