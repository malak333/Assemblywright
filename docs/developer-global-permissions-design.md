# Global permissions and project navigation

The owner requested a repository selector for feature planning, a separate tab
for succeeded Assembly Line results, and a global permission setting beside the
GitHub and AI settings controls. This extends the supervised owner-account
workflow; it grants no authority to the protected-service runtime.

## Project selection and results

Feature planning selects an applicable existing project from the Windows runner's
project list. These are the local projects available to implementation, including
projects connected to GitHub; the menu does not implicitly clone or connect
arbitrary repositories from the owner's GitHub account. A separate explicit new
project choice retains folder-name entry. Selecting a project cannot enqueue a
feature, execute tools, create a remote repository, or publish files.

The Assembly Line defaults to its active-work tab. Succeeded features appear in
a separate Succeeded tab with their existing validation, review, and publication
evidence. This is presentation filtering, not deletion or archival of durable
queue records. Failed, paused, quarantined, and queued work stays discoverable in
the active tab. Filtering cannot alter next-feature selection or advancement.

## Global permission authority

Windows stores one global tool-access policy for the supervised runner. The
Permissions control sits beside GitHub and Settings. New conversations, projects,
and feature execution use this policy; navigation and New Chat never reset it.
The chat panel displays the effective global choice instead of providing a
project-specific override.

The initial choice remains **Ask for approval**. **Approve for me** and **Full
access** retain their existing tool-policy meanings. Approve for me still asks
for guarded operations such as shell commands; Full access permits tools under
the owner's Windows account. These modes do not replace Windows account rights,
authorize review or publication, or remove cancellation and Emergency Pause.
An approval prompt describes a policy decision, not proof that the operating
system denied access.

The owner saves changes explicitly while all work is idle. The authoritative
endpoint authenticates the owner, checks the observed permission revision, and
rejects active work, Emergency Pause, unresolved tool effects, and publication
barriers, including a runner whose shutdown has started. A stale dialog must reload before saving. Saving cannot start work or
approve a pending action. An unsupported or malformed server snapshot must not
make a writable permissions control available.

Migration retains legacy project-specific rows as historical evidence but does
not infer broader global authority from them. The global policy starts at Ask,
and its revision must exceed retained legacy execution/approval revisions so
stale bindings cannot become valid by revision collision. Restart preserves the
saved global policy and its revision. Existing staged candidates must still
match their exact access mode and revision before execution or adoption.
Startup requires the singleton policy to match the latest policy-history entry
exactly. A missing policy with retained history, a mode mismatch, or rollback to
an older formerly valid revision fails closed instead of resurrecting authority.

## Validation and evidence

Unit coverage exercises project selection and empty/stale choices, succeeded
filtering without queue mutation, global snapshot decoding, mutation receipts,
default/migration/persistence, compare-and-set failure, idle and pause guards,
and stale approval/execution bindings. Native process E2E exercises the
authenticated runner API and persisted policy across projects and restart.

The source and native fixture checks are separate from installed Mac UI,
Windows-native operation, hosted GitHub checks, deployment, signing,
notarization, and live model evidence. No browser surface is involved.
