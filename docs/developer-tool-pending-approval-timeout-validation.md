# Developer Tool Pending-Approval Timeout Validation

Date: 2026-10-05

## Scope

This slice distinguishes an ordinary OpenCode event-stream or session timeout from
the same failure while the exact active request is waiting for owner approval. It
does not change project tool-access policy, approve an action, replay a command, or
reset retained action and mutation evidence.

## Boundary decisions

- Windows remains authoritative for the action ledger and approval decision.
- Classification requires one `pending_approval` row bound to the exact project,
  request, chat, feature, and access revision. An unrelated, completed, or stale
  action cannot change the reported failure.
- The check runs before timeout interruption changes the pending row to
  `interrupted`. Existing interruption evidence and effect-reconciliation behavior
  remain unchanged.
- Stop and Emergency Pause retain priority in the biased event loop and continue to
  return `Stopped`; they cannot be misreported as an approval wait.
- A matched failure tells the owner that no approval or replay occurred and to
  review the interrupted action and access setting before starting a fresh request.
  An unmatched failure keeps the existing generic transport or timeout error.
- The staged-tool recovery classifier recognizes this fixed failure only as a clean
  tool hold. Resume still requires its existing exact-snapshot, effect-free, and
  state-binding checks; the diagnostic alone grants no recovery authority.

## Validation

Focused Rust tests cover exact feature, chat, request, and access-revision matches;
unrelated and terminal actions; and the ordinary timeout fallback. Repository tests
remain source validation only and do not establish installed Windows behavior,
live-device completion, signing, notarization, publication, or external release
proof.
