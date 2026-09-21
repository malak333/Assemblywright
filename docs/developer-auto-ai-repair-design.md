# Developer Auto AI Repair

Status: implemented in the repository working tree; focused Mac fixture evidence
exists, while native Windows, installed-app, visual UI, full-gate, hosted, signing,
notarization, live-device, and production evidence remain separate.

## Understanding summary

- Add an **Auto AI repair** toggle beside **Auto-run next feature**, with a
  conditional **Max escalations** control.
- The limit is per feature, accepts `1...100`, defaults to `100`, and counts
  both manual and automated escalations.
- Auto AI repair operates independently of Auto-run. Enabling it on an eligible
  failed feature starts repair immediately.
- Each eligible failure first receives the existing three ordinary repair
  attempts. Automatic AI escalations then continue until success or the limit.
- Only validation failures and independent Codex review rejections trigger
  another repair. No-op and duplicate candidates still consume the budget.
- Repairs are immediate and serial, use the feature's saved model computer, and
  may modify any admitted project file while the validation command stays frozen.
- Success still requires unchanged validation and independent Codex approval.
  Limit exhaustion and operational or ambiguous failures stop safely and retain
  manual recovery.

## Purpose and scope

This Developer-only extension lets one owner authorize a bounded automatic
repair loop for the first unfinished feature. It removes the existing per-proposal
owner approval step only while the explicit Auto AI repair policy is enabled.
Windows remains authoritative for settings, feature state, counters, checkpoints,
file application, validation, review, cancellation, recovery, and audit evidence.
Swift presents revision-bound controls and status.

This design does not activate the protected production runtime. It does not add
parallel features, automatic provider fallback, validation-command rewriting,
feature removal, queue reordering, automatic publication recovery, credential
access, or direct model-tool authority outside the bounded project workspace.
As in the existing Developer runner, however, the owner-selected validation process
runs under the Windows owner account and is not hostile-process containment.

## Requirements and assumptions

- One active feature and one inference, validation, or review operation remain
  the system-wide execution limit.
- Consecutive repair cycles begin immediately and execute strictly serially.
- Existing workspace containment, secret rejection, bounded durable state,
  owner-debugging logs, Stop, and Emergency Pause controls remain mandatory.
  This Developer runner does not claim the production append-only audit or fully
  redacted validation-log boundary.
- "Any project file" means any admitted file under the canonical project root.
  It excludes `.git` internals, credentials, secret-bearing files, runner state,
  out-of-project paths, and link or reparse-point escapes.
- The feature's saved Windows/Mac model target performs repairs. There is no
  fallback or separate automatic-repair model selector.
- The independent Codex reviewer and its saved feature binding remain unchanged.
- Existing installations migrate to Auto AI repair off and maximum `100`.
- Each model, validation, and review step retains its existing finite deadline.
  Quota or entitlement failure is operational and enters `held`; it is never retried
  as a code failure.

## Architecture and authority

The Windows Developer runner's persisted Assembly Line state gains:

- `auto_ai_repair_enabled: bool`, default `false`
- `auto_ai_repair_max_escalations: u32`, default `100`, range `1...100`

Each feature gains a durable `auto_ai_repair_limit` snapshot. The runner stores
the current maximum when the feature first begins execution. If an already-active
or failed legacy feature has no snapshot when automation is enabled, Windows
atomically records the current maximum before starting its loop. Later global
maximum changes affect only features without a snapshot.

Enablement remains a live global owner policy. Turning it on can immediately
start an eligible failed feature. Turning it off cancels active work and pauses
the feature. Re-enabling uses the same limit and cumulative counters; it never
replenishes the budget.

Each feature also records an explicit automatic-repair lifecycle and generation:
`inactive`, `running`, `held`, `limit_reached`, or `quarantined`, plus a monotonic
epoch and bounded reason. Only `running` is eligible to launch another operation.
An operational failure enters `held`; an explicit Resume while the policy remains
enabled may clear that hold. Limit exhaustion and quarantine cannot be cleared by
ordinary Resume. Turning the policy off atomically advances the epoch and records
cancellation intent before process termination, so late completions from the old
epoch cannot restore eligibility.

