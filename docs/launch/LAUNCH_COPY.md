# Launch Copy — ready to post

Draft copy for each venue in [`LAUNCH_CHECKLIST.md`](./LAUNCH_CHECKLIST.md).
Edit voice to taste, but keep two things in every post: the honest limits
(macOS + zsh primary, bash beta) and the privacy stance (no telemetry, ever) —
both earn more trust than any feature list.

---

## Show HN

**Title:**

> Show HN: GLIMPS – terminal output, formatted automatically (no piping)

**URL:** `https://github.com/Krishnarajan7/Glimps`

**First comment (post immediately after submitting):**

> Hi HN, author here.
>
> GLIMPS is a PTY session supervisor written in Rust: it runs your shell inside
> a pseudo-terminal it owns, watches the output stream, and reformats content it
> recognizes — JSON gets pretty-printed, logs get severity coloring, HTTP
> responses get split into status/headers/body, diffs, stack traces, tables and
> ~30 command-specific views (`ls`, `ps`, `dig`, `lsof`, …). No piping to jq/bat,
> no aliases, no per-command config.
>
> The technical core is knowing *where command output begins and ends*. Shell
> hooks like zsh's preexec/precmd can't intercept output — preexec fires before
> the command and precmd after the output is already on screen. So GLIMPS owns
> the PTY (the ChromaTerm/script/tmux model) and uses OSC-133 shell-integration
> markers to distinguish your prompt and typed input from command output. It
> never touches the prompt zone. Owning the PTY also means it knows each
> command's exit code, duration, and PIPESTATUS — so a failed command gets a
> footer translating the exit code (127 → "command not found on PATH"), flagging
> which pipeline stage actually failed, and re-quoting the error line that
> scrolled away, with file:line.
>
> Since it sits in front of everything you type and see, the safety rules are
> stricter than the features: when unsure, it passes bytes through untouched;
> it never reformats binary output, already-colored output, non-TTY/piped
> output, password prompts, or interactive apps (vim/ssh/htop); known
> secret-printing commands pass through raw; raw mode is restored on every exit
> path including panics; and there is zero telemetry or network I/O — the code
> treats that as a security boundary, not a settings toggle. GLIMPS=0 gives you
> a raw shell instantly.
>
> Honest limits: macOS + zsh is the primary path today; bash integration is
> beta (DEBUG-trap based); fish and Windows aren't supported yet; mixed content
> (JSON embedded in log lines) is still passed through.
>
> Install: brew install Krishnarajan7/tap/glimps, or cargo install glimps.
> I'd genuinely love to hear where it misbehaves — the safety invariants doc in
> the repo says what it must never do, and reports against those are gold.

---

## r/rust

**Title:**

> GLIMPS: a PTY session supervisor in Rust that auto-formats your terminal output (JSON, logs, HTTP, diffs) — no piping

**Body:**

