# Developer project chat history

Status: owner-approved design. The original September 8 implementation is being
restored from its saved Swift edits and frozen Windows validation source. Historical
evidence below applies to that original run; current restoration evidence is separate.

## Owner experience

The owner wants to revisit previous chats, see them grouped by project, and start
a new chat with a button. The previous implementation kept one continuous
conversation per project.

Use a collapsible history sidebar inside Project chat. Its header has a visible
**New Chat** button. Project groups expand to show titled conversations, newest
activity first. Each row has a title and a short relative activity time; the
selected chat is highlighted. The conversation header shows its project and title.
History can be hidden to give the conversation room in the existing narrow side
panel; opening an existing chat in a narrow panel returns to the conversation.

- **New Chat** immediately creates and selects an empty conversation in the
  selected project and focuses the composer. With no project selected, it first
  presents the existing project list. Creating a chat never creates a project.
  Repeated presses reuse the selected pristine empty chat when it has no local
  draft or attachments and Windows confirms no messages, requests, approvals or
  provenance. This check occurs atomically with creation; no saved chat is deleted.
- New conversations start as **New chat**. The first submitted message supplies
  a short, deterministic title; an attachment-only message uses its filename.
  The owner can rename a chat. Titles are plain text, bounded, and never require
  a separate model call. Duplicate titles are allowed; identity uses a stable ID.
- Clicking an earlier chat opens its saved transcript and allows continuation.
  Reading or selecting a chat never starts inference or repeats a tool action.
- Draft text and selected attachments stay with their chat while navigating
  during the app session. Restore the last selected project and chat on reopen;
  missing or unavailable data shows an explicit state rather than another chat.
- Existing retained messages appear in **Previous conversation** under their
  project. Preserve attachments, attribution, request IDs and repair provenance.
  Messages discarded by the old retention limit cannot be reconstructed.
- Readable history and model context are separate. Save new transcript messages
  durably and page older messages; the model receives a bounded selection from
  the selected conversation plus current project context. Other chats are not
  implicitly added. A new chat starts with fresh conversational context while
  retaining access to the same project files under its existing permissions.
- Browse or create an empty chat while another reply is running. Show the active
  project/chat and a **Return to active chat** action. Its Stop control remains
  accessible. Send stays unavailable while the existing global work gate is busy.
  Pending approvals are visibly associated with their originating chat.
- Preserve explicit Windows/Mac AI selection and project-wide access controls.
  New Chat does not reset permissions. Sending always shows the selected AI and
  access mode. Historical replies retain their actual model attribution.

Renaming and navigation are included. Search, archive/delete, cross-project moves,
branching, and concurrent model work are deferred to keep this slice bounded.

## Approach and alternatives

Recommended: durable Windows-owned conversations with a lightweight Swift history
browser. This supports reopening and continuing distinct topics after restart.
A transcript-only visual grouping would still blend model context. A Mac-only
history index would not make conversations durable on the authoritative host.

## Persistence and request binding

Add a conversation ledger keyed by immutable chat ID and project, with bounded
title, creation/activity metadata and revision. Persist messages separately from
the bounded prompt window; use bounded cursor pagination for chat and message lists.
Keep finite message, attachment, title and database storage limits. At capacity,
reject new chat/request admission with a clear reason; do not discard saved history.
Before accepting a request or starting inference/tools, transactionally reserve
its bounded user/assistant payload allowance and terminal, action and recovery
evidence budget. Concurrent metadata writes cannot consume those reservations.
Admission limits cannot reject terminal or recovery writes for accepted work;
tool output and action counts remain bounded by the reserved execution budget.
If physical storage fails despite reservation, fail closed and preserve unresolved
request evidence for restart quarantine; never retry a possibly effectful request.
The implemented admission limits are 2,000 conversations, 2,000 messages per
conversation, 10,000 request records and 10,000 creation receipts. Titles allow
80 characters. Conversation and message pages contain at most 50 entries; the
model prompt uses at most 40 messages from its selected conversation.

The logical ledger is capped at 256 MiB, counting retained transcript bytes,
pending request reservations and historical chat-bound action details, summaries
and output. Each admitted request reserves 64 MiB for action/recovery evidence
plus 512 KiB for its serialized assistant terminal message (67,633,152 bytes in
total). A request allows at most 48 action rows, with at most 1 MiB of details,
32 KiB of output and a 1,000-byte summary per row. Reservations are released only
after completion, ordinary failure or cancellation commits. A physical terminal
write failure retains the pending request and reservation, blocks new work, and
requires restart recovery. These are logical admission bounds; SQLite pages,
indexes and retained migration backup tables have separate physical overhead.
Existing attachment limits remain four attachments and 6 MiB total per message,
with 2 MiB/1,600-pixel images and 128 KiB text attachments.

