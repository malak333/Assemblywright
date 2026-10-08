# Developer project picker and publication recovery validation

## Reported failure and source correction

On 2026-10-07, GitHub discovery displayed `malak333/aw-fft-demo3` and
`malak333/aw-fft-demo4`, while the feature project picker omitted both. Neither
project had a directory under the installed Windows Developer workspace. Remote
repository selection only filled the publication connection form; it did not
create a local workspace. The project list came from real Windows directories.

The source correction adds an explicit empty-local-project action and projects the
same bounded directory list into all Developer project pickers. Selecting a remote
repository suggests a local name, and creating it preserves the repository/branch
draft for the subsequent explicit connection save. Directory creation does not
clone a repository or upload files. Existing directories are preserved, and file,
link/reparse, stale-revision, active-work, pause and cancellation cases are rejected.

## Installed diagnosis and demo setup

The authenticated installed runner's `/chat/projects` initially omitted both
names. Direct Windows observation confirmed that neither directory existed.
The owner-authorized repair created these two empty directories without changing
other workspaces, and authenticated observation then listed both. Both projects
were explicitly connected to their own GitHub repositories on `main`.

Each repository initially contained only `README.md` and had no required check.
The following prerequisite PRs added a Windows `Validate` workflow and were merged
normally before the feature ran:

