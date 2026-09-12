# Developer GitHub Setup

The owner reported failure connecting `malak333/inches-feet-demo` and requested
one-time GitHub setup, repository discovery, and repository creation from the app.
Native diagnosis found an invalid saved Windows GitHub login and HTTP 401 from
`gh api user`. The previous publication dialog exposed only a generic error.
Read-only verification through the independently authenticated Mac account found
the requested repository already exists and is public, but is empty: no default
branch, no `main` branch, and no effective required-check rules. Renewing Windows
authentication is necessary but does not establish repository readiness.

## Behavior

Windows owns GitHub authentication and repository operations. The Mac displays
account status, opens GitHub's fixed device-authorization page, shows the one-time
code, and observes completion. The owner completes browser login/authorization;
passwords and access tokens never enter Assemblywright's UI, model prompts, or
queue. GitHub CLI retains credentials using its existing OS credential storage.
Sign-in is bounded, cancellable, and serialized with other Developer work.

The dialog lists paginated repositories accessible to the authenticated account,
including canonical URL, visibility, default branch, and write permission. Selecting
one fills the existing connection form. Manual URLs remain supported. An expired
login, network error, missing repository permission, absent base branch, missing Git
identity, and incompatible required-check policy receive distinct actionable text.
An empty repository selection links to that repository's GitHub setup page and
explains that it needs an initial README/base branch and protected required checks;
it must not be presented as ready or silently supplied a fictional default branch.

Create repository is a separate explicit action: the owner chooses a valid name
and public/private visibility (private initially selected) and confirms the exact
account/name/visibility before any request. Initial creation is limited to the
authenticated user's account. It initializes a README/base branch and never uploads
project files. Creation is distinct from connecting or publishing a feature.
The UI reports successful creation even if subsequent publication prerequisites
still need setup; it does not weaken the existing strict app-bound checks policy.

Creation has a durable operation ID and frozen expected account/target/visibility before the external
request. A collision cannot silently adopt an existing repository. Uncertain results
remain visible and reconcile by observation; retry cannot blindly create again or
claim unproved creation. A discovered existing repository may be selected through
the normal explicit connection flow. No automatic deletion or policy change occurs.
Creation rechecks the live account against the confirmed `expected_login` immediately
before its request. Verified receipts bind the immutable GitHub repository ID,
canonical owner/name, and visibility; a later account switch cannot redirect it.
The wire receipt represents the immutable numeric repository ID as a decimal
string, so native clients consume the same identity without numeric conversion.

Every sign-in exit (including cancellation) performs live account verification.
Cancellation stops further work, but cannot undo credentials already saved by gh:
the UI reports the observed account truthfully. Lost/ambiguous completion or restart
retains an operation-bound attention record and blocks dependent work until explicit
`reconcile_sign_in` verifies the account. Only public operation metadata persists;
device codes and tokens do not. Refresh cannot silently clear an ambiguous sign-in.
GitHub CLI's effective credential precedence remains authoritative for repository
operations. Sign-in compares the saved login with that effective account; an
environment credential masking the saved login produces an actionable attention
result and displays the effective account instead of claiming the new login is in use.

## Implementation contract

Add authenticated `GET /github` for cached setup state and strict `POST /github`
actions. Keep the existing `/publication` connection and feature protocol intact.
Every POST carries `expected_revision`; operations bind a unique `operation_id`
where they can be long-running or create external state. Account/list requests are
bounded. Setup writes reserve the existing idle/connection-operation exclusion so
feature starts, publication, tools, and planning cannot race credential changes.
Stop, Emergency Pause, shutdown, and cancellation retain their authority.

Response fields: `revision`, `account` (`state`, `login`, `message`),
`repositories` (`name_with_owner`, `url`, `visibility`, `default_branch`, `can_push`),
`repository_page`, `has_more`, `sign_in` (optional `operation_id`, `user_code`,
`verification_url`, `state`, `message`), `creation` (optional `operation_id`,
`repository_url`, `repository_id`, optional `default_branch`, `name_with_owner`, `visibility`, `state`, `message`), `busy`,
and `can_mutate`.

Actions: `refresh_account`; `list_repositories` with `page`; `begin_sign_in` with
`operation_id`; `cancel_sign_in` and `reconcile_sign_in` with `operation_id`;
`create_repository` with `operation_id`, `expected_login`, `name`, `visibility`;
`reconcile_creation` with `operation_id`.
All carry `expected_revision`. Account state is `unknown`, `signed_out`,
`signed_in`, or `unavailable`. Sign-in states are `starting`, `waiting`, `succeeded`,
`cancelled`, `failed`, or `attention`; creation states are `creating`, `succeeded`, `attention`,
`existing`, or `absent`. Invalid/missing fields cannot authorize controls. Cached identity
is not authorization: verify the account again at every repository mutation.

Operation IDs are client-generated UUIDs echoed unchanged. POST responses advance
the authoritative revision even for read/refresh actions; stale revisions fail.
Begin-sign-in acknowledges the same operation in starting/waiting/terminal state;
creation acknowledges the same operation and exact confirmed account/name/visibility
in creating/succeeded/attention state. Cancellation may observe succeeded if gh
already saved valid credentials, with no automatic follow-on work. Reconciliation
is allowed only for its retained operation; creation inspection yields
succeeded/existing/absent/attention and never invokes creation again. Observed absence
is terminal for that operation and permits a new UUID while retaining prior evidence.
Repository pages are one-based, contain only the requested page, echo
`repository_page`, and expose `has_more`; the UI resets on account changes and
appends only an acknowledged next page. Before discovery and after account resets,
`repository_page` is zero, with an empty list and `has_more: false`; this is not a
loaded page acknowledgement. `verification_url` is exactly
`https://github.com/login/device`, and device codes match `[A-Z0-9]{4}-[A-Z0-9]{4}`.
Use short polling timeouts and a separately bounded mutation timeout suitable for
existing connection validation (up to 180 seconds).

