# Contributing To GLIMPS

Thanks for wanting to help. GLIMPS is a Rust terminal tool, so correctness and
trust matter more than flashy formatting. A useful contribution either makes the
terminal experience clearer, makes pass-through behavior safer, or makes install
and release flow more boring.

## Start Here

1. Clone the repo.
2. Install a stable Rust toolchain.
3. Run the local checks:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all --all-features
cargo bench --no-run
```

For manual macOS dogfood without installing globally or editing your shell
config:

```bash
scripts/dogfood-macos.sh session
```

## Architecture Map

- `src/pty.rs`: PTY supervisor, raw terminal mode, resize, signals, shutdown.
- `src/init.rs`: zsh integration and OSC-133 marker emission.
- `src/format/osc133.rs`: prompt/input/output zone scanner.
- `src/format/mod.rs`: the only formatting dispatch path.
- `src/format/html.rs`, `json.rs`, `diff.rs`, `linefmt.rs`: individual
  formatters.
- `docs/FORMATTER_DESIGN_GUIDE.md`: how to add formatters without breaking
  terminal safety.
- `docs/SAFETY_INVARIANTS.md`: promises every change must preserve.

## Good First Issues

Good newcomer tasks are small, testable, and avoid PTY lifecycle changes:

- Add or improve formatter fixtures in `tests/corpus/`.
- Improve README examples and demo script copy.
- Add tests for edge cases in one formatter.
- Improve semantic colors without changing plain-theme output.
- Add docs for a known beta limitation.

PTY lifecycle, raw-mode restore, signal handling, and scanner state-machine
changes are not good first issues. They are welcome, but should include focused
integration tests and a clear explanation of failure modes.

## Formatter Rules

Read [`docs/FORMATTER_DESIGN_GUIDE.md`](./docs/FORMATTER_DESIGN_GUIDE.md)
before adding or changing a formatter.

- Default to pass-through when uncertain.
- Never mutate prompt/input zones.
- Never frame or color binary output.
- Keep buffering bounded.
- Plain theme output should remain stable and human-readable.
- Add tests for both recognized and rejected input.

## Pull Requests

Please include:

- What user-visible behavior changed.
- What safety invariant might be affected.
- What tests you ran.
- Screenshots or terminal captures for visual changes, when useful.

Small PRs are easier to review than sweeping rewrites.
