<!-- title: Color `git diff --name-only` path lists -->
<!-- labels: good first issue,formatter,command-aware,safety -->

When you run `git diff --name-only` (or `git show --name-only`), you get a plain
list of changed file paths. It's readable, but a little color makes it much
faster to scan — which file is which, at a glance.

This is the *simple* case on purpose: it's a list of paths, not a full patch. No
hunk parsing involved.

### What to build

Command-aware coloring for path-only output from `git diff --name-only` and
`git show --name-only`. Each line is a path; color it as a path. For
`git show --name-only`, the commit header block above the paths should still get
its existing commit-header coloring.

### Where to look

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`
- `scripts/dogfood-macos.sh` (add a demo line so it's easy to see)

### Done when

- `git diff --name-only` colors each path as a path.
- `git show --name-only` still colors the commit header when present.
- Blank lines and any non-path prose pass through untouched.
- Existing full `git diff` patch formatting is unchanged.

### Keep it safe

Only format this through the **command-aware `git` path** — do not infer paths
globally. Random output from some other command that happens to look path-ish
must stay untouched. This is the "use command + shape together" rule from the
design guide.

### Prove it

- A command-marker test for `git diff --name-only`.
- A command-marker test for `git show --name-only` with a commit header.
- A rejection test showing ordinary (non-git) output is left alone.

---

**New to GLIMPS?** Read [CONTRIBUTING.md](https://github.com/Krishnarajan7/Glimps/blob/main/CONTRIBUTING.md)
and the [Formatter Design Guide](https://github.com/Krishnarajan7/Glimps/blob/main/docs/FORMATTER_DESIGN_GUIDE.md).
To try your change like a real user, run `scripts/dogfood-macos.sh session` — it
wraps a throwaway zsh and cleans up on exit, without touching your `~/.zshrc`.
Comment here to claim it, and ask anything before you start.
