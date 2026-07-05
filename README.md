# GLIMPS

**Zero-config smart terminal output formatter.** GLIMPS wraps your shell in a
PTY and quietly improves the scrollback you already have. It repeats your
command above its output so you can find what you ran, then formats output it
recognizes: JSON, HTML, logs, HTTP responses, diffs, stack traces, Git output,
tables, and common project files. No manual piping, no flags, no guessing what
kind of output is coming.

> Status: **beta** — functional and heavily tested. macOS + zsh today; Linux is a
> supported build target (CI covers Linux + macOS). Builds and runs **identically
> on Apple Silicon and Intel Macs** — same install, no architecture-specific
> steps. Homebrew packaging and broader shell support are on the roadmap.

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
- bash/fish shell integration

Those should not be advertised as available until the release/tap flow is tested
from a real version tag.

## Enable zsh Integration

After installing the binary, add one guarded line to your `~/.zshrc`:

```bash
echo 'command -v glimps >/dev/null 2>&1 && eval "$(glimps init zsh)"' >> ~/.zshrc
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

- zsh is the only shell integration today. Bash and fish are planned, but not
  public-beta blockers.
- Homebrew and crates.io installs are not live yet. Use the repo-local dogfood
  session or `cargo install --path .` from a checkout until the tap/release flow
  is verified from a real version tag.
- The current formatter handles whole JSON/HTML/diff/HTTP-response documents,
  streaming log/HTTP/stack-trace lines, and command-aware `cd`, `find`, `ls`,
  `du`, `df`, `ps`, `dig`/`nslookup`, `man`/help output, Markdown project files,
  YAML/TOML/INI/dotenv-style config files, CSV/TSV files, SQL query files,
  JSON-lines streams/files, common source-code extensions shown through reader
  commands, common database CLI result tables, and Git status/branch/log/stat
  output. It also displays command exit code, duration, and failure summaries
  when the shell integration provides the command-end marker.
  Mixed-content output, such as JSON embedded inside non-JSON log lines, is
  planned later.

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
| [`CONTRIBUTING.md`](./CONTRIBUTING.md) | Contributor setup and review expectations |
| [`docs/COMPETITIVE_PRODUCT_GAP_ANALYSIS.md`](./docs/COMPETITIVE_PRODUCT_GAP_ANALYSIS.md) | Competitor lessons and product gap roadmap |
| [`docs/FORMATTER_DESIGN_GUIDE.md`](./docs/FORMATTER_DESIGN_GUIDE.md) | Rules for adding safe formatters |
| [`docs/GOOD_FIRST_ISSUES.md`](./docs/GOOD_FIRST_ISSUES.md) | Copy-ready beginner issue specs |
| [`docs/LAUNCH_HARDENING_CHECKLIST.md`](./docs/LAUNCH_HARDENING_CHECKLIST.md) | Public-beta hardening checklist |
| [`docs/FRESH_MAC_DOGFOOD.md`](./docs/FRESH_MAC_DOGFOOD.md) | Fresh-machine dogfood procedure |
| [`docs/PUBLIC_BETA_RELEASE_RUNBOOK.md`](./docs/PUBLIC_BETA_RELEASE_RUNBOOK.md) | Maintainer release and Homebrew verification runbook |
| [`docs/SAFETY_INVARIANTS.md`](./docs/SAFETY_INVARIANTS.md) | Public safety invariants |
| `src/pty.rs` | The PTY supervisor |
| `src/format/` | All output transforms (the one formatting seam) |

## Build & test

```bash
cargo build --release
cargo test --all          # unit + property + golden + corpus tests
cargo bench               # latency benchmarks
scripts/release-readiness.sh
```

## License

MIT — see [`LICENSE`](./LICENSE).
