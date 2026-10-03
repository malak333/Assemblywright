# Production Build

The owner selected the existing supervised Developer workflow as the production
Assemblywright product. The default Mac app now presents its Windows queue,
planning, project chat, repair, review, and connected-repository publication UI.
Windows remains the durable authority. This decision promotes the usable workflow;
it does not activate the separately protected service's unavailable effect adapters.

## Build and launch

Configure the existing app-owned Windows connection and reviewer as described in
[the workflow guide](developer-build.md), then run:

```sh
./scripts/production-build.py --build
```

This compiles the Mac app and Windows `assemblywright-developer` runner in release
mode and creates `target/production/Assemblywright.app` with the canonical
`com.nobiletechnology.assemblywright` bundle identity. Later launches use
`./scripts/production-build.py`, `Open Assemblywright.command`, or that app directly.
The launcher manages the existing connection; opening an unconfigured app alone
shows the connection state and does not provision SSH, start a model, or start a
Windows runner. A configured production bundle retains the existing supervisor
kickstart behavior. After `--stop` unloads the supervisor, use the production
launcher to reinstall/load it before opening the app directly. The compatibility `developer-build.py` command keeps its debug
profile and `Assemblywright Developer.app` identity.

The canonical signed distribution command remains
`./scripts/package-distribution.sh`. Its release-built `Assemblywright.app` opens
the same supervised workflow. That command packages the Mac app and release CLI;
it does not provision the Windows runner or the app-owned SSH connection. Provision
those with the production launcher before live use. Full distribution still
requires the existing Developer ID identities and notarization credentials.
The production launcher provides an ad-hoc signed local build, not a notarized
installer. Product selection, optimized compilation, and distribution credentials
are separate facts.

## State and compatibility

The retained Mac state is `~/Library/Application Support/Assemblywright/Developer`,
including `runtime.json`, `connection.json`, the app-owned SSH identity, and the
existing connection supervisor. The Windows runner executable name, configured
remote root, queue database, project files, histories, provider choices, and
publication bindings stay unchanged. No service database is imported, no new queue
is created, and no authority is inferred from a bundle rename. Do not run the debug
and production launchers concurrently against the shared connection. Existing
maintenance guards reject rebuilds while work is active.

`ASSEMBLYWRIGHT_DEVELOPER_CONFIG` remains an explicit compatibility override for
the supervised connection file. `ASSEMBLYWRIGHT_RUNTIME=protected-service` selects
the retained protected-service shell for its separate operator and evidence flows.
For a distribution intentionally dedicated to that mode,
`ASSEMBLYWRIGHT_APP_RUNTIME=protected-service ./scripts/package-distribution.sh`
binds it into signed bundle metadata; the default package profile is `developer`.
The isolated-HOME launch smoke always tests the default supervised profile.
That opt-in does not activate dispatch, migrate state, or relax enrollment,
identity, policy, cancellation, Emergency Pause, or audit rules. An explicit
supervised configuration continues to select the supervised workflow.

## Safety and evidence

This product intentionally uses the existing owner-account execution contract.
Planning approval, one active feature, bounded tool permissions, immutable
validation commands, fresh independent review, exact publication checks, and
ambiguity quarantine continue to apply. Process lifetime controls under the owner
account do not establish hostile-process containment. Models cannot select a
publication destination, bypass review, or approve a merge.

[Build commands](build-test-commands.md) and
[workflow coverage](developer-build-testing.md) define the native Rust, Swift,
HTTP, filesystem, and process gates. The production-build GitHub workflow additionally
compiles the Windows supervised runner in release mode without altering the pinned
protected-service workflows or their required-check identities. The distribution launch check uses an isolated
HOME plus Foundation’s fixed-user home (`CFFIXED_USER_HOME`) to prove the new
default starts without provisioning a connection, spawning a
helper, or exposing a listener. Fixtures do not prove live Windows deployment,
real-model quality, Developer ID signing, notarization, clean-profile Finder launch,
or live account publication. Those remain separately recorded release evidence.

## Upgrade and recovery

Before rebuilding, stop active feature/chat/planning/publication work through the
app and wait for the existing maintenance guards to admit shutdown. Retain the
Windows queue and project backups and the Mac connection directory. Rebuild through
the production launcher, reconnect, and inspect queue/checkpoint, Emergency Pause,
provider, and repository bindings before starting work. Ambiguous effects require
existing exact recovery; do not replay them or create a replacement queue.

A rebuild first obtains fresh authenticated idle status. An unavailable status
means no shutdown request was sent and no process was stopped; reconnect before
retrying. After the shutdown POST is sent, the launcher allows a bounded fifteen
seconds for a parsed JSON-object acknowledgement. A timeout, HTTP error, malformed
JSON, or non-object response leaves the shutdown outcome unknown and retains the
connection supervisor. Inspect and reconnect before another maintenance attempt;
an acknowledgement error does not prove that the runner ignored the request.
The launcher does not force-stop the supervisor or continue to replacement after
an unverified acknowledgement.

For rollback, use the compatibility Developer launcher against the same retained
state, subject to the existing schema compatibility and maintenance guards. The
protected Windows service and its database are independently provisioned and are
not replaced by either launcher. A signed installer rollout requires separate
platform installation and live-device verification; a GitHub merge alone is not a
deployment receipt.
