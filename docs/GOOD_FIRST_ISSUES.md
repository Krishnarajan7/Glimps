# GLIMPS Good First Issue Specs

These are copy-ready issue specs for GitHub.

They are small on purpose. A good first GLIMPS issue should let someone improve
the product without touching the PTY supervisor, raw terminal mode, or OSC-133
scanner. Those areas are important, but they are not where a newcomer should
learn the codebase.

When copying one of these into GitHub, keep the human part. Explain what a user
will notice, what GLIMPS must leave alone, and how the contributor can prove the
change is safe.

Recommended labels for all formatter issues:

- `good first issue`
- `formatter`
- `safety`

Add `command-aware`, `docs`, or `release` when relevant.

## 1. Color `git diff --name-only` Path Lists

Labels: `good first issue`, `formatter`, `command-aware`, `safety`

Scope:
Add command-aware coloring for path-only output from `git diff --name-only` and
`git show --name-only`. This is not full patch diff work; it is the simpler
"make a list of changed paths easier to scan" case.

Likely files:

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`
- `scripts/dogfood-macos.sh`

Acceptance criteria:

- `git diff --name-only` colors each path as a path.
- `git show --name-only` still colors commit headers when present.
- Empty lines and non-path prose pass through.
- Existing `git diff` full patch formatting is unchanged.

Safety notes:
Do not infer paths globally. Only format through the `git` command-aware path.
Random command output that happens to look path-ish should stay untouched.

Suggested tests:

- command-marker test for `git diff --name-only`;
- command-marker test for `git show --name-only` with a commit header;
- rejection test showing ordinary command output is untouched.

## 2. Improve Unified Diff Metadata Coloring

Labels: `good first issue`, `formatter`, `safety`

Scope:
Improve `src/format/diff.rs` so diff metadata is easier to read. The useful
parts are `diff --git`, `index`, `--- a/file`, `+++ b/file`, rename/copy
metadata, and mode lines.

Likely files:

- `src/format/diff.rs`

Acceptance criteria:

- File paths in diff headers are visually distinct from metadata keywords.
- Hunk/add/remove coloring remains unchanged.
- Non-diff text with plus/minus lines still declines.
- Plain theme output remains byte-identical.

Safety notes:
The formatter must still require a real hunk header before claiming a diff. A
few lines beginning with `+` or `-` are not enough.

Suggested tests:

- update existing diff positive test for metadata colors;
- keep `declines_without_a_hunk_header`;
- keep plain-theme identity property.

## 3. Add `git stash list` Coloring

Labels: `good first issue`, `formatter`, `command-aware`, `safety`

Scope:
Color `git stash list` lines: stash id, branch/ref text, and message.

Example:

```text
stash@{0}: WIP on main: 1a2b3c4 add formatter guide
```

Likely files:

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`

Acceptance criteria:

- `stash@{N}` is highlighted.
- Branch/ref portion is highlighted separately.
- Message remains readable.
- Non-stash output from other commands is untouched.

Safety notes:
Only enable under `git stash list`; do not globally match `stash@{`.

Suggested tests:

- command-marker test for two stash lines;
- rejection test for similar prose under a non-git command.

## 4. Add Ripgrep/Grep Result Coloring

Labels: `good first issue`, `formatter`, `command-aware`, `safety`

Scope:
Color common `rg` / `grep -n` output shaped like `path:line:column:match` or
`path:line:match`. This should make search results easier to scan without
turning every colon-separated line into a fake grep result.

Likely files:

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`
- `scripts/dogfood-macos.sh`

Acceptance criteria:

- Path, line number, optional column number, separators, and match text are
  visually distinct.
- Lines without a numeric line field pass through.
- Binary match notices or tool warnings pass through unless confidently shaped.

Safety notes:
Only enable for `rg`, `grep`, `egrep`, and `fgrep` command views.

Suggested tests:

- `rg -n TODO src` style output;
- `rg -n --column TODO src` style output;
- rejection test for colon-heavy prose.

## 5. Add `tree` Output Coloring

Labels: `good first issue`, `formatter`, `command-aware`, `safety`

Scope:
Color `tree` output by dimming branch glyphs and highlighting leaf names. The
goal is a calmer directory tree, not a brand-new tree renderer.

Example:

```text
├── src
│   └── main.rs
```

Likely files:

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`

