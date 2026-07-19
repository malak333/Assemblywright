# Jarvis Project Facts

These notes capture durable facts for future agents working on this repository.

## Apple Peer Identity Boundary

- Default app-supervised IPC uses `unix_socket_peer_identity_v1`. Its strict
  startup transport contains the bounded absolute `socket_path`, nonempty
  maximum-4096-byte `peer_code_requirement`, and exact
  `peer_identity_profile` (`adhoc_exact` or `developer_id_hardened`). The bearer
  remains a separate startup-stdin value.
- Both Swift and Rust use `LOCAL_PEERTOKEN` and Security.framework dynamic-code
  validation before request framing, retain `getpeereid` current-EUID checks,
  and still require the per-launch bearer. PID/path lookup is not an identity
  authority. Missing tokens, malformed requirements, wrong code, mixed
  profiles, and unsigned peers fail closed.
- Package signing keeps the app identifier at
  fixed `com.nobiletechnology.jarvis` identifier and explicitly assigns the
  bundled core `com.nobiletechnology.jarvis.core`; package bundle-ID overrides
  are rejected because they cannot satisfy the fixed identity policy. Never
  rely on codesign's hash-derived identifier for the bare core Mach-O.
- An ad-hoc designated requirement is exact-build cdhash evidence without a
  TeamIdentifier. The unsigned launch lane therefore proves identity mechanics
  by accepting the legitimate Swift client and closing/resetting a same-EUID
  wrong-code Python connection before it receives a framed `401`; it does not
  prove publisher trust. The `developer_id_hardened` profile separately
  requires Apple-generic Developer ID Application leaf/intermediate certificate
  extensions, stable IDs, the same nonempty team, and hardened-runtime flags;
  ordinary Apple Development signatures do not satisfy that profile. Developer ID signing,
  notarization, clean-profile/Finder launch, device authentication, App Sandbox,
  and live-device QA remain external or owner-recorded evidence.
- Synthetic subprocess tests that exercise Codex adapter success or output
  limits use a wide timeout margin so parallel release-gate scheduler load
  cannot turn a functional assertion into an unrelated timeout failure;
  dedicated timeout tests retain their intentionally narrow bounds.

## Repository And Scope

- The repository is public at `https://github.com/malak333/Jarvis`.
- Production implementation work should assume public-repo hygiene: no secrets,
  no private-source material, no hidden readiness claims, and release evidence
  that can be reviewed from the branch/PR.
- Public PR/release evidence includes `.github/workflows/release-local.yml`,
  which runs `./scripts/release-local.sh` on macOS for pull requests, pushes to
  `main`, and manual dispatch. `./scripts/release-ci-workflow-smoke.sh` is part
  of the local gate and validates that the workflow still points at the
  canonical release-local script. The workflow pins the runner to `macos-15`,
  pins `actions/checkout` and `dtolnay/rust-toolchain` by commit SHA, and
  installs Rust `1.95.0` with `clippy` and `rustfmt`; `rust-toolchain.toml`
  carries the same Rust pin for local `rustup` users. This is CI evidence for
  repo-owned local verification only, not Developer ID signing, notarization,
  clean-profile install, Finder launch, live-device QA, or plugin marketplace trust evidence.
  The same boundary is exposed as the `release_ci_gate` feature in `/contract`
  and release readiness.
- On the owner's Windows development machine, install the repo-pinned Rust
  `1.95.0-x86_64-pc-windows-msvc` toolchain through `rustup` with the minimal
  profile plus `clippy` and `rustfmt`. Visual Studio 2022 Build Tools with the
  C++ toolchain and Windows SDK are prerequisites. Restart PowerShell and Codex
  so `%USERPROFILE%\.cargo\bin` reaches `PATH`; an already-running Codex process
  can temporarily invoke `C:\Users\mike\.cargo\bin\cargo.exe` directly.
- A feature or phase is not complete merely because its code passes focused
  tests. Before merge, update the applicable implementation and boundary docs,
  add durable facts here, add or name matching E2E/focused integration proof,
  and extend the docs-drift/workflow guards when a new proof lane is introduced.
  If a broader E2E cannot exist yet, record the exact unimplemented boundary
  instead of making a broader readiness claim.
- The product direction is a local-first macOS assistant foundation, legally
  distinct from Marvel/JARVIS branding and assets.
- The current repo contains a Rust workspace with `jarvis-core` and
  `jarvis-cli`, plus portable `jarvis-protocol` and headless executable
  `jarvis-master` crates and a Swift shell under `apps/mac` with management
  tabs, core
  supervision, voice input/output adapters, release/evidence-status
  inspection, scheduler notifications, Keychain credential launch injection,
  and packaged-smoke support.
- The first distributed-development implementation slice is contracts only.
  `jarvis-protocol` owns protocol version 1, typed cross-device identifiers,
  bounded capability/handshake/job/result envelopes, strict unknown-field
  rejection, bound-before-decode raw frame ceilings, nil-identity rejection,
  lease/deadline ceilings, exact leased-attempt result binding, and a golden
  mac-bridge handshake fixture. `jarvis-core` exposes those contracts
  only through the default-off `distributed-development` feature.
- The second distributed-development slice is the portable, library-only
  `jarvis-master` schema-v1 lifecycle kernel. It persists explicit device
  registrations, revocation state, connection epochs and sequence high-water,
  queued steps, immutable leased job envelopes, attempts, cancellation,
  expiry, terminal payload digests, and restart reconciliation in an isolated
  SQLite database. It enforces the 256-step nonterminal admission ceiling,
  four global leases, one live lease per device connection, exact registered
  capability matching and context/result limits, and abandon-before-reissue.
  It is not wired into `jarvis-core` and is not the final unified Windows state
  store.
- The third distributed-development slice adds a foreground `jarvis-master`
  executable around that kernel. `setup` creates the isolated database and a
  256-bit development bearer without printing the secret; `serve` holds an
  exclusive data-directory owner lease and binds only to loopback; `health`
  returns schema, startup reconciliation, and bounded state counts; and
  `fixture-worker` completes one deterministic inference-shaped job from a
  separate process. Every route requires the bearer with digest comparison.
  This is local development transport, not Windows service installation, mTLS,
  remote trust, live inference, or unified authority. Device enrollment is a
  separate local operator CLI foundation and is not accepted by this listener.
- `master_process_e2e` is the phase E2E for the executable boundary. It starts
  real child processes, rejects a second owner for the same database, completes
  one authenticated loopback fake-worker job, proves the generated bearer is
  absent from setup output, rejects missing authorization and an oversized
  request body, checks durable state counts, and restarts against the same
  database to verify connection reconciliation.
- The fourth distributed-development slice advances `jarvis-master` to schema
  version 2 and adds a Windows enrollment-identity CLI. Its ECDSA P-256 CA key
  is serialized only long enough to be protected by current-user Windows DPAPI;
  disk stores the DPAPI blob, public CA certificate, and public metadata, while
  SQLite stores only the CA fingerprint. Partial authority files fail closed
  instead of silently regenerating a new CA.
- Initial enrollment grants reserve a server-selected device ID, name, role,
  registry revision, and exact capability set. They carry 256 bits of random
  secret material, expire after ten minutes, are single-use, and store only a
  SHA-256 digest. At most 16 devices and 32 outstanding grants are accepted.
  Grant secrets are returned once to the local operator and accepted for
  issuance only in a strict bounded stdin document, never as CLI arguments.
- Device issuance verifies the client CSR signature but discards every
  client-requested identity field. The master emits its own CN and
  `urn:jarvis:device:<uuid>` SAN, client-auth usage, random 20-byte serial, and
  30-day validity. The client private key remains client-owned. Rotation uses a
  new grant bound to the unchanged device registry and revokes the replaced
  serial; device revocation disables all active serials and disconnects active
  work through the existing abandonment path.
- `enrollment_identity_e2e` proves grant secrecy, invalid-secret and replay
  denial, expiry, invalid-CSR recovery, issuance, rotation, revocation, and
  transactional schema-v1-to-v2 migration on every supported host through an
  injected protector. On Windows it additionally exercises real DPAPI and the
  real CLI stdin boundary. This is local identity issuance proof and supplies
  the authority used by the separate remote transport proof.
- The fifth distributed-development slice keeps the bearer-authenticated
  loopback listener as the default and adds explicit
  `serve --remote-bind <concrete-ip>:<port>`. The single foreground master then
  serves both listeners. Remote bind rejects unspecified and multicast IPs; the
  master creates a fresh 24-hour ECDSA P-256 server key in memory and a CA-signed
  server-auth certificate whose SAN is the exact bind IP. Rustls is restricted
  to TLS 1.3 and requires a client certificate under the private enrollment CA.
- After the TLS handshake, the master extracts exactly one
  `urn:jarvis:device:<uuid>` SAN, serial, and certificate digest, then requires
  that exact certificate/device tuple to remain active in SQLite. The strict
  `AuthenticatedHandshakeRequest` must repeat the server-owned registration and
  SHA-256 digest of the fixed-label 32-byte TLS exporter. Replaying it on a new
  TLS channel fails. Subsequent requests are bound to the accepted connection
  epoch and certificate device; only a Mac-bridge certificate may enqueue work.
  Socket close durably disconnects the epoch and abandons affected work through
  the existing reconciliation path.
- `remote_mtls_e2e` is the phase E2E for this transport. On Windows it uses real
  DPAPI identity material and real master/client processes to prove TLS 1.3
  mutual authentication, pre-handshake health denial, exporter-bound health,
  exporter replay denial, epoch advance
  after disconnect, revoked-certificate denial, and the role boundary on a
  persistent authenticated connection: MacBridge enqueue succeeds and enrolled
  inference-worker enqueue is unauthorized. It is same-host loopback proof with
  generated Rust clients, not private-overlay discovery/reachability, live Mac
  Keychain enrollment, Windows service installation, or live inference.
- The next Mac bridge slice adds shared `EnrollmentInvitation` and
  `EnrollmentCsrReply` contracts plus Windows `enrollment pair`. The confirmed
  pairing process holds the raw grant only in zeroizing memory, flushes a
  public endpoint/CA/device invitation, accepts one public CSR reply on stdin,
  and never emits the grant. A failed or interrupted pre-issuance exchange
  leaves only its digest-only, ten-minute grant to expire.
- `KeychainJarvisMacBridgeIdentityStore` generates a non-exported Secure Enclave
  P-256 key and stores staged/installed public binding state in a distinct
  device-only Keychain namespace. Certificate promotion requires the staged
  device/role/revision, public key, exact CA fingerprint, certificate digest,
  signed device SAN, and current validity to match. Normal Jarvis startup does
  not load these items.
- `NetworkJarvisMacTLSChannelFactory` pins the enrollment CA and exact IP,
  presents the Keychain identity, forces TLS 1.3 and HTTP/1.1, derives
  `EXPORTER-Jarvis-Developer-Mode-v1`, and keeps the application handshake and
  authenticated health request on the same bounded persistent connection.
  `DeveloperBridgeTests` proves exact document decoding, fail-closed binding,
  exporter encoding, accepted registry revision, and channel cancellation with
  deterministic seams. Tailscale plus a real Windows owner-account service and
  real Keychain identity remain an owner-recorded live proof, not CI evidence.
- Adversarial review of this slice caught four reusable trust-boundary lessons.
  Certificate authentication alone must not expose even remote health before
  the exporter-bound application handshake. Swift actors are reentrant across
  awaits, so one persistent HTTP/1.1 channel needs an explicit single-request
  gate plus hard connection/request deadlines. A non-exported key must produce
  its CSR before the staged journal is persisted so a signing failure cannot
  strand an unrecoverable journal. A redacted receipt must select allowed
  fields rather than embedding a larger health object that can contain an
  operator-entered maintenance reason.
- `scripts/mac-windows-bridge-live-e2e.sh --check` validates and builds the live
  harness without credentials. After enrollment, `--run` exercises the
  production Keychain/TLS CLI across Tailscale, requires authenticated remote
  master health plus a positive epoch, and forbids grant, certificate PEM, and
  raw maintenance-reason fields. This is a repeatable owner/device E2E, not
  hermetic CI or release-signing evidence.
- The sixth distributed-development slice adds an explicit Windows Service
  Control Manager lifecycle without removing foreground mode. The same
  single-owner master runtime can be installed, started, stopped, inspected,
  placed into/out of maintenance, recovered through stop/start reconciliation,
  and uninstalled. SCM configuration uses automatic start and bounded restart
  delays of 5, 15, and 60 seconds, then stops retrying until the 24-hour failure
  window resets. Incomplete post-create configuration attempts delete the
  partially installed service.
- LocalSystem service identity is deliberately loopback-only because it cannot
  decrypt the interactive owner's DPAPI-current-user enrollment CA. Remote mTLS
  installation requires the same owner account and accepts account/password only
  through a strict bounded stdin document. Password bytes and the parsed password
  are zeroized after SCM configuration and never enter argv, environment, files,
  receipts, health, or logs. The service executable and initialized data directory
  are canonicalized before registration. A confirmed elevated owner-account
  install resolves the exact account SID and idempotently grants
  `SeServiceLogonRight` through the native LSA policy API after SCM recovery
  configuration succeeds. Failure deletes the partially installed service.
  Uninstall preserves that account right because other Windows services may
  share it; revocation requires a separate explicit local-policy review.
- Service maintenance uses `maintenance-mode.json` in the master data directory.
  Missing means inactive; malformed, oversized, or non-regular marker state fails
  closed as active. SCM Pause/Continue updates both service state and the runtime
  marker. Maintenance blocks new enqueue and lease requests with HTTP 503 while
  keeping health, result acceptance, stop, recovery, and explicit exit available.
  Health exposes bounded host-mode, service-identity, maintenance-active, and
  maintenance-reason evidence. Uninstall deletes only SCM registration and never
  deletes master data.
- `windows_service_lifecycle_e2e` is the phase E2E for the SCM boundary. It is
  ignored by ordinary test runs because installation needs elevation. The Windows
  CI job sets `JARVIS_REQUIRE_WINDOWS_SERVICE_E2E=1` and runs it explicitly, making
  SCM access denial a failure. It installs a UUID-suffixed temporary LocalSystem
  service, proves automatic-start/recovery receipt, real service health,
  maintenance denial, recovery-restart preservation of the active maintenance
  reason and admission block, resume and completed work, explicit
  stop/status/start health transitions, uninstall, and data preservation. A
  non-elevated manual run reports a skip. The same elevated workflow separately
  grants and enumerates `SeServiceLogonRight` for the current runner account,
  proving the native policy boundary without receiving or persisting a password.
  Supplied-password owner-account startup, remote mTLS under that account, host
  hardening, crash-loop timing, upgrades, backup/restore, and live cross-device
  behavior remain separate evidence gates.
- A 2026-07-18 live Windows owner-host acceptance installed the service under the
  same interactive account that owns the DPAPI CA, bound the exact private-overlay
  address, survived recovery while durable maintenance remained active, rejected
  new work with HTTP 503, resumed and completed work, and retained overlay TCP
  reachability. This is local operator evidence, not a repeatable enrolled-Mac
  mTLS or cross-device release gate.
- `.github/workflows/windows-protocol.yml` runs portable protocol and master
  process formatting, clippy, and tests on `windows-latest`. It proves the
  foreground executable, single-process ownership, authenticated loopback
  development transport, enrollment identity, TLS 1.3 mTLS transport, real SCM
  service lifecycle, fixture worker, and restart boundaries described above.
  It does not compile the current Unix/macOS runtime and does not prove
  owner-account remote-service identity, private-overlay reliability, live Mac
  exchange/inference beyond the bridge foundation, Codex-account dispatch, repository mutation, signing, or
  live-device behavior.
- On Windows, do not include
  `cargo check -p jarvis-core --features distributed-development --locked` in
  the portable master gate. The existing `jarvis-core` release runtime imports
  Unix-domain sockets and Unix filesystem APIs, so that dormant-feature
  consumption check belongs on a supported macOS host. The Windows gate is
  formatting, clippy, and tests for `jarvis-protocol` and `jarvis-master`.
- `distributed_protocol_contract_e2e` is the phase E2E for the implemented
  portable seam. It serializes a Mac MLX capability handshake, Windows-master
  acceptance, one digest-bound leased job, exact result acceptance, and
  wrong-lease rejection. It is not process, network, mTLS, live-model,
  cross-device, or recovery proof.
- `master_lifecycle_e2e` is the phase E2E for the durable kernel. It uses a
  file-backed SQLite database plus an in-process fake worker to prove durable
  enqueue/lease/result, duplicate/wrong-lease/cancelled/expired/late rejection,
  capability-specific limits, restart abandonment, connection-epoch advance,
  and safe reissue. It is not an executable, process lock, network, mTLS,
  enrollment, cross-device, live-model, or worker-residue proof.
- Durable target decision from the distributed-development design: Windows is
  the sole stateful master for tasks, policy, audit, memory, repositories,
  worktrees, Git, utilities, and future orchestration. Its weaker GPU may remain
  a restricted co-located inference capability; placement does not grant that
  worker master authority.
- Durable target decision: the M1 Mac remains the primary native interface,
  Apple-capability bridge, and stronger stateless MLX inference worker. Future
  workers join through the same capability/job protocol rather than becoming
  additional masters.
- Durable target decision: cloud coding uses the owner's Codex account/CLI
  workflow as an on-demand capability. Jarvis will not use the OpenAI Platform
  API path for this design. Full-agent Codex remains a future gated Developer
  capability on Windows and is not enabled by the protocol foundation.
