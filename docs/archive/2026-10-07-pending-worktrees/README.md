# Pending workspace source archive

The owner requested publication and merge of all Assemblywright changes on
2026-10-07. These patches preserve the remaining local source edits from two
older worktrees based on the commits recorded in `manifest.json`. They are
untrusted historical proposals, archived for review and recovery. They are not
applied to application source and do not supersede current designs or tests.

Current main already contains subsequent repair, permissions, tool approval and
publication changes. Applying an older cumulative patch wholesale would revert
those fixes. The additional execution-status tests lack their protocol and route
implementation, and `windows_active_control.rs` is an unregistered wrapper.
The archived local Codex worker is the retired lane; archiving does not authorize
its execution or restore it to the development workflow.

Each JSON patch stores unified-diff lines without changing their bytes. Extract
the patch by concatenating its `lines` array and encoding it as UTF-8; the
manifest digest applies to those extracted bytes. Each patch contains the tracked source difference against its recorded base and
its untracked Python, Rust and Markdown source. Generated Python caches, build
outputs and the redundant debug backup are excluded. Original worktrees remain
untouched. SHA-256 digests and path inventories allow exact content verification.
The reviewed pipeline correction is published separately as active source in
PR #435. Archived files have no compilation or execution role.
