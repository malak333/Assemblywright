# Scalable Developer repair and complete review

Status: the prior standalone source snapshot was independently reviewed,
validated by the full local gate and native Windows workflows, and installed
in the Developer runner. This rebased release branch has additional changes
and requires its own exact-head gate, independent review, and installation
evidence before those claims apply to it. The recovered website still
requires fresh validation, browser acceptance, and independent provider approval;
feature acceptance remains open. See the separate
[validation evidence](developer-scalable-repair-validation.md).

## Problem and acceptance

The `aw-fft-demo` recovery exposed three incompatible limits: repair context
failed at 128,000 bytes, independent review rejected cumulative candidates above
40 files, and tool mutation capture could not admit a PNG to text-only review.
Generated `site/` files consumed context and review capacity. A 1-by-1 PNG also
demonstrated why file existence and passing unit tests do not establish that an
approved map requirement is satisfied.

The owner approved targeted context, complete batched review, genuine image
review, and safe automatic recovery. The same implementation must support new
features, preserve existing failed-feature evidence, and recover the current
feature through unchanged validation and independent review. No requirement,
protected test, review gate, or per-feature escalation budget may be weakened to
make a candidate pass.

## Context and inventory

Windows retains authority over a bounded, complete project inventory. Inventory
records bind admitted paths to exact content hashes, byte lengths, types, and
source/generated classification. Omitted content is explicit; selection is never
presented as full project contents. Sensitive or unsupported entries retain
appropriate protected evidence without disclosing secret contents to providers.
Hard inventory capacity failure stops before project mutation rather than
silently dropping entries.

Repair prompts prioritize the actual failure, approved plan, source, and relevant
tests. Generated output remains in the inventory and candidate evidence but does
not displace relevant source in the initial text budget. Bounded retrieval of
additional files or portions uses the frozen inventory and validates exact
bindings. A stale inventory must be refreshed before proposing changes.

## Complete independent review

The runner freezes one cumulative candidate, including earlier failed repairs.
It partitions evidence deterministically into bounded review packets. Each
packet binds the complete candidate identity and its ordinal and total count.
Every expected file or asset must be covered exactly once; missing, duplicate,
reordered, stale, or mismatched evidence cannot yield approval.

Every batch must receive an independent decision. Final aggregate review binds
the complete manifest and ordered packet/decision receipts, the approved plan,
and exact validation evidence. A final approval requires all batch approvals and
the aggregate decision. Batching never means skipping cross-file assessment or
accepting partially reviewed bytes. Current file hashes are rechecked before
completion or publication. Legacy evidence remains readable without making old
approvals authorize a new candidate.

## Image evidence

Supported raster assets are captured as exact bytes with their content hash,
validated media type, decoded dimensions, and bounded encoded/decoded sizes.
Images are supplied through the reviewer's actual image input, not represented
only as base64 text or a filename. The reviewer receives the relevant approved
requirements and associated source context to judge usefulness and correctness.
Dimensions alone do not prove that a map is accurate or meaningful.

Corrupt images, unsupported formats, path escapes, links/reparse points, changed
bytes, unsupported visual transport, and missing asset review fail closed. Image
staging is private and temporary, preserves the existing tool-free review
boundary, and participates in cancellation and process cleanup.

The response schema uses discriminated `anyOf` entries for text and assets,
matching the provider's [supported Structured Outputs schema subset](https://developers.openai.com/api/docs/guides/structured-outputs#supported-schemas).
Native fixtures verify bindings and transport, while a real provider run remains
necessary to prove provider acceptance.

## Recovery and operation

Preflight detects capacity and admission problems before effects. The runner
automatically selects bounded context and batches complete review; it does not
ask the owner to make routine packet-sizing decisions. Retries are finite and
limited to failures proved safe to repeat. Unknown process effects, incomplete
cleanup, changed authority, emergency pause, credentials, and unsupported data
remain holds requiring a concrete recovery decision.

Recovery of an existing tool-effect hold must reconcile the exact retained
mutation evidence with current bytes under the new admission rules. It preserves
history and counters and proceeds through fresh validation and review; it must
not erase quarantine evidence or replay the original tool effects. No automatic
provider fallback, budget reset, or validation-command change is authorized.
Where legacy evidence contains only an unreviewable path, it cannot prove the
current image bytes. Recovery requires explicit adoption of a newly displayed
exact snapshot digest. Swift includes that digest in the existing Resume
confirmation; Windows recomputes it before adopting the new candidate. This is
fresh owner-directed recovery evidence, not retroactive tool provenance.

Legacy tool history can contain dependency-cache files and obsolete outputs.
Adoption uses the complete current inventory plus retained cumulative review
files; it does not recreate obsolete ledger-only paths or feed cache binaries to
image review. Current retained files remain mandatory. Project-chat mutations
use normal deterministic project targeting without rewriting their original
attribution. A failed admission exposes a bounded reason in the Developer UI.

## Automatic repair preparation with generated outputs

Live review exposed a further boundary: a source-only JSON repair can change a
generator or template while leaving previously generated pages stale. Automatic
repair preparation must support the existing selected Windows tool model in a
disposable project copy, so it can correct source and run the actual build before
the combined proposal is authorized. Tool work is directed to the disposable
copy under the existing owner-selected tool access. Complete live snapshots
before and after preparation detect drift and hold authorization for inspection
of the live workspace. This extends the existing staged ordinary-repair path;
it does not grant a new provider, tool access level, or publication capability.
As with the existing Developer build, the copy is not OS-enforced containment:
Full access runs under the owner account, and snapshot checks cannot prove the
absence of transient or external effects. The separate production containment
requirements remain deferred under the documented Developer exception.

