# GLIMPS Safety Invariants

GLIMPS sits between a user and their terminal, so safety is product behavior, not
just implementation detail. These invariants are public promises that code,
tests, and release checks should preserve.

## Terminal Control

- Always restore the real terminal mode on normal exit, signal-driven exit, and
  panic paths.
- Do not use unbounded shutdown waits. A stuck PTY read must not keep GLIMPS
  alive after the shell is gone.
- Keep `exit`, Ctrl-D, SIGTERM, and terminal resize covered by PTY integration
  tests.

## Byte Safety

- Default to pass-through when content is uncertain.
- Never frame, colorize, or reformat binary output.
- Never mutate prompt/input zones. Only transform bytes inside OSC-133 command
  output zones.
- Preserve already-colored output unless a formatter has explicitly claimed a
  clean output run.
- Flush any buffered tail on clean PTY EOF so user output is not silently lost.

## Trust And Privacy

- Do not log, store, upload, or persist terminal contents.
- Do not add telemetry, crash upload, analytics, or network I/O to normal runtime
  behavior.
- Keep `GLIMPS=0` and `enabled = false` working as startup-time off switches.
- Do not rewrite a user's prompt.

## Resource Bounds

- Keep output buffering bounded by config limits that are clamped to safe ranges.
- Stream unbounded plain-text output line-by-line.
- Prefer zero-copy pass-through when no formatting work is needed.