- Implemented `jarvis-core` surfaces include shared task/audit/safety types,
  a shared Axum router served by default over app-supervised UDS and by an
  explicit loopback compatibility server, runtime-backed command execution with
  `FakeLocalModel` by default, an opt-in Ollama-compatible local HTTP provider,
  or an opt-in ChatGPT/OpenAI-compatible HTTP provider behind explicit
  env/config, sensitivity, redaction, and audit guardrails, emergency-pause
  state, inspectable scheduler state, scheduler recovery/attention, a
  conversation runtime with SQLite task/audit persistence hooks, local-first
  model routing policy, SQLite repository migrations, memory item persistence,
  append-only audit table triggers, release readiness/evidence-status,
  approval execution and permission-center review, bounded activity
  events/progress, installed-plugin metadata/provenance/grants, diagnostics,
  plugin manifest validation, and deterministic first-party test plugins.
- Production first-party inventory is separate from deterministic test
  fixtures: `fake_*` plugins do not appear in `/tools/model`, the default provider
  advertisements, or production manifest inspection. `system_status.status`
  is the bounded always-present status action. Explicit repeatable
  app-owned security-scoped bookmark grants add local-only
  `workspace_inspect.list` and `workspace_inspect.read_text`; no configured
  root means those tools are absent. Before launch, Swift resolves the complete
  bookmark set fail closed and sends opaque IDs plus resolved paths in one
  bounded versioned stdin envelope, never argv or environment. The legacy
  `serve --workspace-root <id>=<absolute-path>` flag remains only for explicit
  CLI compatibility and exposes the path in that manual process's argv. Rust
  holds root descriptors, rejects
  traversal/symlink/hidden/credential-like/special/binary/oversized targets,
  returns bounded untrusted data, redacts content/absolute paths from audit,
  and blocks workspace results from ChatGPT continuation. This is local
  containment evidence, not an OS sandbox or same-user IPC proof.
  Root listing uses the explicit `@root` sentinel; empty paths are rejected.
  Runtime ceilings fail closed beyond 200 visible entries, 64 KiB per read,
  16 KiB per line, and 128 KiB cumulative tool output per task.
  Bookmark tests cover persistence, stale refresh/failure, balanced access,
  redacted presentation, trusted-wake coexistence, unexpected child exit,
  bounded stdin timeout/failure force-reaping, and supervisor cleanup.
  Cross-process CLI E2E proves app-style stdin configuration and rejects mixed,
  malformed, or oversized startup documents. None of this proves App Sandbox,
  sandbox-extension inheritance by the core child, same-user IPC, signing, or
  live-device QA.
  Historical pending or approved `fake_*` approvals are preserved after
  upgrade but appear as critical `removed_fixture_approval` policy-review
  attention. The removed fixture cannot execute, and decided history is not
  silently rewritten or deleted.
- IPC `/commands` uses repository-backed runtime storage when `IpcState` is
  constructed with `SqliteRepository`, records a local-first model-router audit
  entry, and dispatches only the exact configured production `PluginHost`.
  `dry_run` skips plugin execution and records audit evidence. For
  provider-originated requests, `system_status.status` is the valid status
  pair; `status`, `fake_*`, `chrome_extension`, and unconfigured workspace
  actions remain unavailable unless the exact pair appears in the route-scoped
  runtime catalog. `/tools/model` and `jarvis tools list` expose its default
  first-party portion. Installed tools are absent unless the individual command
  opts in and a reactive local route admits an eligible `local_wasm` action.
- Repository-backed `/commands` also persists append-only SQLite model-route
  records. The stored and inspectable route copy keeps provider/outcome/policy
  evidence but omits `context_for_model`, so restart recovery can prove route
  selection without retaining raw command bodies or route context.
- Selected model-provider failures are returned as structured failed command
  responses. `ConversationRuntime` marks the task failed, appends
  `model_step_failed` with redacted diagnostics, preserves selected route
  evidence, and lets IPC return `accepted: false` instead of a transport-level
  command error.
- Repository-backed IPC exposes `/activity/summary`, and the CLI exposes
  `jarvis activity summary`, as a pollable progress surface for task status
  counts, active task count, redacted recent task metadata, and recent audit
  entries. It omits command bodies from recent tasks and is deterministic
  repository evidence for current activity.
- Repository-backed IPC also exposes `/activity/events`, and the CLI exposes
  `jarvis activity watch`, as bounded server-sent events carrying activity
  summary snapshots with redacted recent task metadata plus redacted
  `activity_progress` frames for installed-plugin subprocess progress and
  model-step completion/failure audit evidence plus model-output chunk metadata.
  Model-output chunk frames expose sequence, byte/character counts, and
  `content_redacted: true`, not raw token text. This is local
  progress-streaming evidence for current task/audit state, not provider-native
  raw token streaming. The Swift Runs tab can manually watch a bounded event
  stream, decode `activity_summary`, `activity_progress`, and `activity_error`
  frames, update the visible activity summary from the latest summary event, and
  render plugin, model-step, or redacted model-output chunk metadata without
  opening an unbounded background listener.
- Installed `local_subprocess` plugins can emit bounded newline-delimited
  stderr JSON frames with `jarvis_progress: true`, `stage`, and `message`.
  Jarvis records parsed sequence/stage/message events in the run response and
  append-only audit entries, then emits redacted `activity_progress` SSE frames
  from recent audit evidence while redacting raw stderr. Installed-plugin run,
  audit, and activity-summary evidence also use the redacted provenance view:
  local source paths, manifest paths, subprocess command paths, and provenance
  hashes stay out of those operator surfaces. This is bounded, audit-backed
  plugin progress evidence, not per-token or unbounded real-time plugin UI
  streaming.
- Installed subprocess audit evidence distinguishes process execution from OS
  sandbox enforcement. A completed local subprocess can report
  `subprocess_started: true`, but the current runner reports
  `os_sandbox_enforced: false` and an explicit sandbox boundary because it
  validates manifest/provenance/grants and clears inherited environment
  variables without enforcing an OS sandbox or host-level egress policy. Those
  external controls remain part of plugin-trust QA evidence.
- Installed subprocesses now start in dedicated Unix process groups. The runner
  checks the shared active cancellation/pause state while the child runs and
  routes cancellation, emergency pause, timeout, output-limit, pipe-failure,
  and leader-exit cleanup through bounded TERM-to-KILL group termination,
  direct-child reaping, and bounded I/O-worker joins. Unit tests cover a
  TERM-ignoring in-group descendant plus blocked stdin/oversized output. The authenticated
  `authenticated_approved_installed_execution_can_be_cancelled_after_claim` E2E
  waits for an in-group descendant heartbeat, proves it stops before the plugin timeout,
  verifies effect-possible/non-retryable terminal evidence, and rejects replay
  after restart. This stops members that remain in the dedicated process group,
  but cannot contain deliberate `setsid`/`setpgid` escape, undo an effect already
  issued, or establish OS sandbox or egress enforcement.
- Installed `local_wasm` plugins are a distinct compute-only runtime. They use
  the `wasm_compute` grant and custom `jarvis_json_v1` ABI with required
  `memory`, `jarvis_alloc`, and `jarvis_run` exports. Wasmi links no imports or
  WASI, so the guest receives no environment, filesystem, network, clock, or
  process authority. Hard ceilings are 4 MiB module, 256 KiB request, 1 MiB
  output, 16 MiB memory, zero table elements, and 10 million fuel per invocation. Only low-risk,
  non-proactive, no-memory/model/network compute actions qualify.
- Model-planned installed WASM is disabled by default. The command's explicit
  `installed_wasm_tools` opt-in adds only enabled `wasm_compute`, current
  exact-provenance, eligible `local_wasm` schemas after a reactive local route
  is selected. Cloud/proactive routes, first-party identifier collisions, and
  every `local_subprocess` action stay excluded. The deterministic extension is
  capped at 16 actions, 1 KiB per description, 16 KiB per input schema, and
  64 KiB combined. Private, credential-adjacent, and restricted commands stop
  for the same explicit confirmation policy as first-party model tools before
  guest entry. Execution repeats grant,
  eligibility, schema, and exact-provenance validation immediately before
  entering Wasmi; advertisement is never execution authority.
  Discovery snapshots at most 64 enabled `wasm_compute` candidates under the
  repository mutex, hashes outside the lock, and accepts only unchanged
  records. Source-tree provenance is limited to 8,192 entries, 4,096 files,
  64 levels, and 64 MiB.
  The Swift console keeps installed-WASM advertisement and actual tool
  execution as separate default-off toggles. Installed schemas can be planned
  in dry-run mode; the operator must explicitly enable execution, which applies
  to every model-planned tool in that console.
  This is local-model-only guest-language confinement. It is not OS sandboxing,
  marketplace/publisher trust, malware analysis, same-user/process IPC
  isolation, signing/notarization, or live-device evidence.
- WASM install provenance binds the exact module bytes. Schema v12 migrates
  existing installed-plugin rows without enabling them or broadening grants;
  restart retains the WASM grant and provenance contract. Pause, cooperative
  cancellation, timeout, traps, and fuel exhaustion fail closed and suppress
  output. Audit/IPC/Swift expose only redacted runtime and confinement fields.
- Installed-plugin runs can carry a unique `cancellation_id`; local IPC
  `POST /runtime/cancellations/:id` and `jarvis plugins cancel-run` set the
  shared cancellation state checked before Wasmi start, between fuel slices,
  and before output acceptance. The registry accepts cancellation only after
  activation immediately before runtime entry, caps concurrency at 128 IDs,
  and consumes IDs on every exit. Legacy
  subprocess effects are not reversible.
  Output acceptance atomically finalizes the active ID and returns its
  cancellation state; later requests cannot report acceptance for a published
  completion.
- The Swift Plugin tab has no installed-plugin execution control. It labels a
  verified Wasmi record `WASM confined • no imports • no filesystem • no
  network` and labels the distinct subprocess path `not OS sandboxed`.
  `wasm_confinement_enforced: true` is language-level confinement only;
  `os_sandbox_enforced` remains false unless a real OS mechanism exists.
- The WASM phase is covered by focused core tests, cross-process
  `local_ipc_e2e` restart/execution tests, Swift decoding/presentation tests,
  docs drift, and the canonical local release gate. It does not prove an OS
  sandbox, same-user IPC isolation, marketplace/publisher trust, malware
  analysis, signing/notarization, or live-device evidence.
- Installed subprocess and WASM execution snapshot/revalidate repository-owned
  state and exact current provenance while holding the repository mutex, then
  release it before guest work. Repository access is reacquired only for
  redacted audit persistence. Pause/cancel checks after unlock and immediately
  before output/completion-audit acceptance prevent plugins from blocking
  unrelated SQLite work or publishing a late success.
- `/contract` includes a `compatibility` block with supported version range,
  additive-change, deprecation, removed/deprecated endpoint, and client
  requirement policy, plus a `features` list with stable keys, status, proof,
  and boundary fields so Swift and release docs can distinguish implemented
  repo-owned surfaces from manual or target production claims without scraping
  prose. `jarvis contract` emits JSON by default and also accepts `--json` so
  scripts can use the same explicit machine-output flag pattern as other
  inspection commands.
- `/release/readiness` and `jarvis release readiness` derive a conservative
  read-only release summary from contract feature metadata, release-checklist
  blockers, and explicitly enabled release evidence status. Default readiness
  treats standard `target/` evidence files as inventory only; with
  `JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external` on the running core,
  readiness can compute `production_ready: true` only when every required
  `/release/evidence-status` item is present, no missing or invalid evidence
  remains, and evidence-cleared features leave no pending readiness features.
  The structured readiness payload exposes `evidence_mode_enabled`; this must
  reflect the running core state, so setting the env var only on a client CLI
  process does not clear readiness against a conservative core.
  This remains validated owner-recorded release evidence, not Jarvis-performed
  signing, notarization, stapling, live-device QA, plugin trust QA, or manual
  release QA. The CLI command prefers the IPC endpoint when it is running and
  falls back to the same local `IpcState` readiness summary when the server is
  unavailable, so operator triage does not require a prestarted core. When an
  IPC core is already running, setting the external-mode env var only on the CLI
  process does not clear readiness; the core must be started or restarted with
  that env var after owner evidence is complete.
  `operator_release_qa_smoke` is an implemented readiness feature for the local
  repository-backed operator QA lane; it does not clear clean-profile
  installed-app or live-device manual gates.
- `/release/evidence-status` and `jarvis release evidence-status` expose the
  standard release evidence doctor inventory as structured JSON with present,
  missing, or invalid status for signed artifact paths, signed-distribution
  provenance report, live-device QA report, plugin-trust QA report, and final
  evidence bundle. The app bundle item additionally validates `Info.plist`
  bundle id, short version, build version, and approved microphone/Speech
  privacy prompt copy against expected release metadata,
  and the bundled core item validates the packaged `jarvis-cli.version` marker
  without executing the artifact path. This is read-only file/report inventory
  plus semantic validation. It does not perform signing, notarization,
  stapling, installation, Finder launch, executable runtime behavior,
  live-device QA, marketplace review, malware scanning, OS sandboxing, or
  host-level egress enforcement.
- The live-device QA evidence item is stricter than generic JSON presence:
  `/release/evidence-status` validates schema/type, rejects `self_test_fixture`,
  checks the installed app path, expected bundle identifier, short/build
  version, requires UTC voice-check timestamps ending in `Z`, rejects
  future-dated generated reports, and requires completion to be at or after
  start. It also requires the observed transcript
  to match the spoken test phrase after trimming and the observed command text
  to match the expected command text after trimming, with
  `voice_command_observation.command_result_evidence_id` shaped as
  `task:<uuid>` or `audit:<uuid>` from live command/audit evidence. When the
  check runs through CLI/IPC evidence-status, that ID must resolve through
  repository-backed IPC state to an existing task row or a task-associated audit
  row; fallback/no-server CLI evidence-status fails closed instead of accepting
  shape-only IDs. The live-device and bundle scripts keep shape preflights, and
  `release-evidence-doctor.sh --assert-complete` then delegates to
  `jarvis release evidence-status --json`, optionally through
  `JARVIS_EVIDENCE_STATUS_ENDPOINT`, so final doctor completion cannot accept
  unresolved task/audit evidence. It now
  also requires a `bundled_core` block that binds the installed
  `Contents/Resources/bin/jarvis-cli` path, `jarvis <version>` output, and
  SHA-256 digest to the same live-device report, all live-device
  `validation_flags` and `voice_loop` flags set to true, non-empty
  microphone/Speech usage descriptions, non-empty audio output device label,
  structured notification observation for kind/title/body/thread/timestamp where
  kind is `due_now`, `failed`, or `blocked_by_emergency_pause` and the thread is
  `jarvis.scheduler`, plus non-voice owner notes for clean-profile, Finder
  launch, notification, restart, and manual QA with an ordered UTC notification
  timestamp. The
  app usage descriptions must match the approved `Info.plist` copy exactly:
  `NSMicrophoneUsageDescription` is `Jarvis uses microphone input only when you explicitly start local voice capture.`, and
  `NSSpeechRecognitionUsageDescription` is `Jarvis uses speech recognition only to turn your spoken command into a local assistant request.`. The
  `release-live-device-qa.sh --assert-complete` path rejects whitespace-only
  and placeholder owner evidence-note values such as `TODO`, `pending`, `n/a`,
  `fixture`, and `self-test fixture`; `/release/evidence-status` enforces the
  same non-placeholder live-device evidence-note checks before that report can
  clear readiness, and
  `JARVIS_QA_SELF_TEST_FIXTURE=true` is reserved for the
  script's internal fake-fixture self-test. Invalid or stale hand-written reports stay
  `invalid` and cannot clear `live_voice_loop` in evidence-aware readiness mode.
- Signed provenance, plugin-trust, and final bundle evidence items are also
  stricter than generic JSON presence: `/release/evidence-status` validates
  signed provenance version/bundle metadata, bundled core path/version/SHA-256
  binding, Apple-tool-derived signing/notary/staple/Gatekeeper evidence fields
  plus notary log SHA-256 bindings from `codesign`, `pkgutil --check-signature`, `xcrun notarytool`,
  `xcrun stapler`, and `spctl`, required signed-distribution flags,
  plugin-trust release-version binding, plugin-trust UTC review timestamp
  ordering, rejects future-dated generated reports and any plugin-trust
  `review_source` other than `owner-asserted-manual-review`, validates final
  bundle version, requires SHA-256-shaped
  artifact/report digests including the signed provenance digest, verifies
  signed-provenance zip/pkg/core/notary-log digests against the current artifact files in
  evidence-status, bundle, and doctor assertions, verifies final-bundle
  schema/type identity, artifact/report paths, and digests against the current
  configured files, revalidates the signed-provenance, live-device QA, and
  plugin-trust QA child reports referenced by the final bundle, and requires
  `validation_flags.local_signature_validation=true`. Final bundle owner
  evidence also requires non-placeholder owner evidence notes plus
  `reports_archive_uri` to be URI-shaped and durable; blank values, missing
  schemes, placeholders, examples, fixtures, and self-test archive paths are
  invalid production evidence. The bundle script applies that archive URI
  validation in its fake self-test lane too, and the CLI E2E suite covers
  temporary archive URI rejection before the script can write the final bundle.
- `release-evidence-doctor.sh --assert-complete` must stay aligned with that
  final-bundle semantic floor. It should reject minimal or hand-written final
  bundles that omit artifact/report paths, point at stale artifact/report paths,
  omit, malform, or stale SHA-256 digests, omit a UTC generation timestamp, use
  the wrong release version, reference semantically invalid signed-provenance,
  live-device QA, or plugin-trust QA child reports even when their digests
  match, set `validation_flags.local_signature_validation=false`,
  or pair the packaged bundled core with a stale `jarvis-cli.version` marker.
