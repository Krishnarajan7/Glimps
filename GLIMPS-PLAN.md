# GLIMPS — Design Rationale

> Why GLIMPS is built as a PTY session supervisor rather than a shell hook, and
> the tech-stack and scope decisions behind the current implementation. For
> install, release, and dogfood instructions, see [`README.md`](./README.md),
> [`docs/FRESH_MAC_DOGFOOD.md`](./docs/FRESH_MAC_DOGFOOD.md), and
> [`docs/PUBLIC_BETA_RELEASE_RUNBOOK.md`](./docs/PUBLIC_BETA_RELEASE_RUNBOOK.md).
> The versioned roadmap lives in [`ROADMAP.md`](./ROADMAP.md).

## Summary

GLIMPS is a zero-config terminal output formatter. It runs the user's shell
inside a pseudo-terminal it owns, watches the output stream, and reformats
content it can confidently recognize (JSON, HTML, logs, HTTP, diffs, stack
traces, and common command output) with structure and color — with no manual
piping and nothing to configure.

The decisions that define the project:

- **Output is intercepted by owning the PTY the shell runs in — not by a zsh
  `preexec`/`precmd` hook.** Shell hooks physically cannot see a command's
  output; owning the PTY is the only mechanism that can.
- **The command/output boundary comes from OSC-133 shell-integration markers**,
  so the prompt and typed input are never reformatted — only command output is.
- **Rust for the core binary**, for fast startup, a single static binary, and
  mature PTY and parsing libraries.
- **Pass-through is the default.** Only output GLIMPS is confident about is
  reformatted; everything else is emitted byte-for-byte.

---

## 1. Output interception: why a PTY supervisor

### Shell hooks cannot capture output

A common first instinct is a zsh plugin that hooks `preexec`/`precmd`. That
cannot work:

```
Assumed:   zsh preexec hook  ->  wrap command in a PTY  ->  format output
Reality:   preexec runs BEFORE the command and has no handle on its stdout.
           precmd runs AFTER the output is already on screen. Too late.
           zsh exposes NO hook that sits on a command's output stream.
```

There is no shell hook layer that intercepts output. A `source`-one-line-into-
`.zshrc` install built on hooks is impossible for the same reason.

### The model that works

GLIMPS owns a PTY and runs the shell *inside* it. Every byte the shell (and
every command in it) writes passes through GLIMPS before it reaches the real
screen. This is the same model used by `script`, `tmux`, `asciinema`, and
ChromaTerm.

```
You type a command
      |
      v
[ your real terminal (Terminal.app / iTerm / Ghostty) ]
      |   keystrokes
      v
[ GLIMPS supervisor process ]  <-- owns the PTY master
      |   forwards input
      v
[ PTY slave  ==  your zsh, running normally inside ]
      |   command runs, writes output to the PTY
      v
[ GLIMPS reads the raw output stream ]
      |   detect content type -> reformat -> re-emit ANSI
      v
[ back out to your real terminal screen ]
```

### Two delivery modes

1. **Whole-session (primary):** GLIMPS launches the shell; all output flows
   through it. Interactive programs (vim, ssh, htop) keep working because the
   PTY makes them believe they have a real terminal. GLIMPS detects them and
   passes their bytes through untouched.
2. **Per-command (secondary, opt-in):** a zsh ZLE widget rewrites the command
   line so a single command runs wrapped. It is fragile with complex
   pipelines/redirections, so it is a secondary path, not the default.

### Consequences of the model

| Naive assumption | Reality in GLIMPS |
|---|---|
| "Hook into `preexec`" | PTY session supervisor (the ChromaTerm model). |
| "Add one `source …` line to `.zshrc`" | Init line re-execs the interactive shell once, inside the GLIMPS supervisor. |
| "Skip the hook for interactive commands" | Interactive programs still bypass — GLIMPS passes their PTY bytes straight through. |
| "Under 10 ms latency" | Achievable for batch output, but a session-wide PTY copies every byte (including vim redraws), so the real target is *imperceptible on interactive redraw*, which requires a near-zero-work fast path. |

The reformatting and zero-config experience — not the PTY mechanism itself — are
what GLIMPS adds on top of the proven model.

---

## 2. Where GLIMPS fits