The proposal must retain the exact combined text and supported image bytes,
current live baselines, complete review findings, protected inputs, project
snapshot, and cancellation/epoch bindings. One proposal digest authorizes the
whole resulting change. A separate application manifest records the exact
workspace after durably recorded progress without changing that immutable
proposal digest. Resuming partial application must match this recorded state and
its original one-shot request, unique preparation and authorization receipts,
candidate digest, feature attempt, and current policy. An unexpected restart
may continue only remaining paths after those proofs are rechecked. Explicit
Stop or Emergency Pause revokes continuation. Unrelated drift stops further writes. Drift, protected-input changes, unsupported binaries,
deletions without a supported contract, uncertain cleanup, or missing tool
approval remain holds. Application remains restart-safe; immutable validation
and complete independent review still follow it. A tool response claiming a
successful build cannot substitute for those gates. The bounded JSON path may
remain available where tools are unavailable, but cannot claim that it executed
a build or synchronized generated output.

Temporary execution has its own durable lifecycle and mutation ledger. Creating,
copying, running, capturing, and cleaning a stage must never advance the live
workspace revision or appear as live project effects. Every exit either verifies
cleanup or retains an actionable hold; startup cannot delete a stage whose tool
processes may still be running. Registration precedes copying so partial copies
remain discoverable after interruption.

Generated files omitted from the disposable copy still use their exact current
live hashes as application baselines. The unchanged complete project snapshot
must be checked again before authorization. Access mode and revision, selected
model, runtime and tool catalog, feature epoch, protected inputs, and complete
review findings are bound to the candidate and rechecked at the effect boundary.
Public proposal status exposes image hashes, type, and dimensions without image
payloads; automatic candidates cannot be manually approved through the legacy
text-only UI. Durable payload retention is bounded and compacted only when the
exact candidate and interruption-recovery evidence are safely retained elsewhere.

The staged authority fields require Developer queue format `queue_v12`. Forward
migration accepts `queue_v11` and saves its original snapshot before rewriting;
older runners must reject the new queue rather than interpret an active typed
candidate as an empty legacy text proposal. This is Developer persistence
versioning, separate from the production wire protocol and master schema.

The native boundary and deployment evidence is recorded separately from the
remaining live website acceptance in the validation document.

### Interrupted review recovery

An explicit Stop remains a stop. When a staged candidate was fully applied and
Stop interrupted its validation or review, a new owner-bound Resume may adopt
the exact retained result for fresh validation and independent review. This is
not continuation of the interrupted application authorization. The original
interrupted proposal, attempts, escalation count, and receipts remain intact;
recovery must not replay file writes or invoke implementation again.

The recovery digest must bind the feature, project, current checkpoint, policy
and execution identities, retained candidate, and complete private application
snapshot. Compacted payloads require reconstruction and verification of the
original candidate digest and its authorization/interruption receipts. Partial
application, unrelated drift, missing evidence, active tools, stale bindings,
or a new Stop/Emergency Pause keep recovery unavailable. Cancellation must
dominate through the final launch boundary, including after adoption persists.

Staged build instructions identify the checked prepared project interpreter
when available and use its installed dependencies with the disposable project
as the working directory. They prohibit global/live environment installation
and repeated dependency-download retries, and describe the actual host shell.
This is model guidance under the existing Developer Full-access exception;
virtual environments are omitted from editable review entries and the staged
copy, but their bytes remain covered by recursive aggregate effect fingerprints.
The guidance is not an OS read-only guarantee.

Immutable validation must not silently advance the private application snapshot.
Staged automatic validation receives `PYTHONDONTWRITEBYTECODE=1` and a fresh,
absent project-contained Python cache prefix. Ordinary Python imports therefore
avoid stale conventional bytecode and incidental cache writes. This environment
guidance is not proof against commands that explicitly ignore or replace it.
A fully applied staged candidate must match its durable private snapshot
immediately before validation and after confirmed process cleanup; complete
entry maps must remain identical. Any unexplained delta is an operational hold,
including explicitly written bytecode, dependencies, or generated output.

The staged validation evidence digest binds its exact proposal, application,
candidate, command, policy, access/runtime/archive identities, successful exit,
and equal pre/post private snapshots. The existing application checkpoint remains
immutable. The runner rechecks it under the effect gate through review-packet
construction and persistence of pending review evidence, closing the gap between
validation and review. Restart must validate again when complete review admission
evidence is absent. These guarantees do not prove validation had no external
effects under the documented Developer access exception.

After reviewer outages and retries, interrupted adoption may account for later
automation epochs using retained unavailable/interrupted review attempts whose
v2 packet and batch digests match the exact current candidate. This is a bounded
upper limit on supported transitions, not reconstruction of an exact event
ledger. The owner request still binds the current epoch and complete recovery
digest; unchanged private effects, candidate receipts, policy, access, runtime,
archive, idle tools, and fresh validation/review remain required. This allowance
authorizes no model call or replay of application writes.

## Required proof

- Focused unit coverage for selection/retrieval, complete inventory, batch
  boundaries and bindings, real image decoding/transport, state migration,
  preflight, safe recovery, cancellation, and unsupported or stale evidence.
- Native runner/HTTP/process E2E on macOS and Windows with projects larger than
  the previous context and 40-file limits, assets, and retained failure history.
- Independent complete-diff review and canonical repository validation.
- For `aw-fft-demo`, a meaningful original or permitted map with source notes,
  unchanged existing tests, successful full build, browser navigation/search/
  image checks, responsive and no-JavaScript checks, and real independent review.
- Exact deployed Windows binary/source evidence and explicit recovery outcome.
  No hosted publication, signing, notarization, or production readiness is
  implied by local Developer evidence.