- The Swift shell also decodes `/release/readiness` through
  `ReleaseReadinessModel` and renders a Release tab with blocking manual gates,
  recommended commands, implemented proofs, pending features, the proof
  boundary, stale cached-readiness warning, and `/release/evidence-status`
  inventory. Present presence-only evidence rows show the caveat on the status
  line. Its effective production-ready display must remain fail-closed unless
  readiness is true, evidence status is complete, every evidence item is
  present, and the refresh is not stale or failed. Swift model tests cover
  stale readiness failures and evidence-status refresh failures after a
  production-ready readiness response. This remains inspection-only and does
  not perform signing, notarization, stapling, installation,
  Finder/LaunchServices validation, or live-device validation.
- `ConversationRuntime` supports bounded fake-model and provider-envelope
  planned first-party tool calls plus explicit reactive-local installed WASM
  calls with schema validation, policy checks, approval
  stops, tool-result audit entries, and feedback of tool results into later
  model steps. Ollama-compatible and ChatGPT/OpenAI-compatible text responses
  can return a strict JSON envelope with `message`, `complete`, and
  `tool_requests`; ChatGPT/OpenAI-compatible responses can also return native
  OpenAI `tool_calls` for advertised first-party tool definitions. Plain text
  remains backward-compatible. Installed model planning is restricted to the
  default-off confined WASM exception, not broad installed-plugin or third-party
  tool execution.
- Live local testing with Ollama `llama3.2` has proven the opt-in
  Ollama-compatible HTTP route can complete real model commands. The runtime
  derives the default provider-visible first-party tool catalog from validated
  first-party manifests, exposes the same redacted default catalog through
  `/tools/model` and `jarvis tools list`, advertises it as an Ollama JSON
  allowlist and ChatGPT/OpenAI-compatible native tool definitions, and rejects
  hallucinated provider plugin IDs/actions before policy checks or tool
  execution. Those recoverable validation misses emit `tool_request_rejected`
  audit evidence plus registered-tool guidance and are fed back to the next
  model step as `rejected` tool results. Malformed provider envelopes,
  including prose mixed with JSON `tool_requests`, still fail as redacted model
  errors instead of leaking tool-planning text as a normal answer.
- The default registered model-tool contract is first-party only. Ollama envelope
  requests use `plugin_id` plus `action`; native ChatGPT/OpenAI-compatible tool
  names use `plugin__action`; both must map back to the same registered
  route-scoped catalog before any policy check or execution. Installed plugin
  registry records are inspectable and separately executable through explicit
  grants. Only an explicitly opted-in reactive local command can add eligible
  installed `local_wasm` schemas; cloud/proactive requests and
  `local_subprocess` cannot target them. `/tools/model` remains the default
  first-party view and all catalogs exclude installed paths, subprocess
  configuration, provenance hashes, audit payloads, memory values, and provider
  route context.
- Installed-plugin safe inspection is redacted by default:
  `/plugins/installed` and `/plugins/installed/:id` omit local `source_path`
  values, manifest paths, subprocess command paths, publisher-signature
  material, and provenance SHA-256 hashes. They keep execution grant,
  integrity status, publisher-origin review state, action metadata, install
  timestamp, and explicit redaction markers for operator review. Mutating
  install, verification, enablement, and run paths remain separate operational
  surfaces.
- The CLI interaction contract is now split between human and machine output:
  `jarvis command`, visible alias `jarvis ask`, `jarvis plugins list/get`,
  `jarvis tools list`, `jarvis tasks list/get/audit`,
  `jarvis routes list/get`, `jarvis activity summary`,
  `jarvis release readiness`, and `jarvis release evidence-status` default to
  concise operator-readable text,
  while `jarvis release readiness --all-commands` prints the complete readable
  verification runbook and `--json` returns the exact IPC payload for scripts,
  diagnostics, task records, route evidence, readiness evidence, release
  evidence inventory, and E2E assertions. Human task inspection omits stored
  command text; use `--json` only when the exact task record is needed. Test
  harnesses may set
  `JARVIS_CLI_JSON=1` to keep legacy JSON parsing across command invocations.
  Read-only release/contract/plugin/tool fallback commands treat loopback
  `PermissionDenied` as transport-unavailable so restricted shells can still
  inspect conservative local metadata instead of failing with a raw OS error.
  `jarvis health` and strict IPC commands such as `jarvis command`,
  pause/resume, scheduler, task/audit/activity/route, memory, approval,
  diagnostics, installed-plugin, and permission-center operations exit
  non-zero when the server is unavailable, but the failure is
  operator-readable and points to `jarvis serve`, `jarvis smoke`, and the
  read-only fallback inspection commands instead of surfacing only a raw
  connection error.
- Release command help text is part of the operator contract.
  `jarvis release evidence-status --help` must describe default
  operator-readable output, `--json` for exact payloads, file/report inventory
  plus semantic validation, owner-asserted plugin-trust review source,
  host-egress evidence fields, child report validity, final-bundle archive URI
  validation, and final-bundle local signature-validation status without
  implying Jarvis performs signing, notarization, live-device QA, marketplace
  review, malware scanning, OS sandboxing, or host-level egress enforcement. CLI E2E covers this with
  `release_help_surfaces_current_evidence_boundaries`.
- `/contract` feature metadata is also release-boundary evidence. The
  `release_evidence_status` proof should name repository-backed live
  command-result evidence, plugin-trust owner-source and host-egress fields,
  final-bundle archive-URI validation, and final-bundle child-report semantic
  revalidation. The `release_evidence_bundle` proof should name live-device
  command observation, plugin-trust review source and host-egress fields,
  durable reports archive URI evidence, SHA-256-bound manifest entries, and
  doctor/status revalidation of child reports. CLI E2E asserts those strings so
  clients do not infer weaker release evidence semantics from `/contract`.
- Provider-envelope coverage includes
  `ollama_http_provider_parses_tool_request_envelope`,
  `chatgpt_http_provider_parses_tool_request_envelope`,
  `ollama_prompt_uses_request_supplied_first_party_tool_inventory`,
  `chatgpt_tools_use_request_supplied_first_party_tool_inventory`,
  `model_request_advertises_registered_first_party_tools_only`,
  `provider_tool_request_envelope_rejects_malformed_tool_requests_without_leaking_prompt`,
  `provider_originated_tool_request_executes_first_party_tool_and_feeds_result`,
  and the cross-process `serve_executes_ollama_provider_tool_request_envelope`
  E2E with an Ollama-compatible stub that asserts the advertised registered
  first-party catalog is a JSON allowlist and excludes invented browser plugin
  IDs. CLI smoke and local IPC E2E also cover readable `jarvis plugins list`
  over `/plugins/manifests` and `jarvis tools list` over `/tools/model`.
- Native ChatGPT/OpenAI-compatible tool-call coverage includes
  `chatgpt_http_provider_parses_native_tool_calls` and the cross-process
  `serve_executes_chatgpt_native_tool_call` E2E.
- Invalid provider-planned tool coverage includes
  `rejects_hallucinated_model_planned_plugin_with_registered_tool_guidance`,
  `rejects_hallucinated_model_planned_action_with_registered_tool_guidance`,
  and the cross-process
  `serve_rejects_ollama_hallucinated_tool_with_registered_tool_guidance` E2E.
  Malformed mixed-format provider output is covered by
  `provider_tool_request_envelope_rejects_mixed_prose_and_tool_json` and
  `serve_rejects_ollama_mixed_prose_tool_json_as_malformed_model_output`.
- Repository-backed IPC state exposes task, audit, model-route, and memory
  inspection routes, persists scheduler jobs, restores them at startup, and all
  IPC states expose `/plugins/manifests` for deterministic first-party plugin
  manifests plus `/tools/model` for the redacted default first-party model-tool catalog.
  Repository-backed IPC also exposes `/plugins/installed` for metadata-only
  local plugin installation. Installed records are persisted with
  `execution_enabled: false` and `execution_grant: metadata_only` by default.
  Installed records also carry a local provenance snapshot with deterministic
  source-tree SHA-256/file count, manifest SHA-256, and, for
  `local_subprocess`, command SHA-256 hashes. Verification detects helper or
  resource drift under `source_path`, rejects symlinks and ambiguous path
  collisions, and keeps generated caches/artifacts out of the digest. This
  proves only local file integrity against the install snapshot, not malware
  safety or cryptographic publisher identity.
  Installed plugin run requests can perform contract-only dry runs that
  validate manifest/action/input schema and audit `side_effect_executed: false`
  without loading or executing plugin code. `local_subprocess` manifests can be
  explicitly enabled through `/plugins/installed/:id/execution` or
  `plugins enable-installed` with an action-scoped grant:
  `execution_grant: subprocess_stdio` for non-network actions, or
  `subprocess_stdio_network` for network-declaring actions, after
  `plugins verify-installed` confirms `matches_install_snapshot`. The runner
  treats the network grant as authority only for network-declaring actions, not
  a superset that can run non-network actions in mixed manifests; only
  currently granted action classes can run through the constrained
  subprocess-stdio JSON boundary.
- Every non-dry-run installed-plugin request evaluates its manifest risk,
  declared scopes, explicit sensitivity (CLI default `workspace`), pause state,
  and any approval through `PermissionEngine`. Eligible Low/default-sensitivity
  requests retain direct execution. Confirm actions or sensitive requests
  return `approval_required`, atomically persist a waiting task and pending
  approval, and do not enter Wasmi or start a subprocess.
- Schema v15 adds the private `installed_plugin_approval_bindings` table. Each
  row is one-to-one with an approval/task and stores canonical input plus its
  SHA-256, a contract SHA-256 covering the exact manifest and provenance state,
  and the execution grant. Approval-required run responses, approval records,
  permission views, audits, and diagnostics expose neither the bound input nor
  either digest; audit evidence records only explicit redaction booleans.
  Schema-validated plugin output after approved execution remains part of the
  existing execution response contract and is not the private binding record.
- Installed plugin publisher-origin claims can be operator-pinned through
  `/plugins/installed/:id/publisher/verify` or `plugins verify-publisher`.
  Verification requires the stored provenance to already match the install
  snapshot and `trusted_origin` to exactly match the manifest author claim, then
  sets `origin_claim_verified: true` and appends an
  `installed_plugin_publisher_verified` audit entry. This is a local review
  control, not cryptographic signature validation, marketplace trust, or malware
  analysis.
- Installed plugin manifests can also include `publisher_signature` with
  `scheme: ed25519-v1`, a base64 Ed25519 public key, and a base64 signature
  over the portable unsigned manifest payload with `publisher_signature` and
  local `source_path` omitted. `/plugins/installed/:id/publisher/signature/verify`
  and `plugins verify-publisher-signature` require local provenance to match
  first, require an explicit `trusted_public_key` that matches the manifest
  public key, verify the signature, set `origin_claim_verified: true`, and
  append `installed_plugin_publisher_signature_verified` with a hashed trusted
  key reference. This proves the portable manifest identity was signed by the
  trusted key while local install paths/files remain covered by provenance; it
  still does not prove marketplace approval, malware safety, or OS-level
  process/network sandbox completeness.
- Plugin actions that request the existing `network` permission must now
  declare `network_access.mode: declared_hosts` and exact plain-hostname
  `allowed_hosts`. Invalid host declarations, including schemes, wildcards,
  paths, ports, whitespace, IP literals, mixed-case hostnames, duplicate
  hostnames, and non-ASCII hostnames, fail manifest validation, and
  `/permissions/policy-review` emits `network_plugin_action` items for installed
  plugins with declared network access. Executable installed plugins with
  network-declaring actions fail closed unless enabled with
  `subprocess_stdio_network`; non-network actions fail closed while the
  installed plugin is enabled under that network grant. This is action-scoped
  runtime grant gating plus manifest governance and review evidence, not
  OS-level network sandboxing or host-level egress filtering.
- Installed local subprocess plugin run audits include the requested action's
  manifest-declared `action_network_allowed_hosts` alongside
  `action_requires_network_grant`, while preserving the explicit
  `os_sandbox_enforced: false` and host-egress proof boundary. This makes
  network targets reviewable without claiming repo-local egress enforcement.
- `./scripts/release-plugin-trust-qa.sh` keeps the plugin trust release gate
  explicit. `--check` validates repo-owned plugin trust prerequisites and
  prints the marketplace review, malware scan, signed publisher policy, OS
  process/network sandbox and host-level egress runbook. `--self-test` proves JSON report
  mechanics with fake validation flags and fake evidence notes only.
  `--assert-complete` writes an owner-recorded JSON report after every
  `JARVIS_PLUGIN_QA_*` flag is true and the owner/timestamp/evidence-note fields
  are populated. The accepted report identity is `schema_version: 1` with
  `evidence_type: owner_recorded_plugin_trust_qa`, the current release
  `version`, and `self_test_fixture: false`; accepted operator reports must also
  use `review_source: owner-asserted-manual-review`. Doctor/status gates reject
  wrong-version, self-test, misidentified, or non-owner-source plugin-trust
  report shapes, and they reject placeholder evidence values such as `TODO`, `pending`,
  `n/a`, or self-test/fixture text in owner-recorded evidence fields. Host-level egress evidence
  must also include the reviewed policy/profile label, ordered UTC egress
  validation timestamp, denied undeclared-host fixture note, and declared-host
  allow fixture note. Each plugin-trust category also requires an archived
  manual evidence artifact URI and SHA-256 digest before evidence-status or the
  final bundle gate can accept the report. Bundle, doctor, and evidence-status
  revalidation reject temporary plugin artifact URIs and bare artifact paths so
  a hand-edited downstream report cannot bypass the durable evidence archive
  requirement. CLI E2E now runs
  `release-plugin-trust-qa.sh --assert-complete` with owner-recorded archive
  URI/SHA-256 evidence fields and verifies the generated plugin-trust QA report
  is accepted by `jarvis release evidence-status`. The review timestamps must be UTC `Z` values, the
  completed timestamp must be greater than or equal to the started timestamp,
  and the completed timestamp must not be later than report generation.
  Artifact URIs in the shell assertion path must include a URI scheme and
  location and cannot point at placeholder, self-test, fixture, or temporary
  paths.
  `--write-template
  target/release-plugin-trust-qa.env` generates a sourceable checklist with all
  plugin trust validation flags defaulted to `false` and all evidence fields
  blank. `/release/readiness` and `jarvis release readiness --all-commands`
  include the template-backed source command for `--assert-complete` before the
  long inline owner-flag example. This is manual external release evidence, not
  repo-local proof of those systems.
- `./scripts/release-evidence-bundle.sh` is the final release evidence
  manifest gate. `--check` prints the required signed distribution artifact
  paths, live-device QA report, plugin-trust QA report, and owner validation
  flags. `--check`, `release-evidence-doctor.sh`, and `/release/evidence-status`
  are read-only inventory plus semantic-validation surfaces: they do not
  perform external validation, but they reject stale or weak signed-provenance,
  live-device, plugin-trust, and final-bundle reports before evidence-aware
  readiness can use them. They do not validate Developer ID signing,
  notarization, stapling, installation, live-device QA, plugin-trust QA,
  owner assertions, or final bundle creation. `--self-test` uses fake
  artifacts/reports to prove bundle mechanics only. The `--check` output points
  operators to `--write-template`, and `--write-template`
  generates a sourceable final-bundle environment template whose
  `JARVIS_EVIDENCE_*` validation flags default to `false`, so operators record
  external checks explicitly before any final bundle claim. `/release/readiness`
  and `jarvis release readiness --all-commands` include the template command and
  the template-backed source command for `--bundle` before the
  owner-flagged `--bundle` command so operators do not have to reconstruct the
  final evidence environment by hand. `--bundle` writes
  `target/release-evidence-bundle.json` after referenced artifacts/reports exist,
  every `JARVIS_EVIDENCE_*` flag is true, and local artifact checks validate the
  app signature, app stapling ticket, installer signature, installer stapling
  ticket, and app zip payload through Apple-tool-derived validation. Production bundles must keep local signature
validation enabled; the script parses every required live-device and
plugin-trust report flag, requires non-empty and non-placeholder
owner-recorded evidence-note fields in both QA reports and the final bundle,
requires plugin-trust `generated_at`, `review_started_at`,
  and `review_completed_at` to be UTC with
  `review_started_at <= review_completed_at <= generated_at`, requires the
  plugin-trust `review_source` to be `owner-asserted-manual-review`, requires the
  live-device QA report's app bundle identifier/version/build metadata and
  approved microphone/Speech privacy prompt copy to match the expected release,
  and records SHA-256 digests for the distribution zip, installer package,
  live-device QA report, and plugin-trust QA report before writing the bundle
  manifest.
- `./scripts/release-evidence-doctor.sh` inventories release evidence readiness
  before final bundling. `--check` reports present, missing, or invalid
  signed-artifact, live-device QA, plugin-trust QA, and final bundle evidence
  without failing the default local gate, checks the bundled core version marker
  beside the packaged executable, tells operators to rerun
  `./scripts/package-distribution.sh --unsigned-launch-check` or the signed
  packaging lane when that marker is missing or stale, and prints the next signing,
  live-device template/assertion, plugin-trust template/assertion, and final
  evidence-bundle template/bundle commands when evidence is missing.
  `/release/readiness` and `jarvis release readiness --all-commands` include
  `./scripts/release-evidence-doctor.sh --assert-complete` as the final
  inventory assertion after the bundle command.
  `--self-test` uses fake artifacts/reports to prove the inventory mechanics
  and the next-step guidance only. Its complete path enforces the same
  plugin-trust UTC timestamp order as the bundle path. A
  complete doctor run is diagnostic status, not proof that signing,
  notarization, stapling, installation, or external validation happened.
