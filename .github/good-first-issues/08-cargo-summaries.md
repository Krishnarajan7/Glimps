<!-- title: Color Cargo build/test summary lines -->
<!-- labels: good first issue,formatter,command-aware,safety -->

Rust developers stare at `cargo build` / `cargo test` output all day. The signal
lines — `Compiling`, `Finished`, `warning:`, `error:`, and the
`test result: ok` / failure summary — are easy to lose in the scroll. Coloring
them turns "did it pass?" into a glance.

### What to build

Command-aware coloring for common Cargo output under `cargo build`, `cargo test`,
and `cargo check`: `Compiling`, `Finished`, `warning:`, `error:`, and test result
summary lines.

### Where to look

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`

### Done when

- `error:` lines are red.
- `warning:` lines are yellow.
- `Finished` and successful `test result: ok` lines are green.
- Test failure summaries are red.
- The existing generic log/error coloring is unchanged.

### Keep it safe

Enable this only under `cargo` commands. Plenty of normal prose contains the word
`warning:` — GLIMPS shouldn't over-color it globally. Command + shape, together.

### Prove it

- A successful build output test.
- A warning output test.
- A failed test-summary output test.

---

**New to GLIMPS?** Read [CONTRIBUTING.md](https://github.com/Krishnarajan7/Glimps/blob/main/CONTRIBUTING.md)
and the [Formatter Design Guide](https://github.com/Krishnarajan7/Glimps/blob/main/docs/FORMATTER_DESIGN_GUIDE.md).
To try your change like a real user, run `scripts/dogfood-macos.sh session` — it
wraps a throwaway zsh and cleans up on exit, without touching your `~/.zshrc`.
Comment here to claim it, and ask anything before you start.
