# Plugin Contract

Jarvis plugins are executable capabilities behind an explicit manifest and the
same policy engine used for built-in tools. The first implementation can be
first-party Rust modules, subprocess plugins over JSON-RPC, or WASM plugins;
the stable commitment is the manifest and audit contract.

## Manifest Fields

Each plugin manifest must declare:

- `manifest_schema_version: 1`.
- Name, version, author or source.
- Source type. Local installation accepts only `local_development` or
  `third_party` metadata; installed metadata cannot claim `first_party`.
- Absolute `source_path` for local installation metadata. The manifest file
  must be a readable file under that canonical directory.
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
- Local plugin installation is metadata-only in the current implementation.
  Validated installed manifests are stored as `execution_enabled: false`.
- Local installed manifests do not create executable plugins. Runtime execution
  remains limited to registered first-party in-process plugins until a safe
  sandboxed runtime is explicitly implemented and tested.
- Installed plugin run requests go through an explicit fail-closed runner
  boundary. The boundary revalidates the stored manifest/version metadata,
  checks the requested action is declared, honors `execution_enabled`, appends
  audit evidence with `side_effect_executed: false`, and returns `blocked`
  without dispatching plugin code.
- Local manifest validation rejects invalid schemas, blocked action risk tiers,
  missing proactive/memory/model permissions, zero or excessive timeouts,
  first-party source claims, relative source paths, unreadable source
  directories, and manifests outside the declared source directory.
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
- Local manifest install acceptance/rejection and disabled registry
  persistence.
- Installed plugin run attempts fail closed with manifest/action validation,
  disabled execution semantics, and durable audit evidence.
