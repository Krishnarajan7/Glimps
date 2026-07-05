<!-- title: Color ripgrep / grep search results -->
<!-- labels: good first issue,formatter,command-aware,safety -->

Search output like `rg -n` or `grep -n` comes out shaped like
`path:line:column:match` or `path:line:match`. Coloring the path, the line
number, and the match separately makes a wall of results far easier to scan.

The trap here is obvious: plenty of ordinary output has colons in it. This must
only fire for real search results, not every colon-separated line.

### What to build

Command-aware coloring for `rg` / `grep -n` result lines: path, line number,
optional column number, separators, and match text each visually distinct.

### Where to look

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`
- `scripts/dogfood-macos.sh`

### Done when

- Path, line number, optional column, separators, and match text are distinct.
- Lines without a numeric line field pass through unchanged.
- Tool warnings / "binary file matches" notices pass through unless confidently
  shaped.

### Keep it safe

Only enable for the `rg`, `grep`, `egrep`, and `fgrep` command views. Do not turn
every `foo:123:bar` line in arbitrary output into a fake grep result.

### Prove it

- An `rg -n TODO src` style test.
- An `rg -n --column TODO src` style test.
- A rejection test for colon-heavy prose that isn't a search result.

---

**New to GLIMPS?** Read [CONTRIBUTING.md](https://github.com/Krishnarajan7/Glimps/blob/main/CONTRIBUTING.md)
and the [Formatter Design Guide](https://github.com/Krishnarajan7/Glimps/blob/main/docs/FORMATTER_DESIGN_GUIDE.md).
To try your change like a real user, run `scripts/dogfood-macos.sh session` — it
wraps a throwaway zsh and cleans up on exit, without touching your `~/.zshrc`.
Comment here to claim it, and ask anything before you start.