- `jarvis release readiness --all-commands` is ordered for release execution:
  local gates, unsigned distribution launch check, signed/notarized packaging,
  live-device QA, plugin-trust QA, final evidence bundle generation,
  evidence-doctor assertion, and then the external evidence-mode readiness
  check.
- The structured release evidence status endpoint mirrors the doctor inventory
  for app/installer artifacts and JSON reports, including required owner-recorded
  live-device and plugin-trust evidence fields plus app bundle `Info.plist`
  metadata checks, live-device bundle/version and timestamp semantic checks,
  plugin-trust review timestamp and owner-review-source checks, final bundle
  version/SHA/archive-URI/local-signature checks, and repository-backed live
  command evidence resolution, so the CLI and Swift Release tab can show
  present, missing, or invalid release evidence without parsing script text.
- Release evidence status rejects false live-device validation flags, false
  live voice-loop flags, false plugin-trust validation flags, and false final
  evidence-bundle validation flags; CLI E2E now covers those semantics and
  proves invalid live-device QA keeps `live_voice_loop` pending even when the
  rest of the release evidence fixture is complete.
- Enabled `local_subprocess` plugins run with an environment boundary: Jarvis
  clears the inherited app/core process environment before spawn and provides
  only a deterministic `PATH` plus `JARVIS_PLUGIN_ID`,
  `JARVIS_PLUGIN_ACTION`, and `JARVIS_PLUGIN_SOURCE_PATH`. Rust unit coverage
  and CLI IPC E2E assert that a secret inherited by the core process is not
  visible inside the plugin subprocess.
- Enabled `local_subprocess` plugin output is bounded before parsing or audit:
  stdout is capped at 1 MiB, stderr is capped at 256 KiB, and either stream
  exceeding its cap kills the child and returns a fail-closed plugin error.
  Normal JSON stdout and bounded `jarvis_progress` stderr lines still execute
  and parse under the same runner. CLI IPC E2E now covers stdout and stderr
  over-limit failures through the installed-plugin run endpoint, including
  failed audit evidence.
- Repository-backed IPC exposes `/permissions/grants`, and the CLI exposes
  `jarvis permissions grants`, as a read-only permission-center summary. It
  combines approval status counts/history, high-risk pending approval count,
  installed-plugin grant state, executable installed-plugin count, provenance
  integrity status, capture method, last verification timestamp, origin claim
  metadata, unverified installed-plugin count, and the
  `side_effects_require_approval` invariant without enabling installed plugin
  execution. The Swift permission center renders those provenance statuses so
  metadata-only, verified, changed, missing, invalid, and legacy-unverified
  plugin grants are visible during review.
- Repository-backed IPC also exposes `/permissions/policy-review`, and the CLI
  exposes `jarvis permissions review`, as a read-only policy review surface. It
  converts pending approvals, high-risk plugin actions, unverified installed
  plugin provenance, unverified publisher-origin claims, network-capable plugin
  actions, active scheduler triggers, unreviewed memory items, and deleted
  sensitive memory retained in local storage into explicit severity-ranked
  review items. Memory review and retention-review items include category/key
  and sensitivity only; memory values are redacted from policy review. The Swift
  Approval Center renders this summary alongside grant history. It is
  inspection-only and does not execute, enable plugin side effects, or
  autonomously rewrite/delete memory.
- Repository-backed IPC exposes `/memory/retention-plan`, and the CLI exposes
  `jarvis memory retention-plan`, as the memory-specific operator queue behind
  policy review. It lists active unreviewed memory and deleted sensitive memory
  retained in local storage with category/key, sensitivity, severity, status,
  reason, and recommended action only. Memory values and provenance strings are
  intentionally omitted, `automation_enabled` is false, and the surface does not
  purge, restore, rewrite, or otherwise mutate memory.
- Approved first-party and installed-plugin approval records can be explicitly
  executed through
  `/approvals/:id/execute` or `jarvis approvals execute <approval-id>`.
  Approve/deny remains side-effect-free. Grant and denial each use one immediate
  transaction to recheck pending state, update the decision, and append a
  redacted decision audit. Free-form actor and reason stay in the approval
  record but not its audit payload. If audit insertion fails, the decision
  rolls back to pending, including across restart, so an unaudited grant cannot
  become execution authority. Before plugin entry, execution
  validates the approved record, still-waiting task, exact action, current risk
  and scopes, current manifest, input schema, current policy, and matching
  approval_granted audit evidence, then uses an
  immediate transaction to insert one
  unique schema-v13 `approval_executions` claim and a redacted
  `approval_execution_claimed` audit.
- Installed-plugin approval execution first verifies canonical-input integrity,
  current provenance, exact manifest/contract digest, execution grant, action,
  risk, scopes, sensitivity policy, pause, cancellation, and the same matching
  `approval_granted` authority evidence. Any mismatch fails before claim and
  runtime entry. The successful claimant reuses the bound input; callers cannot
  substitute new execution input at approval time.
- A durable execution claim permanently consumes its approval. Concurrent and
  later attempts conflict before plugin entry. Success, failure, cancellation,
  and timeout write terminal execution state, task state, and terminal audit
  evidence atomically. A crash, restart, or persistence failure after claim is
  effect-possible ambiguity, never permission to retry automatically; an
  operator must review evidence and create a new approval for another attempt.
- Schema v16 adds a separate approval-execution attention ledger. Before a
  repository-backed core accepts IPC, it projects pre-existing unresolved
  claims into that ledger once. `GET /approval-executions/attention` and
  `jarvis approvals attention` return only IDs, timestamps, revision, and fixed
  effect-possible/no-automatic-retry/action-redacted evidence. Action text,
  input, reason, actor, plugin paths, and provenance digests remain absent.
  The summary exposes a true `unacknowledged_count`, `returned_item_count`,
  fixed `item_limit: 100`, and `items_truncated`; Swift rejects inconsistent
  metadata and shows the operator when additional rows require refresh.
- `POST /approval-executions/attention/:execution_id/acknowledge`,
  `jarvis approvals acknowledge-without-retry`, and the Swift Approval Center
  require the exact observed revision plus `acknowledged_without_retry`.
  The immediate-transaction CAS increments the attention revision and appends
  redacted audit evidence atomically. It never invokes a plugin, changes or
  deletes the permanent consumed claim, or creates/retries an approval.
  Swift rejects revision overflow before IPC and clears a displayed row only
  when the response returns the exact successor revision plus the same
  execution, approval, and task IDs.
- File-backed `SqliteRepository` startup acquires a sibling
  `<database>.owner.lock` before backup/version/migration and retains its
  nonblocking exclusive Unix lease for the repository lifetime. The no-follow,
  close-on-exec lock must be a current-owner regular single-link `0600` file;
  insecure metadata or a second core fails before database open. In-memory
  repositories intentionally do not use this file lease.
  The advisory lease serializes cooperating Jarvis owners; raw SQLite and
  noncooperating writers are outside its enforcement boundary.
- Claim-time grant-chain validation accepts the current redacted decision-audit
  shape and exact legacy raw-metadata audit evidence only when approval ID,
  task, action, approved status, risk, sensitivity, scopes, decision metadata,
  and `side_effect_executed:false` match and the audit timestamp is not before
  `decided_at`. The current shape requires exact actor/reason-presence booleans
  and no raw keys; the legacy shape requires exact raw actor/reason values and
  no redaction/presence keys. An approved row with missing or
  unrelated evidence creates no policy/claim audit, durable claim, or plugin
  entry across restart, and the claim path never fabricates grant evidence.
- Schema-v13 migration backfill collapses any legacy raced terminal audits into
  one deterministic consumed row per approval. Completed evidence takes
  precedence over timed-out, cancelled, then failed evidence; the earliest and
  latest legacy timestamps bound the migrated record. This preserves the
  permanent replay guard without depending on SQLite row visitation order.
- The Swift Approval Center loads pending approvals for grant/deny controls and
  approved-unexecuted first-party or installed-plugin approvals for a Run
  Approved action when the IPC contract
  exposes `/approvals/:id/execute`. It treats either
  `approval_execution_claimed` or `approval_executed` as consumed authority,
  hides claimed records after refresh/restart, and suppresses duplicate Run
  Approved interaction while a request is active.
- The CLI `plugins run-installed` command accepts `--sensitivity` and returns the
  normal redacted pending-approval/task projection for approval-required runs.
  `approvals execute` is invocation-generic and returns an additive
  `installed_plugin_result` for an approved installed execution while preserving
  the existing first-party `plugin_results` contract.
- CLI and Swift approved-execution clients generate a fresh UUID
  `cancellation_id`. Rust registers it, binds it to the approved task, and
  activates it at the durable claim boundary. Authenticated
  `/runtime/cancellations/:id` can then cancel only that active run; a winning
  cancellation discards output and atomically records cancelled execution/task
  state. The Approval Center retains that same UUID for its visible Cancel Run
  control, suppresses duplicate execution/cancellation, and clears it only when
  execution finishes. This cooperative boundary cannot undo an external side
  effect.
- Focused storage and real-server CLI IPC coverage injects a SQLite abort on
  decision-audit insertion. It proves grant and denial both roll back, failed
  grants remain pending after restart, `/execute` rejects that broken chain,
  and recovery creates exactly one redacted grant audit without exposing actor
  or reason text.
- The Swift Plugin tab decodes `/plugins/installed` registry records through
  the same redacted contract used by the CLI and IPC surfaces. It
  shows execution grant, provenance integrity status, origin-review state,
  action metadata, executable/not-executable status, and redaction markers
  alongside first-party manifests, while local paths, subprocess command paths,
  signature material, and provenance hashes stay hidden. Typed lifecycle
  controls can verify provenance, explicitly select a compatible grant, enable
  only after matching provenance, and disable to `metadata_only`. Confirmation
  binds the exact grant plus a redacted lifecycle-contract digest, and Rust
  rejects stale/reinstalled records as compare-and-set conflicts. They display
  exact declared permissions/hosts, serialize per-plugin mutations, refresh
  authoritative records after every result, disable all lifecycle actions while
  registry state is stale, return only redacted mutation records, and never
  install or run code.
  Subprocess UI keeps the not-OS-sandboxed/no-host-egress warning. Rust commits
  the authority mutation and redacted non-execution audit atomically; storage
  failure injection plus authenticated loopback-TCP compatibility Swift E2E
  across enabled and disabled restarts prove rollback, persistence, post-restart
  audits, malformed/stale-request rejection, redaction, and zero plugin
  execution. This E2E is not evidence for the packaged app's default
  peer-identity UDS. The tab still degrades to a
  warning while keeping first-party manifests visible when the registry is unavailable.
- Installed-plugin updates are explicit local operator actions, not remote
  discovery or a marketplace. Replacement manifests, source trees, versions,
  and publisher claims remain untrusted. New local installs require valid
  SemVer 2.0.0. The candidate must retain the exact plugin identity, advance
  its semantic version, match the currently inspected
  lifecycle digest, and pass bounded manifest/provenance validation; success
  captures a new snapshot and always resets execution to disabled
  `metadata_only`. The new snapshot requires fresh provenance verification and
  explicit compatible-grant enablement. Prior authority cannot carry forward.
  A persisted pre-SemVer record may cross once to valid SemVer under the same
  checks; all later updates use strict SemVer precedence.
- The lifecycle-history surface is a bounded redacted projection of durable
  state transitions. Each entry returns only entry ID, plugin ID, lifecycle
  action, normalized outcome, and timestamp; the wrapper adds fixed
  redaction/proof metadata. It omits paths, source/manifest hashes, signature
  material, subprocess configuration, input/output, secrets, and free-form
  operator text. It is not marketplace approval, publisher identity, malware
  analysis, OS sandboxing, host-level egress enforcement, signing/notarization,
  or live-device plugin-trust evidence.
- The update contract separates redacted review from mutation:
  `POST /plugins/installed/:id/update/preview` validates candidate identity,
  version ordering, and lifecycle compare-and-set, then returns validated
  `current_lifecycle_contract_sha256` and opaque
  `candidate_update_contract_sha256`.
  Explicitly confirmed `POST /plugins/installed/:id/update/apply` requires that
  exact reviewed pair; clients never refresh or substitute either token. Rust reloads
  the candidate and rejects exact-snapshot drift before the atomic reset. The
  token is an aggregate integrity binding, not a raw component provenance hash,
  publisher signature, trust verdict, or execution grant. Finally,
  `GET /plugins/installed/:id/history` returns the redacted lifecycle ledger.
- Focused storage proof covers atomic update persistence, install-time
  preservation, disabled authority/publisher-review reset, candidate and
  lifecycle drift, injected audit rollback, and the newest-first 100-entry
  plugin-scoped history bound. Cross-process CLI E2E
  `installed_plugin_update_preview_apply_history_is_cas_bound_redacted_and_persistent`
  covers operator preview/apply/history, redaction, stale-token failures,
  restart persistence, and re-verification before re-enable over authenticated
  loopback compatibility; it is not default-UDS or external plugin-trust proof.
- The CLI has matching `release readiness`, `release evidence-status`,
  `command`/`ask`, `tools`, `tasks`, `memory`, `scheduler`, `diagnostics`, and
  `plugins` subcommands, including
  `plugins install`,
  `plugins installed`, `plugins installed-get`, `plugins enable-installed`, `plugins
  verify-installed`, `plugins verify-publisher`,
  `plugins verify-publisher-signature`, `plugins disable-installed`, and
  `plugins run-installed` for disabled-by-default local manifests, auditable
  publisher-origin review, trusted-key signature verification, and explicit
  subprocess execution. `command --installed-wasm-tools` is the separate
  default-off per-command surface for eligible reactive-local installed WASM
  model planning; the Swift command console exposes the matching toggle.
- A local unsigned distribution launch proof exists, and installed plugin
  execution now has constrained local subprocess plus no-import Wasmi compute
  proof. Developer ID signing,
  notarization, installer validation, App Store distribution, owner-recorded
  live-device voice-loop validation, broader plugin marketplace trust,
  OS-level process/network sandboxing, host-level egress filtering, plugin
  malware analysis, and broader production operations are still external/manual
  gates.
  The SwiftUI shell and IPC client live under `apps/mac`, including a
  command transcript, activity/audit panel, approval decision and approved-run
  controls,
  management tabs, permission grant-history summary, degraded-mode handling,
  typed transcript staging, adapter-backed voice input/output controls,
  final-transcript handoff into the text command path, and a core supervisor
  abstraction for configured or bundled local core binaries.
- The architecture docs must preserve two diagrams: the current implemented
  Rust/Swift surfaces and the end-goal production architecture. Keep the
  current-vs-target phase table aligned with code before answering readiness
  questions, and show release evidence flow changes such as repository-backed
  command-result evidence validation in both current and target diagrams.
- The active architecture docs should also describe the current production
  sweep structure, but that workflow context must remain separate from
  readiness proof.
- The Swift shell has a core supervisor abstraction, management tabs,
  release/evidence-status/runbook inspection, scheduler notification controls, Keychain
  launch credential injection, adapter-backed voice input/output controls, and
  unsigned distribution launch evidence. It is not a Developer ID signed or
  notarized packaged app, and it still needs clean-profile Finder/LaunchServices
  and live-device validation before production app claims.
- Release runbooks are no longer CLI-only. IPC exposes
  `/release/live-device-runbook`, `/release/signed-distribution-runbook`, and
  `/release/plugin-trust-runbook` as redacted safe-inspection endpoints derived
  from readiness plus evidence-status, and the Swift Release tab renders them
  when available. Swift tests decode the live CLI `--json` payloads for all
  three runbooks through the no-server fallback path to catch CLI/Swift drift.
  These surfaces are operator guidance only; they do not perform signing,
  notarization, installation, live-device QA, plugin-trust review, or final
  evidence bundling.
- The Swift Memory tab now uses the Rust IPC memory contract for list,
  include-deleted refresh, create, load, update of mutable fields, review,
  soft-delete, restore, and redacted retention-plan rendering. Category and key
  remain creation-time fields in the current IPC contract; the Swift edit path
  updates value, provenance, and sensitivity. Restore clears `deleted_at`
  through `/memory/:id/restore` and stays subject to the active
  `(category, key)` uniqueness guard. `/memory/retention-plan` is rendered as
  an operator review queue only. The same tab now renders count-only
  `/memory/index/status` and invokes explicit `/memory/index/rebuild`. The
  versioned sibling manifest is atomically rebuilt from active SQLite rows;
  SQLite remains canonical and corrupt/stale projections fail closed. A
  separate disabled-by-default command option and Swift console toggle now
  permit bounded deterministic lexical context for selected local,
  non-proactive routes. Only reviewed active Public/Workspace/Personal records
  are eligible. Private/CredentialAdjacent/Restricted, cloud, proactive, and
  non-current-index paths fail closed. Vector/embedding retrieval, autonomous
  purge, and rewrite are not performed.
- Repository-backed IPC exposes `/memory/classification`, and the CLI exposes
  `jarvis memory classification`, as a read-only memory corpus summary. It
  groups memory by sensitivity and category, reports active/deleted/reviewed
  and unreviewed-active counts, and never returns memory values beyond the
  existing item list/get endpoints. The Swift Memory tab renders this summary
  above the item list. `/contract.safe_inspection_paths` includes this
  aggregate classification route but intentionally excludes raw `/memory` and
  `/memory/:id` because those explicit memory-management routes return stored
  values.
