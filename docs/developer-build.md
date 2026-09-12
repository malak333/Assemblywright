# Supervised Developer Build

The owner requested a working end-to-end application before further security
hardening on 2026-09-05. This build implements that choice under the existing owner
accounts. It runs actual model-generated changes and validation on Windows and
presents the queue and controls in the Mac app.

## Use it

Configure the app-specific background connection and required Windows Codex
reviewer paths, then build with:

```sh
./scripts/developer-build.py --build
```

Afterward, open `target/developer/Assemblywright Developer.app` directly. You can
also use `Open Assemblywright Developer.command` or run
`./scripts/developer-build.py`. The dedicated background connection reconnects
without requiring an interactive SSH terminal. The launcher
starts the configured local model if its API is unavailable. It uses the existing
`local-ai-mac` controller and model; it does not install a new model configuration.
Run `--help` to change the Windows host, SSH control socket, remote root, or model
controller. Default connection values reflect the owner's current two-machine setup.

1. Enter a simple project folder name. A new folder is created under the displayed
   Windows workspace root; later features with the same name use that project.
2. Describe the feature and enter the command that should validate it. For Python
   projects, `python -m unittest discover -s tests -v` is a useful starting point.
3. Choose **Brainstorm with ChatGPT**, answer its questions, confirm the requirements
   and approach, and approve the resulting design documents.
4. Choose **Approve documents and add to queue**, then **Start**. The confirmation
   identifies Windows execution, independent cloud review, and automatic advancement.
5. Review each feature's status, checkpoint, changed files, validation, and review.
   Files remain in the Windows project folder.

The accepted [GitHub publication extension](developer-github-publication-design.md)
adds automatic publication for projects with a saved GitHub connection: an exact
reviewed commit on its own feature branch, a pull request, required GitHub checks,
normal automatic merge, and verified remote base before the queue advances.
Unconnected projects continue locally and older completed results are not uploaded.
The protected production setup/planning surface remains separate in the original
application. Implementation and installed validation are recorded in the GitHub
publication section below.

## GitHub publication

Use **GitHub** in the Developer app and check the Windows account status first. If
sign-in is missing or invalid, choose **Sign in**, open the displayed GitHub device
page, and enter the one-time code. Finish authorization in GitHub; Assemblywright
does not ask for a password or access token. Windows retains the GitHub CLI login.
An interrupted or uncertain sign-in offers explicit reconciliation and reports the
account actually observed; cancellation cannot undo credentials already saved by gh.

Choose a writable repository from the paginated list to fill its URL and initialized
default branch, or enter an existing GitHub repository URL manually. Save the
connection while Developer work is idle. The connection publishes subsequent
successful features through their own branch and pull request, then merges normally
after the exact required checks pass and verifies the remote base before advancing.

To create a repository, enter its name, explicitly choose **Private** or **Public**,
and confirm the displayed account, name, and visibility. Creation initializes a
README under that account. It does not upload project files or automatically connect
the project. Configure any missing required checks, then save the connection. A
collision or uncertain creation result offers observation and reconciliation instead
of blindly sending another creation request. See the
[GitHub setup design](developer-github-setup-design.md) for recovery details.

