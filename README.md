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

<p align="center">
  <a href="https://github.com/Krishnarajan7/Glimps/actions/workflows/ci.yml"><img src="https://github.com/Krishnarajan7/Glimps/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://crates.io/crates/glimps"><img src="https://img.shields.io/crates/v/glimps.svg" alt="crates.io"></a>
  <a href="https://github.com/Krishnarajan7/Glimps/releases/latest"><img src="https://img.shields.io/github/v/release/Krishnarajan7/Glimps?include_prereleases" alt="GitHub release"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey" alt="macOS and Linux">
</p>

**Zero-config smart terminal output formatter.** GLIMPS wraps your shell in a
PTY and quietly improves the scrollback you already have. It repeats your
command above its output so you can find what you ran, then formats output it
recognizes: JSON, HTML, logs, HTTP responses, diffs, stack traces, Git output,
tables, and common project files. No manual piping, no flags, no guessing what
kind of output is coming.

> Status: **public beta** — functional and heavily tested. macOS + zsh is the
> primary early-adopter path; bash integration is beta, and Linux is a supported
> build target covered by CI. Prebuilt binaries for Apple Silicon and Intel
> (macOS and Linux) ship with each release, alongside Homebrew and crates.io
> packages. Broader shell support (fish) is on the roadmap.