- Diagnostics export now includes aggregate active, unreviewed, and sensitive
  memory counts when repository backing is enabled. It still omits memory
  values, and memory policy review similarly redacts values while surfacing
  unreviewed memory plus deleted sensitive retained memory for user review.
- Diagnostics uses a dedicated health projection rather than embedding the
  explicit `/health` response. It exposes `emergency_paused`,
  `emergency_pause_updated_at`, and `emergency_pause_reason_present`, but never
  arbitrary emergency-pause reason text; the legacy reason field is null or the
  fixed `redacted` compatibility marker. `/health`, pause, and pause-status
  retain the reason for deliberate operator inspection. Core and real-server
  CLI tests use secret sentinels, and Swift tests decode and present only the
  redacted presence summary.
- The Swift shell has a Keychain-backed launch credential boundary for
  app-supervised model provider secrets. `JarvisCoreCredentialProvider` reads
  known credentials such as the OpenAI API key from Keychain and injects only
  missing process environment values when launching the bundled core; explicit
  environment values still win, and the provider does not auto-enable ChatGPT.
- The Swift Model tab can select the app-supervised local model without
  requiring shell env edits. It decodes `/health` active provider/model fields,
  lists installed Ollama models from `/api/tags`, merges recommended
  downloadable Llama/Mistral/Phi/Gemma/Qwen options including Gemma 4,
  Gemma 3, Qwen3.6, Qwen3, Qwen2.5, Qwen2.5-Coder, and Qwen2.5-VL tags, shows
  RAM estimates from the installed Ollama model size or the curated pre-download
  estimate, pulls missing selections through streamed `/api/pull` JSON-line
  progress, shows determinate download progress when Ollama supplies byte totals
  and indeterminate status otherwise, automatically reloads installed inventory
  after a successful pull, treats `:latest` inventory aliases as satisfying the
  selected base model, gates Start until the selected model is installed, starts
  and stops selected Ollama residency through `/api/generate` keep-alive requests,
  and restarts the supervised core with `JARVIS_LOCAL_MODEL_PROVIDER`,
  `JARVIS_LOCAL_MODEL`, `JARVIS_OLLAMA_BASE_URL`, and
  `JARVIS_LOCAL_MODEL_TIMEOUT_MS` overrides. The tab cannot reconfigure a
  separate terminal-owned `jarvis serve` process; that external core must be
  stopped before the app can relaunch a supervised core with new model settings.
  If Ollama returns a `/api/pull` 412 update-required error for a newer model
  family, Jarvis surfaces a normalized "Update Ollama before retrying" failure.
  For loopback endpoints, the Model tab exposes a separately confirmed
  **Upgrade Ollama…** action for Homebrew formula installations. It resolves
  only the fixed Apple Silicon/Intel Homebrew executable locations, invokes no
  shell or user-derived arguments, passes a minimal environment, verifies the
  installed formula version before and after upgrade, and restarts the Ollama
  Homebrew service only if it was already running. Remote endpoints,
  non-Homebrew installations, and failures remain manual and visible; Jarvis
  never silently starts a stopped service.
- The Swift Model tab exposes separate `OpenAI API` and `Codex account`
  provider selections for approved ChatGPT/OpenAI-compatible cloud routing.
  Both disable the local provider for the app-supervised core, set
  `JARVIS_CHATGPT_ENABLED=true`, pass the chosen `JARVIS_CHATGPT_MODEL` and
  `JARVIS_CHATGPT_TIMEOUT_MS`, and pass the Model-tab choices for
  `JARVIS_CHATGPT_REASONING_EFFORT` and
  `JARVIS_CHATGPT_REQUIRES_APPROVAL`. The Codex-account picker includes the
  current installed catalog (`gpt-5.6-sol`, `gpt-5.6-terra`, `gpt-5.6-luna`,
  `gpt-5.5`, `gpt-5.4`, `gpt-5.4-mini`, and
  `gpt-5.3-codex-spark`) and filters effort choices to the selected model.
  Turning `Ask before every cloud prompt` off lets ordinary Public, Workspace,
  and Personal conversation use the explicitly selected cloud provider without
  a repeated prompt. Private and Credential-adjacent routes keep one-shot
  approval, Restricted routes remain blocked, proactive routes cannot consume
  the command grant, and tool/action approvals are unchanged. `OpenAI API` sets
  `JARVIS_CHATGPT_AUTH=api_key`, passes `JARVIS_OPENAI_BASE_URL`, and receives a
  Keychain-backed OpenAI credential injected by `JarvisCoreCredentialProvider`.
  `Codex account` sets `JARVIS_CHATGPT_AUTH=codex_account` and
  `JARVIS_CODEX_EXECUTABLE`, shells through a logged-in Codex CLI, and does not
  require or store an OpenAI Platform API key. The subprocess receives its
  redacted prompt over stdin, starts in a temporary directory with a private
  final-message file, ignores user config and project rules, fixes approval
  policy to `never`, requests the CLI read-only sandbox, uses strict config with
  web search disabled, disables the current CLI tool/integration feature set,
  inherits only an account/network environment allowlist, discards child logs,
  kills the child if its private response file crosses 1 MiB, and repeats the
  size check before reading. Prompt delivery runs concurrently with timeout and
  response-file monitoring so a non-reading child remains bounded. The Jarvis request payload contains only redacted
  route context, while Codex may still add its own runtime/system context. A CLI
  lacking the complete constrained argument contract fails closed before model
  execution. `/health` now reports
  `chatgpt_enabled`, `chatgpt_auth_mode`, `chatgpt_model`,
  `chatgpt_requires_approval`, and `chatgpt_reasoning_effort`, so the Swift Model tab can display the active
  cloud provider/model and reject an already-running core with the wrong auth
  mode. CLI E2E starts a real `jarvis serve` with the same API-key cloud
  environment, checks the `routed-codex-cloud-model+first-party-plugins`
  runtime label, executes through a stub OpenAI-compatible endpoint, and
  verifies selected-model reporting plus API-key redaction. A separate
  cross-process CLI E2E starts `jarvis serve` with a stub Codex executable and
  verifies the expected CLI argument contract, prompt delivery over stdin, environment
  minimization, auth-mode health, response routing, and executable-path/secret
  redaction. Model-selection restart now waits for the supervised process to
  exit and validates the newly launched core's provider, auth mode, and model
  health before reporting it available; it does not accept stale health from a
  terminating core, and a shutdown timeout aborts restart without replacing the
  still-running process handle.
- A Swift Console command stopped by the sensitive cloud-route policy (or by
  the operator-selected every-prompt policy) exposes a one-shot
  `Approve & Send` action instead of a sensitivity selector. Swift retains the
  pending command in memory and retries it with `cloud_route_approved=true`;
  Rust accepts that approval only for the current non-proactive request, records
  it in route audit evidence, and continues to block Restricted content. This
  is the per-command data-routing decision; it is distinct from the Codex CLI's
  browser/device login, which establishes account authentication but does not
  approve later Jarvis prompts. It is also distinct from tool/plugin approvals,
  which stay in the Approval Center and retain their durable decision/execution
  workflow. The real-server Codex-account IPC E2E proves a normal Personal
  command runs without repeated approval when configured, a Private command
  waits, and the approved exact Private retry executes. The stub also verifies
  the selected reasoning effort reaches the constrained Codex CLI config.
- Codex-account selection must not read the OpenAI API-key Keychain item.
  `CoreSupervisor` resolves model environment overrides before credential
  injection, `JarvisCoreCredentialProvider` injects the API key only for an
  enabled `api_key` route, and `ModelConfigurationModel` reads or writes that
  Keychain item only while the `OpenAI API` provider is selected. A stale macOS
  SecurityAgent dialog can outlive the process that requested it; dismissing
  that already-issued dialog is not evidence that the current Codex-account
  core attempted a Keychain read.
- The Swift shell exposes production-facing management tabs for approval
  evidence, runs/audit, scheduler create/inspect/cancel/run-due/recover-stale,
  redacted diagnostics, release readiness, and voice state. Voice supports typed transcript staging,
  manual submit, and opt-in final-transcript auto-submit into the same text
  command path. The voice model handles interruption, resume/cancel,
  unavailable, and degraded typed-fallback states,
  owns a protocol-backed macOS Speech/AVFoundation adapter model from the
  SwiftUI Voice tab, and exposes permission request, start/stop capture, and
  interrupt controls. Production builds of the Voice tab do not expose manual
  state override buttons that can forge release-visible voice status, and the
  auto-submit toggle is disabled with an explicit reason when no submit handler,
  unavailable voice capture, or busy command submission prevents real
  auto-submit. The Voice tab also owns a protocol-backed AVFoundation
  speech-output adapter with preview, stop, interrupt, and natural completion
  handling. Swift tests cover both adapter boundaries with fakes, including
  speech-output completion returning the model to idle, utterance identity
  protection so stale completion/cancel callbacks cannot mark newer playback
  idle, and auto-submit availability reasons, and do not require live microphone
  access or live audio output. The app still must not claim real
  voice parity until entitlements, clean-profile permission prompts, live
  microphone capture, spoken transcript handoff, live audio output,
  owner-recorded manual device validation, and repository-backed command-result
  evidence are complete for the release candidate.
- The scheduler is inspectable, cancellable, explicitly runnable through
  `scheduler run-due`, and opt-in runnable as a bounded background loop with
  `jarvis serve --scheduler-background`. Scheduler jobs are in-memory without
  repository backing and durable when the IPC state is started with
  `SqliteRepository`. The background loop uses the same audited run-due path,
  per-tick limit, deterministic due ordering, and fail-closed emergency-pause
  behavior as manual execution. Repository-backed IPC exposes
  `/scheduler/attention`, and the CLI exposes `jarvis scheduler attention`, as
  a redacted app handoff summary for due, running, failed, and
  emergency-pause-blocked scheduler jobs.
  Repository-backed `/permissions/policy-review` also surfaces manual,
  one-time, and recurring scheduler triggers as redacted review items, with
  due and recurring jobs raised above future one-time/manual jobs and scheduler
  command text omitted from the payload.
  Due-job execution appends a redacted `scheduler_proactive_policy_checked`
  audit entry before command submission. The audit uses the same trigger
  classification as `/permissions/policy-review`, marks `command_redacted:
  true`, and keeps scheduler command text out of the policy audit payload.
  Scheduler-originated first-party plugin calls are submitted as proactive
  calls, so actions must opt in with manifest `proactive` plus
  `proactive_run` permission. Non-opted-in scheduled plugin actions fail closed,
  record redacted `plugin_execution_blocked` evidence, and do not execute side
  effects.
  `jarvis scheduler recover-stale` and `/scheduler/recover-stale` provide
  explicit operator recovery for persisted stale `Running` jobs after a crash
  or killed process. Recovery marks matching jobs failed, returns diagnostic
  scheduler job fields without commands, and records
  `scheduler_stale_running_recovered` with command redaction evidence. This is
  explicit recovery unless `jarvis serve --scheduler-recover-stale-on-startup`
  is provided. Startup recovery runs the same stale recovery path before the
  server accepts IPC traffic, marks the audit payload with `automatic_recovery:
  true`, and remains bounded by age/limit flags.
  Schema v14 adds `scheduler_notification_occurrences`, a bounded one-row-per-
  occurrence outbox. Due visibility and the running transition commit before
  command execution; failure and stale-running recovery atomically change the
  same row to `failed`, increment its revision, and reset acknowledgement.
  Pending rows are capped at 1,024, acknowledged retention is capped at 1,024,
  list responses are capped at 64 with failed and pause-blocked occurrences
  ahead of ordinary due rows, and a full outbox blocks a new claim before
  command side effects. Authenticated operators can inspect the same surface
  with `jarvis scheduler notifications` and acknowledge an observed revision
  with `jarvis scheduler acknowledge-notification`; CLI E2E proves restart
  replay, redaction, both acknowledgement dispositions, and removal from the
  pending list. Swift acknowledges with revision CAS only after
  notification-center submission or explicit no-authorization suppression.
  This is at-least-once handoff: concurrent consumers or a crash after
  submission but before acknowledgement may repeat the stable occurrence-
  revision request; neither acknowledgement nor tests prove live OS display.
  Release-readiness feature metadata describes this as explicit plus opt-in
  startup recovery, with no default background recovery or distributed lease
  claim.
  The Swift Scheduler tab renders this summary above the job list and owns
  typed controls for bounded `/scheduler/run-due` and
  `/scheduler/recover-stale`, refreshing jobs and attention after each action
  and rendering concise last-action state without exposing scheduler command
  bodies. It also owns a protocol-backed notification model plus macOS
  `UserNotifications` adapter controls for due, failed, and
  emergency-pause-blocked attention items. Swift tests use fake IPC and
  notification adapters to cover run/recovery routing, model refresh,
  authorization, delivery, duplicate suppression, and denied-permission
  fail-closed behavior. Broader production trigger policy and live OS
  notification validation remain target architecture.

## Proof Boundaries

- Local release proof currently means `./scripts/release-local.sh`, which wraps
  `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo
  test --workspace -- --ignored`, `cargo build --workspace`, `cargo run -p
  jarvis-cli -- smoke`, `./scripts/release-operator-qa-smoke.sh`,
  workspace package tarball creation, packaged CLI verification against the
  freshly packaged core source, package distribution no-sign preflight,
  package preflight handoff guidance self-test, version-consistency self-test,
  signed-provenance self-test,
  `./scripts/package-distribution.sh --unsigned-launch-check`,
  `./scripts/release-live-device-qa.sh --check`,
  `./scripts/release-live-device-qa.sh --self-test`,
  `./scripts/release-plugin-trust-qa.sh --check`,
  `./scripts/release-plugin-trust-qa.sh --self-test`,
  `./scripts/release-evidence-bundle.sh --check`,
  `./scripts/release-evidence-bundle.sh --self-test`,
  `./scripts/release-evidence-doctor.sh --check`,
  `./scripts/release-evidence-doctor.sh --self-test`, `swift test
  --package-path apps/mac`, and `swift build --package-path apps/mac`.
  It also runs `./scripts/storage-migration-backup-smoke.sh` so file-backed
  migration backup/recovery and representative schema v1-v13 fixture
  preservation stay part of the default local release evidence.
- Local-model proof now includes stubbed provider-envelope E2E plus live
  Ollama route viability observed during manual testing. The proof is still a
  local runtime boundary claim, not a finished conversational assistant claim:
  model-specific tool discipline can vary, and Jarvis relies on the runtime
  advertised inventory plus fail-closed validation for safety.
- Swift focused integration coverage now pins the Model tab support path:
  `ModelConfigurationModel` tests cover Ollama inventory merging including
  `:latest` alias matching, RAM estimate display, missing-model
  auto-download-on-select, streamed download progress, automatic inventory reload
  after pull completion, supervised-core launch environment overrides, and
  selected-model start/stop. The same focused model coverage pins loopback-only
  Homebrew upgrade availability, exact no-shell command sequencing, filtered
  environment inheritance, pre/post version checks, already-running service
  restart, stopped-service preservation, and non-formula failure guidance.
  `ollamaUpgradeProcessEndToEnd` closes the app-side E2E gap by running
  `ModelConfigurationModel` through `FoundationJarvisCommandRunner` into a
  temporary executable fake Homebrew command, then asserting the observed
  version transition, service-restart sentinel, exact command log, and final UI
  status without modifying the real Homebrew/Ollama installation.
  `JarvisMacAppTests` covers the visible Model tab
  Start/Download/Stop/Upgrade gating and progress presentation, while the
  `OllamaModelRuntimeController` HTTP test uses an injected `URLSession` to
  assert streamed `/api/pull`, update-required pull-error normalization, plus
  `/api/tags` and `/api/generate` keep-alive request shape without requiring a
  live Ollama daemon.
- The current E2E expectation for Rust/CLI foundation changes is
  `cargo test -p jarvis-cli --test local_ipc_e2e`; the ignored variant is
  release-proof coverage and is included by `./scripts/release-local.sh`.
  The CLI E2E also reuses the complete release-evidence fixture across
  `jarvis release evidence-status` and
  `./scripts/release-evidence-doctor.sh --assert-complete`, including the
  bundled core executable `--version` check, so the Rust CLI status and shell
  doctor inventory do not drift independently.
  The E2E covers scheduler proactive policy audit evidence during `scheduler
  run-due` by asserting both one-time and recurring due jobs emit redacted
  `scheduler_proactive_policy_checked` audit entries.
  It also covers stale-running scheduler recovery by persisting a running job
  across restart, running `scheduler recover-stale`, and asserting redacted
  recovery output plus `scheduler_stale_running_recovered` audit evidence.
  Startup stale recovery is covered by
  `serve_can_recover_stale_scheduler_jobs_on_startup`, which starts `jarvis
  serve` with the opt-in recovery flags and asserts the recovered job, redacted
  audit entry, and `automatic_recovery: true` marker.
  Swift scheduler action coverage is in `apps/mac/Tests/JarvisMacCoreTests`,
  including typed client paths for `/scheduler/run-due` and
  `/scheduler/recover-stale` plus `SchedulerModel` run/recovery refresh
  behavior.
- Focused provider-failure recovery coverage is
  `cargo test -p jarvis-core model_provider_failure_returns_failed_response_with_route_evidence -- --nocapture`
  plus
  `cargo test -p jarvis-core command_schema_returns_failed_runtime_response_for_model_provider_error -- --nocapture`.
