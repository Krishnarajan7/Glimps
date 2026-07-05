# GLIMPS Formatter Design Guide

This guide is for the person adding the next formatter. The job is not to make
the terminal colorful for its own sake. The job is to make output easier to read
when GLIMPS can prove what it is looking at, and to leave everything else alone.

That last part matters. GLIMPS is not a command you pipe into when you feel like
it. It sits in the session. A formatter that guesses too eagerly can ruin normal
terminal output.

## The Rule

If you cannot prove the shape, pass through.

Every formatter must keep the promises in
[`docs/SAFETY_INVARIANTS.md`](./SAFETY_INVARIANTS.md):

- never mutate prompt or typed input;
- never frame, color, or reformat binary output;
- never log, store, upload, or persist terminal contents;
- keep buffering bounded;
- keep the plain/no-color path clean and predictable.

Missing a formatting opportunity is fine. Breaking a user's terminal is not.

## Where Formatting Belongs

All output changes go through [`src/format/mod.rs`](../src/format/mod.rs).

The scanner divides bytes into zones:

- prompt, typed input, and markers: always pass through;
- command output: the only zone formatters may touch.

Do not add formatting in the PTY supervisor or shell integration. Those layers
should move bytes and preserve terminal behavior. Formatting belongs at the
formatter boundary.

## Pick The Smallest Formatter

### Streaming Line Formatter

Use this when each line can be recognized on its own and the output might never
end.

Good fits:

- log severity lines;
- HTTP status lines;
- stack-trace lines.

Usual path:

- add a recognizer in `src/format/linefmt.rs`;
- wire it through the streaming registry only if it is a global formatter;
- add positive tests and rejection tests.

### Buffered Document Formatter

Use this when GLIMPS needs the whole output run before it can safely format.

Good fits:

- JSON documents;
- HTML documents;
- HTTP responses;
- unified diffs.

Usual path:

- create or update a formatter module under `src/format/`;
- implement `BufferedFormatter`;
- add the formatter to `enabled_buffered` in `src/format/mod.rs`;
- keep `could_start` cheap and conservative;
- return `None` when parsing or structural proof fails.

Buffered formatters must respect config limits. If output is too large,
malformed, or incomplete, emit the original bytes.

### Command-Aware Formatter

Use this when the command narrows the output shape enough to be safe.

Good fits:

- `cat README.md`;
- `git status --short`;
- `sqlite3` result tables;
- `ls -la`.

Usual path:

- add or reuse a `CommandView` in `src/format/mod.rs`;
- detect the command or file extension in `command_view` / `file_content_view`;
- implement the line colorizer in `src/format/linefmt.rs`;
- prove unrelated command output is untouched.

Command-aware formatting is often the safest polish because it combines two
signals: what command ran and what shape came back.

## Color And Layout

- Only insert ANSI SGR escapes around existing bytes unless the formatter is a
  structural renderer such as JSON or HTML.
- Do not wrap, align, truncate, reorder, or invent user output unless that is the
  explicit behavior of the formatter.
- Preserve line endings. CRLF input must not become doubled CRLF.
- Reuse colors from `Theme`; do not make each formatter invent its own palette.
- With `Theme::plain()`, color-only formatters should be byte-identical.

## Tests We Expect

For every formatter change, add focused tests in
[`src/format/tests.rs`](../src/format/tests.rs) or the formatter module.

Minimum useful coverage:

- a positive sample: recognized output is colored or structured;
- a rejection sample: similar-looking non-target output passes through;
- a line-ending sample when CRLF could be affected;
- a plain-theme identity test for color-only formatters;
- a split/chunk test when streaming boundaries matter.

For buffered formatters, malformed and incomplete input should decline.

For command-aware formatters, test through the OSC-133 command marker helpers so
the real dispatch path is covered.

## Manual Dogfood

If a visual change is worth shipping, it is worth trying in a real GLIMPS
session. Add a command to
[`scripts/dogfood-macos.sh`](../scripts/dogfood-macos.sh) when it helps manual
review.

That script must stay non-invasive:

- no global install;
- no edits to `~/.zshrc`;
- no login-shell changes.

## Before Opening A PR

Check the basics:

- the formatter has a clear proof of shape;
- false positives are tested;
- binary and already-colored output are not newly touched;
- buffering remains bounded;
- `cargo fmt --all -- --check` passes;
- `cargo clippy --all-targets --all-features -- -D warnings` passes;
- `cargo test --all --all-features` passes.

Small formatter PRs are easier to review and safer to ship. If a change touches
PTY lifecycle, terminal raw mode, or OSC-133 scanning, it is no longer a simple
formatter PR; give it integration coverage and a very clear explanation.