Subprocesses use the existing bounded publication process containment. Only parsed,
validated device challenge fields may cross to the Mac; raw provider output is not
an error message. The one-time challenge is transient; no access token or raw auth
output is persisted. Durable creation records contain only operation/effect evidence.
Connection failures remain inside the GitHub dialog and explain the next step.

## Validation and review

Use the unit-testing-test-generate and native e2e-testing workflows: invalid/expired
auth, network errors, malformed identities/challenges, unauthorized/stale/busy
requests, repository pagination, creation confirmation/collision/uncertainty,
cancellation/restart, retained publication safeguards, and Swift selection/ack logic.
Run native Windows processes and Mac Swift tests, independent high-risk review,
documentation checks, and the canonical gate. Verify installed account/setup state
and unchanged owner queue separately from fixture tests. Real creation requires an
explicit exact target/visibility action; no test repository is created implicitly.

References: [GitHub CLI sign-in](https://cli.github.com/manual/gh_auth_login),
[repository creation](https://cli.github.com/manual/gh_repo_create), and
[device authorization](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps).

## Validation record

This record belongs to the original September 8 implementation and repository setup
follow-up. It guides exact recovery but is not current validation of the restored
source or current external GitHub state.

The owner-reported connection failure was reproduced with the installed Windows
CLI: its active saved credential was invalid and account lookup returned HTTP 401.
The independently authenticated Mac account could read the existing target, which
was public and empty. No repository was created, initialized, connected, or
published during this diagnosis. A temporary native Windows device-login challenge
was issued, but expired without browser authorization; this does not prove a
repaired live login.

Focused Swift setup tests passed 8/8, including initial/reset page zero, exact
operation acknowledgements, identity/visibility consistency, repository pagination,
account truth after sign-in, and empty existing repositories. Existing GitHub and runner tests passed 15/15;
the optional live runner test was skipped. Installer tests passed 21/21, including
refusal to interrupt active setup/publication. Final Developer runner unit tests
passed 107/107 on Mac and 109/109 on Windows (one Windows child fixture ignored in
the ordinary suite). Native setup E2E passed on both platforms, including process
loss during creation, observation-only recovery, exact one-create evidence,
expired/masked credentials, empty branch discovery, and redirected identity
rejection. Native Windows publication regression also passed with real bare Git
transport, required checks, cancellation, reconciliation, and verified merge.

Independent high-risk review approved backend authentication/creation, atomic
admission and persistence, Swift decoding, native E2E, and installer lifecycle
guards with no remaining findings. Five final backend/fixture/script source hashes
were verified equal on Mac and Windows. Logs and installed evidence are under
`target/developer-github-onboarding/`; `windows-final.log` records the final native
passes and supersedes the earlier fixture-routing regression failure.

The final September 8 Swift run enabled the installed connection and passed all 24
focused setup/publication/runner tests with no skips. The canonical local gate was
not green: its separate Mac relay suite failed
`authenticated_uds_mlx_success_and_cancellation_are_separate_and_bounded` while
reading the frame prefix with `UnexpectedEof` (9 passed, 1 failed). The GitHub-focused
tests and native E2Es passed separately; none of this retained evidence substitutes
for validating the restored source at its new commit.

The original updated Windows runner and Mac Developer bundle were installed while
idle. Before/after comparison preserved six queue entries, their evidence hashes,
connection state, auto-run, and emergency-pause settings. The installed Windows
binary matched the tested build; the Mac bundle passed strict ad-hoc signature and
executable UUID checks. This was Developer installation evidence, not production
signing, notarization, or production readiness.

## Owner-authorized repository setup follow-up

The owner subsequently requested initialization and required checks for the empty
`malak333/inches-feet-demo` repository. The initial README established `main` through
a normal non-force push; CI was added through
[PR #1](https://github.com/malak333/inches-feet-demo/pull/1) on
`codex/repository-setup`. Six validator tests passed locally; the Windows hosted check
passed on the exact PR head and again on merged main. The merged commit was
`9457efcf10b2a217284fffe3eea7fa2824d44743`; its parents and complete tree matched
the frozen base and reviewed setup commit.

Protected main required app-bound `Windows Python validation` (GitHub Actions app ID
15368), up-to-date branches, and pull requests. Administrator enforcement was enabled,
with no bypass allowance, force pushes, or deletion. Human review count was zero so
the Developer runner's independent Codex review could be followed by automatic merge
after hosted checks. Repository auto-merge was enabled. The authenticated Windows
connection save preserved the six queue entries and their evidence.

This follow-up published README and CI scaffolding only. Existing application files
were not uploaded or retroactively declared reviewed. Operator evidence remains under
`target/inches-feet-repository-setup/evidence.json`. These are retained September 8
facts, not current proof of the restored implementation or current GitHub policy.

Closeout for the original slice updated documentation and safety contracts together
and added durable recovery facts. Its unit and native E2E workflows covered Rust,
Swift, HTTP, process, persistence, and Git boundaries. No browser application was
added, so Playwright and cross-browser testing were not applicable.
