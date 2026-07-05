# Contributing To GLIMPS

Thanks for looking at GLIMPS. This project is small, sharp, and a little
dangerous in the way terminal tools are dangerous: it sits between a person and
their shell. That means a good contribution is not just "more color." A good
contribution makes output easier to read without making the terminal less
trustworthy.

If you only read one thing, read this: **when GLIMPS is unsure, it should get
out of the way.** Pass-through is a feature.

## Using AI

Using AI tools is fine. GLIMPS itself is being built in a world where AI is part
of how people write software.

But please do not submit code you do not understand. Before opening a pull
request, you should be able to explain:

- what changed;
- why the change is safe;
- what output it recognizes;
- what output it intentionally refuses to touch;
- what tests prove both paths.

AI can help you get there. It can read the code with you, explain edge cases,
write a first draft, or suggest tests. The final responsibility is still yours.
If a maintainer asks how your formatter behaves on binary output, prompts,
already-colored text, or a full-screen app, "the agent wrote it" is not an
answer.

## First Setup

You need a stable Rust toolchain.

```bash
git clone https://github.com/Krishnarajan7/Glimps
cd Glimps
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all --all-features
cargo bench --no-run
```

For the full pre-release gate, run:

```bash
scripts/release-readiness.sh
```

That script does not install GLIMPS globally, edit `~/.zshrc`, or start an
interactive session.

## Try It Like A User

For manual macOS testing without changing your shell startup files:

```bash
scripts/dogfood-macos.sh session
```

That command builds the repo-local binary and starts a temporary zsh wrapped by
GLIMPS. It does not install anything globally. It does not edit your real
`~/.zshrc`. When you exit, the temporary setup is cleaned up.

Inside that session, try normal commands, not only perfect demos:

```bash
echo '{"alpha":1,"items":[2,3]}'
printf 'INFO boot\nWARN disk\nERROR boom\n'
cat README.md
find src -maxdepth 2 -type f
git status --short
man printf
vim README.md
false
exit
```

The boring cases matter. `vim`, `less`, `ssh`, `tmux`, `fzf`, binary output, and
already-colored output should not be "improved" into broken output.

## Architecture Map

The shortest useful map:

- `src/pty.rs`: owns the child shell, terminal mode, resize, signals, and
  shutdown.
- `src/init.rs`: emits the zsh integration GLIMPS needs.
- `src/format/osc133.rs`: separates prompt/input zones from command output.
- `src/format/mod.rs`: the formatting dispatch path. Most formatter work starts
  or ends here.
- `src/format/html.rs`, `json.rs`, `diff.rs`, `linefmt.rs`: current formatter
  implementations.
- `tests/corpus/`: real-ish terminal output samples and expected behavior.
- `docs/FORMATTER_DESIGN_GUIDE.md`: how to add formatters safely.
- `docs/SAFETY_INVARIANTS.md`: the promises changes must preserve.

If you are changing PTY lifecycle, raw-mode restore, signal handling, or the
OSC-133 scanner, slow down and add integration coverage. Those paths are where
small bugs become ruined terminal sessions.

## Good First Work

Good first issues are small, visible, and easy to prove:

- add a fixture for a real command in `tests/corpus/`;
- improve one formatter's false-positive tests;
- polish README examples;
- improve demo script wording;
- add a command-aware formatter for a simple, stable output shape;
- improve no-color/plain output without changing the meaning.

Less good as a first PR:

- rewriting the PTY supervisor;
- changing scanner state-machine behavior;
- adding large dependencies;
- changing install/release claims;
- formatting output when the shape is guessed rather than known.

If in doubt, open a small issue or discussion first. A ten-line question can save
a two-hundred-line PR from going sideways.

## Formatter Rules

Read [`docs/FORMATTER_DESIGN_GUIDE.md`](./docs/FORMATTER_DESIGN_GUIDE.md)
before adding or changing a formatter.

The rules are simple, but strict:

- default to pass-through when uncertain;
- never mutate prompt or typed-input zones;
- never frame or color binary output;
- never break already-colored output;
- keep buffering bounded;
- keep no-color output useful;
- add tests for both "yes, format this" and "no, leave this alone."

For command-aware formatting, use the command and shape together. Seeing `cat`
is not enough. Seeing `cat package.json` plus valid JSON is much safer. Seeing a
table-shaped output from `ps` is safer than coloring every random line with
spaces in it.

## Pull Requests

Please keep PRs small enough to review carefully. A focused formatter with good
fixtures is much more likely to land than a giant "make output nicer" rewrite.

In your PR description, include:

- what changed for users;
- what GLIMPS now recognizes;
- what it still passes through;
- which safety invariant might be affected;
- which tests you ran;
- screenshots or terminal captures for visual changes.

Before asking for review, run the nearest useful checks. For formatter changes,
that usually means:

```bash
cargo fmt --all -- --check
cargo test --all --all-features
```

For PTY, config, release, or safety-sensitive work, run the full readiness
script too:

```bash
scripts/release-readiness.sh
```

## Bugs And Feature Ideas

Before opening an issue, search existing issues and docs. If the idea is broad,
start with the problem, not the implementation. "I cannot read curl redirects in
scrollback" is more useful than "add a giant HTTP parser."

Good bug reports include:

- OS and terminal app;
- shell and prompt setup;
- GLIMPS version or commit;
- command you ran;
- what happened;
- what you expected;
- whether the same command works outside GLIMPS.

If the bug involves private output, reduce it to a small fake sample. Please do
not paste secrets from your terminal into an issue.

## Project Taste

GLIMPS should feel calm. The terminal is already busy. The job is to make the
important parts easier to see, not to decorate everything that moves.

Prefer:

- boring correctness over clever detection;
- one great formatter over five half-sure ones;
- readable defaults over configuration sprawl;
- honest install docs over launch-day hype;
- tests that preserve trust.

Thanks again for helping. Terminal tools earn users slowly and lose them very
quickly. Let us keep GLIMPS on the earning side.
