# GLIMPS Safety Invariants

GLIMPS sits between a person and their terminal. That is a trust position, not
just an implementation detail. These are the promises we protect in code,
tests, docs, and releases.

If a change breaks one of these, it is not a polish regression. It is a product
bug.

## Terminal Control

- Restore the real terminal mode on normal exit, signal-driven exit, and panic
  paths.
- Do not wait forever during shutdown. A stuck PTY read must not keep GLIMPS
  alive after the shell is gone.
- Keep `exit`, Ctrl-D, SIGTERM, and terminal resize covered by PTY integration
  tests.

## Byte Safety

- Default to pass-through when content is uncertain.
- Never frame, colorize, or reformat binary output.
- Never mutate prompt or typed-input zones.
- Only transform bytes inside OSC-133 command-output zones.
- Preserve already-colored output unless a formatter has explicitly claimed a
  clean output run.
- Flush any buffered tail on clean PTY EOF so user output is not silently lost.

## Trust And Privacy

- Do not log terminal contents.
- Do not store terminal contents.
- Do not upload terminal contents.
- Do not add telemetry, crash upload, analytics, or network I/O to normal
  runtime behavior.
- Keep `GLIMPS=0` and `enabled = false` working as startup-time off switches.
- Do not rewrite a user's prompt.

## Resource Bounds

- Keep output buffering bounded by config limits clamped to safe ranges.
- Stream unbounded plain-text output line-by-line.
- Prefer zero-copy pass-through when no formatting work is needed.

The best GLIMPS bug is the one the user never notices because GLIMPS quietly
decided not to touch the output.
