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