Use a backed-up, versioned, transactional migration from the existing
`developer_chat_project`/`developer_chat_request` storage. Each old project receives
one deterministic legacy chat mapping. Preserve every retained message and exact
request binding, including interrupted recovery state. Retry of migration cannot
duplicate chats, reinterpret completed requests or replay pending work.

The authenticated Developer runner API advertises chat-history support and exposes
list, create, get and rename operations. Creation uses an idempotency ID; replaying
it with a different project fails. Rename binds the observed chat revision. Existing
legacy endpoints may target only the explicit legacy chat; they cannot select the
currently visible chat implicitly. An old runner gets a clear update-required state.

Bind all send, retry, stop, tool approval, response polling and repair-diagnosis
lookups to exact project/chat/request identity. Windows validates chat ownership
before reading or mutating it. Request IDs remain globally collision checked and
cannot be reused in another chat. Extend tool-session and repair provenance bindings
where necessary without changing their approval semantics. Do not substitute a
chat title, displayed row index or selected UI state for identity.

Swift keys observation tasks, drafts, errors, pending sends and rendered action
closures by project/chat. Discard late responses when that selection changes.
Ambiguous sends reconcile only their original exact request. Opening history cannot
approve tools, enqueue features, start work or apply a repair. Project tool settings,
the shared inference gate, cancellation, emergency pause and restart quarantine
retain their current meaning. Windows remains the durable authority.

## Implementation and acceptance

1. Implement the Windows ledger, migration, bounded APIs and request/provenance
   bindings in Developer modules. Include tool/repair paths affected by chat identity.
2. Add Swift models and the adaptive history browser, new-chat/rename controls,
   per-chat drafts and stable selection. Keep the queue visible in the existing
   workspace and ensure history can collapse at the current minimum window size.
3. Extend Rust tests for legacy migration/recovery, restart persistence, ownership,
   same-title chats, idempotency, pagination/limits and exact action bindings.
   Test repeated pristine New Chat presses, preservation of draft-bearing chats,
   pre-execution quota reservation, concurrent quota exhaustion and recovery after
   a terminal-write storage failure. A capacity limit cannot discard evidence of
   an action already admitted or cause it to execute again.
   Extend Swift tests for creation, navigation, drafts, stale responses, capability
   mismatch and approvals captured before selection changes.
4. Extend native runner HTTP/process E2E: create two chats in one project and one
   in another, exchange messages, restart, reopen and continue each independently;
   retain attachments and tool/repair evidence; reject cross-chat replay/approval.
   Verify navigation and new-chat creation leave queue and project files unchanged.
5. Verify the installed Mac interface against Windows for history browsing,
   keyboard navigation, compact layout, ongoing reply/Stop visibility and pending
   approvals. Record this separately from fixture and repository evidence.
6. Update DESIGN, safety rules, Developer usage/build documentation and the
   knowledge base once accepted. Run focused checks, docs drift, diff checks and
   the canonical local gate; record Windows and live validation separately.
   Obtain independent persistence/trust-boundary review. Commit and publication
   require an explicit owner request.

## Review evidence

Source exploration confirmed one persisted `ProjectChat` per project and one
project-selected Swift panel. Independent review identified admission-quota
reservation and repeated empty-chat creation gaps; the proposal now explicitly
reserves terminal/recovery capacity and reuses a selected pristine empty chat.
The independent reviewer approved the amended design with no remaining findings.
The interactive mockup was checked for creation, reopening chats, draft retention
and reuse of a pristine empty chat. Docs drift and diff whitespace checks passed.
The owner accepted this design and requested implementation.

## Original implementation evidence (2026-09-08)

- Independent high-risk implementation review approved the final migration,
  capacity accounting, exact request/approval/repair bindings, Swift navigation
  and validation manifest with no remaining findings. A final terminal-state
  regression verifies wrong-project rollback and duplicate-finalization rejection.
- Focused Rust chat tests passed 20/20, with additional tool binding/budget and
  terminal-storage regression checks passing. Rust formatting and strict clippy
  passed. Final Developer binary unit runs passed 115 tests on Mac and 117 tests
  on Windows, with one explicitly ignored Windows case.
