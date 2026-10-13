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
| Captured native responses | Windows argument quoting and stream ownership | 320 immediate real child exits cover exact stdout/stderr, newline/EOF termination, success/nonzero exit codes, missing-executable failure and handle release; production helper compiled directly |
| Global permissions | Strict mode/revision acknowledgements and exact approval bindings | Ask approve/reject, Auto/Full edit and command behavior, stopped effects and stale approvals |
| GitHub setup and publication | Canonical Windows paths, repository/visibility validation, exact run/candidate/connection/receipt bindings, explicit throttling classification, bounded backoff and redaction | Native cases cover private creation, required-check polling/failure, `Retry-After`, repeated and alternating throttling, cancel/pause during backoff, ambiguous merge/reconciliation, real runner candidates and two successive clean source updates |
| Generated website | Existing pure filter/movement assertions in the owner suite | Twenty-four Chromium/Firefox/WebKit groups cover semantic content/assets, search/filter recovery, keyboard details, movement, responsive layouts, strict paired screenshots, traces and error-free assets |

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

Fast-exit response capture is a separate native cross-process regression in
`test-windows-process-capture.ps1`, invoked by the full GUI gate. Both readers
start before exit notification is enabled; the returned task completes after
stream drainage and process disposal. This exercises the actual helper instead
of mocking the race or adding another nominal GUI self-test group.

The corrected current native journey is
`5f887284abdd4eb182667109ca93425f`; its submitted demo job and twelve-test
completion are recorded in the pipeline closeout. Current focused totals are
fourteen GUI groups, thirteen local-runner units, eight GitHub units, thirty-one
default-feature pipeline cases, twenty-five GitHub cases, seven isolated
ChatGPT cases and the separate 320-child capture regression. Earlier totals
above are historical checkpoints. The production GUI digest is
`f6c4d697b0f8bd0932131b582e299ade1488f58bdbc47ee3a9ba50d426c9fb0d`.
The current default pipeline suite has nine ignored child-validator helpers;
their parent cases invoke them. The Windows model-path regression rejects
rooted/drive/traversal/duplicate aliases without effects, then completes using a
relative backslash representation through exact canonical allowlist checks.

The existing distributed publication fixture now waits for the full projected
abandonment readiness, including idle controls and exact attention/stage state.
It retains every negative assertion and the original timeout. Seventeen native
publication assertions passed in the parent rerun; the production distributed
recovery rules remain unchanged.
