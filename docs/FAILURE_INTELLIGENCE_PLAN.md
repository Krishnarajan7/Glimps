# Failure Intelligence — the painkiller plan

> Companion to `GLIMPS-PLAN.md` (architecture rationale) and `ROADMAP.md` (versions).
> This doc exists because formatters are a vitamin and this is the painkiller.
> Read it before adding one more content-type formatter.

## The thesis, in three sentences

Every popular terminal tool (`bat`, `jq`, `delta`, ChromaTerm) sees **bytes**.
GLIMPS owns the PTY and reads OSC-133 markers, so it is the only tool in this
space that knows the **command → output → exit code → duration** boundary.
Failure intelligence is the feature that can only be built on top of that
knowledge — which makes it the moat, the pitch, and the launch headline.

The pitch it unlocks:

> "When a command fails, GLIMPS tells you what broke, why, and where —
> without you scrolling a single line."

## What we already have (do not re-plan solved work)

Honest inventory, verified against the code on 2026-07-09:

| Piece | Status | Where |
|---|---|---|
| OSC-133 `D;<code>` exit-code capture | ✅ done | `src/format/osc133.rs:156` (`take_exit_code`) |
| Command duration measurement | ✅ done | `command_started_at` in `src/format/mod.rs` |
| Basic status footer (`failed exit 101 in 4.7s`) | ✅ done | `emit_command_status`, `src/format/mod.rs:510` |
| `command failed: <cmd>` summary line on failure | ✅ done | same function |
| Quiet-on-success rules (silent cmds, `cd`/`pwd` suppression) | ✅ done | same function |
| Control-byte sanitization of GLIMPS-authored chrome | ✅ done | `cmdline::sanitize_display` |
| Exit-code **translation** (127/137/139…) | ❌ missing | — |
| Signal vs. error distinction (Ctrl-C ≠ failure) | ❌ missing | — |
| Error-line pinning (`first error → file:line, ↑ N lines up`) | ❌ missing | — |
| Failure summary panel for long output (test runners) | ❌ missing | — |
| Config surface (`[failures]` section) | ❌ missing | `src/config.rs` has no toggle for the footer |

So this is not a greenfield feature. It is a v0 footer that needs to become
the product's headline. That changes the risk profile: the plumbing (markers,
timing, footer emission, sanitization) is proven; the remaining work is mostly
**presentation and a lookup table**, not new PTY machinery.

## Competitor gap analysis — who almost does this, and why they can't

The one-line version: everyone else is downstream of the PTY, so the
command boundary is invisible to them. Details:

| Tool | What it does | Why it cannot do failure intelligence |
|---|---|---|
| **ChromaTerm / ct** | Regex-colors a live stream, incl. SSH | Sees a byte stream only. No concept of "a command", no exit code, no duration. Closest competitor; structurally blind to this feature. |
| **bat** | Pretty file viewer | Invoked *per file, by the user*. Never sees command execution at all. |
| **jq / fx / jless** | JSON tools | Must be piped manually. Piping destroys `isatty`, and they never see exit codes. |
| **delta** | Diff pager | Git-specific pager; only sees what git pipes to it. |
| **Warp terminal** | Blocks output per command, shows exit status | The real feature overlap — but it is a **proprietary terminal emulator you must switch to**, macOS-first, account-encumbered. GLIMPS delivers the same boundary awareness *inside the terminal you already use*, open source, no account, no telemetry. |
| **Shell prompts (starship, p10k)** | Show last exit code in the *next prompt* | One number, after the fact, no duration attribution, no link to the output above it, nothing pinned. This is the status quo we beat. |
| **zsh `REPORTTIME` / bash `PROMPT_COMMAND` hacks** | Print duration for slow commands | Bolt-on, per-shell config, no failure analysis, no output awareness. |

The Warp row is the important one: Warp validated that per-command exit/duration
awareness is what users actually want — and its adoption is capped by "switch
your whole terminal and make an account." GLIMPS's structural answer: same
insight, zero switching cost, works in iTerm/Terminal.app/Alacritty/kitty/
anything, and the off switch is one env var.

## Why GLIMPS wins this specific fight

1. **Unique input.** OSC-133 `C`/`D` markers give exact command start/end plus
   the exit code. We already parse them. Nobody downstream of the PTY can get
   this without becoming a PTY supervisor — i.e., without becoming GLIMPS.
2. **Additive-only, so the trust story survives.** The feature never rewrites
   command output. It only *adds* a footer under it. Invariant 4 (byte-safety)
   is untouched by design, which keeps the "it can't corrupt your terminal"
   promise intact even as the feature gets smarter.
