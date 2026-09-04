# Release Checklist

Use this checklist before tagging or publishing any Assemblywright release.
Keep the evidence local-first unless the owner explicitly approves hosted
infrastructure. This checklist separates repository validation from
owner-recorded external evidence; a green local gate is never a
production-readiness claim.

## Scope Check

Before starting a release pass, confirm the claim you intend to make.

- Name the exact surface being released and the exact evidence that supports
  it. Do not describe autonomous orchestration, registered-source mutation,
  live selected-provider quality, or GitHub publication coordination as
  implemented. Schema v16 implements bounded review-provider invocation only
  through the default-unavailable owner-loopback gateway; repository proof does
  not establish production provider provisioning or reviewer competence.
- For the brainstorming provider, verify the tool-free planning contract separately
  from sandbox labels. Windows intentionally uses Codex inner `danger-full-access`
  inside Assemblywright's restricted-token AppContainer, exact ACL tree, closed
  environment, pinned executable, and kill-on-close Job; non-Windows uses inner
  `read-only`. Never describe the Windows inner value as host full access or
  implementation authority, and never describe it as inner read-only.
- For Windows planning, verify the schema-v4 master-private locator and the canonical
  Common Application Data runtime instance separately. Record native proof that both
  fixed profiles have only non-inheriting traverse rights on the held shared ancestry,
  unrelated ACLs remain intact, and path/identity/ACL drift fails closed. Repository
  tests are not live Windows deployment evidence. Verify the fixed profile's exact
  deterministic SID before idempotent registration in the current Windows service
  identity; owner registration is not LocalSystem registration. With
  `STARTF_USESTDHANDLES`, require valid inherited stdin, stdout, and stderr handles and
  prove that concurrently drained stderr remains content-free and bounded. Keep source
  contracts, unit tests, the synthetic LocalSystem service E2E, a real Codex call, and
  production deployment as separate evidence layers.
- For the Windows execution-host substrate, run the path-free `DryRun`, then require
  `Check` to reject any owner-account or user-local Master installation. Never convert
  that rejection into a readiness claim. `Apply` is owner-only and requires the exact
  stopped LocalSystem Master plus already-installed stopped Broker/Executor services,
  distinct Master/Broker service SIDs, the restricted LocalService
  Executor SID, SYSTEM-owned inheritance-protected ACLs, allocated disk reserve, and
  a policy digest with effects fixed off. Require the fixed Program Files image names,
  exact release-manifest SHA-256 values, exact valid Authenticode signer, protected
  install/image ancestry, and non-sparse/non-compressed reserve. EffectsEnabled must be
  written and verified as zero before any other Apply mutation; hostile or non-exact
  pre-existing policy/reserve leaves reject without truncation. Run the disposable
  native hostile E2E and
  require an allowed marker from the real restricted-service payload before accepting
  its file/reserve/service-definition denial assertions. Require its call to the
  production provisioner's `SelfTest` to prove the real policy-file, disk-reserve, and
  registry validators reject hostile hardlink, symlink, and effects-enabled fixtures
  unchanged. Require exact complete role-specific service argv, disabled stopped
  own-process/noninteractive configuration, no dependencies, triggers, recovery command
  or actions, exact required privileges, and stopped-state rechecks around every
  mutation cluster and receipt. SelfTest must prove its valid disposable SCM contract
  and reject extra argv, type/start, failure-action, trigger, missing trusted
  inheritance, and inheritable Executor-read drift. Require a later-created sibling
  canary to remain unreadable by the restricted token. The native E2E
  must also build and start the actual Broker and Executor service-host entrypoints,
  require `RUNNING` with a process ID only after Broker runtime construction and full
  Executor semantic-bootstrap validation, stop them
  cleanly, and reject hostile digest, correctly re-digested semantic config, and argv
  starts. Verify the Executor can read/execute only its immutable image/config ancestry
  through non-inheriting grants, cannot read a later-created sibling, cannot mutate it,
  and both configs are read-only. Record this as inert lifecycle
  evidence, not IPC/dispatch evidence.
  Verify serialized Executor configuration contains no receipt-signing seed and the
  Windows service constructs no active Executor runtime. Active runtime wiring requires
  authenticated out-of-band Broker secret injection only after payload-process access
  to the Executor process is denied and proved.
  Record the Job CPU/commit/
  process settings as activation-attested substrate only until production runtime
  creates and verifies the Job. Neither source tests nor the disposable service proof
  establish signing, production provisioning, product-routed IPC, effect activation,
  or two-host use.