- [demo3 setup PR #1](https://github.com/malak333/aw-fft-demo3/pull/1).
- [demo4 setup PR #1](https://github.com/malak333/aw-fft-demo4/pull/1).

Both base branches require the strict `Validate` check from GitHub Actions app
`15368`; administrators are subject to the policy. No checks or merge protections
were bypassed.

## First full run exposed a Windows checkout mismatch

Feature `739d5f8a-a753-41a8-8fd5-850ddab1cbe4` completed real planning, Windows
implementation, exact-byte validation and independent Codex review for one inert
FFT page. It generated exactly `index.html`: 833 UTF-8 bytes with LF line endings,
SHA-256 `decf9751aa786629cc557cb1856f9a012b0d1ceb2fe9ae3bd8cd20a077d202e2`.
There were zero repair attempts and zero escalations.

The app published [demo3 PR #2](https://github.com/malak333/aw-fft-demo3/pull/2)
at head `f766edba4af469b18d8fee6fbd0ec02326ddbac0`. The GitHub blob matched the
validated bytes, but the Windows Actions checkout converted LF to CRLF and the
immutable digest check failed. The app truthfully retained `publication_attention`;
this attempt is not a successful publication proof. The owner-authorized repair
closed that PR unmerged and preserved its candidate, approval and check history.

The checkout correction added `* text=auto eol=lf` through normally merged
[demo3 PR #3](https://github.com/malak333/aw-fft-demo3/pull/3) and
[demo4 PR #2](https://github.com/malak333/aw-fft-demo4/pull/2). A disposable real
Windows Git checkout with `core.autocrlf=true` then reproduced the original
833-byte LF artifact and exact digest. This correction fixes validation inputs;
it does not change the expected digest or convert a failed check into approval.

## Reviewed installation and retained failure recovery

Independent high-risk review approved the complete implementation and its clean
integration with the already installed model-diagnostics and delivered-behavior
review fix from Assemblywright PR #438. The supported production launcher rebuilt
the Mac release app and Windows release runner from the combined source. All 146
current Cargo/toolchain/crate input hashes matched that checkout. The installed
Windows executable matched its release build at SHA-256
`3df572f6f4d8a14814c594f896bb3a7c6e2c51e9f25788903cde318c2ecb56ce`.
The installed Mac executable matched its packaged build at SHA-256
`aa343375ddbf15fda6a107d808767821e9a730790b737b76820f37b5560ebfce`.
The previous binary, consistent SQLite backup, and Mac app bundle were retained.
The separate protected-service app was not replaced.

Authenticated observation included both demo3 and demo4 in `local_projects`.
The new `abandon` action observed demo3 PR #2 closed and unmerged, advanced revision
1695 to 1696, durably recorded `removed / publication_abandoned` and
`abandoned / abandoned`, and cleared the unresolved-publication barrier. A read-only
SQLite observation confirmed that the exact prior commit, PR, approved review,
zero repair/escalation counters, and frozen candidate remained retained. This is
recovery evidence, not successful publication evidence.

Windows focused unit tests passed for local project creation/rejection, abandoned
persistence invariants, and exact closed-unmerged PR binding. Both native Windows
setup/publication E2Es passed using disposable native reviewer/GitHub fixtures,
including mandatory junction rejection and abandonment/restart/history coverage.
Those fixtures used no live GitHub credentials. The E2E harness explicitly built
`developer_review_fixture` and configured
`ASSEMBLYWRIGHT_DEVELOPER_REVIEW_FIXTURE` for Windows.

Visual GUI inspection remains unverified: the computer-use native pipe failed
before returning a snapshot. Packaged-process, Swift, API, Windows and publication
proofs are recorded separately; none establish signing, notarization or general
production readiness.

## Successful installed autonomous pipeline proof

Feature `efd43e66-fd89-44d4-8940-1392a6e69e66` in `aw-fft-demo4` completed real
Codex planning; the parent inspected and approved the exact single-page plan and
started execution. From that start onward the app performed Windows model
implementation, immutable validation, independent Codex review, branch/PR
publication, required hosted checks, normal merge and remote-base verification
without parent candidate edits or publication intervention.

The terminal state was `succeeded / publication_merged`, review `approved`,
publication `succeeded / complete`, with zero repairs and zero escalations. Only
`index.html` changed. [Demo4 PR #3](https://github.com/malak333/aw-fft-demo4/pull/3)
merged at `2026-10-07T23:38:07Z`; its exact reviewed head was
`effc2cab3abd12f63f1cdaec8e6d7418ba8de5cb`, and its merge/main SHA was
`39065f5e0f51d390c36d50f0f9428535f6168dfc`. The required Windows `Validate`
check passed on that head before merge. Separate GitHub and Windows observations
both returned the same 833-byte LF artifact and expected SHA-256
`decf9751aa786629cc557cb1856f9a012b0d1ceb2fe9ae3bd8cd20a077d202e2`.
The runner was idle afterward, with no unresolved publication barrier.

This proves the configured pipeline completed this bounded feature. It does not
establish that arbitrary future model tasks cannot fail. The picker repair,
retained-failure recovery and line-ending setup are independently described above.

## Closeout verdicts and proof boundaries

- Documentation and safety: design, safety rules, setup/publication contracts and
  canonical test commands were updated with the implementation. Independent
  high-risk review approved the complete diff and integration. Authentication,
  exact candidate bindings, required checks, cancellation and emergency pause
  remain enforced; no security bypass was needed for the working proof.
- Durable knowledge: the knowledge base now records local-project discovery,
  exact closed-unmerged abandonment, Windows checkout line endings and the
  successful bounded installed publication proof.
- Unit workflow: `unit-testing-test-generate` was applied to the Rust creation,
  parser, exact PR binding and persisted tombstone invariants, plus ten focused
  Swift GitHub snapshot/selection tests. Windows-specific creation and recovery
  unit tests passed separately.
- E2E workflow: `e2e-testing` was applied using native Rust/Swift, process, HTTP,
  Windows filesystem and real bare-Git boundaries. Setup/publication fixture E2Es
  passed on Mac and Windows; the installed demo4 run separately used real Codex,
  the owner-selected Windows model and live GitHub. No browser matrix was needed
  for this native-app change.
- Deployment: the supported production build, exact input/executable hash
  checks, retained backups, healthy connection, project list, preserved recovery
  record and idle/unpaused terminal state were verified. Visual GUI, signing,
  notarization and broad production readiness remain unverified.
- Source publication: the owner requested a normal feature-branch PR and merge.
  The frozen final source head must pass the hosted Release local gate,
  Production Windows runner and Protocol/master/identity/mTLS/SCM checks before
  merge. The authoritative Windows Git checkout must then fast-forward to the
  published main SHA; docs-only closeout edits do not require a different binary.

The local native relay requires a real executable at the repository's expected
`target/debug` path. An external Cargo target first left that path absent; a
symlink alias then failed the native code-identity path binding. A regular-file
copy of the freshly built agent from this same checkout passed the focused native
relay E2E, including transfer, cancellation and cleanup. Packaging also requires the default Cargo package path. The final full gate
uses the default Cargo target directory populated only with this checkout's own
freshly built artifacts; earlier external-target attempts are not accepted as
full-gate passes. No signature or identity check was weakened.

Canonical validation verdict: PASS. `./scripts/release-local.sh` completed at
`2026-10-08T00:11:08Z` in this isolated checkout with the default Cargo target
layout. It passed formatting, Clippy, workspace/ignored tests, native workflow and
relay recovery, build/connection tests, package/distribution checks, evidence
contracts, partitioned native AppKit/bridge/remaining Swift suites and Swift build.
The gate's final marker was `Assemblywright local release verification: ok`.
The final documentation drift check and `git diff --check` passed separately.
