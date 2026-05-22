# Plugin Contract

Jarvis plugins are executable capabilities behind an explicit manifest and the
same policy engine used for built-in tools. The implemented local plugin
boundary supports first-party Rust modules and local subprocess plugins over
JSON stdin/stdout. The stable commitment is the manifest, local provenance
snapshot, and audit contract; marketplace, WASM, network, and signed-publisher
trust remain target architecture.

## Manifest Fields

Each plugin manifest must declare:

- `manifest_schema_version: 1`.
- Name, version, author or source.
- Source type. Local installation accepts only `local_development` or
  `third_party` metadata, or `local_subprocess` for the constrained executable
  subprocess boundary; installed metadata cannot claim `first_party`.
- Absolute `source_path` for local installation metadata. The manifest file
  must be a readable file under that canonical directory.
- Local installs capture an install-time provenance snapshot with canonical
  manifest path, manifest SHA-256, canonical source path, optional subprocess
  command path, optional subprocess command SHA-256, capture time, verification
  time, and integrity status. Origin claims remain unverified local labels.
- `local_subprocess` manifests must declare a `subprocess` block with a command
  under `source_path`, optional argument array, and `stdin: json` /
  `stdout: json`. Jarvis starts the command directly and never interpolates it
  through a shell.
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
- Local plugin installation stays metadata-only by default. Validated installed
  manifests are stored as `execution_enabled: false` with
  `execution_grant: metadata_only`, including `local_subprocess` manifests.
- Installed plugin execution requires a separate explicit enablement step that
  sets `execution_enabled: true` and `execution_grant: subprocess_stdio`.
  `metadata_only` can never execute. Enablement also requires the local
  provenance snapshot to verify as `matches_install_snapshot`.
- Installed plugin run requests go through an explicit fail-closed runner
  boundary. The boundary revalidates the stored manifest/version metadata,
  checks the requested action is declared, validates input schema, verifies the
  local manifest/subprocess snapshot, honors `execution_enabled` and
  `execution_grant`, checks that the stored source path is canonical, and
  appends audit evidence.
- Enabled installed plugin execution is limited to `local_subprocess` manifests
  with the `subprocess_stdio` grant. The command must canonicalize under
  `source_path`; parent-directory escapes, absolute commands outside
  `source_path`, missing subprocess config, undeclared actions, invalid input,
  malformed stdout JSON, and output-schema mismatches all fail closed with audit
  evidence. Jarvis sends a JSON object containing `plugin_id`, `action`, and
  `input` to stdin and accepts only JSON stdout that matches the action output
  schema.
- Installed plugin dry runs are contract-only. `dry_run: true` validates the
  stored manifest, action name, and input schema, then returns `dry_run` with
  `contract_validated: true` and `side_effect_executed: false`; it never loads
  or executes plugin code.
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
  persistence, including local provenance snapshot fields.
- Installed plugin run attempts fail closed with manifest/action/input
  validation, disabled `metadata_only` execution-grant semantics, explicit
  enablement semantics, local provenance verification, subprocess safe-path
  validation, contract-only dry-run evidence, constrained subprocess execution
  evidence, and durable audit evidence.
