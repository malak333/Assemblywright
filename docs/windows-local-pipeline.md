# Windows local pipeline

Owner-approved pivot: 2026-10-12. The first supported execution topology puts
Assemblywright, local implementation inference, repository edits and validation
on the same Windows computer. The owner's 2026-10-13 Mac-parity correction adds
ChatGPT/Codex planning and review, with separate model/reasoning selections;
local inference continues to implement edits. Mac control and distributed execution are deferred.
This document takes precedence over the earlier two-machine target designs for
this local execution lane.

## Run it

The native Windows GUI is built by `scripts/build-windows-gui.ps1`.
Double-click `Assemblywright.cmd` in the repository to build it if needed and
open it. `scripts/install-windows-gui.ps1` installs the portable bundle under
`%LOCALAPPDATA%/Assemblywright/WindowsApp` and creates Desktop and Start menu
shortcuts named **Assemblywright**.

The window provides a repository picker, feature specification, allowed-file
list, validation executable and arguments, local model settings, and job
open/save. **Run feature** freezes the submitted job and launches the native
runner. **Run demo** starts the bundled verified demonstration. Live stage,
attempt, and logs remain visible while the job runs. Cancel, Emergency Pause,
and Resume use the same native controls; a completed result enables the
candidate folder and report buttons. Closing an active window requests
cancellation and waits for the runner to stop. Resume permits a new run.

The GUI is a WinForms .NET Framework executable; it requires no .NET SDK or
NuGet packages. Its portable folder contains `Assemblywright.exe`,
`assemblywright-local.exe`, `assemblywright-github.exe`, and
`windows-local-pipeline.ps1`. Windows owns this
local control surface; Swift continues to own the deferred Mac surface.

For command-line operation:

From the repository in PowerShell:

```powershell
./scripts/windows-local-pipeline.ps1 -Demo
```

The launcher builds `assemblywright-local`, seeds an unimplemented Python library
and CLI in a disposable Git repository, and submits the specification to the
local model. The model supplies the implementation; twelve owner-written tests
verify the library and CLI. A separate model call reviews the candidate.
Assemblywright retains the finished candidate and report. The fixture does not
provide the implementation or replace inference with canned edits.

For a real repository, save a job document and run:

```powershell
./scripts/windows-local-pipeline.ps1 -Job C:/work/my-feature.json
./scripts/windows-local-pipeline.ps1 -Action status -NoBuild
./scripts/windows-local-pipeline.ps1 -Action cancel -NoBuild
./scripts/windows-local-pipeline.ps1 -Action pause -NoBuild
./scripts/windows-local-pipeline.ps1 -Action resume -NoBuild
```

`-StateDir` overrides the default `%LOCALAPPDATA%/Assemblywright/local-pipeline`.
One runner holds that state's exclusive lock. Ctrl+C or local Emergency Pause
cancels active work. Resume permits a new explicit run; it does not restart an
interrupted job. The distributed service has its own existing pause state.

Example job (replace repository and validation executable with actual paths):

```json
{
  "repository": "C:/work/my-project",
  "specification": "Implement the approved feature and pass the existing tests.",
  "allowed_files": ["src/example.py"],
  "validation": {
    "program": "C:/path/to/python.exe",
    "args": ["-m", "unittest", "discover", "-s", "tests", "-v"],
    "timeout_seconds": 120
  },
  "endpoint": "http://127.0.0.1:8080/v1",
  "model": "qwen3.6-35b-a3b",
  "reasoning_effort": "none",
  "orchestrator_provider": "chatgpt",
  "orchestrator_model": "gpt-5.6-sol",
  "orchestrator_reasoning_effort": "medium",
  "reviewer_provider": "chatgpt",
  "reviewer_model": "gpt-5.6-sol",
  "reviewer_reasoning_effort": "high",
  "max_attempts": 3
}
```

The endpoint is an OpenAI-compatible local HTTP interface, not a cloud service.
The current Windows computer runs llama.cpp with Qwen at port 8080. Changing a
job's model or endpoint is an explicit owner choice. There is no fallback.
The job's selected files supply coding context. Include the files needed to
understand the task, and describe the acceptance criteria in the specification.
Validation uses an executable and an argument array, without an implicit shell.
For a shell script, explicitly select the shell executable and script arguments.

## Mac-style controls, Assembly Line, and feature chat

The header matches the latest historical Mac three-control layout at
`9c049d6`: **GitHub publication**, **Permissions**, then **Settings**, with
branch, half-filled shield, and gear symbols. The earlier Windows pass used a
project-web link and safeguards summary; those were older or substituted
actions and do not satisfy this exact Mac-controls contract.

The gear configures separate ChatGPT/Codex orchestrator and reviewer
model/reasoning selections, with reload/save, alongside separate local
implementation settings. Both role pickers use ChatGPT/Codex. ChatGPT choices come from the signed-in Codex provider catalog;
only its advertised reasoning levels are selectable for that model. On model
change, preserve a supported reasoning level or select that model's advertised
default. New chat and newly
queued features use the saved selections; existing approved jobs retain their
frozen selections. The shield selects global **Ask for approval**, **Approve
for me**, or **Full access**. Native approval decisions bind to the current
edit or validation request; changing the radio selection is not sufficient
until Save succeeds. The main status area retains pause/cancel/resume controls.

The GitHub panel refreshes/signs in the account, discovers and creates
repositories, creates local projects, and saves/updates/disconnects project
connections. Explicitly connecting a project opts future successful features
into branch/PR publication and automatic merge after required checks. The
native adapter binds publication to the completed run and exact candidate,
records progress and receipts, and requires reconciliation after ambiguous
external outcomes. This extends the local lane; it does not manufacture
distributed activation or release receipts.
Creating a local project makes or observes an empty directory. Explicitly
saving its GitHub connection prepares that selected repository/base checkout
when the directory is empty. After a verified merge, the native adapter
fast-forwards only the exact clean connected checkout to the confirmed merge,
so the next queued feature starts from the latest completed feature. Dirty,
changed, or mismatched checkouts require attention and are never overwritten.

For a retained publication needing attention, select its Assembly Line entry
and choose **Reconcile**. The app verifies the exact completed run, connected
repository, candidate, merged PR and durable receipt before marking the entry
Completed. A connection already advanced to that verified merge is accepted
for recovery. Reconciliation of a completed merge retains its original PR.

The left-side **Assembly Line** stores an ordered queue of explicitly approved
features. **Add to Line** freezes the feature form. **Start Line** runs pending
jobs one at a time through the same native runner. Only an owned, successfully
validated and reviewed completed report allows automatic advancement. Failure,
cancellation, pause, or interruption stops advancement until the owner acts.
The line is durable in `assembly-line.json`; a previous running item becomes
interrupted on restart, and the application never silently resumes it. Pending
capacity is bounded to one hundred jobs. Completed items retain their candidate
and report references. Each feature produces an independent local candidate.
Unconnected jobs preserve their source checkout. Connected
projects additionally complete their authorized GitHub publication before
automatic queue advancement.

The right-side **Chat** uses the selected orchestrator provider to discuss and draft features.
**Use as feature** copies the model's latest draft into the editor. It does not
approve, enqueue, run, or publish the draft. Chat supports cancellation and
bounded local history; Emergency Pause cancels current generation and blocks
new sends. ChatGPT uses the existing Codex ChatGPT sign-in and a bounded
response-only child process; local mode uses the explicit loopback endpoint.
Provider, model and reasoning are explicit, with no silent fallback. The
planning client has no tool or repository-write interface. This
owner-requested chat is the narrow exception to the retired general assistant
surface; plugins, personal memory, scheduling, voice, and trusted wake remain
removed.

## ChatGPT role boundary and legacy jobs

The historical Mac Settings contract at `9c049d6` used ChatGPT/Codex, not the
local implementation endpoint. The Windows implementation now separates all
three responsibilities: the orchestrator prepares a plan or chat draft, local
inference proposes scoped repository edits, and the selected reviewer checks
the frozen candidate and validation results. A plan is supplied text; it cannot
widen allowed files, authorize a command, or change the submitted job.

`model` and `reasoning_effort` continue to identify the local implementation
model. Optional `orchestrator_provider`, `orchestrator_model` and
`orchestrator_reasoning_effort` bind an explicit planning stage;
`reviewer_provider` plus the existing reviewer model/reasoning fields bind final
review. Omitting the orchestrator fields and reviewer provider preserves legacy
local-job behavior. Changing saved Settings never rewrites approved queue jobs.
The example model names above reflect the catalog observed on this host;
choose the advertised model and reasoning pair from your installed provider.

Use the existing ChatGPT sign-in in Codex. Assemblywright does not manage API
keys, copy OAuth tokens or modify global Codex configuration. The catalog is
provider metadata; a successfully completed call proves access for that call.
ChatGPT planning and review disable tools, plugins, inherited rules and
repository access, and retain bounded role-specific artifacts. Authentication,
catalog, unsupported model/reasoning, malformed response and inference errors
surface directly. Cancel, Emergency Pause and normal close stop the provider
process tree. The fixed distributed review adapter remains a separate contract.
Prior local-provider chat messages remain visible in local history; changing
to ChatGPT does not automatically include that local transcript in cloud
request context. New requests use context belonging to the selected provider.

**Activity** retains the pipeline log alongside chat. The feature form is saved
in `gui-job.json` so a normal application restart preserves the current draft.

## Completion contract

The runner captures the repository's committed HEAD into an independent,
remote-free candidate, preserving that exact commit and its reachable object
ancestry without importing source configuration or remotes. Generated commits
therefore descend from the frozen source HEAD and can pass the publication
handoff's ancestry check. It sends the specification and allowed file contents to
local inference, checks the returned edits, applies them, and runs the
owner-selected validation command. Failed validation or rejected review feeds a
bounded repair attempt. Each attempt is committed and frozen before validation,
retaining failed candidates for diagnosis. A successful validation and separate
review are required before a completed report can be recorded.

Confinement compares candidate HEAD with the run's exact frozen source HEAD at
both the pre-validation and final completion checks. Historical changes before
that source commit are outside this feature's diff; new changes to forbidden
paths remain rejected. The copied source commit must resolve in the candidate.

