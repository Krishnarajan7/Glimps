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
- Never put user-controlled text (the command line, the working directory) into
  a shell-integration marker or metadata record raw. The hooks replace control
  characters with `?` first, so a pasted line or a hostile directory name
  cannot end a marker early and reach the terminal as a live escape sequence.
  (C1 controls are matched in their UTF-8 form. A raw 0x80-0x9F byte that is
  not valid UTF-8 passes through: a UTF-8 terminal shows it as invalid text,
  and only a terminal in an 8-bit, non-UTF-8 mode would act on it. Matching
  raw bytes would corrupt valid multibyte characters.)

## Byte Safety

- Default to pass-through when content is uncertain.
- Never frame, colorize, or reformat binary output.
- Never mutate prompt or typed-input zones.
- Only transform bytes inside OSC-133 command-output zones. Once GLIMPS's own
  hooks are present, a bare `C`/`D` from another integration (fish 4's own,
  a terminal's) or printed by a command is ignored. In-band markers remain
  forgeable in full (a command that also prints GLIMPS's `7338` before a `D`
  still ends the zone); the private metadata channel is what keeps such
  output from authoring headers, exit codes or results.
- Treat an escape sequence as one unit to its final byte, intermediates
  included (`ESC ( B`): nothing GLIMPS adds may land inside one.
- Preserve already-colored output unless a formatter has explicitly claimed a
  clean output run.
- OSC 8 links (`[links]`) never change a visible byte, and a link always
  points at the text it wraps (a path resolved against the trusted working
  directory, never a forgeable OSC 7338 one). They go only on whole output
  lines that carried no escape of their own and on GLIMPS's own pinned error
  line, never for sensitive or bypassed commands, and detection never touches
  the filesystem.
- Flush any buffered tail on clean PTY EOF so user output is not silently lost.
- Release unterminated output after a short quiet interval so interactive
  questions and password prompts are visible before input is expected.

## Trust And Privacy

- Do not log terminal contents.
- Do not store terminal contents.
- Do not upload terminal contents.
- Do not add telemetry, crash upload, analytics, or network I/O to normal
  runtime behavior.
- Keep `GLIMPS=0` and `enabled = false` working as startup-time off switches.
- Do not rewrite a user's prompt.
- Completion notices (`[notify]`) are opt-in and are the only GLIMPS text that
  can leave the terminal for the desktop. They carry a program name, its subcommand
  only for well-known tools (`cargo build`), an exit code and a duration,
  never other arguments or output,
  and are never sent for sensitive or bypassed commands.
- Never enable terminal focus reporting (`CSI ?1004h`) or any other mode whose
  replies arrive as input: they would reach the shell and full-screen programs
  as keystrokes.
- Screen-share redaction (`[redact]`, `glimps redact on`) is the one feature
  allowed to change shown bytes, and only when the user turns it on: it
  replaces whole characters of a detected secret with `*` (as many as the
  columns they took) and never touches escape sequences or control bytes.
  With it on, sensitive commands stream as plain lines with every complete
  line masked, and no non-binary output falls back to raw pass-through (a
  pause or an oversized document keeps streaming through the masked path).
  Its detection must stay linear in the line length. Turning it off requires
  the trusted metadata channel's command record (when there is one), and
  GLIMPS itself confirms every toggle. It must always be described as best
  effort, with what it cannot reach.
- Sensitive credential commands remain raw pass-through (unless redaction
  is on, as above). A deliberately read
  dotenv file may receive byte-preserving ANSI spans, but its output must never
  feed error pins or be copied into GLIMPS-authored summaries.

## Resource Bounds

- Keep output buffering bounded by config limits clamped to safe ranges.
- Stream unbounded plain-text output line-by-line.
- Prefer zero-copy pass-through when no formatting work is needed.

The best GLIMPS bug is the one the user never notices because GLIMPS quietly
decided not to touch the output.