A single authenticated, revision-bound settings mutation carries both values and
updates them atomically. Success requires the persisted runner revision to advance
exactly once and the returned values to match the request. Stale, malformed,
replay-conflicting, or out-of-range requests change nothing.

Swift never owns the repair loop. Closing or disconnecting the app does not stop
authorized work. Manual and automatic paths share the runner's existing global
inference and execution gates.

## Repair-loop data flow

When validation fails or Codex rejects a candidate, Windows evaluates the feature
against exact durable state:

1. If automation is disabled, retain the normal failed state.
2. If fewer than three ordinary repair attempts have been reserved, reserve and
   run the next existing ordinary repair.
3. Otherwise compare cumulative `escalation_count` with the feature snapshot. If
   exhausted, enter `limit_reached`, stop automatic work, and block Auto-run
   advancement.
4. Reserve the next escalation and increment `escalation_count` before inference.
   Manual and automatic escalations use this same counter.
5. Generate and hash a frozen proposal, revalidate all bindings, apply its exact
   bytes, run the immutable validation command, and request fresh Codex review.
6. On validation failure, review rejection, no-op, or duplicate candidate, finish
   the attempt evidence and begin the next eligible cycle.
7. On validation and review success, continue through the existing local-only or
   publication completion path. Auto-run alone decides whether the next feature
   begins.

Automatic escalation does not require a saved project-chat diagnosis. Its frozen
packet binds approved requirements, immutable validation, current failure or review
evidence, cumulative feature baseline, feature and checkpoint identities, runner
revision, selected model, attempt, policy revision, limit snapshot, and bounded
workspace state. Manual proposals keep their chat-diagnosis binding. Evidence
marks the source as `manual_chat` or `automatic_failure`.

Automatic policy authorization replaces the owner-click receipt; it is not a
fabricated manual approval. Its durable receipt binds the proposal digest, policy
and runner revisions, feature, checkpoint, attempt, limit, model, and exact project
state. Application repeats all before-byte, path, protected-input, and drift checks.

The snapshotted maximum is the feature's absolute cumulative escalation cap.
Earlier manual escalations reduce the remaining automatic budget. Once the counter
equals the snapshot, neither automatic nor manual AI escalation is admissible.
A manual prepare request that arrives at the shared cap, or without retained
escalation or review evidence capacity, durably commits the resulting
`limit_reached` or `held` lifecycle and plain reason in the same mutation before
its rejection returns, and makes no model call. Only owner-directed non-AI
correction followed by bounded revalidation, reviewer correction, or removal
remains available. Enable/disable cycles never replenish escalation authority.

Current implementation ceilings must be reconciled before the feature is enabled.
The absolute lifetime escalation ceiling becomes 100. Each reserved escalation owns
exactly three bounded escalation-ledger slots:
proposal outcome, policy/manual authorization, and application outcome. The ledger
capacity is therefore `3 * 100 = 300`. Review history reserves one bounded slot for
the original attempt, three ordinary repairs, and every lifetime escalation, for
`1 + 3 + 100 = 104` records. Unused reserved review slots terminate as `not_run`.
Reservation of all four slots is atomic and precedes inference. Each record has
fixed field and summary limits; proposal file bytes live only in the current frozen
proposal and cumulative review manifest, not duplicated history. The existing
40-file cumulative review ceiling remains an independent workspace bound: reaching
it enters a visible operational hold rather than pretending the configured attempt
limit was exhausted.

## File authority and independent review

Automatic proposals may change source, tests, and project configuration. They may
not change the immutable validation command or any excluded path or material.
Every changed test or validation-related file is explicitly marked in the Codex
review packet. Review compares the cumulative candidate with each file's earliest
feature baseline and must reject weakened or deleted coverage, hidden skips,
validation bypasses, or departures from the approved requirements.

Project context and proposal admission stay bounded. A truncated context is not a
complete mutation manifest. Only exact admitted paths with verified before bytes
may be included in the frozen proposal.