- Focused memory policy review and retention-plan coverage is
  `cargo test -p jarvis-core permission_policy_review_summarizes_unreviewed_memory_without_values -- --nocapture`
  plus
  `cargo test -p jarvis-core memory_retention_plan_lists_redacted_operator_actions -- --nocapture`
  plus
  `cargo test -p jarvis-core diagnostics_export_is_redacted_and_counts_repository_state -- --nocapture`.
  Cross-process CLI coverage for `jarvis memory retention-plan` is in
  `cargo test -p jarvis-cli --test local_ipc_e2e serve_exposes_local_ipc_contract_and_persists_state -- --nocapture`.
  Swift coverage for the Memory tab retention-plan surface is in
  `swift test --disable-sandbox --package-path apps/mac --filter "Memory retention plan decodes redacted operator actions"`
  plus the package-wide `JarvisMacCoreTests` memory manager and IPC-client
  request tests.
- Focused bounded memory-context proof is
  `cargo test -p jarvis-core memory_index -- --nocapture`,
  `cargo test -p jarvis-core local_memory_context -- --nocapture`, and
  `cargo test -p jarvis-core stale_memory_index_blocks -- --nocapture`.
  Cross-process CLI/server/provider proof is
  `cargo test -p jarvis-cli --test local_ipc_e2e reviewed_local_memory_context_is_bounded_redacted_and_fails_closed_cross_process -- --nocapture`;
  it captures the Ollama request, proves reviewed eligible inclusion plus
  unreviewed/private exclusion, response/audit redaction, stale-index blocking,
  and proactive denial. Swift opt-in proof is
  `swift test --disable-sandbox --package-path apps/mac --filter commandConsoleMemoryContextIsExplicitOptIn`.
- Focused release runbook IPC/App coverage is
  `cargo test -p jarvis-core release_runbooks_expose_current_evidence_without_side_effects -- --nocapture`
  plus
  `cargo test -p jarvis-core contract_endpoint_documents_safe_inspection_paths -- --nocapture`.
  Swift coverage is in the fixture-backed runbook decode test, the live CLI
  runbook JSON fallback decoder test, the management IPC request path test,
  and `ReleaseReadinessModel` refresh assertions in
  `apps/mac/Tests/JarvisMacCoreTests`.
- The focused repository-state test for progress visibility is
  `cargo test -p jarvis-core repository_backed_state_endpoints_expose_tasks_and_audit -- --nocapture`.
  Contract coverage for the activity stream is in
  `cargo test -p jarvis-core contract_endpoint_documents_safe_inspection_paths -- --nocapture`,
  and cross-process CLI coverage is in
  `cargo test -p jarvis-cli --test local_ipc_e2e serve_exposes_local_ipc_contract_and_persists_state -- --nocapture`.
  Swift model coverage for the same contract is included in
  `swift test --package-path apps/mac --filter JarvisMacCoreTests`.
- Every feature or phase should identify the relevant E2E or focused
  integration coverage before a readiness claim is made. If behavior changes
  and no coverage exists, add the coverage or record the blocker. Docs-only
  phases should at least preserve the architecture diagrams, release checklist,
  build/test commands, and KB proof-boundary notes.
- Synthetic timeout tests that compete a blocking detached writer against a
  cooperative Swift task timer need a wide timing separation because Swift
  Testing runs suites concurrently in hosted CI. Keep the injected writer delay
  materially longer than the timeout so scheduler jitter cannot turn the
  expected timeout into a false successful write.
- Do not describe Jarvis as a finished desktop assistant based on the local
  unsigned distribution launch proof alone. Broader readiness still needs Developer ID
  signing/notarization/stapling evidence, clean-profile install and Finder
  validation, owner-recorded live voice/audio validation, marketplace/plugin
  trust QA, malware analysis, OS-level sandbox/egress evidence where
  marketplace claims are made, final evidence-bundle archival, and manual
  clean-profile release QA.
- Do not describe Jarvis as production assistant ready based only on the Rust
  and Swift local gates. The stronger claim requires signed/notarized
  distribution evidence, clean-profile install and Finder validation,
  owner-recorded live voice/audio QA, plugin-trust QA, and a final archived
  evidence bundle.
- `./scripts/packaged-supervision-proof.sh` is local packaged-layout evidence:
  it builds the Rust CLI, copies it into
  `Jarvis.app/Contents/Resources/bin/jarvis-cli`, runs Swift supervisor tests
  against that executable, starts the copied binary with repository-backed
  state, and verifies health, command, audit, diagnostics, emergency pause,
  blocked command, pause status, and resume over IPC. It is not signed,
  notarized, clean-profile packaged app release evidence.
- `./scripts/release-operator-qa-smoke.sh` is local operator-facing QA
  evidence: it starts a repository-backed loopback core with an isolated
  SQLite database, verifies command, audit, model-route, memory
  create/update/review/delete/restore, scheduler attention/run-due, activity,
  permission review, diagnostics, emergency pause, release readiness, and
  restart recovery paths, then removes the temporary state. It is not
  clean-profile installed-app QA, Finder/LaunchServices validation, live
  microphone/Speech validation, spoken transcript handoff, live audio-output
  validation, live OS notification validation, or Developer ID
  signing/notarization evidence.
- `./scripts/packaged-app-release-smoke.sh` is deprecated compatibility only:
  it delegates to `./scripts/package-distribution.sh --unsigned-launch-check`.
  Current local packaged app evidence should cite the unsigned distribution
  launch check because it uses the release-built app layout, bundled core,
  version marker, and unsigned installer payload path.
- Swift voice permission sequencing is repo-owned adapter evidence: the
  `VoiceAdapterStateModel` tracks microphone/Speech permission state, disables
  capture until permissions are granted, and rejects direct start attempts
  before permission without marking the voice path permanently unavailable.
  `MacSpeechVoiceAdapter` also preflights current Speech and microphone
  authorization before installing an audio tap, with deterministic Swift tests
  for denied, restricted, and not-yet-requested states. This is not a substitute
  for manual clean-profile microphone/Speech prompt validation or spoken
  transcript handoff evidence.
- Swift Release tab runbook loading is a separate warning surface from
  readiness/evidence loading: `ReleaseReadinessModel` keeps current readiness
  and evidence status visible when one of the read-only runbook calls fails,
  clears the runbook list, and exposes a warning without treating cached
  readiness as stale or allowing production readiness to become true.
- Swift scheduler notification controls are repo-owned adapter evidence: the
  core model can request authorization, build due, failed, and
  emergency-pause-blocked notification requests, consume the bounded durable
  occurrence outbox, and return explicit submission or no-authorization
  acknowledgements. The
  Swift model tests also prove the reset path permits the same attention item to
  be redelivered for QA recapture after duplicate suppression. The app-level
  macOS notification adapter has a test seam that verifies the real
  `UNNotificationRequest` title, body, sound, thread identifier, scheduler job
  ID, occurrence ID, revision, and notification-kind payload before delivery.
  Stable identifiers make repeated submissions replace the same pending OS
  request where supported, but the handoff remains at-least-once. The scheduler attention UI
  surfaces delivered notification title/body/kind/thread evidence using the
  `JARVIS_QA_NOTIFICATION_*` field names and exposes the model reset path so
  release operators can recapture notification evidence after a duplicate
  suppression check. This is not a substitute for manual clean-profile macOS
  notification prompt and delivery validation.
- Packaged scheduler automation is an explicit local user setting, not an
  implicit authority grant. It defaults off, persists in app preferences,
  clamps the Rust background interval to at least one second and each run or
  stale-recovery batch to at most 64, and applies only after a deliberate
  app-supervised core restart. While enabled, a single cancellable coordinator
  refreshes the redacted scheduler attention projection plus the bounded
  durable occurrence outbox and uses the existing notification path only when
  macOS authorization is already granted. It rechecks lifecycle acceptance
  after asynchronous authorization, submits stable occurrence-revision IDs,
  independently acknowledges successful batch members, and leaves failures for
  restart/poll replay.
  It never prompts from the background, enables trusted wake, or
  proves LaunchAgent, OS wake, or live notification behavior. Exact
  `JARVIS_MAC_SCHEDULER_AUTOMATION_ENABLED=true` is an explicit packaged-test or
  operator launch opt-in; the matching interval override has the same 1,000 ms
  floor. Both are ephemeral and stripped before the core child starts. The
  unsigned packaged launch creates a due job through the authenticated Swift
  UDS client and requires the background loop to complete it with matching
  scheduler audit evidence, then suppresses and acknowledges the durable
  occurrence without claiming live display before the fixed success marker
  appears.
- `./scripts/package-distribution.sh` is the repo-owned distribution packaging
  lane. Its `--check` mode is credential-free and validates local tools plus
  app and bundled-core entitlement templates. Its
  `--entitlements-policy-self-test` is part of `./scripts/release-local.sh` and
  proves the app entitlement template keeps microphone access while the bundled
  core template does not. Its `--running-app-guard-self-test` is also part of
  the local gate and proves that exact app/core executable matches block bundle
  replacement without launching or stopping Jarvis. The companion
  `--running-app-guard-e2e` launches temporary harmless app/core executable
  copies and proves the real process-name plus text-vnode inspection blocks
  that live fixture, then accepts the same bundle after the fixtures stop.
  Every artifact-producing
  mode now inspects the configured distribution bundle immediately before
  deletion and fails with quit-or-alternate-directory guidance when that exact
  app or bundled core is active. This preserves strict runtime signature
  validation and prevents the common rebuild-while-running failure, while the
  narrow inspection/delete race remains explicit. When the UI shows Core
  `invalidSignature` and then `credentialUnavailable`, treat the signature
  failure as primary: the strict identity check stopped launch before IPC
  credential rotation, so the credential error is secondary. Check whether a
  running bundle was rebuilt or replaced, then quit and reopen Jarvis; rebuild
  only if signature validation still fails. If Core instead shows
  `launchedProcessExited` and Console shows `credentialUnavailable`, treat the
  core exit as primary. A previously launched core with parent PID 1 may still
  own `jarvis.sqlite.owner.lock`, the SQLite WAL/SHM files, and the old UDS
  socket, causing the replacement core to exit before it can rotate an IPC
  credential. Stop only that confirmed orphan, then press Start or reopen
  Jarvis. New authenticated app-supervised launches carry
  `--supervised-parent-pid`, validate the direct parent before opening SQLite,
  and self-exit when the app disappears; manual/external cores remain explicit
  operator-owned processes. The crash-style unsigned launch E2E kills the app
  abruptly, requires core self-exit and socket cleanup, and relaunches on the
  same database to prove the owner lease was released. Its
  `--unsigned-structure-check` mode builds release Rust/Swift artifacts,
  assembles `target/distribution/Jarvis.app`, optionally ad-hoc signs
  when `codesign` is available, creates an unsigned `/Applications` installer
  package, inspects the package payload for the app executable, bundled core,
  and `Info.plist`, and validates package identifier, version, and
  `/Applications` install location metadata. Its `--unsigned-launch-check` mode
  is part of `./scripts/release-local.sh`, validates the same package metadata,
  launches the release-built app executable with an isolated temporary HOME,
  and requires the app-owned Swift client to verify health, dry-run command,
  task/audit inspection, diagnostics, pause/block/resume, and durable SQLite
  state over the default app-supervised UDS before emitting a fixed non-secret
  marker. It also requires stable app/core code identifiers and closes/resets a
  same-EUID wrong-code Python peer before any framed response. Failures suppress
  the marker and post-pause cleanup attempts a bounded resume. A separate explicit relaunch keeps the weaker TCP/token CLI
  compatibility path tested. The CLI exposes `jarvis --version`, and the
  packaging/evidence scripts require the bundled `jarvis-cli --version` output
  to match the expected release version before local artifact evidence can pass.
  Full mode requires the owner's Developer ID
  Application, Developer ID Installer, and either a notarytool keychain profile
  or Apple ID/team/app-specific-password notarytool credentials; signs the
  app bundle and app executable with hardened-runtime microphone entitlements
  while signing the bundled core with a narrower hardened-runtime entitlement
  template that omits microphone access; notarizes and staples the app zip;
  then creates, signs, notarizes, and staples a `/Applications` installer
  package. It records and validates Developer ID, notary UUID, exact notary
  `Accepted` status, preserved notarytool log paths plus SHA-256 digests, signed
  installer package identifier/version/`/Applications` metadata, stapler
  success, exact Gatekeeper acceptance, and top-level `Jarvis.app` zip payload
  shape before writing the signed-distribution provenance report. The provenance self-test includes rejected notary status,
  negated Gatekeeper, and nested app-zip negative fixtures.
  `./scripts/release-version-consistency.sh --check` derives the
  release version from Rust package metadata and keeps package, live QA,
  evidence bundle, and evidence doctor defaults aligned with the
  protocol/master/core/CLI crate and local dependency versions in the default
  local release gate. The unsigned structure and launch
  checks still do not prove Developer
  ID signing, notarization, stapling, installation, Finder launch, live
  microphone/Speech validation, spoken transcript handoff, App Store review,
  live audio-output validation, or manual QA.
- `./scripts/release-live-device-qa.sh --check` is part of
  `./scripts/release-local.sh`. It validates repo-owned live QA preconditions
  and prints the manual clean-profile install, Finder/LaunchServices,
  microphone/Speech permission prompts, spoken transcript handoff into the
  command path, live audio-output, notification, restart, and release-QA
  runbook, including the exact template, source, and `--assert-complete`
  commands for owner evidence capture.
  `cargo run -p jarvis-cli -- release live-device-runbook` is the side-effect-free
  CLI companion for operators; it combines conservative readiness with current
  `live_device_qa_report` evidence status and prints the exact template,
  assertion, evidence-status, and evidence-aware readiness commands to run. The
  command list includes the release-core command evidence capture, the
  `task:<uuid>`/`audit:<uuid>` recording rule, and external evidence-mode
  evidence-status/readiness commands with the release endpoint placeholder. It
  is part of the default local release gate so the runbook remains executable.
  Its `--assert-complete` mode requires an installed app plus explicit
  `JARVIS_QA_*` owner flags, including
  `JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true`, then writes a JSON evidence
  report with `voice_command_observation.command_result_evidence_id`. The
  script validates the ID shape offline, while `/release/evidence-status`,
  `release-evidence-doctor.sh --assert-complete`, and evidence-aware
  `/release/readiness` require the ID to resolve against task/audit records
  through repository-backed IPC state before the report can clear readiness.
  The script writes the report
  to `JARVIS_QA_REPORT_PATH` or
  `target/release-live-device-qa-report.json`. The report records installed-app
  metadata, voice-loop evidence fields, owner-recorded live voice evidence
  fields for owner/device/profile/non-future timestamps/notes, structured
  spoken-command observation fields with observed transcript matching the spoken
  test phrase, expected command text matching observed command text, validation
  flags, schema identity, UTC report generation timestamp, structured scheduler
  notification observation fields for kind/title/body/thread/timestamp, and
  proof boundary. Live macOS notification prompt/delivery validation is still
  manual clean-profile release QA, but the owner-recorded report now binds the
  notification observation to non-empty title/body values, allowed scheduler
  kinds, `jarvis.scheduler`, and a UTC owner-recorded notification timestamp
  that is not earlier than the voice-check start.
  `release-live-device-qa.sh --assert-complete` and `/release/evidence-status`
  both reject empty or placeholder owner evidence-note fields before this
  report can clear `live_voice_loop`. CLI E2E also removes required
  owner-recorded live voice, command-observation, audio-output-device, and
  notification-observation fields from the report and verifies evidence-status
  plus external-mode readiness fail closed.
  This standardizes manual evidence only; `--check` does not prove live device
  behavior, and the report remains an owner assertion. When the release operator
  explicitly enables evidence-aware readiness, this report can support the
  narrow claim that the live voice loop was validated for that release
  candidate, not a generalized claim that voice is validated for every device or
  future release. Use
  `./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env`
  to generate a sourceable checklist for all required `JARVIS_QA_*` fields. The
  generated template materializes `JARVIS_QA_EXPECTED_VERSION` from the
  canonical Rust package release version so sourced operator evidence stays
  aligned with the app/core version under validation, and it now includes the
  release-core command evidence capture plus post-report external evidence-mode
  readiness/evidence-status commands.
  `--self-test` uses a fake app fixture to validate assertion/report mechanics
  in the local release gate without claiming live device validation.
- `cargo run -p jarvis-cli -- release signed-distribution-runbook` is part of
  `./scripts/release-local.sh` as a read-only operator companion for signed
  distribution. It combines conservative readiness with current
  `/release/evidence-status` inventory for the app bundle, app executable,
  bundled core, signed app zip, signed installer package, and signed provenance
  report, then prints the package-distribution, evidence-status,
  evidence-doctor, and live-device runbook follow-up commands. It does not
  perform signing, notarization, stapling, Gatekeeper assessment, installation,
  live-device QA, or plugin-trust QA. CLI E2E pins the full signed
  distribution evidence key set, the exact operator command sequence, exact
  manual-check handoff text, and parity between `--json` and `--format json` so
  the runbook cannot silently drop a signed artifact or final handoff.
- `cargo run -p jarvis-cli -- release plugin-trust-runbook` is part of
  `./scripts/release-local.sh` as a read-only operator companion for plugin
  trust QA. It combines conservative readiness with current
  `/release/evidence-status` inventory for `plugin_trust_qa_report`, then
  prints the plugin-trust check, template, assertion, evidence-status,
  evidence-doctor, and signed-distribution follow-up commands. It does not
  perform marketplace review, malware scanning, sandbox deployment, host-level
  egress enforcement, signing, notarization, live-device QA, or final evidence
  bundling.