3. **The wow-to-use ratio flips.** Today the common experience is "GLIMPS did
   nothing" (correct, but invisible). Failures happen to every developer every
   day — this makes the *daily* experience visibly valuable, not just the
   occasional JSON blob.
4. **It's honest intelligence.** No AI, no telemetry, no guessing: it decodes
   an exit code the shell already produced and points at a line that is
   literally present in the output. Nothing to hallucinate, nothing to leak.

## The feature, by increments

Ordered so each increment ships alone, is independently demoable, and never
depends on a later one. Effort marks are relative (S/M/L).

### F1 — Exit-code translation dictionary (S) — ship first

Replace the bare `failed exit 137` with a human decode:

```
└─ ✗ exit 137 · 22.9s — killed: out of memory (SIGKILL/OOM)
```

The table (exhaustive on purpose — this IS the feature):

| Exit | Rendered as | Class |
|---|---|---|
| 0 | `✓ · 1.2s` (dim, or hidden per config) | success |
| 1, 2 | `✗ exit 1 · 4.7s` | generic failure |
| 126 | `— found but not executable (permission?)` | env problem |
| 127 | `— command not found on PATH` | env problem |
| 128+N | decode signal N by name | signal |
| 129 (SIGHUP) | `— hangup` | signal |
| 130 (SIGINT) | `⊘ interrupted (Ctrl-C) — not an error` | **user action** |
| 137 (SIGKILL) | `— killed: SIGKILL (often OOM)` | signal |
| 139 (SIGSEGV) | `— crashed: segmentation fault` | signal |
| 141 (SIGPIPE) | `— broken pipe (reader exited early)` | often benign |
| 143 (SIGTERM) | `⊘ terminated — asked to stop` | user/system action |

Non-negotiable detail: **130 and 143 must NOT render red.** Ctrl-C is a user
action, not a failure. A footer that cries wolf on every Ctrl-C trains users
to ignore it, and the whole feature dies of alarm fatigue. Three visual
classes: success (dim/silent), user-action (neutral `⊘`), failure (red `✗`).

Acceptance: golden tests for every row above; `false`, `sleep 5` + Ctrl-C,
`nonexistent-cmd`, and `sh -c 'kill -9 $$'` each render their class correctly.

### F2 — Config surface + kill switch (S) — ships with F1

Per the charter: a feature that can't be turned off is a bug. Add to
`.glimpsrc` (and `.glimpsrc.example`):

```toml
[failures]
enabled = true      # false = no footer, ever (status quo ante)
on_success = "dim"  # "dim" | "off" — how loud success is
explain = true      # false = raw exit code only, no translation text
```

Footer already respects the bypass list and alt-screen detection because it
rides `emit_command_status`; keep it that way. No footer after `vim`, `ssh`,
`less` — those have no meaningful exit "moment" for the user.

### F3 — Error-line pinning (M) — the demo moment

On failure, scan the (already buffered) output for the first
high-confidence error line and repeat it in the footer with a
distance hint:

```
└─ ✗ exit 101 · 4.7s
   error[E0308]: mismatched types → src/pty.rs:214:18   ↑ 47 lines up
```

Detection reuses machinery we already have — the stacktrace and log-severity
detectors know what an error line looks like. Pinning order of confidence:

1. Compiler/tool-formatted errors with a `file:line` (rustc `error[…]`,
   Python `File "…", line N`, tsc, gcc/clang, go).
2. First `ERROR`-severity log line (existing log detector).
3. First line starting `error:` / `Error:` / `fatal:`.
4. Nothing confident → pin nothing. **A wrong pin is worse than no pin** —
   this is invariant 2 (default to pass-through) applied to chrome.

The pinned line is output that already scrolled past, re-quoted inside
GLIMPS-authored chrome — so it goes through `sanitize_display` like the
command does (same injection concern, same fix).

Scope guard (design corrected during implementation): pinning is a
**read-only shadow line assembler** fed at the segment level. The original
"buffered output only" idea was wrong — the flagship case (`cargo build`)
emits *colored* error lines, which the OSC scanner routes to Pass segments,
so they never reach the buffered or streaming paths as whole lines. The
shadow assembler observes Output + in-zone Pass bytes, strips escapes with
a small state machine, and matches on clean text. Strictly bounded: one
line buffer capped at 1 KiB (longer lines are counted, never matched),
three candidate slots, O(n) feed, and it cannot alter emitted bytes — the
worst bug is a missed pin, never corrupted output. Disarmed for bypassed
commands and binary output.

### F4 — Failure summary panel for known runners (M/L) — later, demand-driven

For long failed output from *recognized* test/build runners (cargo test,
pytest, jest/vitest, go test), collapse the shape of the failure:

