# Assemblywright Windows GUI

This is the native WinForms control surface for the same-computer Windows
pipeline. It deliberately has no SDK or package dependency: the repository
build script uses the C# compiler included with Windows .NET Framework and
bundles the existing Rust runner plus PowerShell demo launcher beside the app.

The left pane contains the durable Assembly Line and the feature editor. **Add
to line** is the owner's approval of one exact frozen job. **Start line** runs
pending entries one at a time in FIFO order through the same native runner.
Only a new run owned by this window, with exit code zero and a completed native
report, advances to the next entry. Pause, cancel, failure, interruption, or a
normal stop halts automatic advancement until the owner deliberately starts
again. A project with no saved GitHub connection remains private. A connected
project advances only after the native publication adapter confirms that the
exact reviewed candidate was pushed on its own feature branch, opened as a
pull request, passed required checks, merged, and reconciled to the configured
base branch. After the verified merge, the publication adapter fast-forwards
the clean connected checkout to that merge so the next queued feature starts
from the current base. An unconnected project keeps its validated candidate
private and does not publish or update its source checkout.

The right pane keeps compact pipeline status visible above feature-planning
chat and the activity log. Chat sends only the owner's bounded message, the
current editor specification, and bounded chat history through the packaged
response-only ChatGPT adapter using the selected orchestrator model and
reasoning level. It has no tool, file, queue, execution, source-write, or
publication authority. **Use as feature** copies the latest answer into the
editor and never queues or runs it.

The header matches the Mac developer build: GitHub publication
(`arrow.triangle.branch`), Permissions (`shield.lefthalf.filled`), then
Settings (`gearshape`). The Windows controls use matching vectors and perform
the same work within the local Windows topology:

- **GitHub publication** refreshes or signs in the GitHub account, lists
  repositories, creates an owner-confirmed private or public repository,
  creates an explicitly empty local project, and separately saves, updates, or
  disconnects its publication binding. Saving a connection prepares and
  verifies the runnable baseline. A confirmed publication also updates the
  clean connected checkout to the verified merge. It does not treat opening a
  website as a connection or publication result.
- **Permissions** loads and revision-binds the three Mac modes: Ask for
  approval, Approve for me, and Full access. Ask mode surfaces exact native
  approval requests for proposed edits and validation commands; the owner can
  approve or reject each pending digest. Existing installations begin in Full
  access as requested for the local-first pivot.
- **Settings** keeps the loopback endpoint, model, and reasoning level in a
  separate **Local implementation** section. It loads the signed-in ChatGPT
  account's advertised catalog and configures independent ChatGPT orchestrator
  and reviewer model/reasoning pairs. Only an advertised pair can be saved;
  ChatGPT never receives repository-write or implementation authority. Changes
  are revision-bound, apply to new chat requests and newly queued work, and are locked during
  active execution, chat, setup, publication, or Emergency Pause. The owner
  can change the reviewer on a selected non-running line item without starting
  it.

Pause, resume, and cancel remain permanently visible in pipeline status. Pause
and cancellation also stop or block publication and pending approvals.

Build and run:

```powershell
./scripts/build-windows-gui.ps1
./target/windows-gui/Assemblywright.exe
```

Focused logic checks run without creating a window:

```powershell
./target/windows-gui/Assemblywright.exe --self-test ./target/windows-gui/self-test.json
```

Automation can select a separate state root, preload a job, and render the
real form offscreen:

```powershell
./target/windows-gui/Assemblywright.exe --test-mode --state-dir C:/temp/aw-state --job C:/temp/job.json
```

The GUI writes immutable UTF-8 jobs under `STATE/jobs`, launches only the
bundled runner, and refreshes state through the runner's `status` command.
Pause, resume, and cancel use the runner's native control commands. A GUI that
observes a pipeline started elsewhere labels it as external and does not bind
that active run as its own job.

Local GUI state is atomically retained under the selected state root:

- `gui-job.json` retains the current editor draft, including incomplete drafts.
- `gui-settings.json` retains schema-1 local implementation and ChatGPT role
  defaults, limits, and the settings revision. Older schema-1 settings migrate
  future chat and feature defaults to catalog-advertised ChatGPT choices while
  existing frozen Assembly Line jobs keep their original provider fields.
- `assembly-line.json` retains schema-1 frozen jobs and terminal rows. A row
  left running across startup becomes `Interrupted` and never resumes itself.
- `chat/history.json` retains at most 32 owner, assistant, and visible error
  messages with explicit provider, model, and reasoning metadata.

`--test-mode` keeps the normal controls, dialogs, approval decisions, and state
contracts. It never turns an approval into an automatic approval; native UI
Automation must press the same explicit Approve or Reject control.