> I've been building GLIMPS, a zero-config terminal output formatter. It wraps
> your shell in a PTY it owns, finds the command/output boundary via OSC-133
> shell-integration markers, and reformats output it recognizes — never your
> prompt or typed input.
>
> Rust-specific bits that might interest this sub:
>
> - **The hot path is a streaming byte scanner.** Detection is O(n) with early
>   exit; heavier formatting only runs on bounded, fully-buffered output. The
>   PTY read/write loops never block on formatting.
> - **Panic hygiene as a product requirement:** the supervisor and formatter
>   paths have no `.unwrap()`/`.expect()`/indexing panics (clippy `-D warnings`
>   in CI), because a panic here means a user's terminal is left in raw mode.
>   Raw-mode restore lives in a `Drop` guard so even a panic unwinds cleanly.
> - **Byte-safety is property-tested:** proptest proves formatters never drop,
>   reorder, or truncate bytes and never emit invalid UTF-8, on arbitrary
>   input; partial multibyte sequences across chunk boundaries are buffered,
>   not split. Golden-file tests cover every formatter.
> - Deps kept minimal: portable-pty, crossterm, signal-hook, serde_json
>   (preserve_order — never silently reorder a user's JSON keys), toml, time.
>
> It also does "failure intelligence": owning the PTY means it knows exit
> codes, duration, and PIPESTATUS, so failed commands get a footer explaining
> what broke (127 → command not found; which pipeline stage failed; the error
> line re-quoted with file:line if it scrolled away).
>
> Limits: macOS + zsh primary, bash beta, no fish/Windows yet. Zero telemetry,
> ever. MIT.
>
> Repo: https://github.com/Krishnarajan7/Glimps — contributions welcome, most
> good-first-issues are "teach it one more output type" and don't touch the PTY
> internals.

---

## r/commandline

**Title:**

> I got tired of piping to jq/bat after the fact, so I built a shell wrapper that formats output automatically

**Body (attach/upload `demo/glimps.gif` natively, don't just link):**

> GLIMPS wraps your shell in a PTY and formats output it recognizes as it
> appears: JSON, logs (severity coloring), HTTP responses, diffs, stack traces,
> CSV/TSV tables, and command-aware views for ls, ps, du, df, dig, lsof, man,
> git and more. It prints a small header above each command's output (the
> command, colored, with a timestamp) so scrollback stops being a wall.
>
> When a command fails it tells you what the exit code means, which pipeline
> stage actually failed (false | wc -l "succeeds" otherwise), and re-quotes the
> error line that scrolled off screen.
>
> The rules I held myself to: pass-through by default when unsure; never touch
> vim/ssh/htop/binary/piped output or password prompts; no telemetry ever;
> GLIMPS=0 turns it all off. macOS + zsh is the primary path right now (bash is
> beta, fish planned).
>
> brew install Krishnarajan7/tap/glimps · cargo install glimps
> https://github.com/Krishnarajan7/Glimps

---

## Lobsters

**Title:** `GLIMPS: terminal output, formatted automatically (no piping)`
**URL:** repo. **Tags:** `rust`, `unix`, `show`.
First comment: reuse the Show HN comment, trimmed.

---

## This Week in Rust (PR to rust-lang/this-week-in-rust, draft for next issue)

Updates section entry:

> * [GLIMPS](https://github.com/Krishnarajan7/Glimps) v0.1.0 — a zero-config
>   PTY session supervisor that auto-formats terminal output (JSON, logs, HTTP,
>   diffs, tables) and explains command failures, with property-tested
>   byte-safety. First public release.

Also nominate for Crate of the Week (comment on the pinned issue):

> Self-nominating [glimps](https://crates.io/crates/glimps): a PTY wrapper that
> auto-formats your shell's output with no piping — and a nice case study in
> panic-free Rust, since a panic would leave the user's terminal in raw mode.

---

## Terminal Trove submission

> **Name:** GLIMPS
> **What it does:** Zero-config smart terminal output formatter. Wraps your
> shell in a PTY, finds the command/output boundary via OSC-133, and formats
> recognized output automatically — JSON, logs, HTTP responses, diffs, tables,
> and 30+ command-aware views — plus plain-language failure footers (exit code
> meaning, failed pipeline stage, the error line re-quoted). Pass-through by
> default, zero telemetry, instant off-switch.
> **Install:** brew install Krishnarajan7/tap/glimps
> **Language:** Rust · **License:** MIT
> **Links:** https://github.com/Krishnarajan7/Glimps · https://glimpps.netlify.app

---

## Awesome-list one-liners

`awesome-rust` (Applications):

> * [glimps](https://github.com/Krishnarajan7/Glimps) — Zero-config PTY wrapper that auto-formats terminal output (JSON, logs, HTTP, diffs, tables) and explains command failures [![CI](https://github.com/Krishnarajan7/Glimps/actions/workflows/ci.yml/badge.svg)](https://github.com/Krishnarajan7/Glimps/actions)

`awesome-cli-apps` / `awesome-terminals`:

> - [GLIMPS](https://github.com/Krishnarajan7/Glimps) - Auto-format your shell's output with no piping: JSON, logs, HTTP, diffs, tables, and plain-language failure explanations.

---

## X / Mastodon (attach a clip from site/public/demo.mp4)

Post 1 (launch):

> Your terminal output, formatted automatically. GLIMPS wraps your shell in a
> PTY and pretty-prints JSON, colors logs, splits HTTP responses, explains
> failures — no piping, no aliases, no config. Rust, MIT, zero telemetry.
> brew install Krishnarajan7/tap/glimps
> https://github.com/Krishnarajan7/Glimps #rustlang

Post 2 (failure intelligence angle, a few days later):

> `false | wc -l` exits 0 — the shell only reports the last pipeline stage.
> GLIMPS reads PIPESTATUS and tells you stage 1 failed. It also translates exit
> codes (127 → not on PATH, 137 → OOM-killed) and re-quotes the error line that
> scrolled away. https://github.com/Krishnarajan7/Glimps

---

## Blog post outline (week 2, cross-post to dev.to)

**Title:** "How GLIMPS knows where your command's output begins — and why shell
hooks can't do this"

1. The problem: you can't reformat what you can't intercept — preexec/precmd
   fire before/after, never *during*.
2. The PTY supervisor model (ChromaTerm/script/tmux lineage).
3. OSC-133 semantic prompts: the prompt/input/output state machine.
4. What owning the session buys you: exit codes, duration, PIPESTATUS →
   failure footers.
5. The safety chapter: why "when unsure, do nothing" is the hardest feature;
   byte-safety property tests; the panic-restore Drop guard.
6. What's next + call for contributors.