- `cargo run -p jarvis-cli -- release evidence-bundle-runbook` and the redacted
  IPC `/release/evidence-bundle-runbook` endpoint expose the final handoff
  before production readiness: signed-distribution provenance, live-device QA,
  plugin-trust QA, and `release_evidence_bundle` evidence rows plus final
  bundle, doctor, external evidence-status, and external readiness commands.
  The command and endpoint are read-only and do not generate the final bundle,
  sign, notarize, staple, install, Finder-launch, run live-device QA, perform
  marketplace review, scan malware, deploy a sandbox, or enforce host egress.
  The external handoff package now includes `evidence-bundle-runbook.json` and
  manifest digest coverage for that snapshot.
- It is fair to describe the current repo as a Rust foundation with tested
  scaffolding for IPC, storage, policy, routing, runtime, scheduler, plugin
  contracts, deterministic first-party plugin command execution, bounded
  fake-model and strict provider-envelope planned first-party tool orchestration,
  opt-in Ollama-compatible local HTTP provider behavior, opt-in
  ChatGPT/OpenAI-compatible provider behavior, CLI behavior, and a Swift
  command/management shell with supervisor abstraction, approval decisions,
  adapter-backed voice input/output controls, typed transcript handoff, and
  opt-in final-transcript auto-submit proof when the local gate passes. Live
  microphone/Speech capture, spoken transcript handoff, and live audio-output
  remain pending until a valid owner-recorded live-device QA report passes
  `/release/evidence-status` semantics and evidence-aware readiness is
  explicitly enabled for that release candidate.
- Do not claim autonomous external communication, smart-home control, or
  third-party plugin marketplace readiness for v1.
- Keep public-facing claims scoped to tested local behavior.

## Workflow

- Work in isolated worktrees and branches for reviewable slices.
- Use topic branches and PRs for production work. Treat older phase/worktree
  names as historical coordination context unless the branch is verified active
  in the current checkout.
- Historical phase-3 slices included model-route persistence, plugin-subprocess
  execution, voice adapter controls, packaged app launch proof, permission
  grants UX, and docs architecture alignment. Verify current status from
  `/release/readiness` and the checkout before treating any old worktree name
  as active.
- Older scheduler notification, activity summary, and activity event-stream
  worktree names are historical only unless the branch is re-created and
  verified active in the current checkout.
- When multiple agents are active, stay inside assigned ownership. For docs-only
  architecture work, use `apply_patch` and do not touch implementation files.
- Do not revert or overwrite unrelated work from other agents.
- Keep branch work narrow and commit with clear evidence.
- Push the branch after local verification when requested.
- Treat validation as a merge gate; if a command cannot run, record the blocker
  instead of implying coverage.
- A six-agent autonomous sweep, sometimes referred to as the 6-agent sweep, is
  a coordination model for parallel ownership slices. It is not itself
  readiness evidence; only checked-in code/docs, reviewed PRs, and verification
  output count as proof.
- Durable facts from the May 21, 2026 production sweep: the repo is public at
  `https://github.com/malak333/Jarvis`, work should be split across isolated
  worktrees and `codex/` topic branches, PRs should be reviewable and
  evidence-backed, docs-only workers must not edit Rust or Swift code, and
  readiness language must stay scoped to verified local foundation surfaces
  until distribution-grade app/installer signing, notarization/stapling,
  clean-profile install/Finder validation, owner-recorded live voice/audio QA,
  marketplace/plugin-trust plus OS-level sandbox/egress evidence, final
  evidence bundle archival, and manual release QA gates exist.
- After merging production-readiness PR slices, run the post-merge cleanup
  audit before stronger readiness statements:
  `gh pr list --state open --json number,title,headRefName,baseRefName,url`,
  `gh run list --workflow release-local.yml --branch main --limit 5`,
  `git worktree list --porcelain`, `git branch --merged main --list 'codex/*'`,
  `git branch --no-merged main --list 'codex/*'`, and
  `git status --short --branch`. This distinguishes open review work, current
  public release-local evidence, active worktrees, merged topic branches, and
  historical unmerged branches.
- Durable fact from phase 3 packaged app work: SwiftPM does not create a full
  release `.app` bundle by itself here, so the local smoke assembles the bundle
  deterministically in a temp directory and uses environment-configurable
  supervisor endpoint/database settings to avoid port conflicts and preserve
  clean temp-profile state.
- The user explicitly expects each feature/phase to follow docs and
  documentation, add useful conversation-derived knowledge-base facts, and add
  or confirm end-to-end testing for the discussed scope.
- The June 10, 2026 autonomous production-readiness sweep used six parallel
  audit lanes for release readiness, architecture/KB consistency, E2E coverage,
  Swift voice coverage, release evidence scripts, and GitHub/PR state. The
  live readiness snapshot at sweep start reported `production_ready: false`,
  17 verified features, and one pending feature: `live_voice_loop`. That was a
  historical count from that sweep; the current readiness baseline is tracked by
  the later `verified_feature_count: 16` entries below. The
  pending feature remains a manual external validation gate, not a missing
  repo-local docs-only task.
- PRs #214 through #222 added structural release-evidence hardening, plugin
  trust evidence hardening, package provenance hardening, Mac scheduler action
  controls, GitHub release-local runtime compatibility, archive URI validation,
  release contract wording, evidence-status proof-boundary wording, and current
  sweep snapshot updates while preserving the same readiness boundary from that
  point in the sweep: verified repo-owned features, one pending manual
  `live_voice_loop` feature, and six missing external/manual evidence artifacts.
- `jarvis release readiness` and `jarvis release evidence-status` preserve
  operator-readable defaults. Use `--json` for the canonical machine-readable
  flag, while `--format json` is accepted as a compatibility alias for older
  release scripts or operator notes that used format-style JSON output.
- The release runbook commands follow the same convention:
  `jarvis release live-device-runbook --format json`,
  `jarvis release signed-distribution-runbook --format json`, and
  `jarvis release plugin-trust-runbook --format json` are compatibility aliases
  for their structured `--json` summaries.
- Runbook payload shape has two explicit contracts: CLI `--json` and
  `--format json` produce the operator/snapshot JSON used by release scripts and
  handoff E2E checks, while IPC `/release/*-runbook` endpoints return the
  Swift-facing `ReleaseRunbookResponse` shape. They must preserve the same
  command sequence, manual checks, proof boundary, and evidence summaries, but
  they are not required to be byte-for-byte identical.
- `./scripts/package-distribution.sh --check` is now part of
  `./scripts/release-local.sh`, the readiness recommended-command list, and the
  signed-distribution runbook before the unsigned launch and credentialed
  signing commands; it remains a no-sign preflight for packaging prerequisites
  and entitlement templates, then prints the signed-distribution runbook,
  credentialed packaging, live-device, plugin-trust, final bundle, and evidence
  doctor commands without proving signing, notarization, stapling,
  installation, or live-device QA. The live-device handoff in that output now
  includes release-core command evidence capture, the `task:<uuid>`/`audit:<uuid>`
  evidence-ID recording rule, and endpoint-aware external evidence-mode
  evidence-status/readiness commands. The final bundle handoff now ends with
  both the read-only doctor inventory check and
  `./scripts/release-evidence-doctor.sh --assert-complete`, so copied package
  preflight guidance does not stop before the stronger final inventory
  assertion. `--check-guidance-self-test` is also part of
  `./scripts/release-local.sh` and fails if those handoff commands drift out of
  the package preflight output.
- Release evidence structural hardening now treats the final evidence chain as
  cross-bound evidence, not independent files: app zips are rejected unless they
  contain exactly one top-level `Jarvis.app` payload with `Info.plist`, the app
  executable, and the bundled core; live-device QA `bundled_core.sha256` must
  match signed-provenance `artifacts.bundled_core_sha256`; and final bundle
  owner completion must occur after signed-provenance, live-device QA, and
  plugin-trust child reports are generated but no later than the final bundle
  generation timestamp. Final bundle JSON is written through Python `json.dump`
  rather than heredoc escaping, and the self-test covers multiline, quoted, and
  backslash-bearing owner evidence-note text.
- `release-evidence-doctor.sh --check` remains a read-only inventory and report
  semantics check: it validates the bundled-core version marker and report/file
  bindings without executing the bundled core. Its missing-evidence next-step
  guidance starts with `./scripts/package-distribution.sh --check`, lists both
  supported signing credential forms, and points operators at
  `./scripts/release-external-handoff.sh --write target/release-external-handoff`
  before live-device, plugin-trust, and final bundle capture.
  `--assert-complete` keeps the stronger executable bundled-core `--version`
  check for final local inventory assertion after owner evidence exists.
- For docs-only readiness synchronization phases, record the relevant existing
  E2E or focused integration coverage instead of adding artificial tests.
  Behavior changes still require matching coverage before broader readiness
  language can be used.
- The June 11, 2026 production-readiness sweep refresh was updated again after
  PR #238 from `main` at `4a4661e`: readiness reported
  `production_ready: false`, 17 verified features, and one pending feature
  (`live_voice_loop`) at that historical checkpoint. In the main checkout,
  evidence-status reported 3 satisfied generated local app/core paths, 6
  missing external/manual evidence items, and 0 invalid items; fresh worktrees
  can still report the generated local app paths as missing until local
  distribution commands create them.
  Production readiness still requires signed/notarized artifacts,
  live-device QA, plugin-trust QA, and final evidence bundle reports. PR #231
  made Swift/readiness display fail closed on effective readiness unless
  evidence status is complete, PR #232 clarified exact release evidence script
  handoff commands, and PR #233 hardened the Swift voice UI so unavailable
  capture, missing submit handlers, and busy submitters cannot imply live voice
  loop readiness. PR #234 rejected placeholder owner evidence notes in core
  evidence-status/final-bundle paths, PR #235 ignored stale AVSpeech callbacks,
  PR #236 rejected placeholder live-device QA notes in the shell assertion path,
  PR #237 added a package-check guidance self-test, and PR #238 locked readable
  evidence-status present-item path/detail coverage in CLI E2E plus docs.
- Swift release-readiness fixtures should stay aligned with live
  `jarvis release evidence-status --json` wording, including presence-only
  executable details, `expected evidence path is missing`, `Plugin-trust QA
  report`, and `Release evidence bundle`. The live-device QA shell self-test
  should compare bundled-core version output against `EXPECTED_VERSION`, not a
  hard-coded release string, so version bumps do not create false QA failures.
- The Swift speech-output adapter wraps `AVSpeechSynthesizer` behind an internal
  test seam, trims utterance text before playback, stops existing playback with
  the immediate boundary before replacement utterances, uses the word boundary
  for normal stop and the immediate boundary for operator interruption, tracks
  the active AVSpeech utterance by object identity, and ignores completion/cancel
  callbacks for older utterances. Swift tests cover these concrete adapter
  branches without invoking live audio output.
- Release evidence placeholder hardening now rejects owner-recorded placeholder
  notes in live-device QA reports and final release evidence bundles through
  core IPC/evidence-status validation. The final bundle script rejects the same
  placeholders before writing a bundle, and readable
  `jarvis release evidence-status` output includes each evidence item's path and
  detail, including present presence-only caveats on the item line, so
  operators do not need `--json` for basic triage.
- Final release evidence bundle generation is overwrite-protected by default:
  generated templates set `JARVIS_EVIDENCE_OVERWRITE_OUTPUT=false`, and
  `release-evidence-bundle.sh --bundle` rejects an existing output path unless
  the operator has preserved the old artifact and intentionally sets the
  overwrite flag to `true`. The final bundle output path must also be distinct
  from signed-provenance, live-device QA, plugin-trust QA, app zip, and
  installer package input paths, so the bundle writer cannot replace evidence it
  has just validated.
- `JarvisMacAppTests` covers app-level Release tab presentation for
  presence-only evidence rows. Release tab evidence rows explicitly label the
  evidence path, detail text, and production/manual-gate requirement context so
  operators can distinguish local presence from required external evidence
  without opening JSON. `JarvisMacCoreTests` continues to cover the
  release-readiness model and evidence-status decoding.
- Rust release-evidence tests that must create canonical signed-distribution or
  final-bundle artifact fixtures under `target/distribution/Jarvis.app` should
  hold the shared release-evidence artifact fixture lock while writing and
  validating those files, because the workspace test runner executes evidence
  tests in parallel.
- `jarvis release evidence-status --help` documents that the default readable
  output includes per-item paths/details and same-line presence-only caveats;
  keep this help text, CLI E2E assertions, and `docs/release-checklist.md`
  aligned when the readable release-evidence format changes.
- `jarvis release plugin-trust-runbook` is the handoff from plugin-trust QA
  into final evidence bundling: after `release-evidence-doctor.sh --check`, it
  should list `release-evidence-bundle.sh --check`, template writing, source
  plus `--bundle`, and `release-evidence-doctor.sh --assert-complete`.
- `jarvis-cli serve --db-path <path>` starts IPC with SQLite-backed task,
  audit, memory, and emergency-pause state for manual persistence checks.
- File-backed `SqliteRepository::open` creates a preflight migration backup
  for existing DBs below the current schema version and restores the original
  DB/WAL/SHM files if opening/configuring/migrating fails. Backups are
  app-owned local files, may include personal memory/audit/plugin metadata, and
  are not redacted diagnostics exports. Keychain secrets are not stored in
  SQLite backups.
- Storage migration coverage includes a representative schema v1-v13 fixture
  matrix that preserves task, audit, emergency-pause, memory, scheduler,
  approval, installed-plugin, plugin-provenance, and route records through the
  current schema. This is repo-owned migration proof, not installer upgrade or
  Finder/LaunchServices validation.
- `cargo run -p jarvis-cli -- smoke` now covers baseline command/pause smoke,
  plugin manifest listing, and repository-backed task, model-route, explicit
  memory-management paths, diagnostics redaction, and repository-backed
  scheduler/job state surfaces.
- The 2026-06-12 production-readiness sweep refresh originally recorded the
  merged state through PR #259. A historical follow-on baseline through PR #268
  used `main` at `8cccb5b`, hosted GitHub `Release local gate` green for PR
  #268 run `27428860335` / job `81073692261`, `production_ready: false`,
  `verified_feature_count: 16`, `pending_feature_count: 1`, and, after local
  generated app presence artifacts exist, six missing external/manual evidence
  artifacts. Current release claims should refresh this baseline with
  `cargo run -p jarvis-cli -- release readiness --json` and
  `gh run list --branch main --workflow "Jarvis Release Local Gate" --limit 3`.
  The latest verified main baseline at `042c60e` has hosted GitHub
  `Release local gate` success for push run `29344743720` / job
  `87125361398` and remains conservative with `production_ready: false`,
  `evidence_mode_enabled: false`, `verified_feature_count: 23`, `pending_feature_count: 1`, and
  `live_voice_loop` as the pending manual feature; `/release/evidence-status`
  reports `complete: false`, three satisfied evidence rows, six missing rows,
  and no invalid rows. PRs #283-#324 added repo-owned clarity for voice
  permission gating, external handoff guidance, architecture documentation,
  plugin-trust artifact SHA guidance, final doctor assertion guidance, Release
  tab runbook-load warnings, external handoff mechanics, evidence-note
  rejection, Swift Release tab readiness presentation guardrails, external
  handoff evidence-status parity, and missing live-device evidence-field
  fail-closed E2E coverage without converting owner-recorded external evidence
  into local proof. The post-PR #320 refresh keeps the current-state and
  production-target architecture diagrams aligned with the latest hosted main
  gate while preserving the installed-plugin plus model-step/model-output
  activity-progress proof and the manual/external release boundary.
  It also preserves the handoff manifest digest check, the release-local command heartbeat
  observability, and the rule that the final bundle output path must also be
  distinct from signed-provenance output paths.
  These repo-owned hardening slices do not satisfy signing, notarization,
  installation, live-device QA, plugin-trust QA, or final evidence-bundle
  requirements.
- The macOS Release tab must render runbook readiness through the same effective
  readiness boundary as the page-level production-ready label. A raw runbook
  `production_ready: true` is displayed as blocked when cached readiness,
  incomplete evidence, invalid evidence, or failed evidence refresh keeps
  `effectiveProductionReady` false; the `live_voice_loop` row must continue to
  surface `pending_manual_validation` rather than implying hardware validation.
- Bash final-bundle and evidence-doctor consumers must reject placeholder child
  report owner notes with the same strictness as the live-device/plugin-trust
  report producers and Rust evidence-status path. Child reports with note text
  such as `pending`, `placeholder`, `fixture`, `todo`, or similar cannot clear
  final release evidence assertions.
- Plugin-trust artifact evidence is enforced as a complete six-category set.
  Evidence-status, evidence-doctor, and final bundle validation reject missing,
  placeholder, or non-SHA-256 artifact bindings for marketplace review,
  malware scan, OS sandbox, egress enforcement, signed-publisher policy, and
  manual trust review before plugin-trust QA can count as valid release
  evidence.
- External handoff manifests must bind to the canonical `scripts/release-version.sh`
  release version. `JARVIS_RELEASE_HANDOFF_VERSION` is only an explicit guard and
  fails closed if it drifts. Operator-facing zip evidence should say the app zip
  is Developer ID signed and notarized; stapling is validated against the app
  bundle itself, not the zip container.
- The macOS Voice tab surfaces release live-device audio-output evidence fields:
  `JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL` stays an operator-recorded device label,
  while `JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE` includes the last spoken preview
  text and speech-output status when available. This improves manual evidence
  capture only; it does not prove live audio playback without owner validation.
- CLI E2E now runs `release-live-device-qa.sh --assert-complete` with a
  repository-backed command result and a script-generated live-device QA
  report, then verifies `jarvis release evidence-status` accepts that report and
  external-mode readiness moves `live_voice_loop` to implemented while
  production readiness stays blocked by the remaining signed-distribution and
  final-evidence gates. This proves script/status/readiness compatibility for
  owner-recorded evidence only; it does not automate live microphone, Speech,
  audio-output, or notification validation on a real Mac.