Freezing the validation command freezes its exact command string, not the behavior
of files that command loads. Automatically changing `package.json`, build scripts,
test harnesses, or equivalent configuration can cause the subsequent validation
process to perform different owner-account actions, including effects outside the
project. This Developer runner has no hostile-process or external-effect containment
for that process. The Auto AI repair control is therefore an explicit acceptance of
the same owner-account execution risk as existing Developer validation, amplified
by unattended configuration edits. The UI and documentation must state this; no
production-sandbox or workspace-only effect claim is permitted.

The three ordinary repairs deliberately retain their current narrower tool and
protected-test restrictions. The owner-approved broader any-project-file authority
begins only with policy-authorized escalation; runtime-derived tool and capability
restrictions remain in force throughout both paths.

Durable automatic-repair evidence stores identifiers, hashes, source kind, counters,
statuses, and bounded sanitized summaries or errors. It does not introduce a new
production audit claim or place raw validation output into the state snapshot.
Existing unredacted Developer validation logs remain owner-only debugging material
under their current retention and access boundary. Secret-shaped model output is
rejected or sanitized before durable summary persistence as appropriate; excluded
secret-bearing project material is never admitted to the repair packet.

## Failure, cancellation, and recovery

- No-op and previously attempted candidates consume an escalation and continue.
- Malformed provider output, provider unavailability, persistence failure,
  workspace drift, an invalid path, publication failure, or other operational
  failure consumes any already-reserved attempt but stops for manual recovery.
- Limit exhaustion keeps owner-directed non-AI correction/revalidation,
  reviewer-change, and removal recovery available; it never permits another AI
  escalation, removes, advances, or automatically re-enters the feature. All
  actions remain subject to exact lifetime evidence and storage bounds.
- Stop, Emergency Pause, disabling Auto AI repair, shutdown, and cancellation
  first persist intent and advance the feature's automatic-repair epoch, then signal
  active work. Completion from the prior epoch is rejected. Applied bytes and
  evidence remain preserved; unconfirmed termination enters quarantine.
- A durably ready and unapplied proposal may continue only when its digest and
  every bound byte still match.
- A proposal-generation call interrupted before any write is recorded as
  interrupted and may be followed by a newly reserved attempt.
- Interrupted application, validation, review, persistence, publication, or any
  uncertain external effect is quarantined and never retried automatically.
- Completed receipts are reused only when every exact binding remains valid.
- An operational failure durably enters `held`, preventing restart or status polling
  from silently relaunching work. Only an explicit admissible owner action clears it.

## Owner interface

The Assembly Line header presents the independent controls together:

`[ ] Auto-run next feature   [ ] Auto AI repair   Max escalations [100]`

Auto AI repair uses the same checkbox-style Swift `Toggle` as Auto-run. The compact
numeric control accepts only whole numbers from 1 through 100. It remains visible
and editable while automation is off, allowing the owner to choose the limit before
the enabling mutation snapshots it for an already-failed feature. Changing either
repair setting sends both authoritative values in one request. The UI treats them as pending until Windows
returns exact values with revision `+1`; a conflict refreshes authoritative state.

Nearby help says that enabling Auto AI repair may immediately change files in the
failed project and that validation runs under the Windows account, so modified build
or test configuration can change its effects. The toggle itself is the durable
authorization; there is no second confirmation.

The active feature reports phases such as ordinary repair `2 of 3`, escalation
`7 of 100` preparing, validating, or awaiting Codex review, and limit reached.
Manual mutation controls are disabled while automatic work is active. Stop,
Emergency Pause, and the Auto AI repair toggle remain available. Controls require
keyboard focus, accessibility labels and value announcements, and stable layout
for three-digit values.

Status also exposes elapsed time for the active step and cumulative model,
validation, and review attempt counts. It makes no completion-time estimate because
project commands and provider latency are unbounded beyond their individual finite
deadlines. Quota, entitlement, and deadline failures show the exact operational hold
instead of consuming repeated automatic retries.

Limit exhaustion is shown as: `100 of 100 AI escalations used. Automatic repair
stopped. Correct the project without AI and revalidate, change the reviewer when
applicable, or remove the feature.` Internal lifecycle values such as `held` and
`quarantined` are never the only owner-facing explanation; each status includes a
plain reason and the exact admissible next action.

