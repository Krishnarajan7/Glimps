# GLIMPS Formatter Design Guide

This guide is for contributors adding or changing output formatting. GLIMPS is a
terminal supervisor, not a pretty-printer users invoke manually, so a formatter
must be conservative: a missed format is acceptable; corrupting normal terminal
output is not.

## Core Rule

If the formatter cannot prove the shape, pass through.

Every formatter must preserve the safety invariants in
[`docs/SAFETY_INVARIANTS.md`](./SAFETY_INVARIANTS.md):

- never mutate prompt or typed input;
- never frame, color, or reformat binary output;
- never log, store, upload, or persist terminal contents;
- keep buffering bounded;
- keep the plain/no-color path clean and predictable.

## Where Formatting Happens

All output changes go through [`src/format/mod.rs`](../src/format/mod.rs).

The PTY scanner splits bytes into zones:

- prompt/input/markers: always pass through;
- command output: the only zone formatters may touch.

Do not add formatting in the PTY supervisor or shell integration. Add it through
the formatter seam.

## Choose The Smallest Formatter Type

### Streaming Line Formatter

Use this when each line can be recognized independently and output may be
unbounded.

Examples:

- log severity lines;
- HTTP status lines;
- stack-trace lines.

Implementation path:

- add a small recognizer in `src/format/linefmt.rs`;
- wire it through the streaming registry only when it is a global formatter;
- add line-level tests and rejection tests.

### Buffered Document Formatter

Use this when the whole output run is needed before formatting.

Examples:

- JSON documents;
- HTML documents;
- HTTP responses;
- unified diffs.

Implementation path:

- create or update a formatter module under `src/format/`;
- implement `BufferedFormatter`;
- add the formatter to `enabled_buffered` in `src/format/mod.rs`;
- keep `could_start` cheap and conservative;
- decline with `None` when parsing or structural proof fails.

Buffered formatters must respect config limits. If the output is too large or
does not parse, GLIMPS must emit the original bytes.

### Command-Aware Formatter

Use this when the command tells us the output shape.

Examples:

- `cat README.md`;
- `git status --short`;
- `sqlite3` result tables;
- `ls -la`.

Implementation path:

- add or reuse a `CommandView` in `src/format/mod.rs`;
- detect the command or file extension in `command_view` / `file_content_view`;
- implement the line colorizer in `src/format/linefmt.rs`;
- make sure unrelated command output is untouched.

Command-aware formatters are often the safest way to add polish because the
command name or file extension narrows the input shape.

## Color And Layout Rules

- Only insert ANSI SGR escapes around existing bytes.
- Do not wrap, align, truncate, reorder, or synthesize user output unless the
  formatter is explicitly a structural document renderer such as JSON/HTML.
- Preserve line endings. CRLF input must not become doubled CRLF.
- Keep colors semantic and reused from `Theme`.
- With `Theme::plain()`, output should be byte-identical for color-only
  formatters.

## Required Tests

For every formatter change, add focused tests in
[`src/format/tests.rs`](../src/format/tests.rs) or the formatter module.

Minimum coverage:

- positive sample: recognized output is colored or structured;
- rejection sample: similar-looking non-target output passes through;
- line ending sample when the formatter touches CRLF-sensitive output;
- plain theme identity when the formatter is color-only;
- split/chunk behavior when the formatter depends on streaming boundaries.

For buffered formatters, also test malformed or incomplete input declines.

For command-aware formatters, test through the OSC-133 command marker helpers so
the real dispatch path is covered.

## Manual Dogfood

Before marking a visual formatter complete, add a command to
[`scripts/dogfood-macos.sh`](../scripts/dogfood-macos.sh) when it is useful for
manual verification. The dogfood script must stay non-invasive:

- no global install;
- no edits to `~/.zshrc`;
- no login shell changes.

## Review Checklist

Before opening a PR:

- The formatter has a clear proof of shape.
- False positives are tested.
- Binary and already-colored output are not newly touched.
- Buffering remains bounded.
- `cargo fmt --all -- --check` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- `cargo test --all --all-features` passes.

Small, narrow formatter PRs are preferred. If a change touches PTY lifecycle,
terminal raw mode, or OSC-133 scanning, it is no longer a simple formatter PR and
needs extra integration-test coverage.