| Tool | What it does | Stars (approx) | How GLIMPS differs |
|---|---|---|---|
| `bat` | syntax-highlights files you open | ~59k | must be invoked; GLIMPS is automatic on live output |
| `jq` | pretty-prints JSON you pipe to it | ~35k | manual pipe, JSON only |
| `glow` | renders markdown you point it at | — | manual, markdown only |
| `delta` | better git diffs | — | git only |
| ChromaTerm | PTY-wraps the shell, regex-colors output | ~210 | closest analog, but regex recoloring only via hand-written YAML rules — no structural reformatting (it won't re-indent JSON/HTML, only recolor existing text) |
| `grc` | colorizes known commands' output | — | per-command config, no auto-detect |

The PTY-wrap-the-shell model is proven, which de-risks the hard part. What the
closest analog lacks is exactly GLIMPS's focus: **zero configuration**,
**structural reformatting** (actually re-indenting JSON/HTML rather than only
recoloring), and a good out-of-the-box experience. That product surface — not
the interception trick — is where the effort goes.

---

## 3. Tech stack

Core language: **Rust** — fast startup, single static binary, mature PTY and
parsing crates.

| Concern | Choice | Notes |
|---|---|---|
| PTY management | `portable-pty` (wezterm) | Battle-tested, cross-platform. macOS + Linux now, Windows later. |
| I/O loop | plain threads (reader + writer) | Simple threads first; async only if a real need appears. |
| JSON detect/format | `serde_json` + a custom pretty printer | Parse success = it is JSON; formatting preserves every field. |
| Color output | ANSI SGR codes | Standard 16-color output; no truecolor dependency. |
| HTML detection | tag-density heuristics | No full DOM parse — detect `<tag>` structure and indent. |
| Log severity | pattern scan | ERROR/WARN/INFO/DEBUG recognized near the start of a line. |
| ANSI-already-present | cheap `\x1b[` scan | Already-colored output is left untouched. |
| Binary detection | null-byte / non-UTF-8 scan of the first bytes | Binary output is passed through. |
| Config | `serde` + `toml` (`~/.glimpsrc`) | TOML is friendlier than YAML for users. |
| Benchmarks | `criterion` | Enforce the latency budget per release. |

Out of scope for the core: a full terminal-emulator engine, a GPU renderer, or
any embedded model.

---

## 4. Requirements that shaped the design

1. **Prompt zone vs. output zone.** When GLIMPS wraps the whole session, the
   prompt, autosuggestions, and syntax highlighting also flow through it and
   must never be mangled. GLIMPS injects and reads **OSC-133** shell-integration
   markers (the same ones iTerm and VS Code use) to know exactly where a
   command's output begins and ends. This boundary detection is the technical
   core of the project.
2. **Fast path for interactive redraw.** vim and htop push bytes constantly, so
   GLIMPS must do near-zero work while a bypassed/interactive program is active.
3. **An instant off switch.** `GLIMPS=0` disables all formatting and falls back
   to pure pass-through — essential for trust if anything ever looks wrong.
4. **Security and trust.** GLIMPS sits in front of everything the user types and
   sees, including secrets, SSH sessions, and password prompts. Nothing is
   logged, stored, or transmitted; no-echo password prompts are never touched.
5. **A real test strategy.** Golden-file tests over recorded command output plus
   property tests that prove formatting never panics and is byte-safe on
   arbitrary input.
6. **Mixed content is deferred.** Segmenting a single output into JSON-inside-
   log-lines is the hardest formatting problem, so it is out of the first
   release; whole-document JSON/HTML/HTTP and streaming logs come first.

---

## 5. Install mechanism

```bash
# Build from source (the supported path today)
git clone https://github.com/Krishnarajan7/Glimps
cd Glimps
cargo install --path .

# Enable zsh integration after installing the binary
echo 'command -v glimps >/dev/null 2>&1 && eval "$(glimps init zsh)"' >> ~/.zshrc
```

Homebrew and crates.io distribution are goals but are not advertised until the
release/tap flow is verified from a real version tag. `glimps init zsh` prints a
snippet that:

- checks whether the shell is already inside a GLIMPS PTY (to avoid nesting),
- if not, re-execs the interactive shell inside the GLIMPS supervisor,
- installs the OSC-133 markers so GLIMPS can tell command input from output.

It stays two steps ("add the line, restart the terminal") while being honest
about the mechanism. For a no-install trial, `scripts/dogfood-macos.sh session`
wraps a throwaway zsh and cleans up on exit.

---

## 6. Scope and phasing

Milestones are ordered so each is independently usable. Versioned detail lives in
[`ROADMAP.md`](./ROADMAP.md).

- **Spike / de-risk.** A bare PTY supervisor that launches zsh and forwards
  input/output/resize and nothing else — a transparent passthrough that must
  feel exactly like a normal terminal, with vim/ssh/htop/fzf working untouched,
  plus OSC-133 marker injection and detection. If the passthrough does not feel
  native, nothing built on top matters.
- **JSON.** Detect whole-buffer JSON and pretty-print with colored keys/values,
  alongside `isatty` pipe-safety, ANSI-already-present and binary pass-through,
  and the interactive-command bypass list.
- **Logs, HTTP, and the command header.** Streaming severity coloring, HTTP
  status highlighting, and the ▌ command/output header with an optional
  timestamp and content-type badge.
- **HTML and robustness.** HTML tag detection and indentation, a streaming-mode
  switch for large output, and the golden-file corpus plus a latency benchmark.
- **Polish and release.** `~/.glimpsrc` (TOML) config, prebuilt binaries and a
  Homebrew tap, and a demo recording for the README.
- **Later.** Mixed-content segmentation, URL hyperlinking, error-line pinning,
  and broader shell support.

---

## 7. Risks

- **A single corruption bug loses a user for good.** GLIMPS sits between the user
  and everything they type, including secrets and SSH. Trust is fragile, which
  raises the quality bar — hence the hard safety invariants and the off switch.
- **Interactive-redraw latency** is a real engineering risk; the zero-work fast
  path for bypassed programs is not optional.
- **Escape-sequence handling is the gnarliest part.** PTY plus terminal escape
  sequences is intermediate-to-advanced territory.
- **Broad surface area.** A tool that touches every command attracts bug reports
  for unusual command/terminal combinations — the inherent cost of the category.

These are the reasons the project is worth doing: the difficulty and the
trust-sensitivity are the moat.

---

## Appendix — sources

- zsh hooks (`preexec`/`precmd` run before/after and cannot capture output):
  https://github.com/rothgar/mastering-zsh/blob/master/docs/config/hooks.md
- ChromaTerm — PTY-wraps the shell, works with interactive/SSH (proves the
  model): https://github.com/hSaria/ChromaTerm
- `portable-pty` (Rust PTY crate):
  https://docs.rs/portable-pty/latest/portable_pty/
- PTY + OSC sequences in Rust (real-time capture pattern):
  https://developerlife.com/2025/08/10/pty-rust-osc-seq/
- `bat` (the popularity ceiling of this space): https://github.com/sharkdp/bat
- `jq`: https://github.com/jqlang/jq
- `grc` generic colorizer: https://github.com/garabik/grc
