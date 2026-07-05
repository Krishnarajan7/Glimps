<!-- title: Improve unified-diff metadata coloring -->
<!-- labels: good first issue,formatter,safety -->

GLIMPS already colors diff hunks (added/removed/hunk-header lines). But the
*metadata* around each file — the `diff --git`, `index`, `--- a/file`,
`+++ b/file`, rename/copy, and mode lines — is still a bit of a gray wall. Making
the file paths in those headers stand out helps you find file boundaries in a big
diff.

### What to build

Sharpen `src/format/diff.rs` so diff metadata reads more clearly. File paths in
the header should be visually distinct from the metadata keywords around them.

### Where to look

- `src/format/diff.rs`

### Done when

- File paths in diff headers are visually distinct from metadata keywords.
- Hunk / add / remove coloring is unchanged.
- Non-diff text that happens to contain `+`/`-` lines still declines to format.
- Plain-theme (no-color) output stays byte-identical.

### Keep it safe

The formatter must still require a **real hunk header** before it claims
something is a diff. A few lines starting with `+` or `-` are not enough — that's
how you avoid mangling ordinary prose or code.

### Prove it

- Update the existing positive diff test to assert the new metadata colors.
- Keep `declines_without_a_hunk_header`.
- Keep the plain-theme byte-identity property test.

---

**New to GLIMPS?** Read [CONTRIBUTING.md](https://github.com/Krishnarajan7/Glimps/blob/main/CONTRIBUTING.md)
and the [Formatter Design Guide](https://github.com/Krishnarajan7/Glimps/blob/main/docs/FORMATTER_DESIGN_GUIDE.md).
To try your change like a real user, run `scripts/dogfood-macos.sh session` — it
wraps a throwaway zsh and cleans up on exit, without touching your `~/.zshrc`.
Comment here to claim it, and ask anything before you start.
