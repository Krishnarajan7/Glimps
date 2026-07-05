<!-- title: Color `git stash list` output -->
<!-- labels: good first issue,formatter,command-aware,safety -->

`git stash list` gives you lines like:

```text
stash@{0}: WIP on main: 1a2b3c4 add formatter guide
```

A little color — the stash id, the branch/ref, and the message each treated
differently — makes it much easier to find the stash you actually want.

### What to build

Command-aware coloring for `git stash list`: highlight the `stash@{N}` id, the
branch/ref portion, and keep the message readable.

### Where to look

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`

### Done when

- `stash@{N}` is highlighted.
- The branch/ref portion is highlighted separately from the message.
- The message stays readable.
- Output from other commands is untouched.

### Keep it safe

Only enable this under the `git stash list` command view. Do **not** globally
match `stash@{` — that string could show up in logs or prose, and GLIMPS should
not color it there.

### Prove it

- A command-marker test with two stash lines.
- A rejection test for similar-looking prose under a non-git command.

---

**New to GLIMPS?** Read [CONTRIBUTING.md](https://github.com/Krishnarajan7/Glimps/blob/main/CONTRIBUTING.md)
and the [Formatter Design Guide](https://github.com/Krishnarajan7/Glimps/blob/main/docs/FORMATTER_DESIGN_GUIDE.md).
To try your change like a real user, run `scripts/dogfood-macos.sh session` — it
wraps a throwaway zsh and cleans up on exit, without touching your `~/.zshrc`.
Comment here to claim it, and ask anything before you start.