Repairs put the concrete failing diagnostic first in the model request and
repeat the repair priority after the current source context. Exact unchanged
repair output consumes a bounded attempt, preserves that diagnostic, and skips
edit approval and the redundant validation command. It cannot establish
completion. Snapshot creation polls cancellation during iterative object/tree
traversal and serializes writes with owner stop controls; it rejects objects
over 64 MiB, more than 4 GiB cumulative uncompressed data, two million objects
or tree entries, or one hundred thousand commits before unbounded copying.

The Windows llama.cpp adapter gives coding/review requests a ten-second connect
and fifteen-minute total HTTP deadline with 32,768 maximum generated tokens.
Selected reasoning efforts request bounded thinking tokens:

| Effort | Thinking token budget |
| --- | ---: |
| none | 0 |
| minimal | 128 |
| low | 512 |
| medium | 2,048 |
| high | 4,096 |
| xhigh | 8,192 |
| max | 12,288 |
| ultra | 16,384 |

The request still carries the selected effort and thinking toggle, and always
asks for separate DeepSeek-style reasoning fields. This extraction format also
removes the observed Qwen template's empty thinking envelope when effort is
`none`; it does not enable thinking. The exact requested
policy is retained in role/request artifacts; another compatible provider must
support these selected parameters or return its visible error, with no fallback.
The observed local llama.cpp build supports per-request
`reasoning_budget_tokens` and `max_tokens`. These request limits supplement
client-side incremental one-MiB catalog/four-MiB response bounds. Cancel and
Emergency Pause still interrupt HTTP work on the 150-millisecond poll; the
longer generation deadline does not delay owner controls.

Local implementation and local-review requests also ask for `response_format: {"type":"json_object"}` and
record it in the durable role policy. The native parser still requires one
complete JSON result (optionally wrapped in a whole-response Markdown fence).
The observed llama.cpp build can admit residual planning prose before its lazy
JSON grammar trigger when reasoning is enabled; requesting JSON mode alone is
therefore not proof of valid output. Malformed responses consume explicit bounded
attempts and retain their raw artifacts. Selecting `none` for a submitted job
disables thinking explicitly; it is not a hidden fallback or automatic role change.

For an unconnected project, completion means a validated, reviewed local
candidate is available. A saved GitHub connection adds the authorized
publication and guarded connected-checkout synchronization described above.
Unconnected source checkouts are preserved. This lane has no dependency on the master service,
Mac enrollment, signing, mTLS, or the six distributed activation receipts.
Explicitly selected ChatGPT roles require the owner's existing Codex ChatGPT
sign-in; legacy local-only jobs require no cloud credentials.
This lane does not manufacture those receipts or activate the
distributed kernel. Its job/checkpoint format is separate from protocol v5 and
master schema v19.

The validation command is trusted owner-selected code and runs with the owner's
normal Windows account. AppContainer restrictions and remote proof ceremonies
are deliberately excluded from this lane. File scope checks, isolated
candidates, bounded attempts and output, process cancellation, local pause,
state locking, and truthful terminal reports remain necessary for reliable
completion. Local model review is a fresh call, not an independent model or a
guarantee of semantic correctness. Deterministic tests remain the acceptance
gate.

Validation may produce ignored build/cache files; nonignored untracked files,
tracked file mutation, and candidate HEAD drift reject completion. A Windows
Job Object manages the validator's process lifetime, including descendants;
this is process cleanup and cancellation, not an AppContainer sandbox.

Reports and detailed model/validation artifacts are local development evidence.
Treat them as repository content. They are not the redacted distributed event
stream or portable release receipts. A crashed in-progress checkpoint cannot be
reported as completed or silently resumed.

## Why the old path could not finish autonomously

The former implementation built individual durable transitions and live-proof
controllers. The general coding worker executed preconstructed bounded edits;
it did not ask a model to implement a feature. The worker binary required macOS,
and the orchestration checkpoint ledger had no automatic dispatcher. The local
Windows model existed without an adapter connecting it to repository edits.
Those are missing runtime stages, not enrollment settings or intermittent
model errors. This lane supplies a complete vertical execution path before
distributed operation is revisited.

## Validation

```powershell
./scripts/validate-windows-gui.ps1 -LiveDemo
./scripts/build-windows-gui.ps1
./scripts/test-windows-gui.ps1 -NoBuild
./scripts/test-windows-gui.ps1 -NoBuild -LiveDemo
cargo test -p assemblywright-master --test windows_local_pipeline_e2e -- --nocapture
./scripts/validate-windows-local-pipeline.ps1
cargo clippy -p assemblywright-master --all-targets --locked -- -D warnings
git diff --check
```

The native E2E starts the real runner, a loopback inference test server, and a
real validation subprocess. A live `-Demo` additionally proves that the selected
local model produced a working implementation. Fake inference E2E alone cannot
establish that live result. Windows validation is distinct from macOS signing,
notarization, packaged app QA, and distributed publication evidence.

Native GUI E2E uses Windows UI Automation to invoke the real form buttons,
observe streamed logs and completed result controls, cancel a real validation
process, and exercise pause/resume. Its Codex and loopback inference fixtures
are confined to the isolated test bundle. The GUI script's historical
`-LiveDemo` name selects extended fixture scenarios; it does not prove
authenticated ChatGPT or actual local-model inference. Actual provider proof
requires a separately recorded installed-application run with real role
artifacts. GUI logic self-tests cover job validation/serialization, process argument
quoting, bounded logs, and current versus previous report classification.

## Verified local closeout: 2026-10-12

`./scripts/validate-windows-local-pipeline.ps1 -LiveDemo` passed on this Windows
computer. The launcher was also exercised through Windows PowerShell 5.1,
in addition to PowerShell 7.6.5. Qwen generated both implementation files from
the unimplemented source, passed all twelve acceptance tests, and returned
`{"approve":true,"findings":[]}` in its separate review. The final run completed
on attempt one in approximately sixteen seconds.

- Run: `590b2785-8120-4a3b-96cf-47449b6ecd02`.
- Candidate commit: `adf28cf30e6b9172b6e1728704f60e47756d8b94`.
- Retained evidence root:
  `%LOCALAPPDATA%/Assemblywright/local-pipeline/runs/590b2785-8120-4a3b-96cf-47449b6ecd02`.
- `report.json` SHA-256:
  `2edb3bdceafa842c28589cac31f9ae1644ba95b77c7cc9d3032f43001a227b76`.
- `submitted-job.json` SHA-256:
  `e1b7c4f979210ca46d420bb285873c4b34c104909d09ff54a8e1ffc5409256cd`.
- `artifacts/attempt-1-gate.json` SHA-256:
  `7aaeb4c2ad423d40208fc699f05662b5dc565010155cec03a5f782c42a5e5340`.

The original demo source and completed candidate were both clean; the candidate
had no Git remotes and passed `git fsck`. Its CLI independently produced
`assemblywright-works-on-windows` from `Assemblywright works on Windows!`.

Closeout verdicts: documentation and scoped safety contracts updated; durable
facts added to the repository knowledge base; unit workflow covered three
focused tests; native E2E covered thirteen passing scenarios plus seven
intentionally ignored validator helpers invoked by the harness; format,
package-wide protocol/master clippy and tests, docs drift, and whitespace
checks passed. Independent high-risk review approved with no remaining
findings. Publication was not requested; no source-repository commit or push
was made. The new local executable is built and operational through its Windows
launcher. The existing `AssemblywrightMaster` service stayed running with its
existing deployment and state.

The macOS `release-local.sh` attempt stopped at a missing `shasum` prerequisite
in its proof controller; its Mac/Swift/distribution gate is not claimed here.
Elevated SCM lifecycle, signing/notarization, Mac UI, distributed live proof,
source-branch integration, and GitHub publication remain outside this local
completion proof.

## Windows GUI closeout: 2026-10-12

The native GUI is installed at
`%LOCALAPPDATA%/Assemblywright/WindowsApp/Assemblywright.exe`, with Desktop and
Start menu shortcuts named **Assemblywright**. A normal launch displayed the
`Assemblywright for Windows` window. The installed executable exactly matched
the final compiled bundle, SHA-256
`1ac1ab88d3af5eecf14c1ed1887abe429c6aefc3ffadee8d9cc5d49f8f3aacb6`.

`scripts/validate-windows-gui.ps1 -LiveDemo` passed. Native UI Automation proof
is retained at
`target/windows-gui-tests/87a77fadee404e6caafc764b29483f86/gui-e2e.json`:
the real window launched a successful fixture feature, streamed logs, enabled
candidate/report controls, cancelled validation, paused/resumed, cancelled on
active close, and remained open when cancellation failed before recovering to
a cancelled terminal run. Its GUI **Run demo** action used actual Qwen inference
and completed run `be74b40b-3562-43fd-b45c-6f7378c28098`, passing the twelve
feature acceptance tests and separate review on attempt one.

The final validation-only adjustment rejects empty and embedded CR/LF argument
values that the one-argument-per-line editor cannot preserve. The subsequent
`/warnaserror+` rebuild and five-group self-test passed, recorded in
`target/windows-gui/self-test-final-review.json`. The GUI does not silently
rewrite those jobs; use a wrapper executable for such validation arguments.

Closeout verdicts: GUI design and scoped safety docs updated; durable GUI,
launch, native automation, and Windows path facts added to the knowledge base;
unit workflow passed JSON round-trip, argument quoting, validation negatives,
report ownership, and log bounds; native GUI/process E2E and live model proof
passed; the three local runner units and thirteen native runner regression
cases passed; docs drift and whitespace checks passed. Independent high-risk
review approved with no remaining P0-P3 findings. Deployment is the verified
local GUI bundle and shortcuts; the existing master service and database were
unchanged. Publication was not requested, so no source-repository commit,
push, or hosted-gate claim was made. Mac behavior, signing, distribution trust,
source integration, and external publication remain deferred.

## Windows Mac-parity closeout: 2026-10-12

The installed Windows app now has the gear, shield, and GitHub header icons,
the durable Assembly Line on the left, and local feature chat on the right.
The owner's existing editor draft was retained when replacing the prior GUI.
Desktop and Start shortcuts still target the installed app, which was reopened
normally and verified through native UI Automation. Installed SHA-256:
`2e65b9333c6fb51fe54070e44394d254bbb60e48758d80195cc171c3ce942bd6`.
Launch verification is retained at `target/windows-gui/installed-parity.json`.