- For the authenticated inert Windows execution IPC foundation, run
  `windows_execution_ipc_contract`, both Broker/Executor `inert_execution_ipc` suites,
  `windows_execution_ipc_foundation`, `windows_execution_ipc_source_contract`, and
  `scripts/windows-execution-ipc-e2e.ps1 -Confirm`. Require independent Master
  signatures for exact Broker and Executor endpoints, byte-exact Broker forwarding,
  local-only single-instance pipes, exact peer service-SID token checks, separately
  pinned service acknowledgement keys, path-free zero-effect acknowledgements, and
  append-only intent-before-ack state. Reject wrong SID, unsigned/tampered, replay/gap,
  stale/future, authority/endpoint drift, changed pending requests, and partial
  journals. Restart may recover only the byte-exact pending inert frame or replay the
  original completed acknowledgement. Verify service seeds are distinct out-of-band
  protected 32-byte leaves, are absent from serialized config/argv/environment/output,
  and the Broker never receives the Executor seed. Record that
  `UnavailableAssemblyLineEffectDispatcher` remains installed and that this proof does
  not establish adapter execution, active cancellation, production installation,
  signing, or live-device use.
- Confirm `docs/feature-conveyor-design.md` still marks the implemented slice
  accurately. The repository kernel is default-inert and exposes the
  owner-token-authenticated loopback read-only
  `GET /v1/feature-conveyor/status` observation seam. Its queue-, pause-, and
  optional feature-lifecycle-revision-bound owner-guidance labels remain
  display-only and do not establish claimability.
  The dedicated remote `GET /v1/distributed/feature-conveyor/status` is
  read-only, requires an accepted exporter-bound MacBridge session, denies
  other roles, and forwards no owner token. The Mac renders it as text, never a
  callable owner action. Separately confirm schema v8 designates exactly one
  current non-fixture owner-control MacBridge through the owner-token loopback
  route, and that only its accepted, revalidated session can enqueue one strict,
  already-approved, queue/designation/pause-revision-bound specification. The
  signed helper action requires `--confirm`, bounded stdin, a strict redacted
  receipt, and session close. Confirm the typed app authoring form accepts only
  review-safe fields and explicit digest/grant/provider/dependency bindings,
  inserts the exact validation gate, uses the current authenticated revisions,
  requires a second confirmation, revalidates the helper after stopping
  observation, and resumes observation only after strict receipt validation.
  Confirm the frozen summary includes IDs, digest bindings, grants, fixed
  provisioned provider/model, dependencies, title, and outcome; embedded token
  patterns reject. Prove a lost receipt leaves an explicit in-memory recovery
  action that resends identical bytes only after another confirmation, and that
  Windows exact replay returns one original receipt with no second queue/audit
  mutation while every drift case rejects. Windows must still recompute the
  canonical manifest digest. Separately confirm the
  owner-token loopback repository-grant routes enforce contiguous compare-and-
  set and Emergency-Pause-revision binding, allow revocation while paused,
  expose only current digest metadata, and remain absent from enrolled-device
  mTLS. They perform no repository access. Separately confirm the owner-token
  loopback repository preflight is bound to the exact active registration
  grant and Emergency Pause revision, limits the base branch to one component, uses only bounded point-in-time
  filesystem identity inspection with no Git process or configuration load,
  returns and stores no path, and commits only redacted audit plus a path-free
  receipt. On Windows it must hold non-reparse identity-chain handles, reopen
  and compare the complete canonical pathname and identities immediately before
  the final atomic recheck, and retain the fresh handles through it. Confirm the five-second timeout claim is
  limited to each filesystem-observation await and does not cover authentication
  or database lock/audit latency. This does not create a cross-filesystem-and-
  SQLite snapshot. It does not prove clean-tree or content state and creates no reusable
  snapshot or claimability. A checkout containing valid `.git/worktrees`
  metadata is deliberately ineligible; do not prune or delete that metadata to
  force a pass. Use a dedicated standalone checkout for positive proof, obtain
  the path-free receipt, record the next revoked grant revision, confirm it is
  inactive, and remove only the disposable checkout created for the proof.
  Windows fixtures must use the final normalized DOS path from the held
  directory handle rather than a verbatim `\\?\` path from
  `std::fs::canonicalize`. The slice
  also exposes a separate owner-token loopback snapshot-claim POST. Confirm it
  is absent from the remote mTLS router; rejects stale head/dependency/lease,
  queue/pause/grant/provider bindings; reads raw objects without Git process,
  source config, hooks, attributes, credentials, global config, PATH,
  alternates, links/reparse entries, gitlinks, remotes, or hardlinks; returns no
  path; copies only the exact current commit/tree/blob graph with shallow
  metadata and no parent/deleted history; fail-fast serializes bounded snapshot
  work while retaining the reservation after HTTP timeout until the blocking
  task exits; and atomically binds one durable snapshot receipt to one lease and
  redacted audit. Prove failure/cancellation leaves no lease or referenced snapshot,
  and restart cleans unreferenced state and quarantines a finalized lease. The
  snapshot also contains one deterministic bounded transfer bundle whose raw
  object graph and file manifest are independently digest-bound.
  For the retained historical protocol-v4/schema-v12 compatibility proof,
  schema v10 separately exposes one owner-token loopback-only metadata coding-
  dispatch POST. Confirm it is absent from the enrolled-device router, binds
  the exact feature/specification/lifecycle, feature lease, snapshot ID/digest,
  queue/pause revisions and current singleton `local.coding.v1` worker
  registration, and atomically commits its queued step, immutable binding,
  event, and redacted audit. Prove stale authority, cancellation, Emergency
  Pause, lifecycle departure, registration drift, and restart quarantine block
  lease or acknowledgement. Confirm protocol and native-agent tests accept no
  repository bytes/path, caller-selected commands, tools, providers, tests, or
  credentials. Then prove
  the separate default-off snapshot lane authorizes only exact active
  attempt/lease/cancellation/snapshot bindings, rechecks around every bounded
  read, rejects stale or out-of-order frames, and materializes through the
  authenticated Mac local socket into fresh private per-attempt state. Prove
  object/chunk/bundle/path/digest validation, an aggregate materialized-output
  byte budget, and no remote/hooks/links. Confirm
  the only execution is one fixed child forked from the running agent with no
  `exec`, argument parsing, or remote input; parent pre-opens the workspace,
  blocks signals, and captures the descriptor-table bound and effective UID
  before `fork`; child scans with `F_GETFD`, closes every open descriptor except
  the workspace and gate, waits for the parent-established process group, and
  follows the fixed validated open/truncate/seek/write/sync/close/exit path for
  `README.md`; no post-fork errno, mutable-global, environment, identity,
  descriptor-table, or process-group discovery. Prove Swift launches the agent
  with an empty environment, the agent rejects local-coding startup under a
  nonempty parent environment, and it rejects any
  changed path or output outside the fixed contract; emits bounded path-free
  work-packet/admission/snapshot/path-set/patch digests, one changed file,
  `test_status:not_run`, mutation true, workspace-retained false, and ambiguity
  false. Prove Rust and Swift recompute the exact protocol-owned admission
  transcript, including protocol version, all five identities, epoch, sequence,
  lease duration, and deadline, rather than accepting any nonzero digest, and
  prove cleanup of transfer/materialized state before returning. Prove
  cancellation, pause-driven durable cancellation, deadline/lease loss,
  failure, shutdown, and restart dominate completion, boundedly TERM-to-KILL
  reap the child process group, clean state before acknowledgement, suppress late
  results, and unblock an in-flight local Unix request so cancellation can meet
  its strict two-second acknowledgement deadline. Do not
  infer a host sandbox or host-egress control from the forked child. This slice exposes no retained
  worker checkout, arbitrary coding/test execution, canonical-repository
  mutation, result integration, review provider, publication coordinator, Mac
  control UI, queue advancement, or autonomous activation.
- For Feature 2 restricted-worker activation evidence, require the exact clean
  published `main` controller with no repository/remote/worker/executable/packet
  selector; committed Mac-harness stdin execution with a distinct sanitized-
  receipt descriptor; the separately enrolled singleton `local.coding.v1`
  identity; signed Swift relay; real Rust agent; protocol v5/schema v19; strict
  queued/leased/succeeded order; exact snapshot, packet, artifact, and detached
  remote-free candidate evidence; private retained-pair observation; explicit
  cancel and safe abandon; and empty queue, lease, active distributed state,
  transfer staging, temporary grants, and disposable checkout. Reject dirty or
  hidden-index state, repository/controller drift, malformed or duplicate
  terminal/action records, surviving descendants, unrecoverable partial
  preparation, partial cleanup, or stale prior
  output. The local path-free receipt must expose only fixed category/origin,
  commit/tree, definition and private-transcript digests, proof identity, time,
  pass status, and boundary. Do not admit it automatically or treat it as host-
  sandbox, OS-wide-egress, provider, GitHub, restart, control-streaming,
  signing/notarization, clean-profile, or production-readiness proof.
  Confirm Prepare resumes only marker-bound missing exact revision-1 grants and
  that a lost helper enqueue receipt can continue only from the one exact queued
  lifecycle-revision-1 feature at baseline queue revision plus one.
- For Feature 3 review-provider activation evidence, require exact published
  Mac/Windows `main` parity; the fixed `openai.codex` / `gpt-5.6-sol` selection;
  pinned Codex `0.148.0`; owner-private auth and staged adapter assets; exact
  adapter/Codex/schema digests and source-HEAD-bound deployment manifest;
  owner/SYSTEM-only protected auth DACLs; the master-cleared four-variable adapter launch;
  the existing Windows Job Object gate; adapter-cleared Codex execution with
  `CODEX_HOME` plus only the OS-derived Windows `SystemRoot` and system-directory
  `PATH`, strict config, ephemeral/read-only mode, and every tool surface disabled;
  strict canonical packet/output bindings; and one fixed approval plus one
  fixed rejection. The controller accepts no path/provider/model/executable/
  schema/harness selector, separates committed bytes from the sanitized Windows
  receipt, rejects dirty/hidden/drifted state and malformed evidence, removes
  its private transcript, and publishes only its fixed owner-private receipt
  pair. Run the canonical local gate before live proof, then revalidate that the
  ignored proof directory remains ordinary, owner-owned, mode `0700`,
  non-symlink, and canonical, and that the receipt and digest remain owner-only,
  mode `0600`, single-link, regular, and digest-matched after later build/release activity;
  regenerate an absent or invalid pair through the exact controller and never
  reconstruct it from terminal output. Do not admit it automatically or treat
  it as general reviewer
  competence, actual gateway lifecycle, GitHub, activation, or release proof.
- Confirm the fixed GitHub-publication adapter remains unavailable before exact
  Windows provisioning and never accepts a caller repository, remote,
  executable, credential, command, check name, or adapter evidence. Verify the
  pinned `gh.exe`/`git.exe` identities, owner/SYSTEM-private GitHub CLI
  configuration, fixed `malak333/Assemblywright` repository, protected `main`,
  administrator enforcement, strict required checks, no force/delete policy,
  and exact internal-to-GitHub check mapping. Exercise push, exact PR upsert,
  required-check wait, reviewed-head check, normal non-admin merge, remote-main
  reconciliation, and post-merge `release-local` observation through the same
  adapter while polling deadline/cancellation/current authority. Missing,
  failed, stale, late, or ambiguous evidence after intent must quarantine
  without retry. Keep the route owner-loopback-only and prove authenticated
  remote mTLS still returns 404.
- Confirm GitHub-publication provisioning waits for the exact Windows service
  process to stop, byte-materializes Cargo output into a new owner/SYSTEM-only
  single-link service image, preserves its digest, and uses the same protected
  single-link boundary for rollback before restoring verified health. Verify
  the recovery copy's owner, exact two-principal ACL, link count, and digest
  before the stop.
- Run `windows-github-publication-live-control.ps1 -Action SelfTest` on Windows
  PowerShell 5.1 and require absent-global-state restoration, successful-stderr,
  nonzero-exit, and launch-failure handling to remain exact and fail closed. Run
  `github-publication-proof-controller.sh --check` and `--self-test` in the
  canonical gate, plus the focused `github_publication_adapter`, publication-
  filtered `feature_conveyor_kernel`, and `publication_coordinator_contract`
  suites. Treat these as the `unit-testing-test-generate` coverage for strict
  configuration, policy/provenance, cancellation/deadline, idempotence, and
  the bounded exact-service stop/start process-ID waits that protect executable
  replacement and rollback. Treat the live Windows `Provision` plus `Run` as
  the native E2E for those service-state boundaries. Together, these cover
  strict configuration, policy/provenance, cancellation/deadline, idempotence,
  restart/recovery, redaction, maximum-input, and malformed-state cases; do not
  claim a numeric percentage when `cargo llvm-cov` is unavailable. For live
  proof, require exact clean published source on both
  hosts, valid fixed Windows GitHub authentication, a quiescent protocol-5/
  schema-19 master, and one bounded metadata-only proof-marker candidate merged through protected
  `main`. Bind the exact starting source and resulting remote-main commit,
  hosted checks, protection/no-bypass state, pinned tool/service identities,
  and transcript digest into the owner-private receipt pair. The source
  checkout itself must remain unchanged during the proof. Do not admit the
  receipt automatically or treat it as a queued feature, general repository,
  signing, notarization, clean-profile, or production-readiness proof.
- Apply `e2e-testing` at the native boundary for Feature 4: controlled bare-Git
  remote, committed Mac controller/harness, fixed Windows service and
  credential-owning GitHub adapter, protected public pull request, hosted
  checks, normal merge, cleanup, and final reconciliation. Browser Playwright,
  visual-regression, and cross-browser lanes are not applicable because there
  is no browser product surface.
- Run `restart-recovery-proof-controller.sh --check` and `--self-test`, the
  exact real-agent retained-workspace restart E2E, and the exact master startup-
  quarantine, audit-rollback, publication-intent, and indeterminate-review
  focused tests. Treat dirty/wrong/stale/hidden source, hostile output paths,
  malformed/oversized Windows evidence, stale receipt invalidation, transcript
  deletion, and process-group cancellation as required negative coverage.
- For live Feature 5 proof, require exact clean published source on both hosts,
  explicit Windows `Run` confirmation, the fixed owner/image/service/data paths,
  Emergency Pause clear, and empty distributed plus Feature Conveyor state.
  Bind SCM's `MIKE-PC\mike`/`.\mike` local-account spellings through the same
  fixed SID, and require any `\\?\` executable/data namespace use to be
  consistent before service mutation.
  If Cargo left the installed image hard-linked into `target\release\deps` or
  with inherited checkout ACLs, perform an explicit stopped-service deployment
  that preserves its digest while creating an owner/SYSTEM-only single-link
  service-path file; verify health before `Check`. This prepares trust state but
  creates no proof receipt.
  Observe a stopped, full-database-SHA freeze; exact-HEAD offline rebuild with
  fixed absolute Cargo/Rust/MSVC tools; owner/SYSTEM-only ordinary single-link
  rebuild-to-Cargo-output digest equality; a healthy changed PID; a second
  stopped freeze with identical database SHA and logical/migration continuity;
  exact original-image restoration; and a distinct final protocol-5/schema-19
  healthy PID. Confirm the Mac Git/Cargo/rustc digests, Cargo-config rejection,
  and system-only PATH; confirm the Windows Cargo/rustc/MSVC, original/restored
  image, transient exact-source rebuild, and database digests. Do not infer
  reproducible builds or installed-image source provenance from this proof.
  Admit nothing automatically. Record this only as retained-agent functional
  recovery and idle authoritative-service continuity—not active-effect crash
  recovery, SCM retry-policy proof, signed-helper, Feature 6 streaming,
  activation, signing/notarization, installation, or production readiness.
- Apply `e2e-testing` at the native Feature 5 boundary: committed Bash
  controller/harness, real Rust agent process restart, isolated receipt FD,
  process-group cancellation, Windows SCM/PID, SQLite integrity, and strict
  PowerShell receipt. Browser Playwright, visual regression, and cross-browser
  coverage do not apply because there is no browser surface.
- Run `mac-windows-control-streaming-proof-controller.sh --check` and
  `--self-test`; require strict CLI, raw receipt digest, stale invalidation,
  dirty/wrong/stale/hidden Git rejection, malformed/extra/reordered/oversized
  output denial, endpoint/stream/path redaction, definition/binary drift denial,
  and process-group cancellation/descendant cleanup. For live Feature 6, require
  exact clean published source, stable fixed helper/agent signatures and bytes,
  enrolled exporter-bound mTLS, and the committed native `--run-relay` harness.
  Observe one same-stream durable cursor strictly advancing after a fresh signed
  helper and Rust-agent chain. Hash/delete the private transcript; retain only
  the owner-private path-free receipt/raw-digest pair. Admit nothing and do not
  activate. Record no protocol/schema/runtime-authority, built-binary source-
  linkage, Developer ID, notarization, installation, unattended, or production
  claim. Native Swift/process/mTLS/SQLite E2E applies; Playwright does not.
- Run `windows-owner-evidence-admission.ps1 -Action SelfTest` on Windows
  PowerShell 5.1 and require success plus wrong-digest, wrong-pair, malformed,
  oversize, duplicate-key, reordered-field, whitespace-rewritten,
  wrong-boundary, wrong-status, wrong-schema, and wrong-fixed-identity rejection.
  Prove Status/Check are owner-token-authenticated read-only loopback GETs and
  the exact GET/POST path is absent from the enrolled-device mTLS router. For
  each actual admission, retain the controller-produced raw pair unchanged,
  apply protected owner/SYSTEM-only ACLs, run Check, inspect the category and
  digest, then run Admit with explicit `-Confirm`. Require ordinary single-link
  bounded files, exact pair names/sidecar, strict JSON/category/origin/schema/
  identity/time validation, pause/current-revision preflight, one POST, and a
  matching digest-only receipt including the submitted evidence ID and observed
  time. Prove held no-write/no-delete-share handles bind and revalidate canonical
  paths, stable identities, ACLs, and raw bytes. Prove exact-digest retry is
  idempotent before pause/activation rejection and stale CAS, Emergency Pause,
  activation, unsafe ACL/link/reparse state, and
  interruption reject without automatic retry. Do not activate, self-approve,
  expose the token, or treat local/controller/hosted checks as external proof.
- Confirm protocol-v5/schema-v13 result-artifact admission uses only an exact
  immutable packet with sorted normalized relative paths and deterministic
  write/delete schemas; SHA-256 covers the complete packet and exact canonical
  multi-file artifact; the agent seals the workspace until exact resolution or
  bounded expiry; Swift strictly validates and uploads it through the existing FIFO
  cancellation race; the remote route is mTLS-only and exact-attempt bound;
  SQLite/audit retain no bytes or paths; immutable metadata and redacted audit
  commit together; exact retry is idempotent; missing/mismatched/stale/paused/
  cancelled/expired admission rejects result acceptance; startup removes
  unreferenced artifact directories but retains referenced ambiguity under
  active-feature quarantine, and schema v12 migration creates a verified
  `master.pre-v13.*` backup before adding retention evidence. Record Windows remote-mTLS and live-device proof
  separately from repository tests. Do not claim apply or integration.
- Confirm crash-prepared/concurrent exact retries recover; cleanup is guarded;
  referenced missing, corrupt, reparse/symlink, hardlinked, wrong-permission,
  or identity-drifted evidence blocks startup and terminal result acceptance
  without deleting referenced state. Record live Windows proof for file flush,
  same-volume rename, service-account ACL ownership, reparse/link rejection,
  and crash recovery; do not claim portable Windows directory flush.
- Confirm schema-v14 artifact integration remains owner-token loopback-only and
  absent from enrolled-device mTLS. Prove the companion plan projection is also
  owner-local, path-free, bounded, redacted, and returns only the exact current
  artifact IDs and authority bindings. Require one non-nil integration ID and the
  complete sorted terminal accepted artifact set, exact feature/specification/
  lifecycle/lease/snapshot/base-commit/grant/queue/pause bindings, independent
  stable-handle artifact re-hash and protocol-v5 semantic validation, and
  deterministic application order from immutable dispatch ordinal plus packet
  ID. Prove a private no-remote integration repository is derived only from the
  immutable snapshot and the registered source checkout remains byte-for-byte
  unchanged. Prove duplicate ordinal, overlap, create/replace/delete CAS drift,
  tree-shape conflict, artifact or authority drift, cancellation, pause, and
  concurrency leave no candidate and do not advance. Prove success flushes and
  seals one exact commit/tree, atomically stores immutable artifact linkage plus
  redacted audit, advances only `implementing -> validating`, and exact retry
  returns the original receipt. Prove startup deletes only unreferenced staging,
  validates referenced candidates, and quarantines ambiguity. Do not claim test,
  review, publication, credential/network, registered-source, or autonomous
  authority.
- Confirm schema-v15 test/evidence gating remains owner-token loopback-only and
  absent from enrolled-device mTLS. Prove the request accepts only the immutable
  approved manifest's exact ordered 13-command plan and canonical digest, with
  exact feature/specification/`validating` lifecycle/lease, snapshot,
  integration/artifact set, candidate commit/tree/base commit, queue/pause, and
  all three current grant bindings. Prove executable, argument, shell, path,
  result, raw output, unknown, missing, reordered, and caller-evidence fields
  reject. Prove immutable SQLite rows retain only bounded command identifiers,
  pass/duration/truncation metadata and nonzero result/manifest digests; all 13
  passes atomically append redacted audit and transition evidence and advance
  only `validating -> reviewing`; failure stays `validating`; malformed or
  incomplete evidence does not complete. Prove exact passed retry revalidates
  the frozen candidate and returns the original receipt, exact failed retry is
  the same failure, drift rejects, and interrupted active validation is startup-
  quarantined without automatic retry.
- Keep live validation provisioning blocked until the fixed Windows private
  toolchain and credential-free cache bundle is installed and verified. An
  unprovisioned runner must return `validation_runner_unavailable` before
  durable attempt/audit mutation. Before activation, separately prove the real
  Windows service identity, production executable/argv allowlist,
  worktree/toolchain/cache/scratch ACLs,
  no inherited credentials or handles, credential-store denial, cancellation
  and complete tree reaping, bounded evidence extraction, actual
  above/below-threshold llvm-cov behavior, and OS-wide outbound-egress denial.
  Loopback TCP/UDP nondelivery is not OS-wide egress proof.
- Confirm schema-v16 independent review remains owner-token loopback-only and
  absent from enrolled-device mTLS. Prove the caller cannot supply packet,
  transcript, memory, paths, raw evidence, or provider output; the master
  reconstructs and binds the exact approved specification, frozen candidate
  commit/diff, ordered evidence digests, provider/model, grants, lifecycle,
  queue, and Emergency Pause state. Prove strict deny-unknown-fields review-safe
  DTO and sensitive-context admission, patch polarity, legacy-row sensitive-
  context revalidation, exact ordered requirement
  coverage from the approved manifest's required top-level `acceptance` array, packet-only evidence
  references, default-unavailable production configuration before intent, one
  fresh cleared-environment bounded provider process; on Windows prove the fixed
  verified image-handle lock, gate-before-provider-spawn Job assignment, and
  complete descendant termination; prove strict approval and
  rejection; malformed/outage/incomplete transport without repair charge;
  fixed backoff, three candidate calls, twelve feature calls, cancellation and
  drift suppression, interruption and post-response-drift terminalization/quarantine, immutable
  idempotent decisions, rejection retention, and
  approval-only `reviewing -> publishing`. Keep live selected-provider quality,
  service deployment, GitHub publication, and owner-recorded proof separate.
- Confirm schema-v17 publication remains owner-token loopback-only and absent
  from enrolled-device mTLS. Prove strict path-free bindings, master-derived
  branch policy, immutable pre-effect intents, exact action order, idempotent
  completed retry, strict stage evidence for remote base/head, PR, complete
  checks, branch protection/no-bypass, merge strategy/result, and post-merge
  gate, plus bounded in-flight cancellation/deadline polling,
  pause/cancellation/drift/restart quarantine, remote-main
  equality, fixed post-merge gate, and atomic lease release plus queue advance.
  Prove default-unavailable production transport creates no intent. Keep live
  GitHub credentials, PR/check/merge APIs, branch protection, and reconciliation
  as separate owner-recorded evidence; a controlled bare remote is not proof of
  those hosted boundaries.
  Prove a durable merge intent prevents `merged:false` abandonment from a
  `publishing`-origin quarantine until healthy-main reconciliation is recorded.
- Confirm schema-v18 orchestration remains kernel-only and default-inert: no
  activation writer or owner/device route; immutable path-free checkpoints and
  same-transaction redacted audit; stale CAS/idempotence; initial candidate
  free and three-replacement ceiling; three/12 review budgets; 24 active hours
  with provider/worker/maintenance/owner pauses excluded; restart resume only
  for a complete effect-free pause; ambiguous effects quarantine; and
  substantive failure stops at `attention_required` when a safe replacement-
  candidate contract is unavailable. Confirm cancellation, failure, attention,
  quarantine, and abandonment retain the lease and never auto-advance.
- Confirm the owner-token loopback-only `cancel-active-feature` and
  `abandon-and-advance` routes are absent from enrolled-device mTLS. Prove
  strict duplicate/unknown/oversized-frame denial; exact feature, lifecycle,
  queue, and Emergency Pause compare-and-set checks inside the transaction;
  audit rollback; cancellation-dominant coding cleanup with the feature lease
  retained and no advancement; abandonment denial before cancellation or
  without safe-reconciliation/required healthy-main evidence; and successful
  lease release plus one queue-revision advance. Prove schema-v11 backup-first
  migration backfills a missing retained-lease origin receipt only from one
  exact immutable v10 audit event and restores v10 unchanged when evidence is
  missing, ambiguous, malformed, or names a non-active origin. The live two-device lane must
  finish with no queue entry, active feature lease, distributed lease/attempt,
  transfer staging, or Mac workspace. Direct SQLite mutation or deleting state
  as proof is forbidden.
- Confirm `docs/architecture-map.md` matches the code for any surface that
  changed in this cycle.
- Confirm the version is consistent:

```sh
./scripts/release-version-consistency.sh --check
```

## Code Gate

Run the canonical local gate and treat any failure as blocking:

```sh
./scripts/release-local.sh
```

Nothing in this gate signs, notarizes, staples, installs, or validates on a
live device. It proves the workspace builds, tests pass including ignored
release proofs, the crates package, the unsigned distribution layout is valid
and launches in an isolated HOME with Developer Mode default-off, the release
runbooks render, the evidence preflights and self-tests pass, and the Swift
package builds and tests.

## Safety Gate

Re-read `docs/safety-rules.md` and confirm the change preserves:

- Fail-closed policy. Ambiguous repository, provider, external-effect, review,
  or publication boundaries quarantine and block rather than guessing.
- Planning and action separation. Models propose; the owner authorizes.
- Sensitivity classification and redaction. Audit and event surfaces carry
  metadata and digests, never raw payloads or credentials.
- Explicit cancellation, which dominates completion and suppresses late output.
- Emergency pause, which blocks new leases and publication.
- Durable audit evidence committed in the same transaction as the state
  transition it describes.
- Result acceptance bound to the exact leased attempt.

Confirm no new surface grants a worker or model repository-write, credential,
network, or publication authority it did not previously hold.

## Documentation Gate

```sh
./scripts/release-docs-drift-smoke.sh
```

Update in the same change as the code:

- `README.md` — what is implemented and what is explicitly not claimed.
- `DESIGN.md` — system-level design and non-goals.
- `docs/architecture-map.md` — current implementation and evidence boundary.
- `docs/build-test-commands.md` — canonical commands and proof boundaries.
- `docs/knowledge-base/assemblywright-project-facts.md` — durable facts.
- This checklist, when the release flow itself changes.

For every feature or phase, also complete the closeout contract in
`docs/development-agent-workflow.md`: conversation-derived knowledge review,
focused unit coverage, real-boundary E2E, explicit browser/Playwright
applicability, requirements and safety review, and exact publication evidence.

## Distribution

Build and validate the unsigned layout:

```sh
./scripts/package-distribution.sh --check
```

```sh
./scripts/package-distribution.sh --unsigned-launch-check
```

Then produce the signed artifacts. This step requires Developer ID credentials
and is not reproducible in CI:

```sh
cargo run -p assemblywright-cli -- release signed-distribution-runbook
```

```sh
ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' \
ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' \
ASSEMBLYWRIGHT_NOTARYTOOL_PROFILE='...' \
./scripts/package-distribution.sh
```

## Owner-Recorded External Evidence

These lanes cannot be proven by the repository. Each writes a JSON report that
`release evidence-status` validates structurally.

**Live-device QA.** On a clean release Mac, install from the signed installer
into `/Applications`, launch through Finder, exercise the installed app, and
restart it. Then:

```sh
./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env
```

```sh
set -a && source target/release-live-device-qa.env && set +a
./scripts/release-live-device-qa.sh --assert-complete
```

The report binds the installed app executable's SHA-256, code identifier,
TeamIdentifier, and CDHash to the exact signed provenance report. Owner
evidence notes must contain real observations, not placeholders.

**Final evidence bundle.** Only after signed distribution and live-device QA
reports exist:

```sh
./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env
```

```sh
set -a && source target/release-evidence-bundle.env && set +a
./scripts/release-evidence-bundle.sh --bundle
```

```sh
./scripts/release-evidence-doctor.sh --assert-complete
```

**External handoff.** To generate the operator packet:

```sh
./scripts/release-external-handoff.sh --write target/release-external-handoff
```

## Readiness Confirmation

```sh
ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release readiness
```

`production_ready` stays false until signed distribution, notarization and
stapling, and the final evidence bundle checks all validate. Set the external
evidence mode only after owner-recorded evidence has actually been collected.

## Release Notes

State the exact surface, the exact evidence, and the exact remaining gaps. Do
not carry forward claims from a previous cycle without re-verifying them.
