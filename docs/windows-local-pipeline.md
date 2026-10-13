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