`scripts/validate-windows-gui.ps1 -LiveDemo` passed for this build. Its
eleven-group unit evidence is `target/windows-gui/gui-unit-tests.json`;
native GUI evidence is
`target/windows-gui-tests/ec40ca72d4cb4a32b57f85f3bdaa164e/gui-e2e.json`.
The real window exercised settings persistence, safeguards, the original
GitHub project-link action, text-only draft transfer, two sequential completed
features, failed/cancelled queue stops, explicit restart, a stop held during
Git preflight that launched no runner, chat cancellation, Emergency Pause,
history recovery, and normal/failed-cancellation close handling.

Actual local Qwen inference answered the chat request and completed demo run
`594950c2-baa6-4370-80eb-7ac126ecf6be` on attempt one, passing all twelve
acceptance tests and a separate model review. Its candidate commit is
`33f7278ce57317aea5c365d2fb82865a1b65c442`; evidence is retained under the
native GUI test root's `live state/runs` directory. The two queued fixture
features completed as separate runs `e3465f71-2f00-4ac8-9c0a-6e0f548124cf`
and `5d2a006f-e707-4ffb-8aaa-5dab24959c01`.

Closeout verdicts: documentation and the owner-requested scoped safety
exception are consistent; durable parity, run-ownership, launch, and automation
facts were added to the knowledge base. The unit-testing workflow covered
eleven focused groups, including actual child-process cleanup. The E2E workflow
used native UI Automation, real Git/process boundaries, thirteen native runner
regression cases, and actual local-model chat/implementation. Windows build,
docs drift, workflow validation, and whitespace checks passed. Independent
high-risk review approved after resolving ownership, start reentrancy,
preflight stop, and close lifecycle findings. Deployment is the verified local
GUI bundle and shortcuts; the distributed master service was unchanged.
Publication was not requested: no source-repository commit, push, or hosted-gate
claim was made. Mac/Swift, signing, remote control, source-branch integration,
and GitHub publication remain outside this local parity proof.

## Exact Mac controls closeout: 2026-10-12

The final Windows header follows the latest historical three-icon Mac UI,
`9c049d6`: **GitHub publication**, **Permissions**, **Settings**, in that order.
Native vector equivalents use the Mac symbols `arrow.triangle.branch`,
`shield.lefthalf.filled`, and `gearshape`. Their actions now open the matching
GitHub account/repository/connection controls, global Ask/Auto/Full permissions,
and separate orchestrator/reviewer model and reasoning settings. This supersedes
the earlier project-web-link and safeguards-summary interpretation above.

The tested four-file bundle was installed and reopened normally through the
existing Desktop/Start shortcuts. Native UI Automation verified the installed
header order and all three panels, including real account refresh displaying
`Signed in as malak333`. The owner's editor content was preserved; the prior
empty draft's zero attempt/timeout placeholders were normalized to defaults
during the old app's normal close. Launch evidence and runtime hashes are
`target/windows-gui/installed-exact-controls.json`; the native-rendered window
image is `target/windows-gui/installed-exact-controls.png`.
Installed `Assemblywright.exe` SHA-256:
`d162e44c40f290c398171886b873fc32574e6ffa53f439b3886302ddcd2d5f23`.

`scripts/validate-windows-gui.ps1 -LiveDemo` passed for this exact bundle.
Fourteen GUI self-test groups are recorded at
`target/windows-gui/gui-unit-tests.json`; native UIA evidence is
`target/windows-gui-tests/a7c96e414f09434cba8cb3c4880183ce/gui-e2e.json`.
The real GUI exercised separate model/effort saves, native Ask approvals and
rejection, publication success/attention/wrong-run/cancellation, queue FIFO,
failure/cancel/restart, stopped Git preflight with no runner launch, chat
stop/pause/history, streamed results, and normal/failed-cancellation close.
An isolated GitHub fixture replaces only the test bundle's adapter; the
installed bundle contains the production native adapter.

Actual local Qwen answered the chat request and completed demo run
`98055b3c-098e-4067-ac23-4699be37a749` on attempt one. All twelve acceptance tests
and the separate model review passed. Candidate commit:
`a31e0b910287cbc243bf9dbe01145a91b1f854eb`. Evidence remains in the test root's
`live state/runs/98055b3c-098e-4067-ac23-4699be37a749` directory.

Native tests passed six runner units, three GitHub units, twenty-three runner
E2E cases, and thirteen GitHub E2E cases. Eight ignored validator helpers were
invoked by their parent scenarios. Coverage includes approval corruption and
cancel ordering, advertised model reasoning contracts, connection drift,
publication identity/receipts/reconciliation, child-process cancellation, and
two successive connected publications advancing the clean source. Publication
uses real Git and native process boundaries with fixture GitHub responses;
no real repository creation, push, PR, or merge was performed during this
closeout. Account status was verified against the real signed-in account.

Closeout verdicts: accepted design, documentation, and scoped safety contracts
are consistent; reusable role, permission, approval, publication, source-update,
and native-test facts were added to the repository knowledge base. The
`unit-testing-test-generate` and `e2e-testing` workflows used native Rust,
WinForms UI Automation, Git/process boundaries, and actual local inference;
there is no browser surface. The Windows gate, warnings-as-errors build, targeted
clippy, format, workflow, docs drift, and whitespace checks passed. Independent
high-risk reviews approved the GUI, runner permissions/roles, and GitHub adapter
with no remaining blocking findings. Deployment is the verified local GUI and
runtime bundle; the distributed master service remains unchanged. Source
publication was not requested: no commit, push, or hosted-gate claim was made.
Mac/Swift execution, signing/notarization, remote control, and real remote GitHub
publication remain separate unexecuted proof boundaries.

## Real website workflow closeout: 2026-10-12

The owner requested a Final Fantasy Tactics demo in its own repository to test
the workflow. `C:/Users/mike/Codex/Final-Fantasy-Tactics-Demo` is a separate local
Git repository. Codex prepared the specification, package manifest, initial
README, and independent acceptance harness; actual local Qwen generated the
HTML, CSS, and JavaScript through the visible Windows Assembly Line. Initial
run `1ca6fdc1-b0f2-4487-a52d-3caad4fc2ab3` completed on attempt four after
automatically repairing missing controls and rendering checks. Its candidate
`3ffa7479d6aaef5f5605600628b4faaca5d2d5f8` passed all 24 browser check groups
and a fresh review call.

The final installed-app run `23e06212-7a67-4e50-8ebe-d908a135fcbf` completed on
attempt five. It corrected supplied official character/job facts, added usable
launch instructions, and automatically repaired the remaining unsupported
Archer claim. The content assertions and all 24 Chromium/Firefox/WebKit checks
passed before the separate review approved. Both roles used the same local
`qwen3.6-35b-a3b` in fresh calls; this is not agreement between independent
models. This submitted job explicitly selected effort `none` for both roles.

The exact completed candidate was verified against the clean unchanged source,
frozen tests and CSS, source ancestry, gate, review, and owned GUI queue entry,
then locally fast-forwarded into the demo repository. Final clean HEAD is
`c1e316f8fc01e091f2338bfadf5b4f103870736e`. The owner acceptance-test blob is
`82370a62b5e579b4349eecb16e7b849575ef9df5`; models could not edit it. The
owner's screenshot harness settles pointer/repaint state while retaining all
functional checks and strict paired image equality. Normal `npm install` and
`npm test` also passed in the integrated repo with its own dependencies.
The loopback preview serves the final site at `http://127.0.0.1:4173`.

This live exercise exposed real gaps beyond the earlier fixtures: candidate
ancestry, confinement against the ancient root, repair diagnostics, unlimited
thinking/short HTTP deadlines, and empty thinking envelopes in non-thinking
responses. The corrections preserve strict parsing, file scope, source isolation,
owner controls, and truthful failure records. A real A/B probe established the
response-extraction fix; no server configuration or model choice was changed
automatically. Historical failed and cancelled runs remain available.

Evidence is retained in `target/fft-demo/final-site-integration.json`,
`final-browser-evidence`, `final-integrated-browser-evidence`,
`no-thinking-reasoning-format-probe.json`, `final-runtime-launch.json`, and
`final-installed-runtime.json`, plus `final-ui-completion.json` and the installed state directory's run
artifacts. The verified four-file production bundle was installed and its
Desktop/Start shortcuts checked. GUI SHA-256 is
`101bfc6fcd647bea515bfa2519cb6ee47982ea7d531c0e92cf6ad5e370189ab4`;
native runner SHA-256 is
`153834aac9c123c01df08bb3badcffb3680e9064f3d58159228a219fa9e0f697`.

Closeout verdicts: documentation and scoped safety contracts are consistent;
durable source-binding, repair, inference-format, HTTP-bound and browser-harness
facts were added to the knowledge base. The unit-testing workflow passed fourteen
GUI groups, seven runner units and three GitHub units. The E2E workflow passed
thirty runner and fourteen GitHub native cases, native WinForms UI Automation,
actual installed local-model implementation/review, and the generated website's
24 browser groups. Eight ignored validator helpers were invoked by parent tests.
The final Windows GUI gate, format, targeted clippy with warnings denied, docs
drift and whitespace checks passed. Gate evidence is
`target/fft-demo/final-windows-gate-2.log`; GUI E2E is
`target/windows-gui-tests/9f14ee9e06e4461582ecd215ffeb4767/gui-e2e.json`.
Independent high-risk review approved the native fixes with no P0-P3 findings,
and independently approved the owner harness stabilization. Deployment is the
installed local bundle and running preview; the distributed master service was
unchanged. No Assemblywright source commit/push or real GitHub repository,
PR, merge, hosted-gate, Mac/Swift, signing, notarization or remote-control claim
is made. The applicable Windows gate replaces the Mac-only release gate for
this host and scope.

## Real private GitHub workflow closeout: 2026-10-13