## Testing and evidence strategy

Focused Rust tests cover migration defaults, input bounds, atomic revision and
replay behavior, feature snapshots, shared counters, ordinary-to-escalation
transition, trigger classification, duplicates and no-ops, limit exhaustion,
Auto-run independence, cancellation precedence, recovery, cumulative baselines,
and every prohibited path or mutation class.

Swift tests cover decoding, compatibility, the toggle and numeric field, pending
and conflict states, exact acknowledgements, cancellation presentation,
accessibility, and phase messages.

Native Windows process E2E uses deterministic model and reviewer fixtures with a
real bounded workspace, file writes, validation processes, HTTP controls, and
persistence. It covers success after multiple cycles, the 100-attempt boundary,
review rejection, app disconnection, toggle-off cancellation, ready-proposal
recovery, ambiguous-application quarantine, and automatic source/test/configuration
changes with immutable validation.

Required native macOS UI validation covers narrow and wide layouts, keyboard
navigation, three-digit input, cancellation, and status readability. This is a
native Swift surface; browser and Playwright matrices do not apply. Repository
validation, installed Windows behavior, visual evidence, hosted publication,
signing, and production readiness remain distinct proof layers.

The implemented native process harness is
`scripts/developer-runner-auto-repair-e2e.py`. It uses the real Developer runner,
authenticated HTTP, SQLite migration, disposable project files, validation
subprocesses, and deterministic model/reviewer fixtures. Its current Mac run
reports 21 proof keys covering migration defaults and atomic control, three
ordinary repairs before automatic escalation, source/test/configuration edits with
an unchanged validation-command string, review-rejection continuation, configured
and absolute caps including exactly 100 automatic calls with no 101st call,
app-independent polling, operational hold, disable/Stop late-result rejection,
clean pre-effect restart, and validation/review restart quarantine without replay.
Its shared-limit scenario drives the public authenticated routes the Mac app uses:
project chat plus an owner-approved manual escalation consumes the one shared
per-feature cap; a later automatic opportunity stops at `limit_reached` at that
cap without any repair-model or reviewer call; the owner corrects the project
bytes directly; and the validation-only Resume proceeds without AI through fresh
validation and one fresh independent review that succeed. Fixture results do not
prove live model quality, account entitlement, Windows-native or installed-app
behavior, rendered visual placement, Windows Job behavior, signing or notarization,
hosted checks, live-device QA, or production readiness. Native Windows and visual
macOS validation have not yet been run for this implementation and remain separate
evidence layers.

## Risks acknowledged

- Removing per-proposal approval allows a local model to rewrite tests and project
  configuration unattended. Exact project containment, cumulative review, frozen
  validation, bounded attempts, and immediate owner controls mitigate but do not
  eliminate destructive or low-quality edits.
- A maximum of 100 cycles can consume substantial time and compute. Serial execution,
  visible counters, cancellation, and the hard ceiling bound this exposure.
- Allowing duplicate and no-op attempts to continue can waste the full budget. This
  is an explicit owner decision and must remain visible in evidence and status.
- A persistent enablement setting can begin work while the Mac app is closed. The
  Windows-owned status, durable audit, Stop, Emergency Pause, and restart quarantine
  are therefore required rather than optional UI behavior.
- The configured attempt maximum is not a promise that 100 attempts can always run.
  Independent file, storage, secret, model, and recovery bounds stop earlier and
  must report their exact hold reason.

## Decision log

