# Developer pytest cache recovery

## Cause and correction

The live `aw-fft-demo` feature retained four `.pytest_cache` files in its cumulative
candidate after project-chat validation. Exact-current-snapshot recovery tried to
admit those hidden paths as source, while the reserved-path gate correctly rejected
them. Repair context construction also reached ordinary cache descendants before
recovery filtering, producing `Generated file targets reserved directory`.
Repeated manual proposals did not repair this admission mismatch; an unchanged
manual candidate still correctly fails with `Repair proposal contains no file changes`.

Exact `.pytest_cache` directories now enter repair context as one generated-tree
aggregate. Cache descendants stay out of source retrieval, editable recovery, and
independent source review. Case and separator normalization do not admit lookalike
names, sensitive paths, or other hidden directories. Files are preserved on disk.

Exact-current recovery now captures complete private project effects before and
after candidate construction, rejects an incoherent scan, and binds the stable
private hash into the displayed Resume digest. Equal-length changes in a redacted
cache file invalidate stale Resume. Legacy recovery, validation-only adoption,
original validation, independent review, cancellation, Emergency Pause, counters,
and retained history keep their existing authority boundaries.

## Independent review and tests

Independent high-risk review approved the three production changes and the native
regressions with no P0-P3 findings. Focused tests cover cache context aggregation,
projection, historical/current/prior evidence, private effect drift, sensitive and
reserved path refusal, stale digest rejection, refreshed adoption, and preserved
history. The `unit-testing-test-generate` and `e2e-testing` skills were not available;
Rust units and native runner/HTTP/SQLite/filesystem/process tests cover the real
boundary. Browser tests are separate website acceptance evidence.

The native harness also accounts for the installed runner's pre-existing bounded
reviewer-outage retry. It terminates the distinct second fixture provider before
asserting exhaustion, preserving the original exact unavailable state, candidate
identity, cache/file immutability, and no-model-replay checks.

## Evidence status

The initial cache correction passed 279 installed-source Windows unit tests (one
existing helper ignored), a native Windows build, and the complete disposable
recovery harness. The full local release gate passed Rust workspace and ignored
tests, Swift tests/build, documentation contracts, packaging checks, and release
evidence self-tests before the follow-up changes described below. Final follow-up
validation used focused tests, Clippy with warnings denied, format and documentation
checks, installed-source Windows units, and native process scenarios.

Live inspection identified a separate repeat-adoption gap: project-chat changes
after a prior validation-only adoption leave the feature inactive, while recovery
previously accepted only quarantined features. Native provider-outage testing also
identified rejection of the runner's own pending-review phase. Recovery now accepts
only the exact prior validation-only adoption marker with terminal effect-free
receipts, and verifies its own persisted pending packet, policy, private effects,
and unchanged predecessor review history before and after provider execution.

The follow-up source and complete installed-source patch received independent
high-risk approval. Windows units passed with 281 passed and one existing helper
ignored. The native outage-exhaustion scenario exited successfully in 152.72 seconds;
the distinct provider retry was terminated and exhaustion preserved candidate,
file, counter, and audit evidence without model or write replay. The native
success-after-one-provider-exit scenario exited successfully in 116.34 seconds;
the distinct retry reached approval with unchanged source/cache contents and
validation/model counters. Its legitimate final success receipt and hash-only
candidate compaction preserved all staged digest fields. An inert historical
`not_run` audit slot is accepted alongside provider numbering only with the exact
three-stage terminal effect-free escalation lineage; incomplete, duplicate, or
malformed lineage still fails closed.

The tested Windows runner is installed. Its SHA-256 is
`556f29ba4b9a57a6fda30482bb5a949618c5d7c84c93b8a34fcaa9f664d86834`.
Installation preserved the newer installed source and backed up prior binaries and
the database. The installer timed out waiting for status, then a separate check
confirmed authenticated status returned with the expected binary and source hashes.
The complete `aw-fft-demo` feature record remained identical across both installs,
with canonical SHA-256
`63d169885ad3fd5dfa7346a14faab85c389976b25d4092fd7d40558c0dffe99b`.

## Remaining live boundaries

Before restoration, live admission correctly refused a missing generated file:
`site/mechanics/job-system/index.html`. A stale-digest Resume request returned HTTP
409 with that exact missing-file reason and performed no action. The existing
site generator can rebuild published pages, but there is no direct audited owner
build endpoint: project-tool execution starts a configured model-backed session.
After a complete SHA-indexed project backup, the existing generator and all 32
original tests passed in a disposable copy. Its full generated-tree delta covered
19 site files. Those generated files were restored as an explicit owner correction;
the complete generated tree matched the disposable build and every non-generated
live file, including original tests and environment caches, stayed unchanged. The
next authenticated status exposed a fresh valid recovery digest. No Resume action
has yet been executed; no policy or missing-file guard was bypassed.

The website retains 16 blocking independent review findings, including incomplete
PS1 content, incorrect job data, stale generated output, validation/accessibility
gaps, and a placeholder canonical host. Its existing 32 passing tests do not
establish feature completion. The owner subsequently selected the existing Windows local model for this task's
implementation, retaining ChatGPT/Codex planning and independent review. An audited
project-tool session runs in a separate disposable project copy; its output remains
an untrusted proposal until complete-diff review and independent validation. No
retired Codex worker or global provider integration was recreated.

Live recovery, website acceptance, and a native Computer walkthrough remain
separate proof boundaries. The Mac is now unlocked and Computer can inspect System Settings, but selecting
either Assemblywright bundle fails with a native-pipe error. Diagnostic reports
confirm repeated SkyComputerUseService assertion traps in Array.remove(at:)
during inspection; restarting the tool session did not resolve them. No application
walkthrough or website browser acceptance has been verified. The feature is not
complete. The owner requested GitHub push and merge for the completed fixes;
publication evidence will be recorded separately.

## Connection readiness follow-up

Raw authenticated runner status was valid while the launcher's three-second probe
timed out. The measured latency exceeded that bound, causing the supervisor to
report needs_attention and the production launcher to exhaust its readiness wait.
Authenticated status now has a bounded ten-second timeout; the existing launch
readiness window remains thirty seconds. Endpoint, token, model, provider, workspace and idle-work
predicates remain unchanged. Twelve connection tests and twenty-five launcher tests
passed, including a delayed valid response and invalid-provider rejection.

The publication harness preserves the newer origin/main private staging and
post-apply validation drift assertions. Its full Windows execution passed in
185.36 seconds against the unchanged installed binary. The isolated publication
source is byte-identical to the tested installed recovery source; focused tests,
Clippy, format and documentation contracts passed on that integration.
