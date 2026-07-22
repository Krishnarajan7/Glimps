<p align="center">
  <img src="site/public/favicon.svg" width="96" height="96" alt="GLIMPS logo">
</p>

<h1 align="center">GLIMPS</h1>

<p align="center">
  <a href="https://glimpps.netlify.app/">Website</a>
  ·
  <a href="https://github.com/Krishnarajan7/Glimps/discussions/12">Discussions</a>
  ·
  <a href="https://github.com/Krishnarajan7/Glimps/issues/1">Start contributing</a>
  ·
  <a href="https://github.com/Krishnarajan7/Glimps/issues?q=is%3Aissue%20state%3Aopen%20label%3A%22good%20first%20issue%22">Good first issues</a>
</p>

**Zero-config smart terminal output formatter.** GLIMPS wraps your shell in a
PTY and quietly improves the scrollback you already have. It repeats your
command above its output so you can find what you ran, then formats output it
recognizes: JSON, HTML, logs, HTTP responses, diffs, stack traces, Git output,
tables, and common project files. No manual piping, no flags, no guessing what
kind of output is coming.

> Status: **beta** — functional and heavily tested. macOS + zsh/bash today; Linux is a
> supported build target (CI covers Linux + macOS). Builds and runs **identically
> on Apple Silicon and Intel Macs** — same install, no architecture-specific
> steps. Homebrew packaging and broader shell support are on the roadmap.

