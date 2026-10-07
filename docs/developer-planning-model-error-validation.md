# Developer planning model-error validation

## Contract

Developer planning and independent review invoke the exact model bound to the
Windows-owned request. An account/model compatibility rejection remains a
provider failure: it creates no planning response or review decision, grants no
approval, performs no fallback, and never silently changes the saved model.

The Codex child-process boundary may receive provider stderr containing request
metadata, prompts, paths, or credentials. The master drains stdout and stderr
concurrently so either pipe can fill without deadlocking the child. It retains
only the first 16 KiB of stderr in zeroizing memory and drains all later bytes
without retaining them. The fixed read scratch buffer also zeroizes when its
reader future is aborted during cancellation or timeout. On Windows, the
contained launcher forwards Codex stderr only through its inherited pipe to the
master. Raw stderr is never returned, persisted, audited, logged, hashed into
evidence, or included in a diagnostic.

Only this exact selected-model signature is recognized:

```text
The '<selected model>' model is not supported when using Codex with a ChatGPT account.
```

That signature produces one fixed owner-facing message directing the owner to
choose a supported Orchestrator model and start a new planning session, or use
Change reviewer for an existing queued feature. Every other nonzero exit, empty
stdout, wrong-model signature, truncated signature, or altered provider wording
keeps the generic exited-without-decision result. Catalog visibility remains
separate from account entitlement.

## Safety behavior

- Planning/action separation is unchanged. The diagnostic cannot enqueue work,
  approve a plan or review, edit a project, or advance the queue.
- Cancellation and the 15-minute deadline retain priority. They terminate the
  existing process group or Windows Job and abort both pipe readers without
  converting the outcome into an account/model diagnostic.
- A completed failed child must still have no surviving contained descendants
  before its failure is retryable. Unconfirmed descendants remain unavailable.
- The exact model and reasoning binding remain unchanged. Recovery is an explicit
  owner model selection followed by a new or explicitly rebound request.
- Audit and durable evidence retain the existing fixed failure category and fixed
  message only. Provider bytes and any credential-shaped content stay transient.

## Accepted exact-candidate review directive (2026-10-07)

The owner directed the Developer pipeline to decide completion from the exact
delivered candidate and its requested behavior instead of blocking on hypothetical
hardening for interfaces the candidate does not use. Independent review still
blocks an actual defect, an unmet requirement, missing coverage of implemented
behavior, or a validator that does not function as claimed. A speculative test for
an absent API, attribute, URI scheme, syntax variant, or unrelated future behavior
is non-blocking only after complete source inspection confirms the candidate does
not use it and no actual requirement or safety rule is violated.

For a bounded static project, a real external resource reference remains blocking
when the approved plan requires local-only assets. Missing tests for uppercase URI
schemes, unused HTML form/action attributes, or CSS `url()`/`@import` parsing remain
non-blocking when complete inspection confirms none occur in the delivered files.
This decision policy does not override a rejected batch, omit aggregate coverage,
change schema or candidate bindings, approve a candidate, or grant any model,
reviewer, or worker additional authority.

## Compatibility and validation

The subprocess argv, schema binding, prompt limits, stdout decision limit,
Windows gate byte, Job assignment, Unix process-group containment, timeout, and
cancellation contracts are unchanged. The Windows launcher still returns only a
success/failure exit code; forwarding stderr adds no new authority or output
contract.

Focused Rust coverage verifies exact selected-model classification, rejects
near-matches and wrong-model text, confirms that fixed diagnostics contain none
of the provider metadata, and writes more than the retention bound through a
small asynchronous pipe to prove the reader drains to EOF while retaining exactly
16 KiB. Repository tests are source validation only. Native MIKE-PC tests and a separate
Windows runner built from this worktree passed on 2026-10-07. With real Codex
0.153.4 and gpt-6.1-sol/medium, a planning call returned the fixed unsupported-model
guidance through the real Windows launcher, without raw provider text or fallback.
The final diagnostic and exact-candidate review build was installed into the active
Windows Developer runner after a confirmed idle shutdown. The previous executable
was retained as a recovery backup. The connection restarted successfully with the
saved supported model selections, projects, and queue intact.
Rendered macOS guidance remains unverified because native computer-use inspection
was unavailable. The active application settings were separately changed to the
verified gpt-5.6-sol/medium model for both roles.


## Installed completion proof

On 2026-10-07 the installed Developer runner completed `aw-fft-working`, feature
`94786a56-8627-45af-a0b9-261b110f688c`, at `review_1_approved` with
`status=succeeded`, zero local repairs and zero escalations. Windows generated
exactly the approved 833-byte `index.html`, passed immutable SHA-256 validation,
and obtained independent batch and aggregate approval. Its artifact digest is
`decf9751aa786629cc557cb1856f9a012b0d1ceb2fe9ae3bd8cd20a077d202e2`.
The local HTTP preview returned 200 with the same exact bytes. This is a local-only
static-artifact completion proof, not completion of the larger retained FFT
projects, browser visual acceptance, GitHub publication, or production readiness.

Final source validation passed 27 focused macOS reviewer tests, 26 native Windows
reviewer tests, all 412 runner unit tests and the full local release gate.
Independent high-risk review approved the complete change. The deployed Windows
image SHA-256 is
`94b25f41e638311f1c1f31ece75df0728617a322db680a954782ceba7351a90b`.
The implementation was validated and independently reviewed in an isolated
worktree; unrelated primary-checkout changes were preserved. Source publication
is recorded separately from this installed local-artifact completion proof.