The owner explicitly authorized a private GitHub repository and publication.
The native GitHub adapter created
[`malak333/Final-Fantasy-Tactics-Demo`](https://github.com/malak333/Final-Fantasy-Tactics-Demo).
The separate Windows checkout remains
`C:/Users/mike/Codex/Final-Fantasy-Tactics-Demo`. GitHub enforces the required
**Browser acceptance** context with strict checks and administrator enforcement;
the repository remains private.

Actual local Qwen run `764f7cbd-77f3-4b95-a971-2fa65bcf6336` completed its
keyboard/documentation/workflow correction on attempt one. Candidate
`36d8c651d5755856a8e41f3259d5d7ec4a3b44e8` passed actionlint, the unchanged
24 browser groups, and a fresh review before the native pipeline created and
merged [PR #1](https://github.com/malak333/Final-Fantasy-Tactics-Demo/pull/1).
Required PR run `38413763054` and post-merge run `38414113355` both passed.
The native adapter automatically fast-forwarded the clean local source to
`f17b871b44521451cfb206bfb3d1a6ababa679de`.

That first native merge exposed a GUI-only confirmation bug: legacy .NET path
handling rejected Rust's canonical Windows verbatim path. The corrected GUI
uses bounded no-throw normalization at comparison/display boundaries while
retaining all exact publication and receipt checks. The installed app's
**Reconcile** control recovered the original queue entry to Completed/succeeded.
Direct remote reads verified the same sole PR, candidate, base and merge, with
no duplicate PR and unchanged source parity. The visible status timer displays
the stable **Completed** result after its brief reconciliation message.

A fresh docs-only feature then proved automatic completion with the corrected
installed app. **Add to Line** and **Start Line** launched
`7b7d3b69-dcdc-483e-8421-ecc6fb08057f` from the previous verified merge and
connection revision 10. Actual local Qwen created only `docs/workflow-proof.md`;
candidate `39f2682a545c80252ad3a93c0953bf1874744ea8` passed owner validation
and a fresh same-model review on attempt one. Assemblywright created
[PR #2](https://github.com/malak333/Final-Fantasy-Tactics-Demo/pull/2), waited
for its required check, merged, updated the clean local checkout, and marked
the exact owned GUI queue entry Completed/succeeded automatically. No
reconciliation or operator merge/integration was used for this second feature.

The exact final merge is `7058a3412e0da7951371dec809c00d5a16a011bc`.
Local `HEAD`, `origin/main`, and GitHub's `main` all equal that commit. Required
PR run [`38420497412`](https://github.com/malak333/Final-Fantasy-Tactics-Demo/actions/runs/38420497412)
and final push run [`38420690745`](https://github.com/malak333/Final-Fantasy-Tactics-Demo/actions/runs/38420690745)
passed; their retained actual Linux logs each show all 24 browser groups.
The native durable receipt is verified at revision 21. The unchanged owner
test blob remains `82370a62b5e579b4349eecb16e7b849575ef9df5`. The stricter
final proof validator and all 24 Chromium/Firefox/WebKit groups also passed
in the integrated Windows repo. Both model roles used local
`qwen3.6-35b-a3b` in fresh calls with explicitly selected effort `none`;
this does not establish agreement between independent models.

Retained evidence includes `target/fft-demo/github-final-closeout-proof.json`,
`github-closeout-ui-completion.json`, `github-closeout-durable-status.json`,
`github-real-gui-reconciliation.json`, `github-recovery-remote-proof.json`,
`github-final-integrated-validation.log`, browser screenshots/traces, and
actual hosted logs `github-hosted-38420497412.log` and
`github-hosted-38420690745.log`. The first failed creation, invalid CI context,
earlier failed/cancelled features and GUI attention history remain retained.

The reviewed, warning-as-error-built, self-tested and native-UIA-tested
four-file bundle was installed and each file digest verified. GUI SHA-256 is
`39f588e49be53915e50c63ef1bebd4a0e04be4221fddfc596d5f0afcc391af9c`;
GitHub adapter SHA-256 is
`6c562e8b105afb0baf29d453e85f4c03f0c3cee10b8449e5224bf8447ec32094`.
All four hashes are in `github-installed-runtime-verified.json`. Desktop and
Start shortcuts launch this bundle. The GUI remains open with the completed
feature, and the website preview remains at `http://127.0.0.1:4173`.

Closeout verdicts: documentation, accepted design and scoped safety contracts
are consistent; reusable real-CLI, CI-context, path-format and receipt-recovery
facts were added to the repository knowledge base. The unit-testing workflow
passed fourteen GUI groups, seven runner units and six GitHub units. The E2E
workflow passed thirty runner and twenty GitHub native cases, the full WinForms
UI Automation matrix including verbatim publication/reconciliation, actual
installed local inference/publication, and 24 browser groups locally and in
both hosted gates. Eight ignored validator helpers were invoked by parent
scenarios. High-risk independent review approved the native and GUI fixes and
the publication proof guards. The applicable Windows gates, targeted clippy
with warnings denied, format and documentation/whitespace checks passed.
Publication is the private demo and its docs-only closeout, with exact final
hosted-gate and authoritative Windows source parity. Assemblywright's dirty
application-source worktree remains uncommitted/unpushed. Deployment is the
verified portable local bundle and loopback preview; the distributed master
service was unchanged. Mac/Swift execution, remote control, signing,
notarization, public hosting and production readiness remain unexecuted proof
boundaries. The native Windows gate is applicable to this host; the Mac
distribution gate was not claimed.

## ChatGPT role parity and live closeout: 2026-10-13

The owner corrected the Windows Settings contract to match the historical Mac
app's ChatGPT/Codex orchestrator and reviewer selectors. The installed GUI now
loads the signed-in account's model catalog, displays provider model names,
saves model IDs and advertised reasoning values independently for each role,
and keeps the loopback implementation settings separate. The real saved and
reloaded selections were:

| Responsibility | Provider/model | Reasoning |
| --- | --- | --- |
| Planning and right-pane chat | ChatGPT `gpt-5.6-sol` | `medium` |
| Scoped implementation | Local `qwen3.6-35b-a3b` | `none` |
| Candidate and validation review | ChatGPT `gpt-5.6-terra` | `high` |

The actual installed GUI completed a ChatGPT planning chat using the selected
Sol pair. A second chat was stopped while its real Codex `exec` child was
active: the native foreground runner, contained launcher, console host and
Codex child all exited, no assistant completion was appended, and queue/source
state remained unchanged. The five pre-upgrade frozen queue records were
preserved exactly. Existing local chat stays visible without being copied into
cloud prompts. Both cloud roles remain response-only and tool-disabled; the
existing ChatGPT sign-in supplies authentication without credential copying or
global Codex configuration changes.

The first live documentation proof exposed a parent harness defect: its broad
keyword blacklist rejected a sentence explicitly denying production readiness.
Run `423de69f-b776-44b3-9f58-ec49dc1a853d` truthfully exhausted three attempts
and never reached review or publication. Its failed candidate and diagnostics
remain retained. After it was terminal, the parent corrected only that harness
assertion, verified three truthful descriptions and eight unsupported-claim
rejection cases, and approved a fresh immutable GUI job. The owner browser
suite remained byte-identical throughout.

**Add to line** and **Start line** then launched real run
`1a0b0e51-151e-4f62-87b2-f8d930aa7f57`, queue entry
`a55e6f90-63cc-46e4-b282-efc88db5df87`, from verified source
`7058a3412e0da7951371dec809c00d5a16a011bc`. ChatGPT Sol produced bounded
planning text, local Qwen alone edited `README.md` and
`docs/windows-chatgpt-workflow.md`, and candidate
`b8838356937d86a66b2f0c7b442195d21deb248a` passed the documentation guard
plus all 24 browser groups on attempt one. Actual ChatGPT Terra/high reviewed
that candidate and validation evidence and returned
`{"approve":true,"findings":[]}`. No provider/model fallback occurred.

Assemblywright automatically created and merged
[PR #3](https://github.com/malak333/Final-Fantasy-Tactics-Demo/pull/3), waited
for the required check, updated the clean connected checkout, and marked the
exact queue entry **Completed/succeeded**. No operator source edit, commit,
merge, reconciliation or integration was used for this successful feature.
Verified receipt revision 31 binds the reviewed candidate to merge
`952e230667cc3b06ef5d2114cd0a21000f47bce4`. Local `HEAD`, `origin/main`,
and the directly queried GitHub `main` equal that merge. Its tree equals the
reviewed candidate tree; only the two allowed documentation files changed.

Required PR run
[`38431477924`](https://github.com/malak333/Final-Fantasy-Tactics-Demo/actions/runs/38431477924)
and post-merge run
[`38431779509`](https://github.com/malak333/Final-Fantasy-Tactics-Demo/actions/runs/38431779509)
passed, and their retained actual Linux logs each contain all 24 browser
groups. The integrated Windows merge also passed the corrected documentation
validator and the 24 Chromium/Firefox/WebKit groups. The owner test blob remains
`82370a62b5e579b4349eecb16e7b849575ef9df5`; the repository remains private.

The installed production bundle has the default-off fixture feature disabled.
Its GUI bytes are exactly the native-UIA-tested executable; the bundled native
runner is the default-feature build used for authenticated live proof:

| Installed file | SHA-256 |
| --- | --- |
| `Assemblywright.exe` | `55225a129880bf6daf4e3e71ccd980b0ee094629d3e34475e5fabb0d114dd446` |
| `assemblywright-local.exe` | `fc1aabe85cbbeae89eb845c14fef67fe09fc5206b496ad9232ce3e1e18eb44d7` |
| `assemblywright-github.exe` | `40f29ca30e73cf306ca43271a6e8ec240fd0ee0022cb66c7a9d1d46cd5397c7b` |
| `windows-local-pipeline.ps1` | `fca43cbd40d05e8c597861d0c3ee663b1c5fa443a52ebcd9bd3d82bdbf67e793` |

The actual resolved desktop Codex runtime SHA-256 is
`d13914ced6c7af174d8d638db938284231227312104b06a25e9608c632a7daec`.
The native adapter rechecks that path/digest for its catalog and role calls.
The installed GUI remains open with the corrected Settings panel, and no
provider/publication children survived completion.

Retained evidence includes `target/chatgpt-roles/final-live-workflow-proof.json`,
`final-durable-status.json`, `installed-runtime.json`,
`installed-settings-proof.json`, `installed-settings-final-proof.json`,
`installed-chatgpt-settings.png`, `installed-chat-proof.json`,
`installed-chat-cancel-proof.json`, `installed-pipeline-ui-completion.json`,
`demo-validator-correction-proof.json`, `integrated-validation.log`, both
`hosted-*.log` files, and candidate/integrated screenshots and browser traces.
The live role requests/responses, prompt digests, runtime identity, submitted
job, source HEAD and gate artifacts are retained under that run's state folder.

Closeout verdicts: accepted design, documentation and scoped safety contracts
are consistent; reusable role-separation, authenticated-catalog, history,
runtime-binding, fixture-boundary and harness-negation facts were added to the
repository knowledge base. The unit-testing workflow passed fourteen GUI
groups, eleven local-runner units and six GitHub units. Native E2E passed thirty
default-feature runner cases, twenty GitHub cases and seven isolated ChatGPT
fixture cases; eight ignored validation helpers were invoked by their parent
scenarios. The full WinForms UI Automation matrix passed, explicitly labeled
fixture-only. Separate actual installed GUI chat, cancellation, three-role
execution and publication proof passed, as did the browser matrix locally and
in both hosted gates. Focused clippy with warnings denied, formatting, Codex
workflow validation, documentation drift and whitespace checks passed.
Independent high-risk code and final live review approved without findings.
Publication is the private demo; Assemblywright's unrelated dirty source
worktree remains uncommitted/unpushed. Deployment is the digest-verified Windows
portable bundle; the distributed master service was unchanged. Mac/Swift
execution, remote control, signing, notarization, public hosting and general
production readiness remain outside this proof.

## Publication preparation and stronger Windows gate: 2026-10-13

The owner made the complete documentation/knowledge/unit/E2E/review/publication
checklist a standing instruction for every authorized feature or phase and
explicitly requested pushing and merging this application change. The rule is
persisted in `AGENTS.md` and `docs/development-agent-workflow.md`; the coverage
map is in `docs/windows-local-testing.md`.

The publication branch was isolated from the older dirty checkout and ported
onto current remote base `665973af70ada0677220defd86a3062ae6a9d41a`. The
newer Mac Developer implementation and documentation were preserved. Unrelated
local certificate-rotation edits in `identity.rs`, `main.rs` and
`enrollment_identity_e2e.rs` were excluded entirely.

The hosted audit identified missing WinForms and default-off ChatGPT test
coverage. `.github/workflows/windows-local.yml` now runs the seven explicit
ChatGPT fixture cases in an isolated target, then the production-feature
WinForms build, fourteen self-test groups, full native UIA and native runner/
GitHub regression gate. Pinned actions, read-only job permissions and bounded
artifacts are enforced by CI/docs contract smoke tests. The existing Windows
distributed and macOS release workflows remain intact.

The stronger native gate exposed transient Windows status-file replacement
failures. GUI reads now permit delete sharing and enforce strict bounded UTF-8.
Native atomic replacement retries only Windows access-denied/sharing failures
for twelve attempts with ten-millisecond gaps; persistent failures preserve
old authority and new temporary bytes. A real Windows-handle regression test
checks both transient recovery and fail-closed persistent locking. The existing
C# provider fixture is retained because its non-ChatGPT commands actually
forward to the real Rust runner.

Local publication validation passed: fourteen GUI groups, twelve local-runner
units, six GitHub units, thirty pipeline E2E cases, twenty GitHub E2E cases,
seven isolated ChatGPT cases and the extended native UIA journey
`dffd20a530514f5b8e2e73a8e95dc968`. The parent independently repeated the
native unit/E2E suites and formatting, documentation, Codex-workflow and
whitespace checks. Actionlint and CI contract smoke passed, and independent
high-risk review approved without blocking or material findings.

The reviewed production-feature bundle was installed and all four copied
digests verified. GUI SHA-256 is
`77fe5f07ee42abee2230fa6b39c8a505ef1d273f37294c11af43e6c576068114`;
native runner SHA-256 is
`5175e90e16516065e6edc59fea93fff80eb00602722d5347a90f30058a94fc3f`;
GitHub adapter SHA-256 is
`c6af9031a8904c0be031f0bec949122d5183315ea399cea11bbdd4329f8b9061`.
The launcher digest remains
`fca43cbd40d05e8c597861d0c3ee663b1c5fa443a52ebcd9bd3d82bdbf67e793`.
The older GUI closed normally while idle; state and prior jobs were preserved.

Fresh actual installed run `cb15529f-23e6-4aab-894b-7068bdddf7ab` froze
ChatGPT Sol/medium planning, local Qwen/none implementation and ChatGPT
Terra/high review. Candidate `1aa02e8cddeea9da9ca6a5bcbd00cd9b4d2ad438`
passed its future phase-checklist validator and all 24 unchanged browser groups
on attempt one, and real Terra/high returned `{"approve":true,"findings":[]}`.
It automatically created private demo
[PR #4](https://github.com/malak333/Final-Fantasy-Tactics-Demo/pull/4).
At this preparation checkpoint its hosted check/merge and the application PR's
hosted checks/merge are pending; this paragraph is not publication completion
evidence. The retained records are under `target/publication` in the managed
publication checkout. The installed local lane does not change or deploy the
distributed master service.

## Hosted validation corrections and retained publication recovery: 2026-10-13

The first application PR checkpoint (`95b990da37701289c5815bb49a5bd77524b1d93f`)
exposed two hosted gaps. The macOS warnings-as-errors gate rejected a Windows-only
process-cleanup result binding; the termination call is now retained on both
platforms with the binding restricted to Windows. The extended Windows demo
test used hard-coded endpoint/model defaults and happened to reach this
workstation's live local server. That earlier extended pass did not prove
isolated inference. The demo now takes a validated frozen snapshot of saved
role settings, and its native test explicitly selects an ephemeral endpoint
and verifies all twelve unchanged slugify cases.

The real private-site run above stopped publication on an explicit HTTP 429
while its required browser check succeeded. The adapter now polls routine
checks every fifteen seconds. Only explicit read-only required-check HTTP 429
responses retry, using a valid integer `Retry-After` or sixty-second exponential
fallback, at most four retries across the entire check wait. Pending responses
do not reset that allowance. The existing twenty-minute deadline and responsive
cancel/pause checks remain enforced; effectful operations and unknown or failed
checks are not automatically retried. New native regression scenarios cover
successful retry, repeated/alternating throttling, non-429 failures and stop
during backoff. Eight GitHub units and twenty-five native E2E cases passed in
the independent parent rerun, and independent high-risk review approved.

The reviewed adapter installed for recovery had SHA-256
`69fe7f96138f4e39d9c76f613c0f8b80c45e530ec80877c1ccc4a0998283bb8c`.
The first GUI Reconcile attempt reported an empty-output operation failure
before a new native checkpoint. An explicit native reconciliation then resumed
the exact retained PR and merged it; the GUI's subsequent reconciliation
validated its durable receipt and marked the same queue entry Completed.
Keep those interventions distinct from uninterrupted automatic completion.
Private demo [PR #4](https://github.com/malak333/Final-Fantasy-Tactics-Demo/pull/4)
merged as `7afd2f3f5a16694379e15c27ffbc69b898fcc285`, with completion receipt
revision 44. The clean demo `HEAD` and `origin/main` equal that merge. Required
PR run `38437905346` and post-merge run `38441405139` each passed all 24 unchanged
browser groups. Recovery evidence, native stdout/stderr and actual hosted logs
are retained under `target/publication`. A fresh run on the final installed
bundle is required to establish automatic completion after these corrections.

The corrected full native UIA journey `5f887284abdd4eb182667109ca93425f`
passed with explicit fixture-only evidence. Demo run
`e34fa1b8-6aa1-4684-9b4c-195d188e0998` submitted endpoint
`http://127.0.0.1:52586/v1`, local `gui-fixture/low`, ChatGPT
`gpt-plan/high` and `gpt-review/xhigh`, two attempts and a 47-second timeout,
then passed the twelve unchanged slugify tests. The fixture's nondefault
attempt/timeout settings were seeded; endpoint/model/role changes were made
through the actual Settings controls and saved before submission. Python's
normal unittest stderr is captured through redirected process files instead
of being mistaken for a PowerShell terminating error. Fourteen GUI self-test
groups passed, including a complete demo-settings round trip and invalid
endpoint rejection. The fast-exit process regression passed 320 immediate
children; its production helper hash matches the reviewed GUI source.

The reviewed production-feature bundle at this checkpoint was installed with prior runtime
bytes backed up and all four copied digests verified. It preserves saved roles
and all historical queue entries. The exact GUI bytes passed the full native
UIA journey and include the reviewed captured-process race repair:

| Installed file | SHA-256 |
| --- | --- |
| `Assemblywright.exe` | `f6c4d697b0f8bd0932131b582e299ade1488f58bdbc47ee3a9ba50d426c9fb0d` |
| `assemblywright-local.exe` | `df73b5aa0b24519a9213aaf3f298ccf75805e89b5be02f72fe8f675c9095242d` |
| `assemblywright-github.exe` | `69fe7f96138f4e39d9c76f613c0f8b80c45e530ec80877c1ccc4a0998283bb8c` |
| `windows-local-pipeline.ps1` | `5d9165a729f97e46a1fe16c14d8ceb4eb43847d3c6b9a1dd509cee2c3d10a817` |

Independent high-risk review approved the demo snapshot, throttling and
captured-process changes. The updated GUI opened as PID 12160 and a fresh
owner-approved feature was started through its Assembly Line. Completion and
source publication are assessed separately below; installation alone is not
automatic-pipeline proof.

Fresh run `fcad8634-bb25-4b46-8adb-b39377ba2c39` did not complete. Its first
attempt generated a valid operator guide, but the parent-authored validator
launcher relied on inherited Playwright resolution that the relaunched GUI
did not have. The unchanged acceptance suite provides `FFT_PLAYWRIGHT_MODULE`;
the corrected launcher resolves the pinned module explicitly, and its
standalone rerun passed all 24 groups. This does not rewrite the failed native
report. Later repair responses used a relative Windows backslash path and the
runner rejected that representation, then exhausted its three attempts. The
clean source remained at PR #4's merge and no new PR was created. The native
path correction keeps exact canonical allowlist authority and rejects rooted,
drive/UNC, traversal and duplicate aliases. The failed artifacts are retained;
only a subsequent actual native run can establish completion.

The Windows-only model-response separator correction passed thirteen local
units and thirty-one default native pipeline cases. Nine ignored child
validators were invoked by their parent cases. Unsafe rooted, drive, traversal
and duplicate-alias proposals failed without effects before a relative
backslash proposal completed through the real validator; raw response evidence
and clean source confinement were checked. The parent independently repeated
those suites and all seven isolated ChatGPT cases. Independent high-risk review
approved. The installed native runner was then updated to SHA-256
`c5c23e00a009f6cdc3a59b97a4b3dfb2b5dd957a5f873085c20af6c7aeae1a03`;
the other three tested bundle digests above remain current. Saved settings were
preserved, including the intervening saved reviewer selection; the retained
feature still uses its earlier frozen role pairs.

The existing Windows distributed hosted gate also exposed a recovery-test
sampling race. Its Python fixture accepted an earlier partial attention/idle
state and then asserted abandonment eligibility. The corrected wait requires
the full authoritative projection and exact expected stage before asserting.
The analogous pre-effect race reproduced locally; bounded transition/snapshot/
log diagnostics were added. All negative assertions and the original
sixty-second timeout remain, and no production distributed code changed.
Four worker reruns and the independent parent rerun passed all seventeen native
publication assertions with live GitHub credentials disabled. The retained
failed fixture and passing logs are under `target/publication`.

## Final installed automatic run and source publication: 2026-10-13

After the reviewed runner update and explicit validator dependency correction,
the retained owner-approved feature was retried and Start line selected once.
New actual run `bccd9049-9b3b-458a-bae9-1a45a3da24a8` completed on attempt
one with zero operator recovery actions after Start. Its frozen roles were
ChatGPT `gpt-5.6-sol/medium`, local `qwen3.6-35b-a3b/none`, and ChatGPT
`gpt-5.6-terra/high`. Qwen generated only `docs/windows-run-and-recovery.md`;
the guide validator and all 24 unchanged browser groups passed, and the actual
review response was `{"approve":true,"findings":[]}`.

The application automatically published candidate
`b61b4c9eacd19290a91eef69b3b126545eb07125` through private demo
[PR #5](https://github.com/malak333/Final-Fantasy-Tactics-Demo/pull/5), waited
for its required check, merged it as
`c368438d45bf152032f1fa797fed442c55b77b38`, verified completion receipt
revision 54, and updated the clean connected checkout. The native status is
`succeeded/complete`, the GUI queue entry is Completed, and local `HEAD`,
`origin/main` and the independently read remote main SHA all equal that merge.
The owner acceptance blob remains
`82370a62b5e579b4349eecb16e7b849575ef9df5`. Required PR run `38445311201`
and post-merge run `38445547790` succeeded; their actual retained Linux logs
contain all 24 browser groups. This successful new run does not erase the
earlier interrupted publication or failed harness run.

The installed GUI remains open as PID 46796. Its exact tested GUI digest is
`f6c4d697b0f8bd0932131b582e299ade1488f58bdbc47ee3a9ba50d426c9fb0d`,
and the default-feature native runner digest is
`c5c23e00a009f6cdc3a59b97a4b3dfb2b5dd957a5f873085c20af6c7aeae1a03`.
The GitHub adapter and launcher match the other current digests above. The
actual Codex runtime digest remains
`d13914ced6c7af174d8d638db938284231227312104b06a25e9608c632a7daec`.
Native job, plan, implementation, gate, review and runtime artifacts remain
under the run's state folder. Curated installed, start, durable receipt, role,
source parity and hosted-log proof is under `target/publication`, including
`final-live-workflow-proof.json` and `final-installed-runtime.json`.

Closeout verdicts: applicable accepted docs and scoped safety rules are
implemented; reusable Windows path, dependency, capture, throttling and recovery
facts were added to the knowledge base. The unit workflow passed fourteen GUI
groups, thirteen local units and eight GitHub units. Native E2E passed thirty-one
pipeline cases, twenty-five GitHub cases, seven explicit isolated ChatGPT cases,
the full WinForms UIA journey, the 320-child capture regression and seventeen
distributed publication assertions. Actual authenticated three-role execution
and automatic publication passed on the installed production bundle; candidate,
required PR and post-merge browser suites each passed 24 groups. Independent
high-risk reviews approved every correction. Formatting, focused clippy,
docs/CI/Codex workflow, actionlint and whitespace checks passed.

Application source publication is tracked by
[PR #444](https://github.com/malak333/Assemblywright/pull/444). Its normal
merge must wait for the applicable Windows hosted gates on the exact final
branch head; this native/demo completion record does not substitute for them.
The managed publication checkout preserves the newer Mac code and excludes the
original dirty checkout's unrelated certificate-rotation edits. The deployed
Windows local lane is the verified portable bundle; the distributed master
service was unchanged. Mac/remote live QA, signing, notarization and public
website hosting remain separate evidence boundaries.

The reviewed source corrections are committed as
`a890e7aa208d54e69a32a35dea1735b2fe02e3ee`. This subsequent documentation-only
closeout records the actual deployment and live proof before publishing the
final branch head. The installed runtime inputs are unchanged from those
reviewed source bytes.

### Final hosted portability and fixture-startup corrections

The next macOS clippy run exposed the same Windows-only result binding in the
GitHub adapter's cleanup function. It now binds that result only on Windows and
still calls termination exactly once on other platforms, without suppressing
warnings. All three local modules were checked for the pattern. Windows behavior
is unchanged; eight GitHub units, twenty-five native E2E cases, focused clippy,
formatting and the default build passed again. The rebuilt adapter SHA-256 is
`b6d87dd81c8eeb75bb92209f3bcc5b0da9cf90e45b9f27f443420ab35fac70c7`.
It was installed while idle and independently verified the actual run's
succeeded status and receipt revision 54. The other three installed digests are
unchanged. The successful full automatic run above used the earlier adapter
digest; its retained record is not relabeled as a new execution.

The hosted GUI fixture also exposed an empty-file startup race: file existence
preceded Python's port write, so the test called Trim on null content. The
fixture now publishes its complete ASCII port through same-directory atomic
replacement. The consumer waits for a parsed port in 1..65535, detects early
fixture exit and keeps the original thirty-second bound. No product behavior
or acceptance assertion was removed. Both small corrections passed independent
high-risk review.

The full GUI journey then passed under PowerShell 7, including all twelve
packaged-demo acceptance tests. That rerun exposed a test-host path difference:
PowerShell 7 treats the `?` in Rust's canonical `\\?\` path as a wildcard in
Start-Process WorkingDirectory. Validation now derives an ordinary path from
the retained run, checks the report against it, and uses that ordinary path for
the child. Existing 8.3 aliases are expanded before comparison because hosted
TEMP can use `RUNNER~1` while Rust reports `runneradmin`. A native probe proved
short/long alias equality and rejection of a different existing directory. The
exact candidate assertion, all acceptance tests and production code are intact;
the path correction also passed independent high-risk review.

The final full Windows PowerShell UIA journey passed as well, retaining
`target/windows-gui-tests/1affb4ab87ef4cf3abde036eba1a246d/gui-e2e.json` and
all twelve actual demo acceptance tests. The native 8.3 probe is retained as
`target/publication/gui-long-path-proof.json`; this installation's TEMP itself
has no short alias, so that alias proof uses the existing repository path.

The next distributed hosted gate passed its corrected native Developer workflow
in 1,217 seconds, then reached additional passing master cases before GitHub
cancelled the whole job at thirty-five minutes. No test failure preceded that
outer deadline. Its complete log is retained as
`target/publication/windows-distributed-timeout.log`. The job now has a bounded
sixty-minute budget, pinned by the CI contract; every test command, fixture
deadline and failure check is unchanged. Final merge requires the applicable
Windows hosted gates on the exact final head, following the owner's later
Windows-only scope clarification below.
The source adapter, configuration fixture and publication controller all pin
the updated workflow's canonical LF SHA-256
`a865984156057ad6c0443d29008a1feeaf37796cb06429b0e9e68f7ecb12e7c6`.
The deployed distributed service and its configuration were not modified.
Independent review also caught the distributed publication controller's former
forty-five-minute action deadline, which could expire before a valid longer
hosted gate. At that checkpoint its source deadline covered one hundred minutes:
the longest
pinned gate is the existing ninety-minute Mac release job, plus ten minutes for
scheduling and final reads. Each publication action gets a fresh control. The
configured-workflow budget regression, cancellation, lost-authority and expired
deadline tests protect the relationship without disabling a failure check.
The Windows-scoped CI selector added below extends the dependent workflow
budget; the current bounded action deadline is one hundred ten minutes.

### Captured-process regression measurement on a clean hosted runner

The production Windows build passed on source `77163a8`, and the seven isolated
ChatGPT cases passed again. Its GUI gate then stopped in the captured-process
regression: aggregate handles rose from 299 to 344, above the original tolerance
of 32. That early failure preceded UIA and is retained in
`target/publication/windows-local-77163-failure.log`.

The fixture now primes its own runtime work and requires two full 320-child
warm-up windows that establish no new handle/thread high-water mark, with a
bounded recent range, before measuring the same 320 captures from the actual
last sample. This permits runtime oscillation while rejecting ongoing growth.
The warm-up has a fixed upper
bound; all child output, exit, launch-failure, completion and executable-release
checks remain. The leak tolerance stays 32. A negative control retains 33 actual
process handles, requires rejection, disposes them and requires recovery. Schema
v2 records warm-up, baseline/final and negative-control diagnostics before a
failure is raised, and incomplete checks remain unconfirmed. The production
helper and installed GUI bytes are unchanged by this measurement correction.

The `b2a421c` fixture checkpoint's local SHA-256
`38c83fdf6e1e3d72ee24cb451a093ca45cc5e2490d49aa01cfb380070493cc6b`
passed a normal run, two constrained single-CPU runs and the parent's native
Windows PowerShell rerun. The parent recorded 455 handles / 46 threads before
and after all 320 measured children, rejected 33 retained handles and recovered
after disposal. Proof is retained as
`target/publication/parent-stabilized-capture-final.json` and
`process-capture-highwater-singlecpu-run*.json`. An injected failure retained
its diagnostics with null actual results and an empty assertions array in
`process-capture-injected-failure-final.json`. Independent high-risk review
approved that fixture checkpoint, evidence and docs.

Its next hosted gate retained samples `439:48,449:50,484:57,484:57,484:57`
(handles:threads). The final three readings were identical, but the warm-up
incorrectly reset the no-new-peak count while waiting for the recent range to
narrow. It then reached the four-window cap with only one counted stable window.
The source correction counts no-new-peak windows independently and requires
both that count and the recent-range bound before accepting a baseline. The
four-window cap, actual-last-sample baseline, measured 32-handle bound and
retained-handle control were unchanged at that checkpoint. A deterministic
regression replays
the exact hosted samples and rejects ongoing growth and unbounded variation.
The normal Windows PowerShell native rerun and inherited single-CPU rerun both
passed with the shared decision logic. They retained the original measurement
and negative-control checks; proof is under
`target/publication/process-capture-offbyone-*.json`.
The parent independently passed the final native gate with 451 handles / 45
threads unchanged across the measured captures; the 33 retained handles were
rejected and released. Its exact fixture SHA-256 is
`6d3d61a0c1989965ac920e5a19c4720dd62e4d9bb6fdcdbf1d33b0d67fc38244`,
retained in `target/publication/parent-capture-offbyone.json`. Independent
high-risk review approved the correction.

### Platform and child-fixture boundaries in the full Mac release gate

Source `0b2dcea` passed production Windows, the full Windows GUI gate and the
distributed Windows gate. The Mac workspace test run passed twenty-eight local
pipeline cases but failed two tests asserting Windows Job Object behavior. One
called Windows `tasklist`; the other expected a Windows descendant-containment
failure from the non-Windows validator implementation. The complete failed log
is retained as `target/publication/mac-final-0b2-failure.log`.

Those two parent cases, their Windows child fixtures and helpers are now scoped
to Windows, where both hosted Windows gates continue to execute them. This is
platform-correct test classification, with no production behavior change or
Unix containment claim. The canonical next step's broad `--ignored` invocation
was also audited: validator helpers must run only when selected by their exact
name inside an isolated candidate, and perform no effects when the entire
ignored suite is invoked. Their ordinary parent tests retain the real process
boundary coverage.

The corrected Windows native rerun passed all thirty-one default cases,
including both Job Object scenarios, with nine ignored child helpers. A separate
broad ignored invocation passed all nine helpers as safe no-ops and preserved
repository status and file inventory. Focused clippy with warnings denied,
formatting and whitespace checks passed. Proof is retained as
`target/publication/windows-local-pipeline-full.log`,
`windows-pipeline-broad-ignored.log`, `windows-local-pipeline-clippy.log` and
`windows-local-pipeline-fmt.log`. The portable Mac inventory has twenty-eight
default cases and five ignored helpers. Its hosted execution is separate from
the Windows-only phase's acceptance and merge requirements.

The intervening saved reviewer `gpt-6.1-sol/medium` returned `READY` through an
actual response-only call on this installation; its selection was preserved.
Current successful availability does not erase an earlier account rejection or
make cached catalog visibility sufficient inference proof. That bounded probe
is retained under `target/publication/saved-reviewer-probe-*` and had no queue,
repository or publication authority.

### GUI restart readiness and admission evidence

The `eae8804` hosted GUI run passed the seven isolated ChatGPT cases and all
320 native captures, with 489 handles / 58 threads unchanged. Its cancellation
restart scenario then timed out waiting for the second approved queue entry.
The retained record identified `line cancelled state` but contained no queue,
report or useful UI log. It cannot establish whether Start was admitted or
which subsequent state prevented completion. The failed log and bounded
artifact remain under `target/publication/gui-final-eae-failure.log` and
`hosted-windows-local-eae-failure/`.

The corrected journey waits for the retained terminal status and enabled Start
control, rather than sleeping a fixed 1.2 seconds after exposing the window.
After the single explicit Start invocation, it separately requires the second
entry to become Running or Completed before waiting for completion. Queue and
stopped-run identity checks remain; the completion deadline is unchanged.
Failure evidence includes bounded, sanitized queue, report and UI state before
fixture cleanup. This improves synchronization and diagnosis without asserting
an unproven product cause or changing production queue behavior.

The exact final Windows PowerShell full `-LiveDemo` journey passed as
`target/windows-gui-tests/5199c09beeb1427bbd7b685285a133b4/gui-e2e.json`, SHA-256
`4889fe24916bd79dec8cc9830b1deb11506240d3084dfd8994ea43c9b8a4d8e2`.
Both Failed and Cancelled restarts restored the terminal state, admitted the
second entry as Running, preserved both frozen jobs and completed with distinct
run IDs. Its saved-settings demo also passed all twelve acceptance checks.
PowerShell 7 passed the same functional journey as
`44db5751b61b47d2945919185e642f74`; the final catch-output-only correction was
parsed under both shells and exercised by the exact Windows PowerShell rerun.
At that historical checkpoint, the production GUI digest was
`f6c4d697b0f8bd0932131b582e299ade1488f58bdbc47ee3a9ba50d426c9fb0d`.
Independent high-risk review approved the final harness, evidence and docs.

### Later hosted callback-pool warm-up

The `c59cdb4` hosted capture fixture exhausted its four-window warm-up with
samples `449:50,449:50,459:52,459:52,469:54` (handles:threads). It retained
failed status, null measured results and an empty assertions array; the
320-child measurement and GUI journey had not started. The exact failed log
and bounded JSON remain under `target/publication/capture-final-c59-failure.log`
and `hosted-windows-local-c59-failure/`.

The fixture had primed thirty-two workers for sixteen concurrent captures,
which can queue stdout, stderr, exit and completion callbacks. Its preparation
now queues and drains at least sixty-four workers before measuring. Warm-up
allows at most eight full workload windows and a hard ninety-second deadline,
including remaining-time checks within each batch wait. The shared decision
regression keeps the observed four-window prefix unaccepted, accepts a later
two-window plateau, and rejects alternating continued growth through the cap.

The measured 320 children, exact output and exit checks, 32-handle bound,
33-retained-handle rejection/recovery, actual-last-sample baseline and truthful
failed evidence remain. Production capture code and installed GUI bytes are
unchanged; this adjusts fixture preparation and bounded measurement only.

The final local fixture SHA-256 is
`028f23085fca5297c7137c762fce00ea62e5abe3803ad9757793492738b18e7f`.
Exact-source normal and single-CPU runs passed all eight assertions, retaining
612 / 612 and 634 / 634 baseline/final handles. Their deliberate controls
rejected 613 -> 646 and 635 -> 668 handles, then recovered after disposal.
A forced 100 ms deadline probe exited 1 with a retained warm-up sample and no
PASS result. Proof is retained as
`target/publication/process-capture-c59-frozen-{normal,singlecpu}.{log,json}`
and `process-capture-c59-frozen-deadline-negative.log`.

The parent independently passed the exact final native fixture with 617 / 617
handles, rejected 618 -> 651 and recovered to 618. Its proof is
`target/publication/parent-callback-capture-final.{log,json}`. Independent
high-risk review approved the final fixture and deadline checks; the production
helper remains unchanged at SHA-256
`9c46c60e490de9f0defa39b3d0e7655bcfddb7237095eda81e294b774b46398c`.

### GitHub retry-cap fixture timing under hosted concurrency

Source `81eaac7` passed the full hosted GUI journey, including both explicit
queue restarts and twelve demo acceptance checks. Its 320-child capture proof
retained 619 / 614 baseline/final handles and rejected 615 -> 648 before
recovering to 615. The Windows-local gate then passed twenty-four of twenty-five
native GitHub cases but failed the alternating throttled/pending retry-cap case.
The log and successful GUI/capture artifacts are retained under
`target/publication/gui-final-81e-failure.log` and
`hosted-windows-local-81e-failure/`.

That retry-cap fixture allowed ten seconds for nine real PowerShell check
commands plus three seconds of backoff and 0.4 seconds of polling. Hosted
concurrency could exhaust its observation deadline before the ninth read.
The single case now allows thirty seconds and injects 800 ms into each fake
check read, deterministically crossing the old deadline. It still requires
exactly four retries, nine reads, nonzero exit, attention status and no merge,
with a forty-five-second wall bound including publication setup. Assertion
diagnostics include elapsed time, observed reads and captured output.
Production's twenty-minute deadline, cancellation and retry behavior and the
separate short pending-check deadline test remain unchanged.

The exact test file SHA-256 is
`2ac72a025ff7cb851a895bf674d3170c888a5fefd233634af027a364bb3731fd`.
Its focused delayed case passed in 18.01 seconds; all twenty-five native cases
passed together in 27.60 seconds. Formatting, clippy with warnings denied and
whitespace checks passed. Evidence is retained as
`target/publication/windows-local-github-full-final.log`, and independent
high-risk review approved the bounded test-only correction.
The parent independently passed the exact delayed case in 17.94 seconds;
`target/publication/parent-retry-budget-final.log` retains that native proof.

### Owner's Windows-only scope clarification

The owner clarified that this phase focuses on the Windows GUI and workflow,
with nothing to do with the Mac application. Acceptance and source merge use
the exact-head production Windows, Windows-local GUI/native and Windows
distributed gates. Mac application/Swift work and remote control are deferred.
The earlier Mac-derived icon and role-selection contracts remain references
for Windows behavior; existing Mac source and workflows are preserved.
Automatically triggered Mac CI is separate evidence and does not hold up this
Windows phase. Documentation, knowledge, unit/native E2E, independent review,
normal publication and installed Windows proof remain required.

### Windows GUI gate false-green correction

Hosted run `38479062068` for source `561c639` reported success, but its retained
GUI failure artifact says `SettingsOrchestratorReasoningBox` did not expose
`High`, and it emitted no `gui-e2e.json`. The native suites and capture checks
passed. This run does not establish a successful GUI journey and is not
accepted for publication. Its original logs and artifacts remain under
`target/publication/windows-local-561-success.log` and
`hosted-windows-local-561-success/`; the historical directory name records the
hosted conclusion, not the corrected proof assessment.

The child harness correctly exited 1 after sanitized cleanup, but the caller
continued to Cargo, which replaced `$LASTEXITCODE` with zero. The wrapper now
checks build, capture and GUI exit codes immediately, reserves one new GUID
directory, and consumes only fresh self-test and full-journey JSON from that
invocation. The extended journey must bind a completed run and all twelve
demo acceptance tests. The child explicitly exits zero only after success and
cleanup; a failed child or missing/failed evidence stops before downstream
tests can turn the gate green. The hosted artifact inventory retains the
fresh unit evidence and native gate-regression report.

Settings Reload closes its modal with `DialogResult.Retry`; model discovery is
asynchronous and the replacement is a different window. The UIA journey now
waits for that replacement instead of reacquiring the closing modal, scopes
popup choices to the same process and confirms the selected value. An isolated
delayed catalog fixture exercises this transition. Exact saved role and demo
job assertions remain. These changes affect Windows validation and fixtures;
the native runner and GitHub runtime bytes are unchanged. The separate terminal
projection correction below changes the Windows GUI.

`scripts/test-windows-gui-gate.ps1` runs the actual wrapper entrypoint in
disposable repositories with native process fixtures. It covers immediate
failure propagation, absence of fresh proof despite an unrelated stale pass,
rejected/malformed proof and incomplete extended demos, alongside successful
fresh evidence. Full real GUI validation remains a separate acceptance step.

The gate boundary's exact new test source is SHA-256
`821966c7d3ed26f2729fd324d3cf95f266f335788d7cfcd854155a1be1720ddd`.
All twenty-three actual-entrypoint scenarios passed in Windows PowerShell 5.1
and PowerShell 7.6, with twenty-one rejections proving no downstream
Cargo/docs/git command and two complete fresh-proof successes. Frozen reports
are `target/publication/windows-gui-gate-ps5-frozen.{log,json}` and
`windows-gui-gate-ps7-frozen2.{log,json}`. The parent independently passed all
twenty-three cases as `parent-windows-gui-gate-final.{log,json}`.

### Windows terminal persistence and display projection

The subsequent PowerShell 7 journey retained a second queue entry marked
Failed while its native report was completed and `publication_status` was
`local_only`; the first entry was Completed. Evidence is
`target/windows-gui-tests/6424aa00153b46b29f0cc84d146cf857/failure.json`.
That artifact omitted the terminal exception text, so its exact display
exception is not established. Independent code tracing showed local-only
publication can only be reached after owned completion is confirmed. Terminal
persistence and display refresh shared one throwing method; a projection
exception could therefore enter the outer failure handler and overwrite the
saved Completed result as Failed. A passing rerun would not repair this path.

`AssemblyLineTransition.PersistThenProject` now lets persistence failures
propagate and catches only later display exceptions. `MarkLineTerminal` keeps
the saved result and stops further queue advancement if projection fails;
its error surface is best-effort and does not reclassify completion. The final
queue refresh uses the same projection boundary, and ListView updates always
balance BeginUpdate/EndUpdate through finally. Pending jobs remain frozen until
the owner explicitly starts them after a stopped line.
Finalization also releases the start gate inside this boundary before updating
buttons; a repeated display exception cannot escape from that final cleanup.

The existing FIFO/persistence self-test group uses real file Save/Load and
injected display failures for both local-only and connected completion. It
checks exact Completed status, stage, attempt, run/publication fields and the
unchanged next Pending job. An injected persistence failure must propagate,
skip projection and preserve the old bytes. The rebuilt Windows GUI passed all
fourteen unit groups; the final proof is
`target/publication/terminal-projection-final-units.json`. Its exact GUI digest is
`d77623f23facafcef3b9db1d7b3f735fb17b017777f381b55a78986040cfebc5`.
The native runner, GitHub adapter and model/review/publication authority are
unchanged by this production GUI correction.

### Windows-scoped GitHub CI

The owner requested tagging Windows-only pushes and merges so only Windows
execution runs. The `windows-only` PR label and exact `[windows-only]`
merge/main commit marker select this mode. A short Windows selector job skips
the Mac runner; production Windows, Windows-local GUI/native and Windows
distributed workflows remain mandatory. Manual full dispatch, label removal,
untagged or malformed/unknown event data retain full validation. The operational
commands and proof limits are in [Windows CI scope](windows-ci-scope.md).

Eighteen actual-handler event cases passed in both Windows PowerShell 5.1 and
PowerShell 7.6. The workflow contract binds dynamic check naming, selector
outputs, Windows execution, shared cancellation and unrelated-event isolation.
An unrelated label uses a distinct ignored check name so it cannot shadow
earlier full-release evidence with a higher skipped check ID. Applying the
Windows label intentionally skips the protected Mac context; that skip is not
full-release evidence. The existing protected publication parser still rejects
skipped, neutral and cancelled conclusions even when the workflow run itself
reports success.

The release selector workflow's canonical LF SHA-256 is
`6c2e8710ec2cae10287dbed0d1f7343059d1630a7ca8749191c0d77dc7e083a4`.
All five source/controller/test pins are updated consistently. Its separate
five-minute selector precedes the Mac runner's ninety-minute budget. The
deadline regression conservatively sums configured job budgets, covering this
ninety-five-minute dependency path plus ten minutes for queue/final reads.
The publication action remains explicitly bounded at one hundred ten minutes;
its cancellation and fresh-authority checks are unchanged.
All six publication-adapter tests passed, including exact workflow bytes and
the skipped-check rejection. Proof is
`target/publication/windows-ci-scope-publication-adapter-final.log` and
`release-local-selector-{ps5,ps7}.{log,json}`. No Mac application code or protected
Windows service/configuration is changed by this CI routing slice.

### Final Windows canonical validation and installed deployment

The complete `scripts/validate-windows-gui.ps1 -LiveDemo` wrapper passed on
committed source `b0e98cc724d10b37811517ea35f4cfba45a6a4b4`. Its fresh evidence
directory is `target/windows-gui-tests/1d006c47caaf49969ceacbf11d3bbf13/`;
`gui-unit-tests.json` records fourteen groups and `gui-e2e.json` records the full
native journey. Extended demo run `116c365a-cd2e-4ca9-927c-602d9959e4ae` completed
all twelve acceptance tests with the exact saved fixture models/reasoning,
two attempts and forty-seven-second timeout. This is isolated fixture proof,
without an authenticated ChatGPT or live local-model claim.

The wrapper also passed thirteen local-runner units, eight GitHub units,
thirty-one pipeline cases, twenty-five GitHub cases, the separate 320-child
capture regression and docs/whitespace checks. Nine ignored pipeline child
helpers remain helper-selection proof. The canonical log is
`target/publication/windows-canonical-b0e98cc.log`; runtime bindings are in
`final-wrapper-runtime-build.json`. This rebuilt bundle's GUI bytes differ from
the preceding focused `d77623` build; the earlier unit record remains historical.

The tested default-feature bundle was installed through the normal Windows
installer, preserving Desktop and Start shortcuts. Independent inspection at
2026-10-14 00:06 UTC found one responding installed process, PID 58752, its
window, the exact gear/shield/GitHub/Start/chat control IDs and Completed display.
All four installed SHA-256 values match the canonical tested bundle:

| Runtime | SHA-256 |
| --- | --- |
| `Assemblywright.exe` | `e1cee4fcee83c5b91db505456a8d9bbc0dbdb56a991601b1bb1b773cacdfc124` |
| `assemblywright-local.exe` | `329addc69685dc4b348b8acec2d9ecbb4cfe65204fbdb02f5baa32026a60ce90` |
| `assemblywright-github.exe` | `613f76e946848fa3df5657cd49e710c261fccd52518c070b64054fffbfc02b46` |
| `windows-local-pipeline.ps1` | `5d9165a729f97e46a1fe16c14d8ceb4eb43847d3c6b9a1dd509cee2c3d10a817` |

The persisted before-install hashes for `assembly-line.json`, `gui-job.json`,
`gui-settings.json`, `latest.json` and `permissions.json` remain exact. Native
run `bccd9049-9b3b-458a-bae9-1a45a3da24a8` remains completed. Proof is
`target/publication/terminal-projection-installed.json`, with its pre-install
snapshot and strict read-only verifier retained alongside it. That original
automatic published demo used the earlier `f6c4d69` GUI; it is not relabeled as
an execution on this new bundle. No fixture runtime was installed.

Earlier immediate installation-readiness probes failed in Windows PowerShell
5.1 and remain failed evidence. Extending their bound did not repair observation.
Independent same-PID inspection established the difference: PowerShell 5.1/.NET
Framework saw fallback pane nodes with numeric handle IDs, while PowerShell
7.6.5/.NET 10 found all five exact AutomationIds in under one second. The latter
runtime independently verified the installed controls, bytes and retained state.
This establishes current installed health without claiming the failed probes
passed or asserting an unmeasured startup SLA. No product behavior was changed
to accommodate this deployment-only verifier.

Documentation and safety compliance, durable knowledge, focused unit coverage,
native E2E and independent high-risk source review passed for this Windows phase.
The unit and E2E skills were applied to the native/process boundaries; the real
demo's browser matrix remains separate. Source publication requires fresh hosted
Windows gates and the Windows scope selector on the final docs closeout SHA.
Mac application validation and protected Windows service deployment are outside
this local GUI phase.

### Hosted selector CLI exit correction

Final-head hosted Windows-local run `38489365334` at `4e85942` passed all eighteen
selector assertions, then failed the caller's immediate `LASTEXITCODE` check.
The pure PowerShell matrix had left this value unset; the successful assertion
report did not establish a successful CI step. Its retained log and artifacts
are `target/publication/final-4e-windows-scope-step-failure.log` and
`hosted-windows-local-4e85942-failure/`. No GUI journey ran in that failed gate.

The matrix now emits its report only after guarded cleanup, writes evidence,
then explicitly reports zero to its caller. Exceptions continue to fail the
native process. `scripts/test-release-local-selector-cli.ps1` invokes this
real script in fresh PowerShell processes: initially unset and inherited
nonzero exit codes must pass, while injected selector and cleanup rejections
must fail without passing JSON or stdout. All four cases passed under Windows
PowerShell 5.1 and PowerShell 7.6; the unchanged eighteen event cases remain.
The new native regression also rejects the original `4e85942` script with the
same unset-code failure, as `target/publication/selector-cli-red-proof.json`.

Windows CI executes and retains both matrices; its contract smoke binds the
new native entrypoint and artifact. The CLI suite returns naturally after
reporting success, and CI launches it in a separate PowerShell process so it
cannot terminate the calling step. Actual workflow command sequences passed
in both PowerShell hosts, proving caller continuation and both fresh artifacts
as `target/publication/selector-step-{ps5,ps7}/workflow-step.json`.
This correction changes CI/test scripts
and documentation only. The installed canonical bundle, owner state and
earlier automatic demo evidence remain unchanged. The final publication still
requires fresh successful Windows workflows and the successful scope selector
on the reviewed final head.