> **Want to help?** GLIMPS is beta and there's real, scoped work with clear
> acceptance criteria waiting for you. Browse the
> [`good first issue` list](https://github.com/Krishnarajan7/Glimps/issues?q=is%3Aissue%20state%3Aopen%20label%3A%22good%20first%20issue%22),
> start with [issue #1](https://github.com/Krishnarajan7/Glimps/issues/1), and
> ask questions in [Discussions](https://github.com/Krishnarajan7/Glimps/discussions/12).
> Most tasks teach GLIMPS one more small output type and don't require touching
> the PTY internals.

<!-- DEMO: run scripts/render-demo.sh (see demo/README.md) to produce
     demo/glimps.gif from demo/glimps.tape, then replace this comment with:
     ![GLIMPS in action](demo/glimps.gif)
     The static example below is the same idea. -->

## What it looks like

The first problem GLIMPS solves is painfully ordinary: after a few commands,
scrollback turns into a wall. You know you ran the thing, but finding where its
output began is annoying.

So GLIMPS prints a small **header bar** before command output. The command is
repeated there, syntax-colored, with an optional timestamp. If the output is a
known format, GLIMPS also makes it readable. Here is the basic idea with JSON:

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

The `▌` line is GLIMPS marking where output begins. Logs get severity coloring
as they stream. HTTP responses are split into status, headers, cookies,
redirects, and body. Long HTML becomes an indented tree. Diffs, stack traces,
Git output, CSV/TSV, SQL, JSON-lines, source files, config files, and database
tables get focused formatting too.

Just as important: output GLIMPS should not touch is left alone. Full-screen
apps like `vim`, SSH sessions, binary output, and already-colored output pass
through as normal. If GLIMPS is not confident, it gets out of the way.

Try these inside a GLIMPS session:

```bash
echo '{"alpha":1,"items":[2,3]}'
printf 'INFO boot\nWARN disk\nERROR boom\n'
printf 'HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{"ok":true}\n'
printf '<!doctype html><html><head><title>Glimps</title></head><body><h1>Hello</h1></body></html>\n'
printf 'Traceback (most recent call last):\n  File "app.py", line 7, in <module>\nValueError: broken config\n'
printf 'name,age,active\nAda,37,true\n"Lovelace, Ada",12,false\n' > /tmp/glimps-users.csv
cat /tmp/glimps-users.csv
printf 'CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);\nSELECT * FROM users WHERE id = 42;\n' > /tmp/glimps-schema.sql
cat /tmp/glimps-schema.sql
sqlite3 -header -column :memory: 'CREATE TABLE users(id INTEGER, name TEXT, active TEXT); INSERT INTO users VALUES (1,"Ada","true"); SELECT * FROM users;'
printf '{"level":"info","count":2}\n{"level":"error","ok":false}\n' > /tmp/glimps-events.jsonl
cat /tmp/glimps-events.jsonl
printf '// GLIMPS source sample\npub fn main() {\n    let answer = 42;\n    println!("ok");\n}\n' > /tmp/glimps-main.rs
cat /tmp/glimps-main.rs
printf '# deploy helper\ndef greet(name):\n    return f"hi {name}"\n' > /tmp/glimps-app.py
head -20 /tmp/glimps-app.py
cat README.md
cat Cargo.toml
cd docs
find src -maxdepth 2 -type f
ls -la
du -sh src tests .
df -h
ps aux | head -5
dig 360astra.io
false
git status --short
git --no-pager log --oneline --decorate -5
git branch -a
git --no-pager diff --stat
git --no-pager diff --numstat
git --no-pager diff --name-status
man printf
git --no-pager diff -- README.md
```

## Why

Most terminal helpers ask you to predict the output first. Use `jq` if it is
JSON. Use `bat` if it is a file. Use a pager or a Git tool if you remembered in
time. Those tools are great, but the normal shell loop is messier than that.

GLIMPS lives one layer lower. It sees the command output as it happens, marks the
boundary, and formats only the parts it understands. The goal is not to replace
your favorite CLI tools. The goal is to make the default terminal experience
less punishing when you did not know you needed them.

## Try Without Installing

Want to see the real terminal behavior before changing your shell startup files?
Use the repo-local dogfood session:

```bash
git clone https://github.com/Krishnarajan7/Glimps
cd Glimps
scripts/dogfood-macos.sh session
```

That builds `target/debug/glimps`, starts a wrapped zsh using a temporary
`ZDOTDIR`, and cleans up when you exit. It does **not** install GLIMPS globally,
does **not** edit `~/.zshrc`, and does **not** change your login shell. This is
the recommended first test on both Apple Silicon and Intel Macs.

## Install From Source

This is the supported install path today. It requires
[Rust/Cargo](https://rustup.rs).

```bash
git clone https://github.com/Krishnarajan7/Glimps
cd Glimps
cargo install --path .
```

`cargo install --path .` builds a native binary for the machine you run it on.
The steps are the same on Apple Silicon Macs, Intel Macs, and Linux.

Not shipped yet:

- `brew install glimps`
- `cargo install glimps` from crates.io
- fish shell integration

Those should not be advertised as available until the release/tap flow is tested
from a real version tag.

## Enable Shell Integration

After installing the binary, add one guarded line **near the top** of your rc
file — `~/.zshrc` for zsh, `~/.bashrc` for bash:

```bash
# zsh: near the top of ~/.zshrc
command -v glimps >/dev/null 2>&1 && eval "$(glimps init zsh)"
```

```bash
# bash: near the top of ~/.bashrc
command -v glimps >/dev/null 2>&1 && eval "$(glimps init bash)"
```

Restart your terminal. That's it. The snippet re-execs your interactive shell
inside GLIMPS once per session and installs the
[OSC-133](https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md)
shell-integration markers GLIMPS uses to tell your prompt and typed input apart
from command output. It never touches your prompt.

> **Why "near the top"?** The snippet re-execs your shell *inside* GLIMPS, and
> the re-exec'd shell re-sources the same rc file. Anything **above** the line
> runs in the throwaway outer shell *and again* inside GLIMPS; anything **below**
> it runs only once. Put it high (after any critical `PATH`/env setup, before
> plugin managers and prompt frameworks) so your rc isn't run twice per session.
> Login files like `.zprofile`/`.bash_profile` are **not** re-run — the inner
> shell is interactive and inherits your environment.

Prefer not to touch your rc file? Just run `glimps` to start a wrapped shell, and
`exit` to leave.

## Diagnose Your Setup

After installing or changing shell integration, run:

```bash
glimps doctor
```

The doctor checks the installed binary, supported shell, rc-file integration,
configuration syntax, `PATH`, TTY/`TERM` state, active-session flags, and the
private metadata channel. It is read-only: it does not edit shell files, install
anything, or make network requests. Warnings describe non-fatal conditions;
failed checks make the command exit with status 1 so it also works in scripts.

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
- **Secret-printing commands pass through raw.** Known credential readers such
  as Keychain password reveal commands, `gh auth token`, password-manager CLIs,
  cloud secret fetches, and direct reads of common secret files are not
  formatted, pinned, or quoted in failure summaries.
- **The terminal is always restored** on exit — including on crash.
- **Simple off switch.** Start a shell with `GLIMPS=0` to skip wrapping, or set
  `enabled = false` in `~/.glimpsrc` to turn it off persistently for new
  sessions.

```bash
GLIMPS=0 zsh     # start a raw, unwrapped shell
export GLIMPS=0  # keep future shells raw from this environment
```

If you're already inside a GLIMPS-wrapped shell, run `exit` first, then start a
new raw shell with `GLIMPS=0 zsh`.

## Known beta limits

- zsh and bash shell integration are supported today. fish is planned, but not a
  public-beta blocker.
- **bash integration is beta.** It uses a `DEBUG` trap (bash has no native
  `preexec`). GLIMPS chains any `DEBUG` trap that was installed *before* its line,
  and tools built on `bash-preexec` (atuin, etc.) chain GLIMPS the same way — but
  a tool that installs a *raw* `DEBUG` trap *below* the GLIMPS line will override
  it and quietly stop the output markers. If you use such a tool, put the GLIMPS
  line after it. The command captured for the header is the full history line, so
  it needs interactive history enabled (the default).
- Homebrew and crates.io installs are not live yet. Use the repo-local dogfood
  session or `cargo install --path .` from a checkout until the tap/release flow
  is verified from a real version tag.
- The current formatter handles whole JSON/HTML/diff/HTTP-response documents,
  streaming log/HTTP/stack-trace lines, and command-aware `cd`, `find`, `ls`,
  `du`, `df`, `ps`, `dig`/`nslookup`, macOS networking output (`ifconfig`,
  `scutil --dns`, `route get default`, `netstat -rn`, `lsof -i`,
  `networksetup`), system status output (`launchctl list`, `pmset -g`),
  `man`/help output, Markdown project files, YAML/TOML/INI/dotenv-style config
  files, CSV/TSV files, SQL query files, JSON-lines streams/files, common
  source-code extensions shown through reader commands, common database CLI
  result tables, and Git status/branch/log/stat output. It also displays
  command exit code, duration, and failure summaries
  when the shell integration provides the command-end marker.
  Mixed-content output, such as JSON embedded inside non-JSON log lines, is
  planned later.

## Uninstall

1. Remove the line from your rc file:
   ```bash
   sed -i '' '/glimps init/d' ~/.zshrc    # zsh, macOS
   sed -i '' '/glimps init/d' ~/.bashrc   # bash, macOS
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
| [`CONTRIBUTING.md`](./CONTRIBUTING.md) | Contributor setup and review expectations |
| [`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md) | Community participation and enforcement rules |
| [`SECURITY.md`](./SECURITY.md) | Private vulnerability reporting and response policy |
| [`docs/COMPATIBILITY.md`](./docs/COMPATIBILITY.md) | Verified platform matrix and known beta issues |
| [`docs/REPOSITORY_SETTINGS.md`](./docs/REPOSITORY_SETTINGS.md) | Maintainer-only GitHub trust and ruleset setup |
| [`docs/COMPETITIVE_PRODUCT_GAP_ANALYSIS.md`](./docs/COMPETITIVE_PRODUCT_GAP_ANALYSIS.md) | Competitor lessons and product gap roadmap |
| [`docs/FORMATTER_DESIGN_GUIDE.md`](./docs/FORMATTER_DESIGN_GUIDE.md) | Rules for adding safe formatters |
| [`docs/GOOD_FIRST_ISSUES.md`](./docs/GOOD_FIRST_ISSUES.md) | Copy-ready beginner issue specs |
| [`docs/LAUNCH_HARDENING_CHECKLIST.md`](./docs/LAUNCH_HARDENING_CHECKLIST.md) | Public-beta hardening checklist |
| [`docs/FRESH_MAC_DOGFOOD.md`](./docs/FRESH_MAC_DOGFOOD.md) | Fresh-machine dogfood procedure |
| [`docs/PUBLIC_BETA_RELEASE_RUNBOOK.md`](./docs/PUBLIC_BETA_RELEASE_RUNBOOK.md) | Maintainer release and Homebrew verification runbook |
| [`docs/SAFETY_INVARIANTS.md`](./docs/SAFETY_INVARIANTS.md) | Public safety invariants |
| `src/pty.rs` | The PTY supervisor |
| `src/format/` | All output transforms (the one formatting seam) |

## Contributing

GLIMPS is small, sharp, and sits between a person and their shell — so a good
contribution makes output easier to read *without* making the terminal less
trustworthy. The single rule that matters most: **when GLIMPS is unsure, it gets
out of the way.**

The friendliest way in is to teach GLIMPS one more small, well-shaped output
type. Those tasks are labeled and scoped, and none of them require touching the
PTY supervisor, raw-mode handling, or the OSC-133 scanner:

- **Pick a task:** the [`good first issue` list](https://github.com/Krishnarajan7/Glimps/labels/good%20first%20issue).
  Each one names the files to touch, what "done" looks like, and what output your
  change must leave alone.
- **Read first:** [`CONTRIBUTING.md`](./CONTRIBUTING.md) and
  [`docs/FORMATTER_DESIGN_GUIDE.md`](./docs/FORMATTER_DESIGN_GUIDE.md).
- **Try it like a user:** `scripts/dogfood-macos.sh session` wraps a throwaway
  zsh and cleans up on exit — it won't touch your `~/.zshrc`.

Comment on an issue to claim it before you start. A ten-line question beats a
two-hundred-line PR that went the wrong way.

## Build & test

```bash
cargo build --release
cargo test --all          # unit + property + golden + corpus tests
cargo bench               # latency benchmarks
scripts/release-readiness.sh
```

## License

MIT — see [`LICENSE`](./LICENSE).