> **Want to help?** GLIMPS is beta and there's real, scoped work with clear
> acceptance criteria waiting for you. Browse the
> [`good first issue` list](https://github.com/Krishnarajan7/Glimps/issues?q=is%3Aissue%20state%3Aopen%20label%3A%22good%20first%20issue%22),
> start with [issue #1](https://github.com/Krishnarajan7/Glimps/issues/1), and
> ask questions in [Discussions](https://github.com/Krishnarajan7/Glimps/discussions/12).
> Most tasks teach GLIMPS one more small output type and don't require touching
> the PTY internals.

![GLIMPS in action](demo/glimps.gif)

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
Git output, CSV/TSV/PSV, SQL, JSON-lines, source files, config files, and database
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
printf 'name|region|active\nAda|europe|true\n' > /tmp/glimps-users.psv
cat /tmp/glimps-users.psv
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

## When something breaks

Formatting is the part you see first. This is the part that only GLIMPS can do.

Because GLIMPS owns the PTY and reads OSC-133 markers, it knows where a command
started, where its output ended, what it exited with, and how long it took. Tools
that sit downstream of the PTY see bytes; they never see that boundary. So GLIMPS
can add a footer under failed output that says what broke and where:

![GLIMPS failure intelligence](demo/failure.gif)

Four things are happening there:

- **Exit codes are translated.** `127` becomes `command not found on PATH`. `137`
  becomes `SIGKILL: force-killed, often out of memory`. See
  [`src/format/exitcode.rs`](./src/format/exitcode.rs) for the full dictionary.
- **Pipeline failures stop hiding.** `false | wc -l` exits `0`, because the shell
  reports only the last stage. GLIMPS reads the whole pipeline status array and
  warns that stage 1 failed.
- **Non-zero is not always failure.** Ctrl-C exits `130`, and GLIMPS calls it
  `interrupted`, in a notice color — not a red error.
- **The error line gets pinned.** When the thing that actually broke has scrolled
  out of reach, GLIMPS repeats it under the footer with `file:line` and how far up
  it was, so you don't page back through a test run to find it.

None of this is AI, telemetry, or guesswork: it decodes an exit code the shell
already produced and quotes a line that is already on your screen. The footer is
purely additive — it never rewrites command output, which is what keeps the
byte-safety promise in [`docs/SAFETY_INVARIANTS.md`](./docs/SAFETY_INVARIANTS.md)
intact. Turn any of it off in `.glimpsrc` under `[failures]`.

## Why

Most terminal helpers ask you to predict the output first. Use `jq` if it is
JSON. Use `bat` if it is a file. Use a pager or a Git tool if you remembered in
time. Those tools are great, but the normal shell loop is messier than that.

GLIMPS lives one layer lower. It sees the command output as it happens, marks the
boundary, and formats only the parts it understands. The goal is not to replace
your favorite CLI tools. The goal is to make the default terminal experience
less punishing when you did not know you needed them.

## How GLIMPS compares

Every tool below is excellent at its job — GLIMPS borrows lessons from all of
them. The difference is *when they run*: they run when you remember to invoke
them, GLIMPS runs on everything automatically because it owns the PTY.

| Tool | What it is | How GLIMPS differs |
|---|---|---|
| [ChromaTerm](https://github.com/hSaria/ChromaTerm) | PTY wrapper that colors output via user-defined regex rules | Same architecture, different brain: GLIMPS ships zero-config structural parsers (JSON, HTTP, diffs, tables) instead of asking you to write regex rules, and uses OSC-133 to know exactly where output begins so it never touches your prompt |
| [grc](https://github.com/garabik/grc) | Per-command colorizer with a config ecosystem | grc must be prefixed per command (`grc ping …`) or aliased per tool; GLIMPS wraps the whole session once and auto-detects both commands and content types |
| [bat](https://github.com/sharkdp/bat) / [jq](https://github.com/jqlang/jq) / [fx](https://github.com/antonmedv/fx) | Superb viewers for files and JSON | You pipe into them *after* predicting the output type; GLIMPS formats output you didn't predict, and gets out of the way when you do pipe (non-TTY output is never touched) |
| [delta](https://github.com/dandavison/delta) | Best-in-class git diff pager | Configured per tool (git); GLIMPS colors diffs, logs, and stack traces from *any* command with no per-tool setup |
| [lnav](https://github.com/tstack/lnav) | Powerful log-file navigator TUI | lnav is a destination you open; GLIMPS colors log severity live in your normal scrollback |

And the part none of them can do from downstream of the PTY: because GLIMPS owns
the session, it knows each command's exit status, duration, and pipeline stages —
that's what powers the failure footers above.

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
the recommended first test on a Mac. Dogfood command history is kept separately
at `${XDG_STATE_HOME:-$HOME/.local/state}/glimps/dogfood_history`, so it survives
normal exits without modifying your regular zsh history.

After editing GLIMPS source, rebuild and resume the active dogfood session with:

```bash
glimps-update
```

The command restarts the wrapper with the new binary while preserving dogfood
history and the current working directory. Running jobs and unsaved shell-local
variables cannot cross that controlled restart.

## Install

**Homebrew (macOS / Linuxbrew):**

```bash
brew install Krishnarajan7/tap/glimps
```

**Shell installer** (prebuilt binaries for Apple Silicon and Intel, macOS and
Linux, with [build provenance attestations](https://github.com/Krishnarajan7/Glimps/attestations)):

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Krishnarajan7/Glimps/releases/latest/download/glimps-installer.sh | sh
```

**From crates.io** (requires [Rust/Cargo](https://rustup.rs)):

```bash
cargo install glimps
```

**From source:**

```bash
git clone https://github.com/Krishnarajan7/Glimps
cd Glimps
cargo install --path .
```

All paths produce the same single native binary. See the
[compatibility matrix](./docs/COMPATIBILITY.md) for what has been physically
verified. fish shell integration is not shipped yet (zsh is primary, bash is
beta — see [Known beta limits](#known-beta-limits)).

## Enable Shell Integration

The guided way — it shows you the exact change, asks before touching anything,
and backs up your rc file first:

```bash
glimps setup
```

Or add the line yourself: one guarded line **near the top** of your rc
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
color = true        # false = structure but no color (NO_COLOR also honored)
theme = "dark"      # "light" swaps in a palette tuned for light backgrounds
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
pretty_max_lines = 4000 # pass through instead of pretty-printing past this
```

A missing or broken `~/.glimpsrc` falls back to defaults (GLIMPS warns once and
keeps going). Set `GLIMPSRC` to use a different path. See
[`.glimpsrc.example`](./.glimpsrc.example) for the annotated reference.

## Privacy & safety

GLIMPS sits in front of *everything* you type and see — including secrets, SSH
sessions, and password prompts. That's a position of trust, and it's built to
earn it. These are hard rules enforced in the code:

- **Nothing is transmitted or persistently logged.** No telemetry, analytics,
  crash upload, or network calls. A private temporary file carries command,
  working-directory, and exit-status metadata from the shell hooks to the local
  supervisor; it is truncated as records are consumed and removed when the
  GLIMPS session ends. Command output is never written to it.
- **Default to pass-through.** When content type is uncertain, GLIMPS does
  nothing. It only reformats output it's confident about; everything else is
  byte-for-byte unchanged.
- **Never touches** binary output, already-colored output, no-echo password
  prompts, full-screen apps (vim/less/htop/fzf), or output that isn't going to a
  terminal (piped/redirected).
- **Secret-printing commands pass through raw.** Known credential readers such
  as Keychain password reveal commands, `gh auth token`, password-manager CLIs,
  cloud secret fetches, and direct reads of common secret files are not
  formatted, pinned, or quoted in failure summaries. Deliberately reading a
  dotenv file with `cat`, `head`, `tail`, or `sed` is the narrow exception:
  GLIMPS may add byte-preserving ANSI color to that requested text, but disables
  error pinning so values are never duplicated into its own summaries.
- **The terminal is always restored** on exit — including on crash.
- **Simple off switches, at every scope.** `glimps off` pauses formatting for
  the current session instantly (`glimps on` resumes) — no restart needed.
  `GLIMPS=0` starts a raw shell. `enabled = false` in `~/.glimpsrc` turns it
  off persistently.

```bash
glimps off       # pause formatting in this session (glimps on resumes)
GLIMPS=0 zsh     # start a raw, unwrapped shell
export GLIMPS=0  # keep future shells raw from this environment
```

## FAQ

**Doesn't putting a formatter between me and my shell add latency?**
Not measurably. Typed input is written straight to the PTY — keystrokes are
never routed through the formatter. On the output side, detection is a single
O(n) scan with early exit, and the pass-through path benchmarks in the hundreds
of MiB/s (365 MiB/s on the dev Apple Silicon machine — reproduce with
`cargo bench`). Heavier whole-document formatting only ever runs on bounded,
fully-buffered output; the PTY read/write loops never block on it. CI runs the
benchmarks against latency budgets so regressions can't merge.

**Something looks mangled — how do I get it out of the way right now?**
`glimps off` — formatting pauses instantly for the session, `glimps on` brings
it back. Please also [open an issue](https://github.com/Krishnarajan7/Glimps/issues/new/choose)
with the command; "never mangle output" is a hard invariant here, so those
reports jump the queue.

**I use a light terminal background.**
Set `theme = "light"` in `~/.glimpsrc` — same semantic colors, darker tones.

**Does it respect NO_COLOR?**
Yes. With `NO_COLOR` set (any non-empty value), GLIMPS keeps structure —
indentation, command headers, badges — but emits no color escapes. Same as
`color = false` in `~/.glimpsrc`.

**What about a giant JSON response?**
If pretty-printing would inflate a document past `pretty_max_lines` (default
4000 lines), GLIMPS shows the original bytes instead — it never floods your
scrollback to make a point, and it never truncates or hides output.

**Why does the README say to put the integration line near the top of my rc?**
The line re-execs your shell inside GLIMPS, and the re-exec'd shell re-sources
the same rc — so anything *above* the line runs twice per session. `glimps
doctor` warns if the line sits below a plugin manager or prompt framework.

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
- Release verification is young. Homebrew, the shell installer, and crates.io
  went live with v0.1.0; if an install path misbehaves on your machine, the
  repo-local dogfood session (`scripts/dogfood-macos.sh session`) and
  `cargo install --path .` from a checkout always work — and please
  [open an issue](https://github.com/Krishnarajan7/Glimps/issues/new/choose).
- **Pipes and stdout redirects turn command-aware views off.** With
  `lsof -i | grep LISTEN` the text on screen is `grep`'s output, and with
  `lsof > out.txt` the only thing reaching the terminal is stderr, so applying
  `lsof`'s view to either would color bytes it does not own. Silencing *stderr*
  is fine and keeps formatting: `lsof 2>/dev/null`, `2>>file` and `2>&1` all
  still get the full view. Whole-document formatting (JSON, HTML) and the
  streaming log/HTTP/stack-trace coloring are unaffected either way.
- The current formatter handles whole JSON/HTML/diff/HTTP-response documents,
  streaming log/HTTP/stack-trace lines, and command-aware `cd`, `find`, `ls`,
  `du`, `df`, `ps`, `ping`, `dig`/`nslookup`, macOS networking output (`ifconfig`,
  `scutil --dns`, `route get default`, `netstat -rn`, `networksetup`),
  open files and sockets (`lsof` and its flags, read from the table schema each
  invocation prints), macOS disk and file metadata (`diskutil info`, `GetFileInfo`, `xattr -l`), system status
  output (`launchctl list`, `pmset -g`),
  `man`/help output and manual-index searches (`whatis`, `apropos`, `man -k`, `man -f`), Markdown project files, YAML/TOML/INI/dotenv-style config
  files, `.gitignore` patterns, `.gitleaksignore` fingerprints, adaptive CSV/TSV/PSV tables, SQL query files,
  JSON-lines streams/files, common source-code extensions shown through reader
  commands, common database CLI result tables, and Git status/branch/log/stat
  output. It also displays
  command exit code, duration, success breadcrumbs for recognized silent actions
  such as `cd`, `touch`, `mkdir`, `rm`, and `killall`, and failure summaries
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

GLIMPS is a **PTY session supervisor**, like ChromaTerm / `script` / `tmux`,
assisted by lightweight shell hooks. The hooks report command boundaries,
working directory, and status; they do not intercept command output. Owning the
PTY is what lets GLIMPS read the raw output stream and reformat only the command
output zone — never your prompt or input. Full rationale in
[`GLIMPS-PLAN.md`](./GLIMPS-PLAN.md).

| File | What |
|---|---|
| [`GLIMPS-PLAN.md`](./GLIMPS-PLAN.md) | R&D findings, feasibility, tech-stack rationale |
| [`ROADMAP.md`](./ROADMAP.md) | Versioned plan (v0.1 → v2.0) |
| [`CHANGELOG.md`](./CHANGELOG.md) | Release history |
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
- **Try it like a user:** `scripts/dogfood-macos.sh session` wraps a temporary
  zsh configuration and cleans it up on exit. It won't touch your `~/.zshrc`;
  dogfood history is retained in a separate state file for the next session.

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
