# Plugin Contract

Jarvis plugins are executable capabilities behind an explicit manifest and the
same policy engine used for built-in tools. The first implementation can be
first-party Rust modules, subprocess plugins over JSON-RPC, or WASM plugins;
the stable commitment is the manifest and audit contract.

## Manifest Fields

Each plugin manifest must declare:

- Name, version, author or source.
- Capabilities provided.
- Required permission scopes.
- Risk tier for each action.
- Input schema and output schema.
- Whether each action can run proactively.
- Whether each action can access memory.
- Whether each action can call models.
- Audit fields emitted before, during, and after execution.
- Timeout behavior.
- Cancellation behavior.

## Action Contract

Every plugin action should be treated as a policy-controlled operation:

```text
request received
  -> manifest is loaded and validated
  -> input schema is validated
  -> permission scopes are checked
  -> risk tier is evaluated
  -> approval is requested when required
  -> action runs with timeout and cancellation support
  -> output schema is validated
  -> audit entry records the decision and result
```

## Required Audit Fields

Plugin execution must emit enough information for the Activity and Audit view
to explain what happened:

- Plugin name and version.
- Action name.
- Task id and session id when available.
- Requested scopes.
- Effective risk tier.
- Approval status.
- Input summary with sensitive fields redacted.
- Output summary with sensitive fields redacted.
- Files touched, external actions attempted, and network targets when relevant.
- Start time, end time, timeout, cancellation, and failure state.

## Safety Rules

- A plugin cannot execute actions outside its manifest.
- Unknown manifest fields are allowed only when versioned and ignored safely.
- Missing required fields fail validation.
- Side-effecting actions require policy evaluation even for first-party plugins.
- Proactive actions must be opt-in and visible in scheduler state.
- Memory access must be scoped by category and sensitivity label.
- Model calls from plugins must go through the model router; plugins cannot
  bypass local-first routing or ChatGPT approval policy.

## Testing Expectations

The current branch has deterministic first-party in-process plugin APIs for
contract testing. Release verification should keep covering:

- Manifest validation success and failure cases.
- Unknown action rejection.
- Scope enforcement.
- Risk tier mapping to approval outcomes.
- Cancellation and timeout behavior.
- Audit entries for allowed, approval-required, denied, blocked, failed, and
  cancelled actions.
- Proactive action gating.
