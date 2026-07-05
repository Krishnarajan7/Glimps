<!-- title: Add source-file coloring for more languages -->
<!-- labels: good first issue,formatter,command-aware,safety -->

When you view a source file through a reader command (`cat`, `head`, …), GLIMPS
colors comments, strings, numbers, keywords, and function-ish calls for the
languages it knows. This task extends that to a few common ones it doesn't cover
yet — think Lua, Elixir, Dart, R, C#.

Keep the first pass modest. One or two languages done well is a better PR than
five done vaguely.

### What to build

Extend reader-command source coloring to a small set of currently-unsupported
languages. Map the file extension to a language-specific keyword set and reuse
the existing comment/string/number/keyword machinery.

### Where to look

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`

### Done when

- The new extensions map to a language-specific keyword set.
- Comments, strings, numbers, keywords, and function calls are colored.
- Already-supported languages still pass their tests.
- Unknown extensions still pass through unchanged.

### Keep it safe

Keep it lightweight — do **not** pull in a syntax-highlighting dependency. New
dependencies need a security/justification review first, so a first PR that adds
one will stall. Reuse what's already there and keep the change inside the
formatter path described in `docs/FORMATTER_DESIGN_GUIDE.md`.

### Prove it

- One test per added language (or one table-driven test with representative lines).
- A rejection test for an unknown extension.

---

**New to GLIMPS?** Read [CONTRIBUTING.md](https://github.com/Krishnarajan7/Glimps/blob/main/CONTRIBUTING.md)
and the [Formatter Design Guide](https://github.com/Krishnarajan7/Glimps/blob/main/docs/FORMATTER_DESIGN_GUIDE.md).
To try your change like a real user, run `scripts/dogfood-macos.sh session` — it
wraps a throwaway zsh and cleans up on exit, without touching your `~/.zshrc`.
Comment here to claim it, and ask anything before you start.