```
└─ ✗ exit 1 · 38.2s
   ✗ 3 failing · ✓ 214 passing · ⊘ 2 skipped
   first: auth › rejects expired token → tests/auth.spec.ts:88  ↑ 612 lines up
```

This is per-runner parsing — each runner is a scoped, contributor-friendly
task shaped exactly like a formatter (detector + renderer + golden tests),
so it feeds the good-first-issue pipeline. Do not build a generic "test
output parser"; build 3–4 specific ones behind one trait.

### Explicit non-goals (so this doesn't become a platform)

- No AI/LLM explanation of errors. (v2.0 territory, local-only, opt-in — see ROADMAP.)
- No suggested fixes ("did you mean…"). Decoding facts, not advising.
- No network lookups of any kind. Invariant 5 is absolute.
- No rewriting of the failing output itself — footer/chrome only, ever.
- No per-command plugins/hooks in v1 of this feature.

## Safety invariant mapping (required for anything touching `src/format/`)

| Invariant | How this feature respects it |
|---|---|
| 1 — terminal never broken | Footer is normal text writes inside the existing output path; RawGuard untouched. |
| 2 — default pass-through | Uncertain error pin → no pin. Unknown exit semantics → raw number, no story. |
| 3 — never reformat bypass/binary/ANSI/piped | Footer rides existing `emit_command_status` gating: no marker → no footer; alt-screen/bypass → no footer; not a TTY → nothing. |
| 4 — byte-safety | Output bytes are never edited. Pinned line is a sanitized *copy* in GLIMPS chrome. Property tests must cover footer emission on arbitrary bytes. |
| 5 — zero exfiltration | Everything is a local lookup table. Nothing leaves the process. |
| 6 — off switch | `GLIMPS=0` kills everything; `[failures] enabled=false` kills just this. |

Every increment lands with: golden tests, property test (arbitrary bytes +
arbitrary exit code → no panic, no byte loss), `pty-safety-auditor` run,
and a `tests/pty_integration.rs` case driving the real binary.

## Repositioning — the part that isn't code

The feature only pays off if the story leads with it. Concretely:

1. **README first screen** becomes a failed `cargo build` with the footer,
   not a pretty JSON blob. JSON moves to second.
2. **Demo GIF** (`demo/glimps.tape`): beat 1 = command header on `ls`,
   beat 2 = `cargo build` fails → footer decodes it, beat 3 = JSON.
   Currently the tape has no failure beat at all — that's backwards
   relative to this plan.
3. **One-liner everywhere** (site, repo description, Show HN title):
   command understanding first, formatting second. Draft:
   *"GLIMPS — a transparent shell wrapper that knows when your command
   failed, why, and where. (It also formats your JSON.)"*
4. **Do not ship new content-type formatters until F1–F3 are done.**
   Every formatter added before the pivot deepens the vitamin identity.
   Good-first-issues can keep flowing (they're contributor fuel), but
   maintainer time goes here.

## Sequencing against the existing roadmap

- F1 + F2 slot into **v1.0 pre-launch** — small, high-value, and they make
  the launch demo. ROADMAP already lists "error-line pinning / summary panel"
  under v1.x; F1/F2 are cheaper than that and should jump the queue.
- F3 is the **launch headline** if it lands in time; otherwise it's the
  first post-launch release (a strong "week two" story for early adopters).
- F4 stays v1.x, demand-driven, contributor-powered.

## Risks, stated plainly

- **Footer fatigue.** If the footer shows too often on success, users mute it
  and the failure signal dies with it. Mitigation: success is dim-or-silent
  by default; the loud path is failures only. Measure by dogfooding: if *you*
  notice the success footer in a day of use, it's too loud.
- **Wrong pins destroy trust faster than no pins.** A footer confidently
  pointing at the wrong line reads as lying. Mitigation: confidence ladder
  with "pin nothing" as the default rung; goldens for each pin source.
- **Exit-code folklore.** 137 is *often* OOM but is literally just SIGKILL.
  Wording must stay factual ("killed: SIGKILL (often OOM)") — decode, don't
  diagnose. Same for 141/SIGPIPE, which is frequently benign (`head` closing
  a pipe).
- **bash marker fragility.** The DEBUG-trap integration is beta; a raw trap
  below the GLIMPS line silently kills the `D` marker and with it the whole
  feature. Known limitation — document it in the failures section of the
  README, don't pretend it away.

## What "this worked" looks like

- The launch demo's most-quoted moment is the exit-137 decode, not the JSON.
- Issues/comments start referencing the footer ("can it also decode X?") —
  pull, not push.
- You stop personally scrolling for errors within a week of dogfooding F3.
  (Same bar as v0.1's exit criterion: daily use without turning it off.)