- Final-source Mac and Windows history, legacy chat and repair-escalation process
  E2Es all passed. The history fixture verifies old-schema backups and exact legacy
  hashes, migration/restart, attachments, pagination, isolated prompts, active
  browsing/cancellation and rejection of cross-chat approvals. Its SQLite handles
  explicitly close after transaction completion for Windows cleanup.
- The full complementary Swift package run passed 274 executed cases with six
  opt-in live cases skipped; the separate native window run passed two more cases.
  Its final summary reports 280 cases, including the six skips. The installed
  read-only Swift history and runner checks then passed 2/2 against the actual
  Windows runner. Native window tests and HTTP fixture tests use complementary
  processes because their combined host can exit before async tests finish.
- The idle-checked Developer installer rebuilt both hosts and reconnected to
  Windows. All six queue entries were preserved exactly, with queue SHA-256
  `f15d2a8c6e8f5fdb0f8960feb59589bf3a88e3c7b9c83d75668819fb0c2afe31`;
  saved connection and model settings were unchanged. The installed runner
  advertises history support. Four retained conversations across the five-project
  workspace reopened through the installed API and real Swift model without
  creating chats, starting inference or changing projects.
- The installed Mac executable matches the built product after normalizing the
  bundle's ad-hoc signature on disposable comparison copies. This installation
  does not establish signed distribution or notarization.
- Native UI automation could not obtain the window: after the owner unlocked
  the Mac, its helper repeatedly closed its pipe before returning UI state.
  The owner was asked to reopen the updated app. Rendered layout, keyboard
  navigation and the installed running-reply/approval presentation remain
  unverified; the reviewed mockup and native/model tests are separate evidence.
- The broader local gate passed its preliminary contracts, formatting and
  workspace clippy, then encountered repeated native process startup delays in
  the Mac workspace test run. Its relay suite passed all ten cases after 431
  seconds of delayed code-signing inspection; a later process sample remained
  entirely in `_dyld_start` before test initialization. The identified gate
  process groups were stopped with SIGTERM (exit 143), retaining partial logs and
  the sample. A complete workspace/packaging/local-gate pass is not claimed.

No commit, push, hosted publication or production service deployment was requested
or performed for this slice.

Local raw validation logs, source fingerprints and the startup sample are retained
under `target/developer-chat-history/validation-8591d515/`.

## Restoration source and current evidence

The September 8 source was left uncommitted and later replaced by a partial
implementation. Commits `cc21f2a` and `129d4f1` preserved that later recreation,
which did not implement the approved reading pane, rename, drafts, pagination
and exact action bindings. Restoration uses the original thread's file-change
records for Swift and the frozen Windows source archive, compared with retained
source fingerprints. It preserves later unrelated application changes and must
also accept retained chats written by the replacement schema without deleting them.

Current restoration checks:

- The frozen backend archive matches all 213 retained source fingerprints.
  The original Swift history model, chat view and history tests were recovered
  from the original file-change records. Later Settings/GitHub controls remain.
- Windows Developer binary tests pass 78/78, with no compiler warnings. The
  native history, chat and repair-escalation process E2Es pass on the restored
  source. New migration tests cover the replacement JSON schema, exact old
  request/repair hashes and ownership rejection with rollback.
- Swift history, chat, attachment, runner and repair tests pass. The two AppKit
  window tests pass in a separate process. A full complementary Swift run passed
  243 executed cases with four opt-in cases skipped; subsequent focused checks
  cover the restored adjacent repair binding and project-chat tests.
- Independent high-risk review approved the final source with no remaining
  findings. It checked migration, exact approvals, stale response handling and
  invalidation of prior build evidence after approved side-chat tool mutations.
- Docs drift, CI registration and whitespace checks pass. The existing Cargo
  native workflow suite now runs the history E2E on both hosted platforms.
- The Developer app and Windows runner rebuilt successfully and reconnected.
  Queue content and connection/model configuration fingerprints are unchanged.
  The installed Swift history/runner read-only checks pass 2/2. The local model
  became healthy after its initial startup timeout; no fallback was selected.
- Workspace strict clippy passes after a named migration row type and equivalent
  iterator cleanup in the existing review sanitizer. The final full local gate
  remains in progress. Native UI automation twice failed before returning a window with
  `Sky Computer Use native pipe closed before response`; rendered layout remains
  unverified. The original source recovery and native tests are separate evidence.

Final main-branch publication and gate closeout are pending. Historical validation counts
above apply only to the original September 8 run. The production service remains
unchanged: this work targets the separately installed Developer app and runner.