The retained September 8 evidence records that `inches-feet-demo` was connected to
`malak333/inches-feet-demo` on protected `main` with automatic merge enabled after
[setup PR #1](https://github.com/malak333/inches-feet-demo/pull/1) passed the required
Windows check and merged. That follow-up initialized README and CI scaffolding only;
it did not publish historical application files. This is historical evidence for the
original implementation and must not be treated as current restoration validation.

## Project chat history

The accepted [chat history design](developer-chat-history-design.md) adds a
**History** browser with conversations grouped by project. Expand a project and
select a titled chat to reopen and continue it. **New Chat** starts a separate
topic in the selected project; with no project selected, choose one first. Use
the pencil button to rename the current chat. The history button gives the
conversation more room in the side panel, and a wider panel can show both.

Existing retained messages are available as **Previous conversation**. Draft text
and attachments stay with their chat while navigating during an app session;
the last selected chat per project is remembered across app launches. Older saved
messages can be loaded without adding every message to the AI's limited context.
Messages discarded by the previous retention policy cannot be recovered.

History keeps up to 2,000 chats and 2,000 messages per chat, subject to a 256 MiB
logical storage limit and reserved space for finishing active work. At capacity,
new work shows an explicit error and saved history remains available. The full
[storage and execution bounds](developer-chat-history-design.md#persistence-and-request-binding)
also cover request receipts, attachments and tool evidence.

While a reply or tool action runs, history remains available. **Return to active
chat** opens its conversation and **Stop** remains visible. A second reply waits
for the existing global work gate. Creating a chat keeps the project's access
mode; the current AI and permission choices remain visible before sending.

## Planning and project chat

New developer features use the bundled brainstorming workflow with the fixed
ChatGPT/Codex planner. Windows stores the questions, owner answers, confirmed
requirements and assumptions, selected approach, design confirmations, decision
log, and approved documents. Planning cannot write project files, run validation,
or enqueue itself. Direct enqueue cannot bypass the approval gates; the approved
documents remain bound to implementation, repair, and review.

Use **Project chat** for questions about an existing project. Select the Windows or
Mac local model explicitly. Replies retain model attribution in Windows-owned,
conversation-specific history. When the Developer tool runtime is configured,
the selected project access mode controls file operations and commands: **Ask**,
**Approve for me**, or **Full access**. Approvals remain bound to their exact chat
and request, and chat cannot enqueue or reorder features. Project edits invalidate
older validation and review evidence before feature work resumes. Without
the tool runtime, chat uses the local model without tool execution. This is the
supervised Developer workflow, not activation of the protected production runtime.
Image and bounded text attachments are untrusted references. Unsupported vision,
unavailable models, and context limits fail explicitly without fallback.

## Repair a failed feature from chat

The accepted contract is [Developer chat repair escalation](developer-chat-repair-design.md).
After receiving a useful saved diagnosis, choose **Repair this feature…**, select
the local model, and prepare a proposal. Preparation does not write files. Review
the complete before/after bytes and every marked protected test or validation input.
**Approve and apply** authorizes only that exact proposal; Windows rechecks the
feature, diagnosis, checkpoint, revision, and current bytes before writing.

Each escalation has one application attempt and does not reset the feature's three
ordinary repair attempts. Windows runs the original validation command and then a
fresh independent Codex review. A failed test or rejected/unavailable review stops
the feature and auto-run. The repair sheet keeps Close and available actions pinned
while long content scrolls.

## Controls and checkpoints

- **Stop** cancels model waiting or terminates the active validation process tree.
  It preserves applied files and leaves the feature paused.
- Windows starts validation suspended and assigns it to a kill-on-close Job before
  allowing it to run. Stop and Emergency Pause wait for the tree to exit; a runner
  crash also closes that Job. An unconfirmed termination is reported as a failure
  with Emergency Pause latched, rather than a successfully paused feature.
- **Emergency Pause** also latches a pause. **Clear Emergency Pause** only clears
  that latch; **Resume** remains a separate action.
- **Resume** reuses a prepared change set and skips files whose new digest already
  matches. Once the applied checkpoint exists, it runs validation without asking the
  model again or rewriting those files. An owner edit conflicting with the model input or a prepared
  change is preserved and reported. Changed files use synced temporary files and
  atomic replacement to protect existing bytes from interrupted writes.
- **Auto-run on** advances within the queue present at Start/Resume after validation
  succeeds and independent Codex review approves the exact candidate. New features added during execution wait
  for another explicit Start.
  **Auto-run off** leaves the next feature queued for **Start next feature**.
- A failed model response or validation stops advancement. A retry with no prepared
  change set asks the model again. A retry after application reruns validation; fix
  failing code in the project before resuming if needed.
- On runner restart, interrupted runs become paused. Completed results and saved
  change sets remain durable. Validation commands can run again after interruption,
  so use repeatable test/build commands.

The portable Mac runner exists for native test coverage and uses process groups;
it does not provide the Windows runner-crash Job guarantee. The launcher executes
owner projects on Windows.

The Windows runner owns a separate SQLite database and an exclusive state-directory
lock. It binds only to loopback. The launcher forwards the runner to Mac port 17796
and the Mac model to Windows port 18080 through the existing SSH connection. The
loopback token is stored in the local developer configuration, not printed by the
launcher. The installed `AssemblywrightMaster` service and owner checkouts remain
separate from this build.

The model URL must also be loopback HTTP; use SSH forwarding for the Mac model.
Validation logs stay in the developer state directory for owner debugging and are
not automatically redacted. The developer snapshot is not a production append-only
audit ledger. If an Emergency Pause cannot be saved, the request reports failure
and the current runner keeps its emergency latch until a successful explicit clear.
Storage failure is not reported as a durable acknowledgement; restart never
automatically resumes work.

The runner accepts at most 100 features, 40 generated files per change set, and a
1 MiB model change set. Project context is bounded and skips hidden directories,
common dependency/build directories, and symbolic links. Validation has a 15-minute
limit and a 2 MiB log limit. These are developer usability limits, not a claim of
hostile-process containment or protected production execution.

## Local model and reviewer runtime setup

Windows model startup is managed independently of the Mac connection supervisor.
Start the Windows inference service first, then configure `--windows-model-url`
and `--windows-model`. Reconnection restores the runner channel; it does not start
that model service. The former `--windows-model-start-script` option never executed
its script and is no longer offered. Explicit use returns setup guidance; saved
legacy script paths are accepted for compatibility and removed on settings rewrite.
An unavailable selected model fails without automatic fallback.

The reviewer executable is hash-bound for each runner lifetime, runs from a separate
working directory with a cleared environment, and uses strict configuration to
turn off the supported tool, memory, plugin, browser, and automation features.
Runtime upgrades require checking these settings against the new CLI:

```sh
python3 scripts/developer-review-catalog-e2e.py --codex-executable /absolute/path/to/codex
```

The opt-in native probe reuses the actual reviewer arguments with a temporary home
and a loopback fixture provider. It rejects any exposed tool catalog and returns no
model output. It does not use account authentication or call a real model. The
installed Mac CLI passed with zero tools in its captured request. These controls
are supervised developer evidence, not a claim of hostile-process containment.

## Native validation

```sh
cargo test -p assemblywright-master --bin assemblywright-developer
cargo build -p assemblywright-master --bin assemblywright-developer
python3 scripts/developer-runner-e2e.py --binary target/debug/assemblywright-developer
python3 scripts/developer-runner-chat-e2e.py --binary target/debug/assemblywright-developer
python3 scripts/developer-runner-review-e2e.py --binary target/debug/assemblywright-developer
python3 scripts/developer-runner-planning-e2e.py --binary target/debug/assemblywright-developer
python3 scripts/developer-runner-escalation-e2e.py --binary target/debug/assemblywright-developer
swift build --disable-sandbox --package-path apps/mac --product AssemblywrightMacApp
```

On Windows use `python` and the `.exe` executable path. The E2E script starts a
separate temporary runner and a labeled fixture model, then exercises authenticated
HTTP, real file writes, real command execution, cancellation, checkpoint reuse,
auto-run, and restart. It does not contact the owner's live queue or local model.
Live-model evidence is recorded separately below.

## Real local-model evidence

On 2026-09-05, local Qwen generated three actual Windows features in
`C:\a\aw-developer-20260905\projects\temperature-demo`: temperature conversion
and tests (10 passing), Kelvin support (20 passing), and documentation (20 passing).
The live control run stopped validation in 0.265 seconds at its applied-files
checkpoint, confirmed Emergency Pause blocked Resume, resumed saved work, and
automatically advanced to the README feature. All three ended `succeeded`.

These live-model results are separate from disposable fixture-model E2E. The
publication closeout adds regression coverage for owner edits during generation,
atomic file replacement, starting queue boundaries, failed validation, database
failure, and Windows runner-crash cleanup. See
[developer-build-testing.md](developer-build-testing.md) for the current coverage
matrix and verification evidence.

An opt-in native Swift test instantiates the app's real `DeveloperRunnerModel`,
reads its saved connection configuration, and decodes the live Windows queue.
Set `ASSEMBLYWRIGHT_DEVELOPER_LIVE_CONFIG` to the developer `runtime.json` and run
`swift test --disable-sandbox --package-path apps/mac --filter DeveloperRunnerTests`.
The ordinary package gate skips this test without a live configuration. The
computer-use bridge could launch the app but closed its native pipe while reading
accessibility state, so this does not claim visual UI automation.

The working developer phase is published separately from unfinished production
integration drafts. Signed production installation and hostile containment remain
separate work.

## Chat repair closeout evidence (2026-09-07)

Historical implementation evidence before the latest owner interaction passed 41
Rust tests, 244 Swift tests, and seven native Windows developer process E2Es. The
current isolated publication candidate has separately passed 44 focused Rust tests
and 235 Swift tests. Its Cargo wrapper ran all seven developer process E2Es locally,
including validation-failure and reviewer-rejection cases that prove auto-run does
not advance across failure or restart. Native Windows rerun remains pending. These results cover exact proposal binding, zero-write preparation,
protected-test approval, cumulative review baselines, cancellation, restart, and
the validation/review route. The repository's phase checklist is
[`development-agent-workflow.md`](development-agent-workflow.md); focused results,
native E2E, full gates, hosted publication, deployment, and owner visual evidence
remain separate proof layers.

The owner's 12:50-12:56 screenshots confirmed that the repaired modal was visible with
its Close and action buttons pinned. The owner approved and applied proposal 4.
The unchanged validation command ran 24 tests and failed two: a hidden/unrealized
Tk window still reported height 1, and a brittle widget-tree search could not find
the nested Input card. Codex review did not run, the queue did not advance, and the
GUI feature remains unresolved. The applied proposal therefore has application and
owner-observed visual-modal evidence, but no successful validation, independent-review,
or machine-automated screenshot-regression evidence.

The first complete local gate passed at 17:31. Subsequent recovery fixes have focused
coverage but still require a final complete local-gate rerun. Native Windows and
exact-SHA hosted gates also remain pending, so publication closeout is not claimed.
Production deployment, signing, and notarization do not apply to this supervised
developer slice.
