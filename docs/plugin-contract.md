# Plugin Contract

Jarvis plugins are executable capabilities behind an explicit manifest and the
same policy engine used for built-in tools. The implemented local plugin
boundary supports first-party Rust modules and local subprocess plugins over
JSON stdin/stdout. The stable commitment is the manifest, local provenance
snapshot, optional trusted-key publisher signature verification,
manifest-level network host declarations, and audit contract; marketplace,
WASM, OS-level network sandboxing, and malware-analysis trust remain target
architecture.

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
  manifest path, manifest SHA-256, canonical source path, deterministic
  source-tree SHA-256 and file count, optional subprocess command path, optional
  subprocess command SHA-256, capture time, verification time, and integrity
  status. Source-tree hashing rejects symlinks, ignores Jarvis-owned generated
  artifact/cache paths, and fails closed if the manifest or subprocess
  entrypoint would be excluded. Origin claims remain unverified local labels
  until an operator pins the author claim or verifies a manifest signature
  against an explicit trusted public key.
- Optional `publisher_signature` with `scheme: ed25519-v1`, a base64 Ed25519
  public key, and a base64 signature over the manifest payload with
  `publisher_signature` omitted. Signature verification requires local
  provenance to match first and a trusted public key supplied by the operator;
  an embedded public key cannot self-authorize.
- Optional per-action `network_access`. Actions that request the `network`
  permission must set `network_access.mode: declared_hosts` and provide
  non-empty `allowed_hosts` with plain hostnames only. Wildcards, schemes,
  paths, ports, whitespace, and non-ASCII hostnames fail manifest validation.
  This is manifest governance and review evidence, not an OS network sandbox.
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
  Actions that declare network access require
  `execution_grant: subprocess_stdio_network` instead. `metadata_only` can
  never execute. Enablement also requires the local provenance snapshot to
  verify as `matches_install_snapshot`.
- Installed plugin run requests go through an explicit fail-closed runner
  boundary. The boundary revalidates the stored manifest/version metadata,
  checks the requested action is declared, validates input schema, verifies the
  local source-tree provenance snapshot, honors `execution_enabled` and
  `execution_grant`, checks that the stored source path is canonical, and
  appends audit evidence.
- Model-originated tool requests are stricter than direct plugin registry
  inspection. They may target only registered first-party plugin actions
  advertised to the provider. `/tools/model` exposes the redacted registered
  first-party model-tool catalog used for provider grounding; Ollama receives it
  as a JSON allowlist of exact `plugin_id` and `action` values, and
  ChatGPT/OpenAI-compatible native tool definitions are projected from the same
  catalog. The normative path is provider response parsing, canonical envelope
  or native-name normalization, lookup against the registered first-party
  catalog, input schema validation, policy/approval, then execution. Unknown
  plugin IDs, undeclared actions,
  non-object or schema-invalid inputs, and non-first-party requests fail closed
  before policy checks or execution, emit `tool_request_rejected` audit
  evidence, and are returned to the model as `rejected` tool results for
  bounded recovery on the next step. Oversized tool plans and malformed provider
  envelopes still fail the task. Installed plugins, including enabled
  `local_subprocess` plugins, are never model-planned tools.
- Enabled installed plugin execution is limited to `local_subprocess` manifests
  with the `subprocess_stdio` grant for non-network actions and
  `subprocess_stdio_network` for network-declaring actions. The command must
  canonicalize under `source_path`; parent-directory escapes, absolute commands
  outside `source_path`, missing subprocess config, undeclared actions, invalid
  input, malformed stdout JSON, output-schema mismatches, and network actions
  enabled without `subprocess_stdio_network` all fail closed with audit
  evidence. Jarvis sends a JSON object containing `plugin_id`, `action`, and
  `input` to stdin and accepts only JSON stdout that matches the action output
  schema. Subprocess stdout is capped at 1 MiB and stderr is capped at 256 KiB;
  a stream that exceeds its cap is killed and fails closed before raw output is
  parsed or audited. Jarvis clears the inherited process environment before
  spawn and exposes only a minimal allowlist: a deterministic `PATH` for
  interpreter resolution plus `JARVIS_PLUGIN_ID`, `JARVIS_PLUGIN_ACTION`, and
  `JARVIS_PLUGIN_SOURCE_PATH`. This prevents app/core secrets from reaching
  subprocess plugins by default; it is still not a full OS sandbox.
- Publisher signature verification uses
  `/plugins/installed/:id/publisher/signature/verify` or
  `jarvis plugins verify-publisher-signature`. It fails closed until local
  provenance matches, requires the trusted key to match the manifest key,
  verifies the Ed25519 signature, stores `origin_claim_verified: true`, and
  appends `installed_plugin_publisher_signature_verified` audit evidence.
  This proves the manifest was signed by the trusted key; it does not prove
  marketplace approval, malware safety, OS-level process/network sandboxing, or
  host-level egress enforcement.
- Policy review emits `network_plugin_action` items for installed plugin
  actions that declare network access so the operator can inspect
  network-capable plugins before enabling execution.
- `./scripts/release-plugin-trust-qa.sh --check` keeps the manual marketplace,
  malware-analysis, OS-level process/network sandbox, and host-level
  egress-enforcement review path executable
  in the local release gate. `--self-test` proves only the assertion/report
  mechanics with fake evidence notes; `--assert-complete` writes owner-recorded
  evidence after external validation flags and non-empty owner/timestamp/evidence
  fields are present.
- `./scripts/release-evidence-bundle.sh --bundle` references the plugin-trust
  QA report alongside signed distribution artifacts and live-device QA evidence
  for final release review. It records evidence paths, owner flags, and
  owner-recorded plugin trust notes only; it does not turn plugin marketplace,
  malware, OS-level process/network sandbox, or host-level egress checks into
  repo-local proof.
- Installed plugin dry runs are contract-only. `dry_run: true` validates the
  stored manifest, action name, and input schema, then returns `dry_run` with
  `contract_validated: true` and `side_effect_executed: false`; it never loads
  or executes plugin code.
- Local manifest validation rejects invalid schemas, blocked action risk tiers,
  missing proactive/memory/model/network permissions, invalid network host
  declarations, zero or excessive timeouts, first-party source claims, relative
  source paths, unreadable source directories, and manifests outside the
  declared source directory.
- Side-effecting actions require policy evaluation even for first-party plugins.
- Proactive actions must be opt-in and visible in scheduler state. Scheduler
  execution enforces that declaration at run time and fails closed with redacted
  audit evidence when a scheduled plugin action is not proactive-enabled.
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
  validation, minimal subprocess environment isolation, network declaration
  validation, contract-only dry-run evidence, constrained subprocess execution
  evidence, publisher-origin/signature verification evidence, and durable audit
  evidence.
