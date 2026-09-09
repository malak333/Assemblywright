# Developer Chat Repair Escalation

The owner approved this supervised developer-build extension on 2026-09-07 after
a feature exhausted its three ordinary repair attempts and project chat identified
a likely test defect. It does not change production authority.

## Accepted behavior

- Project chat offers explicit Windows AI and Mac AI selection. Windows retains
  project-specific history and records the model that produced each reply. An
  unavailable model or unsupported image fails explicitly without fallback.
- Chat messages and attachments are untrusted reference material. They cannot edit
  project files, execute commands, enqueue work, or approve a repair.
- The first failed feature can use a completed, saved chat diagnosis to prepare a
  repair proposal. Windows binds preparation to the exact project, feature,
  checkpoint, revision, diagnosis, requirements, validation command, and project
  bytes. Preparation performs no writes.
- The proposal shows its explanation, complete before/after bytes, and every
  protected test or validation input it would change. Only **Approve and apply**
  authorizes those displayed bytes. Drift, cancellation, stale state, or ambiguous
  recovery requires a fresh proposal.
- Each escalation permits one application attempt and preserves all ordinary
  repair, checkpoint, validation, and review history. It does not reset the
  three-attempt ordinary repair budget or create a continuing test-edit exemption.
- After application, Windows runs the unchanged validation command and requests a
  fresh independent Codex review. Failure stops the feature and prevents auto-run
  advancement.
- Review compares the cumulative feature against each file's earliest baseline,
  including absence for a newly created file. Later repairs update expected final
  bytes without replacing the original baseline. Missing or ambiguous legacy
  evidence fails closed.
- The repair sheet bounds and scrolls long content while keeping its title, Close
  button, and available actions visible. Approval appears only for a ready proposal.

Cloud implementation and ChatGPT project chat remain outside this slice. Local
chat, proposal generation, and feature execution share one inference gate so their
model work is serialized.

## Evidence boundary

Focused Rust and Swift tests cover provider selection and attribution, exact
diagnosis/proposal bindings, stale approval rejection, protected-test approval,
cumulative baselines, cancellation, recovery, and attempt history. Native
HTTP/process E2E covers zero-write preparation and the Windows validation/review
path. These fixture and repository checks are distinct from installed UI evidence,
live model quality, signing, notarization, hosted publication, and production
readiness.

The 12:50-12:56 owner-observed run showed the repaired sheet with its Close and action
buttons pinned and visible. The owner approved and applied proposal 4. Its unchanged
validation command ran 24 tests and failed two: one asserted geometry while the Tk
window was hidden/unrealized and still reported height 1; the other used a brittle
widget-tree search and could not find the nested Input card. Codex review did not
run, the feature did not advance, and the GUI feature remains unresolved. This is
useful evidence that a visible modal and successful byte application do not prove
the proposed tests or feature correct. It is not machine-automated screenshot
regression evidence.
