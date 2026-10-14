# Windows-only CI scope

For an owner-scoped Windows-only change, add the `windows-only` label to its
pull request before pushing updates. Preserve the exact `[windows-only]`
marker in the merge or direct-main commit message. The PR title can carry the
marker so GitHub's normal merge message retains it.

```powershell
gh pr edit NUMBER --add-label windows-only
gh pr merge NUMBER --merge --subject "[windows-only] Describe the Windows change"
```

Merge only after the exact candidate passes production Windows, Windows-local
GUI/native and Windows distributed checks, and the scope selector succeeds.
The existing Mac runner job is skipped for the labeled PR and marked main
push; the selector itself runs on Windows. This is a CI scope label and commit
marker, not a Git release tag or a command to skip all CI.

Other PRs and main commits keep full CI. Removing `windows-only` restores the
Mac gate. Explicit `workflow_dispatch` always selects the full gate. Missing,
malformed, unknown and non-main event data also select full validation. A
literal `[windows-only]` marker is case-sensitive; the PR label follows
GitHub's case-insensitive label comparison.

Unrelated label events select `ignore`. They use a separate concurrency group
and an alternate check name, preserving an existing full gate and its evidence.
Relevant events share the release gate's concurrency group so applying the
Windows-only label can cancel an in-progress Mac job. Changing a scope does
not cancel any of the three Windows validation workflows.

The selector and workflow use the job conditions and concurrency behavior
documented in [GitHub's job conditions](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-jobs-with-conditions)
and [job concurrency](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency).
GitHub displays skipped jobs as successful for merge checks; that display is
not proof that the skipped platform ran. The protected full-release publication
adapter continues to require actual successful check evidence and rejects
skipped, neutral and cancelled checks. Its pinned workflow digest and fixtures
must change together whenever the selector workflow changes.

Run the real selector matrix and workflow contracts on Windows:

```powershell
./scripts/test-release-local-selector-cli.ps1
./scripts/test-release-local-selector.ps1 -EvidencePath target/windows-ci/windows-ci-scope.json
& 'C:/Program Files/Git/bin/bash.exe' ./scripts/release-ci-workflow-smoke.sh
cargo test -p assemblywright-master --test github_publication_adapter --locked
```

The selector matrix exercises actual event-file handling through the selector
script in Windows PowerShell 5.1 and PowerShell 7. Hosted proof must additionally
show successful selection,
a skipped Mac runner job, and successful Windows workflows on the exact head.
Use a `[windows-only]` merge message and verify the same selection on published
main. These checks affect CI routing; Mac application work remains deferred.

The native CLI regression starts fresh PowerShell processes with an unset or
inherited nonzero exit code, then reproduces the hosted caller's immediate
exit-code check. Selector rejection and cleanup rejection must exit nonzero
without a passing report. The event matrix emits success only after cleanup
and evidence writing succeed, explicitly reporting exit zero to its caller.
All four CLI cases run in the Windows-local gate; their separate artifact is
`target/windows-ci/windows-ci-scope-cli.json`.