| Decision | Alternatives considered | Rationale |
| --- | --- | --- |
| Fully automatic proposal generation and application | Retain per-attempt approval; automate ordinary repairs only | The owner selected unattended repair after explicitly enabling the policy. |
| Windows-owned state machine | Mac UI loop; separate repair subsystem | Preserves durable authority and recovery without a second controller. |
| Per-feature limit of 1...100, default 100 | Per-run or lifetime budget; unbounded input | Matches the requested default while retaining a hard safety ceiling. |
| Snapshot maximum per feature | Live mutation of the active budget | Prevents mid-feature budget drift and replay ambiguity. |
| Auto repair independent of Auto-run | Require both settings | Separates current-feature recovery from queue advancement. |
| Three ordinary attempts before escalation | Escalate immediately; keep ordinary attempts manual | Preserves the existing low-cost repair sequence. |
| Manual and automatic escalations share one counter | Separate or resettable budgets | Prevents enable/disable cycles from replenishing authority. |
| Immediate serial cycles | Cooldown or adaptive backoff | The owner preferred minimum latency while retaining one-operation concurrency. |
| No-op and duplicate attempts continue | Stop immediately or after three repeats | The owner explicitly chose to consume budget and continue. |
| Allow all admitted project files | Preserve manual approval for tests; source-only repair | The owner explicitly authorized unattended source, test, and configuration changes. |
| Frozen validation and independent Codex review | Permit validation edits; accept model self-review | Retains objective completion and separation of implementation from review. |
| Feature's saved model with no fallback | Dedicated selector; automatic fallback | Keeps routing deterministic and preserves saved feature bindings. |
| Disable cancels immediately | Finish the cycle; affect later features only | Gives the owner an immediate stop control. |
| Limit exhaustion preserves manual recovery | Lock repair; remove and advance | Failure must not silently discard work or bypass owner recovery. |
| Exact-checkpoint restart recovery | Blind retry; always require Resume | Allows safe continuation while quarantining uncertain effects. |

## Structured review objections and resolutions

| Reviewer objection | Resolution |
| --- | --- |
| A disabled maximum field prevented choosing a lower limit before an enable-and-start mutation. | Accepted. The field remains editable while automation is off, and enablement sends the chosen value atomically. |
| Limit exhaustion claimed manual recovery without defining an admissible transition. | Accepted and tightened after user-advocate review. The snapshot is the absolute shared escalation cap; recovery after exhaustion is non-AI only. |
| Operational stops lacked a durable state and late-result ordering. | Accepted. Added lifecycle, monotonic epoch, persisted cancellation intent, explicit Resume recovery from `held`, and quarantine on uncertain termination. |
| Current limits of 20 escalations, 80 history records, and 40 cumulative files conflict with a configured maximum of 100. | Accepted. Implementation must raise attempt/evidence capacity with pre-reserved bounded terminal evidence; the 40-file bound remains independent and produces a truthful hold. |
| The design incorrectly implied production-grade redaction and audit for Developer validation logs. | Accepted. Durable state stores bounded sanitized summaries and hashes; existing owner-only unredacted validation logs retain their explicitly weaker boundary. |
| Existing ordinary repairs cannot change protected tests although escalations may change any admitted project file. | Accepted clarification. Ordinary attempts retain their narrower current restrictions; the broader authority starts only at automatic escalation. |
| Evidence capacity remained inconsistent with post-limit manual recovery. | Accepted and tightened after user-advocate review. The feature has an absolute shared cap of 100; atomic admission reserves three escalation-ledger records and one review record per attempt, with exact capacities of 300 and 104. |
| Mutable project configuration can change the effects of an unchanged validation command under the owner account. | Accepted. The design now disclaims hostile-process and workspace-only effect containment, exposes the risk at enablement, and retains finite per-step deadlines and operational holds. |
| A 101st manual AI attempt contradicted the owner-approved hard ceiling and made the UI misleading. | Accepted. The configured snapshot is the absolute shared manual/automatic cap; post-limit recovery is non-AI and status states the allowed next actions plainly. |

## Disposition

The owner confirmed the understanding lock, selected the Windows-owned approach,
and accepted the architecture, repair flow, safety/recovery rules, interface, and
testing strategy section by section. Sequential skeptic, constraint-guardian, and
user-advocate reviews raised the objections recorded above; every objection was
accepted and resolved. The independent arbiter found no unresolved blocker and
declared the final design **APPROVED**. The owner then approved implementation setup.
The repository working tree now contains the Windows-owned state machine,
revision-bound Swift controls, focused Rust/Swift coverage, and the native process
harness. The ordered implementation record is
[`developer-auto-ai-repair-implementation-plan.md`](developer-auto-ai-repair-implementation-plan.md).