- PR #254 made release runbooks a current implementation surface rather than a
  CLI-only operator path: `/release/live-device-runbook`,
  `/release/signed-distribution-runbook`, and `/release/plugin-trust-runbook`
  are read-only IPC endpoints, and the Swift Release tab renders those payloads
  when available. This is app guidance visibility only, not evidence completion.
- PR #256 and PR #257 aligned live-device QA operator guidance across the
  generated `target/release-live-device-qa.env` template, CLI fallback
  `jarvis release live-device-runbook`, and IPC `/release/live-device-runbook`:
  the command path now tells operators to capture a release-core
  `jarvis command "status check" --json` result, record the returned
  `task:<uuid>` or task-associated `audit:<uuid>`, then rerun
  evidence-status/readiness against the release endpoint with
  `JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external`. Rust E2E, core unit tests,
  Swift decode tests, and the live-device shell self-test cover this guidance
  without claiming live-device QA was performed.
- The live-device QA env template now carries one
  `JARVIS_RELEASE_CORE_ENDPOINT` value plus the app-owned
  `JARVIS_IPC_TOKEN_FILE` path that the command-evidence capture and
  post-report external evidence-status/readiness checks reuse. CLI fallback and
  IPC live-device runbooks pin the same sourceable endpoint handoff so
  operators do not collect `task:<uuid>`/`audit:<uuid>` evidence from one core
  and verify readiness against another endpoint.
- The release-evidence doctor missing-evidence next-step guidance now mirrors
  the same live-device release-core command capture and endpoint-aware external
  evidence-mode handoff as package preflight, the generated live-device
  template, and the live-device runbook. Package preflight and evidence doctor
  now also tell operators to set `JARVIS_RELEASE_CORE_ENDPOINT` once and reuse
  it for command evidence plus external evidence-status/readiness checks. Their
  shell self-tests pin those strings as guidance only; they still do not perform
  live-device QA or create release evidence.
- The signed-distribution and plugin-trust runbooks now reuse the same guarded
  `JARVIS_RELEASE_CORE_ENDPOINT` external evidence-status command before doctor
  checks or final bundling. CLI E2E, IPC unit coverage, and Swift runbook
  fixtures pin the command text so copied operator commands fail fast instead of
  silently inspecting a different running core.
- Release readiness recommended verification commands now include the same
  structured scheduler notification fields required by live-device QA plus the
  per-category plugin artifact URI/SHA-256 fields required by plugin-trust QA.
  Core unit tests and CLI local IPC E2E pin those fields so the readiness
  examples cannot drift behind the assertion scripts.
- `./scripts/release-external-handoff.sh` is the single operator handoff
  generator for the remaining external production gates. `--write` creates
  `release-live-device-qa.env`, `release-plugin-trust-qa.env`,
  `release-evidence-bundle.env`, read-only readiness/evidence-status/runbook
  JSON snapshots, `release-evidence-checklist.md`,
  `release-handoff-manifest.json`, and a README with the ordered release
  sequence. The checklist names exact signed-distribution
  artifact paths, live-device command/result and scheduler notification fields,
  per-category plugin artifact URI/SHA-256 bindings, and the final reports
  archive URI. The generated manifest records schema version, evidence type,
  generation timestamp, release version, git commit, snapshot endpoint, proof
  boundary, byte counts, and SHA-256 digests for each generated handoff file.
  The shell self-test verifies the expected file list, release version, current
  git commit, byte counts, and SHA-256 digests before passing.
  The generated README's final evidence-status/readiness commands use
  `JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external` plus guarded
  `JARVIS_RELEASE_CORE_ENDPOINT` expansion, matching the live-device,
  signed-distribution, plugin-trust, package-preflight, and evidence-doctor
  handoff guidance. CLI E2E also verifies that the generated runbook snapshots
  preserve the same command arrays, key evidence rows, feature state, and proof
  boundaries as fresh direct CLI runbook JSON, that the generated
  `release-evidence-status.json` snapshot preserves the same completion state,
  missing evidence, invalid evidence, and evidence item rows as a fresh
  external-mode direct CLI evidence-status query, and that manifest digests match
  the generated handoff files. `--check` and `--self-test` are part of the local
  release gate and prove only template plus snapshot/checklist/manifest
  generation with validation flags defaulted false; they do not sign, notarize,
  install, Finder-launch, validate live device behavior, review plugin trust,
  enforce egress, or archive final evidence.
- Owner evidence-note validation now rejects embedded placeholder wording, not
  only exact placeholder values. Shell assertions and Rust evidence-status
  reject operator notes containing terms such as `TODO`, `pending`, `fixture`,
  `example`, `self-test`, `replace-me`, or `changeme` in live-device,
  plugin-trust, and final-bundle evidence reports.
- `./scripts/release-docs-drift-smoke.sh` is part of `release-local.sh` and
  keeps the canonical release command matrix plus external evidence-mode,
  `task:<uuid>`/`audit:<uuid>`, owner-recorded external evidence, and
  `release-external-handoff.sh` boundary phrases represented in
  `docs/build-test-commands.md`, `docs/release-checklist.md`,
  `docs/architecture-map.md`, and this KB.
- The Swift shell now has native menu-bar presence through `MenuBarExtra`. The
  menu and main window share one `JarvisCoreSupervisor`, `CommandConsoleModel`,
  and `ModelConfigurationModel`; `jarvis-main` is the stable scene identifier
  used to reopen the existing shell. Menu actions expose only open, health
  refresh, supervised core start/stop, and quit. Swift contract tests pin the
  scene ID and all supervisor-state presentations, while signed installed-app
  rendering and window-reopen behavior remain manual Finder/LaunchServices QA.

- The Ollama generation path uses `stream:true` NDJSON transport with a 1 MiB
  wire cap, 512 KiB assembled-response cap, and at most 256 redacted metadata
  chunks. It handles split UTF-8 and LF/CRLF framing, requires one terminal
  `done:true`, rejects malformed/error/post-terminal data, and parses tool
  envelopes only after the complete stream validates. Runtime cancellation
  polls the active provider future, discards partial state, and rechecks after
  completion so cancellation wins before audit or tool execution. Swift decodes
  deduplicated provider-native count metadata after completion; it does not show
  raw partial text or claim live-token transcript streaming.

### Active interactive command cancellation

- `POST /commands` has an additive optional UUID `cancellation_id`. Current
  Swift and CLI clients generate it before submission; CLI accepts
  `--cancellation-id` for coordination and exposes `jarvis cancel-command`.
  The Swift console rejects keyboard, voice, or direct overlapping submission
  before changing `activeCancellationID`, so one submit cannot orphan another.
- Rust registers and activates the handle for the full active command, binds it
  to only the task created by that request, propagates cancellation into the
  provider/tool paths, and caps the shared registry at 128 active handles.
  Duplicate and over-capacity registration fail closed. Finalization retains
  the 1,024 most recently consumed UUIDs as FIFO tombstones; recent reuse also
  conflicts, preventing a delayed stale cancel from targeting later work in
  that process-local window. Clients must always generate fresh random UUIDs
  because tombstones are evicted after the cap and lost on core restart.
- Authenticated `POST /runtime/cancellations/:id` returns
  `outcome: cancellation_requested` with `active_execution_found: true` only
  when that exact execution is active. Unknown, completed, or already-finalized
  handles return `outcome: not_found`; they do not cancel unrelated work.
- Guard finalization is the result-acceptance linearization point. When
  cancellation wins, the task becomes cancelled and late model steps/plugin
  results are removed from the command response. When completion wins, a later
  cancel is honestly not found. Swift shows Cancel only while its generated
  handle remains active.
- For installed `local_subprocess` runs, the active handle is also polled inside
  the worker wait loop. Cancellation or emergency pause terminates the full
  dedicated process group, escalates TERM to KILL after a bounded grace, reaps
  the leader, and joins stdin/stdout/stderr workers with a bound before returning.
  This controls only processes that remain in the group; deliberate
  `setsid`/`setpgid` escape remains unenforced without an OS-sandboxed helper.
  Approved cancellation still records `effect_possible: true` and
  `automatic_retry_allowed: false`; process termination is not effect rollback.
- Focused Rust race tests, real-server CLI/Ollama-stub E2E, and Swift model tests
  cover targeted cancellation, active/not-found evidence, late-output
  suppression, and UI lifecycle. This remains process-local cooperative
  cancellation: it cannot reverse an external effect, survive a core crash, or
  establish distributed cancellation, signed distribution, or live-device QA.

## Safety Guardrails

- Local model routing is the default.
- ChatGPT is the only approved cloud model and requires explicit env opt-in,
  explicit routing, sensitivity checks, minimized redacted context, and audit
  evidence.
- Side effects pass through capability scopes plus risk tiers.
- High-risk or uncertain actions fail closed.
- Emergency pause, cancellation, and auditability are architectural
  requirements.
- Plugins must declare capabilities, scopes, risk tiers, schemas, proactive
  behavior, memory access, model access, audit fields, timeout behavior, and
  cancellation behavior before execution.
- Installed plugin execution remains disabled by default and must not be
  expanded into arbitrary local code execution. The current executable boundary
  is limited to `local_subprocess` manifests that declare JSON stdin/stdout,
  use a command canonicalized under `source_path`, are explicitly enabled with
  `execution_grant: subprocess_stdio` for non-network actions or
  `subprocess_stdio_network` for network-declaring actions, validate input and
  output schemas, run with the declared timeout, clear inherited environment
  variables, and emit audit evidence including whether the subprocess started.
  These grants are action-scoped; a network grant does not execute plain
  non-network actions in mixed manifests.
  Subprocess stderr may contain bounded progress frames, but raw stderr plus
  local plugin paths and provenance hashes remain redacted from response and
  audit payloads. Any broader executable path or
  real-time plugin progress
  stream needs a stronger OS-level process/network sandbox or equivalent host isolation boundary,
  explicit grant state beyond `metadata_only`, policy checks,
  timeout/cancellation behavior, and E2E audit coverage.

- Schema v11 adds a disabled-by-default trusted macOS system-wake rule. Swift
  stores its P-256 private key and monotonic counter in device-only Keychain
  items. Normal startup never obtains those wake credentials. Explicit initial
  provisioning prepares only the public key while the current app-owned core
  stays running, stops only after preparation succeeds, and uses one bounded-
  stdin restart whose bytes are then discarded. Every later normal restart
  relies on the persisted Rust enrollment and uses neither trusted-wake
  Keychain access nor bootstrap stdin. Swift signs active-session/challenge/
  generation-bound wake payloads. Rust validates
  signature, replay, nonce, skew, generation, pause, and proactive policy,
  persists redacted scheduler/audit evidence, and writes a dispatch-start CAS
  before the existing command funnel. Exact retries resume only a still-
  accepted current event; started or terminal events never redispatch. The
  signer allocates counters above the Keychain value, current epoch
  milliseconds, and Rust's durable high-water so local counter loss or clock
  rollback recovers without weakening replay protection. Ambiguous dispatches
  are visible in Swift and can only be resolved explicitly without retry. This is
  enrolled-key possession, not Apple attestation or OS-wake provenance. The
  explicit initial bootstrap always sets `allow_rotation: false`; the core now
  permanently rejects bootstrap mutation of an existing key or command.
- Supported key control is explicitly two-step. `rotate` requires an old-key,
  active-session, domain-separated P-256 proof binding the source generation,
  old fingerprint, candidate fingerprint, exact confirmation, timestamp, and
  nonce. `recover` intentionally has no old-key proof and requires the stronger
  `RECOVER LOST TRUSTED WAKE KEY AND BLOCK PENDING WORK` phrase. The packaged
  app route requires its per-launch bearer while an explicit legacy server does
  not; in either mode this is local operator accident prevention, not device
  authentication, OS identity, ownership proof, or same-user/process isolation.
- Prepare uses one SQLite IMMEDIATE transaction and CAS to reject ambiguous
  dispatch, block accepted old-generation work, cancel its scheduled handoff,
  disable the rule, increment generation, reset high-water, persist only a
  short-lived one-shot token hash plus new fingerprint, and append redacted
  audit. Pending or expired-unretired grants quarantine enablement. Expired
  history is retained and audited when an explicit replacement prepare retires
  it.
- The Swift key ring stages a candidate while retaining the active key, journals
  source/target generations, old/new fingerprints, expiry, and the returned
  one-shot token in device-only Keychain, and uses one explicit supervised
  `--trusted-wake-key-control-stdin` restart. Rust atomically validates and
  consumes the grant, installs only the staged public key, and leaves the rule
  disabled. Swift promotes the candidate only after status proves target
  generation plus candidate fingerprint. Journal-first cleanup converges after
  install/cancel crashes; unjournaled staged candidates are safely discarded.
  Expired or near-expiry grants never stop the healthy core. There is no
  automatic retry, rollback, or enablement. Manual SQLite or Keychain mutation
  is not a supported workaround. CLI prepare accepts one maximum 8192-byte JSON
  document only through `--document-stdin`; proof, candidate, confirmation, and
  token fields never enter argv. Its response token is a secret and must flow
  directly into trusted device-only Keychain journal code, which constructs the
  distinct supervised install document. The raw response is not install stdin;
  neither form may enter terminal output, shell history, logs, or files.
- Focused proof lanes are `cargo test -p jarvis-core trusted_wake -- --nocapture`,
  `cargo test -p jarvis-cli --test local_ipc_e2e trusted_wake -- --nocapture`,
  and `swift test --disable-sandbox --package-path apps/mac --filter TrustedWake`.
  They cover adversarial proof bindings, signed rotation, destructive recovery,
  legacy bypass rejection, wrong key, token replay, old signature rejection,
  grant expiry/quarantine, crash reconciliation, lifecycle serialization, and
  audit redaction. The controlled bootstrap test fixture waits asynchronously
  with a bounded deadline; a synchronous semaphore wait on the main actor can
  starve the provision task under CI load and must not be reintroduced. They do
  not prove Apple attestation, OS provenance,
  background launch, same-user/process isolation, live-device behavior, or
  production readiness.
- App-supervised IPC defaults to Unix-domain transport with audit-token code
  identity, same-EUID, and bearer defense in depth. Swift rotates 32 random
  bytes and a generation-random socket leaf per launch, shares that state
  between `JarvisCoreSupervisor` and `JarvisIPCClient`, and sends
  `ipc_transport:{kind:"unix_socket_peer_identity_v1",socket_path:
  "/absolute/path.sock",peer_code_requirement:"...",peer_identity_profile:
  "adhoc_exact|developer_id_hardened"}` plus the bearer only in bounded startup
  stdin. The runtime directory is current-owner `0700`, the socket is `0600`,
  and both peers retrieve `LOCAL_PEERTOKEN`, validate dynamic code against the
  designated requirement before framing, and require the connected peer's
  `getpeereid` EUID to equal their current EUID. Rust still protects the
  complete router with exactly one strict Bearer value and constant-time digest
  comparison.
- The UDS wire contract is one four-byte big-endian length plus one exact,
  versioned JSON request, a required client write-half close, and one framed
  response per connection. Requests admit only GET,
  POST, DELETE, or PATCH and exact nullable authorization/accept/content-type
  fields plus a standard padded-base64 body. Responses carry exact version,
  status, nullable content type, and padded-base64 body. Unknown fields,
  malformed or trailing frames, oversized frame/body, hard deadline expiry, and
  concurrency exhaustion fail closed. Stop, failure, replacement, and observed
  exit clear matching launch state; cleanup removes only a validated socket
  leaf and never recursively removes a directory.
- Exact `JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true` replaces default UDS with the
  explicitly weaker authenticated loopback TCP plus owner-only
  `ipc-session-auth.json` compatibility path. `JARVIS_MAC_IPC_AUTH_FILE` is an
  absolute override only in that mode. File parsing remains bounded,
  no-follow, current-owner, regular-file, single-link, and permission checked;
  strict loopback and legacy no-downgrade checks remain. App-only settings,
  exact release-smoke readiness opt-in, and `JARVIS_IPC_TOKEN_FILE` are stripped
  from the child. The packaged lane accepts the non-secret readiness line only
  after the Swift supervisor completes authenticated health and then verifies
  child exit plus socket cleanup. Cross-process Rust,
  Swift, and packaged-layout coverage must prove default route parity,
  audit-token requirement acceptance, same-EUID wrong-code pre-frame rejection,
  bearer rejection, strict framing and bounds, socket
  ownership/modes/path bounds, lifecycle cleanup/restart invalidation, and the
  explicit compatibility path. This proves bounded local transport, designated-
  requirement checks for the evaluated signature profile, same-EUID checks,
  bearer possession, and repository-owned lifecycle. The ad-hoc profile proves
  only exact-build cdhash mechanics; it does not prove Developer ID publisher
  identity, device authentication, XPC, App Sandbox/egress enforcement,
  notarization, or live-device behavior.
- Signed distribution and live-device evidence are now joined by the exact app
  executable. `package-distribution.sh` records its path/SHA-256 and structured
  codesign Identifier, ten-character TeamIdentifier, and CDHash.
  `release-live-device-qa.sh --assert-complete` requires the signed-provenance
  report, revalidates the installed executable with codesign, stapler, and
  Gatekeeper, records the installed identity plus signed-provenance path/SHA,
  and fails on drift. Bundle, doctor, Rust unit, and CLI E2E validators enforce
  the same cross-report binding, including
  `release_evidence_status_rejects_live_app_executable_digest_mismatch`. This is
  point-in-time candidate evidence, not installation provenance, continuous
  integrity, Apple attestation, or proof that manual live-device QA occurred.
