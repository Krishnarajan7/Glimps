# GLIMPS

**Zero-config smart terminal output formatter.** GLIMPS runs your shell inside a
PTY it owns and does two things automatically: it shows **where your input is** —
repeating your command, syntax-colored, right above its output — and it
**reformats recognized content** (JSON, HTML, logs, HTTP status) with structure
and color. No manual piping, no flags, no remembering what's coming.

> Status: **beta** — functional and heavily tested. macOS + zsh today; Linux is a
> supported build target (CI covers Linux + macOS). Builds and runs **identically
> on Apple Silicon and Intel Macs** — same install, no architecture-specific
> steps. Homebrew packaging and broader shell support are on the roadmap.

<!-- DEMO: render demo/glimps.tape with VHS (see demo/README.md) to produce
     demo/glimps.gif, then replace this comment with:
     ![GLIMPS in action](demo/glimps.gif)
     The static example below is the same idea. -->

## What it looks like

The problem GLIMPS was built to solve first: in a long scrollback you can't find
your own command. So before a command's output, GLIMPS prints a **header bar** —
your command, repeated and syntax-colored — so your input is never lost in the
wall of output. When it also recognizes the output (here, JSON), it pretty-prints
it with a content badge:

```
$ curl -s api.example.com/user
▌ curl -s api.example.com/user                       14:23:01
 JSON
{
  "login": "octocat",
  "id": 1,
  "admin": true,
  "plan": { "name": "pro", "seats": 10 }
}
```

The `▌` line is GLIMPS marking where output begins (command name, strings, and
flags each colored); the timestamp is optional. Logs get severity coloring as
they stream (great for `tail -f`); HTTP status lines are colored by class; long
HTML becomes an indented tree; unified diffs get added/removed/hunk coloring; and
stack traces / panics are highlighted so the failure jumps out. Anything GLIMPS
doesn't recognize — `ls`, `git`, `vim`, **a binary file** — passes through
**exactly** as it would without GLIMPS (binary and already-colored output are
detected and never touched).

## Why

Terminal output is a flat monochrome wall. Scroll up after a few commands and you
can't tell where your command ended and its output began — let alone read a 200-line
JSON blob. Tools like `jq`, `bat`, and `glow` each fix one format but make you know
what's coming and pipe it yourself. GLIMPS does it automatically, for everything,
transparently: it marks your input with a colored command header and reformats the
output it recognizes — because it sits on the output stream itself, not behind a
manual pipe.

## Install

**From source** (works today; requires [Rust](https://rustup.rs)):

```bash
git clone https://github.com/Krishnarajan7/Glimps
cd Glimps
cargo install --path .
```

This is the same on **Apple Silicon and Intel Macs** (and on Linux) — `cargo`
builds a native binary for whatever machine you run it on; there are no
architecture-specific steps or separate downloads.

**Homebrew** (planned for the 1.0 release):

```bash
brew install glimps   # not yet available
```

## Enable

Add one line to your `~/.zshrc`:

```bash
echo 'eval "$(glimps init zsh)"' >> ~/.zshrc
```

Restart your terminal. That's it. The snippet re-execs your interactive shell
inside GLIMPS once per session and installs the
[OSC-133](https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md)
shell-integration markers GLIMPS uses to tell your prompt and typed input apart
from command output. It never touches your prompt.

Prefer not to touch `.zshrc`? Just run `glimps` to start a wrapped shell, and
`exit` to leave.

## Configuration

GLIMPS works with no config. To customize, copy
[`.glimpsrc.example`](./.glimpsrc.example) to `~/.glimpsrc`:

```toml
color = true        # false = structure but no color
separator = true    # the ▌ command header above each command's output
timestamp = true    # HH:MM:SS shown in the header

[formatters]
json = true
html = true
logs = true         # ERROR/WARN/INFO/DEBUG coloring
http = true         # HTTP status coloring
diff = true         # unified-diff coloring (added/removed/hunk lines)
stacktrace = true   # stack-trace / panic highlighting (Rust, Python)

[limits]
buffer_cap = 1048576   # bytes buffered to detect JSON/HTML
line_cap   = 65536     # max streamed line length
sniff_cap  = 64
```

A missing or broken `~/.glimpsrc` falls back to defaults (GLIMPS warns once and
keeps going). Set `GLIMPSRC` to use a different path. See
[`.glimpsrc.example`](./.glimpsrc.example) for the annotated reference.

## Privacy & safety

GLIMPS sits in front of *everything* you type and see — including secrets, SSH
sessions, and password prompts. That's a position of trust, and it's built to
earn it. These are hard rules enforced in the code:

- **Nothing is logged, stored, or transmitted.** No telemetry, ever. GLIMPS only
  moves bytes between your shell and your screen.
- **Default to pass-through.** When content type is uncertain, GLIMPS does
  nothing. It only reformats output it's confident about; everything else is
  byte-for-byte unchanged.
- **Never touches** binary output, already-colored output, no-echo password
  prompts, full-screen apps (vim/less/htop/fzf), or output that isn't going to a
  terminal (piped/redirected).
- **The terminal is always restored** on exit — including on crash.
- **Instant off switch.** Set `GLIMPS=0` in your environment to skip wrapping and
  formatting, or `enabled = false` in `~/.glimpsrc` to turn it off persistently.

```bash
GLIMPS=0 zsh     # a raw, unwrapped shell
export GLIMPS=0  # disable GLIMPS for this and future shells
```

## Uninstall

1. Remove the line from `~/.zshrc`:
   ```bash
   sed -i '' '/glimps init zsh/d' ~/.zshrc   # macOS
   ```
2. Remove the binary: `cargo uninstall glimps` (or delete it from your `PATH`).
3. Optionally delete `~/.glimpsrc`.

Restart your terminal. Fully gone.

## How it works (the honest version)

GLIMPS is a **PTY session supervisor**, like ChromaTerm / `script` / `tmux` —
*not* a shell hook. zsh's `preexec`/`precmd` hooks run before/after a command and
can't intercept its output; owning the PTY can. GLIMPS reads the raw output
stream, uses OSC-133 markers to find exactly where command output begins and
ends, and reformats only that — never your prompt or input. Full rationale in
[`GLIMPS-PLAN.md`](./GLIMPS-PLAN.md).

| File | What |
|---|---|
| [`GLIMPS-PLAN.md`](./GLIMPS-PLAN.md) | R&D findings, feasibility, tech-stack rationale |
| [`ROADMAP.md`](./ROADMAP.md) | Versioned plan (v0.1 → v2.0) |
| [`CLAUDE.md`](./CLAUDE.md) | Engineering charter & safety invariants |
| `src/pty.rs` | The PTY supervisor |
| `src/format/` | All output transforms (the one formatting seam) |

## Build & test

```bash
cargo build --release
cargo test --all          # unit + property + golden + corpus tests
cargo bench               # latency benchmarks
```

## License

MIT — see [`LICENSE`](./LICENSE).
