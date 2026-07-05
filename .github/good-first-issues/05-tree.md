<!-- title: Color `tree` output -->
<!-- labels: good first issue,formatter,command-aware,safety -->

`tree` prints a directory listing with branch glyphs:

```text
├── src
│   └── main.rs
```

Dimming the branch characters and highlighting the leaf names gives you a calmer
tree where your eye lands on the file and folder names instead of the lines.

### What to build

Command-aware coloring for `tree`: dim the branch glyphs, highlight file and
directory names. This is a coloring pass, not a new tree renderer.

### Where to look

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`

### Done when

- Tree branch characters (`├──`, `│`, `└──`) are dim.
- File and directory names are highlighted.
- Summary lines like `12 directories, 34 files` are dim or pass through.
- Ordinary text from other commands is untouched.

### Keep it safe

Only enable under the `tree` command. Those Unicode box-drawing glyphs also show
up in docs, READMEs, and logs — GLIMPS must not color them there.

### Prove it

- A command-marker test with nested tree output.
- A summary-line test.
- A rejection test for non-tree output that contains box-drawing characters.

---

**New to GLIMPS?** Read [CONTRIBUTING.md](https://github.com/Krishnarajan7/Glimps/blob/main/CONTRIBUTING.md)
and the [Formatter Design Guide](https://github.com/Krishnarajan7/Glimps/blob/main/docs/FORMATTER_DESIGN_GUIDE.md).
To try your change like a real user, run `scripts/dogfood-macos.sh session` — it
wraps a throwaway zsh and cleans up on exit, without touching your `~/.zshrc`.
Comment here to claim it, and ask anything before you start.