Acceptance criteria:

- Tree branch characters are dim.
- File/directory names are highlighted.
- Summary lines like `12 directories, 34 files` are dim or pass through.
- Plain ordinary text from other commands is untouched.

Safety notes:
Only enable under the `tree` command because Unicode tree glyphs may appear in
docs or logs.

Suggested tests:

- command-marker test with nested tree output;
- summary-line test;
- rejection test for non-tree output.

## 6. Add `docker ps` Table Coloring

Labels: `good first issue`, `formatter`, `command-aware`, `safety`

Scope:
Color default `docker ps` table output: headers, container IDs, image names,
status, ports, and names. Keep it practical; the default table is the thing most
people see.

Likely files:

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`

Acceptance criteria:

- Header row is dim or key-colored.
- Container ID and name are easy to scan.
- `Up` statuses are green; `Exited` statuses are red or yellow.
- If the row cannot be split confidently, pass through.

Safety notes:
Do not implement a general whitespace table formatter. Keep this command-aware
and shape-checked.

Suggested tests:

- default `docker ps` output;
- `docker ps -a` exited container output;
- rejection test for unrelated whitespace-aligned text.

## 7. Add `kubectl get pods` Status Coloring

Labels: `good first issue`, `formatter`, `command-aware`, `safety`

Scope:
Color `kubectl get pods` table output: pod name, READY, STATUS, RESTARTS, and
AGE. Start with pods because pod status is the quick "is my cluster okay?" view.

Likely files:

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`

Acceptance criteria:

- Header row is distinct.
- `Running` is green.
- `Pending`, `CrashLoopBackOff`, `Error`, and `ImagePullBackOff` are warning or
  error colored.
- Non-pod `kubectl` output passes through unless explicitly supported.

Safety notes:
Start with `kubectl get pods` only. Do not try to support every Kubernetes table
in the first PR.

Suggested tests:

- one running pod;
- one failing pod;
- rejection test for `kubectl config current-context` output.

## 8. Add Cargo Build/Test Summary Coloring

Labels: `good first issue`, `formatter`, `command-aware`, `safety`

Scope:
Color common Cargo output lines for `cargo build`, `cargo test`, and
`cargo check`: `Compiling`, `Finished`, `warning:`, `error:`, and test result
summary lines. Rust users stare at this output constantly, so small clarity wins
matter.

Likely files:

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`

Acceptance criteria:

- `error:` lines are red.
- `warning:` lines are yellow.
- `Finished` / successful `test result: ok` lines are green.
- Test failure summaries are red.
- Existing generic log/error coloring remains unchanged.

Safety notes:
Enable only under `cargo` commands so prose containing `warning:` is not
over-colored globally.

Suggested tests:

- successful build output;
- warning output;
- failed test summary output.

## 9. Add More Source File Extensions To Code Coloring

Labels: `good first issue`, `formatter`, `command-aware`, `safety`

Scope:
Extend reader-command source coloring to a small set of common languages not yet
covered, such as Lua, Elixir, Dart, R, and C#. Keep the first pass modest.

Likely files:

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`

Acceptance criteria:

- File extensions map to a language-specific keyword set.
- Comments, strings, numbers, keywords, and function-ish calls are colored.
- Existing supported languages still pass.
- Unknown extensions still pass through.

Safety notes:
Keep this lightweight. Do not add a dependency unless the dependency/security
story is reviewed first.

Suggested tests:

- one test per added language or one table-driven test with representative lines;
- rejection test for an unknown extension.

## 10. Add Demo/Docs Visual Evidence Checklist

Labels: `good first issue`, `docs`, `release`

Scope:
Add a short checklist for creating before/after terminal captures and the demo
GIF from `demo/glimps.tape`. This issue is about making the project easier to
show honestly.

Likely files:

- `demo/README.md`
- `docs/LAUNCH_HARDENING_CHECKLIST.md`
- `README.md`

Acceptance criteria:

- The checklist names required demo commands.
- It records what must not be overclaimed before Homebrew/release is live.
- It explains where to place generated demo media.
- It keeps generated media ignored unless intentionally committed.

Safety notes:
Docs must keep install claims honest: no `brew install glimps` until the tap is
verified from a real tag.

Suggested tests:

- read-through only;
- `git diff --check`;
- verify `.gitignore` behavior for generated demo media.
