# GLIMPS — R&D Findings & Full Setup Plan

> Smart, zero-config terminal output formatter (formerly "Pipelight" in your doc).
> This file is my full research + build plan. Read it top to bottom, then we decide.
> Date: 2026-06-25

---

## 0. TL;DR (read this first)

- **Can it be built? YES.** The idea is real, useful, and technically achievable. A tool that does almost exactly this (ChromaTerm) already exists and works — which proves the hard part is solvable.
- **BUT your doc's architecture is broken as written.** The doc says "zsh plugin hooks into `preexec`, then wraps the command in a PTY." **`preexec` / `precmd` hooks physically cannot capture a command's output.** `preexec` runs *before* the command; `precmd` runs *after* the output has already been printed to your screen. Neither can touch stdout. If we build on that assumption, the project fails on day one. The fix is below (Section 2) and it's well-understood.
- **The "one line in `.zshrc`" install in the doc is also wrong** for the same reason. The real install is "GLIMPS wraps your shell session" (Section 6).
- **About "100K stars":** Be realistic. The most popular tool in this entire space, `bat`, has ~59k stars after ~8 years. `jq` has ~35k. ChromaTerm — the tool closest to *exactly* your idea — has ~210 stars. 100K is a moonshot, not a baseline. The idea is good; 100K depends far more on marketing, timing, and a killer demo GIF than on the code. I'll build the plan to *maximize* that chance, but I won't pretend it's likely.
- **Tech stack:** Rust is the right core choice (the doc is correct here). I'll give you the exact crates and structure.
- **Your personal problem** ("can't tell input from output, especially long HTML") is genuinely solved by this — and the *separator + content badges + structural reformatting* are the parts that solve it, more than the colors. See Section 8.

---

## 1. What the doc gets RIGHT

Your doc is actually a strong product spec. These parts are solid and we keep them:

- **The problem is real.** Flat monochrome terminal output with no separation between command and result is a genuine daily annoyance, especially for `curl`/API work.
- **Rust for the core binary** — correct. Fast startup, single static binary, great PTY + parsing libraries.
- **The edge-case section (5.x) is excellent** — interactive command bypass, binary pass-through, ANSI-already-present detection, `isatty()` pipe safety, streaming mode for `tail -f`, size limits. This is exactly the stuff that kills naive versions. Whoever wrote this understood the traps.
- **The v1 scope cut list is sensible** (URL hyperlinking, error pinning, Windows, fish/bash → later).
- **Two-step install as a hard requirement** — correct instinct, even if the actual mechanism is different from what's written.

So: the *product thinking* is good. The *architecture* is where the doc is wrong.

---

## 2. THE CRITICAL FIX — How output interception actually works

This is the single most important section. Everything depends on it.

### Why the doc's approach can't work

```
Doc says:   zsh preexec hook  ->  GLIMPS wraps command in PTY  ->  formats output
Reality:    preexec runs BEFORE the command and has no handle on its stdout.
            precmd runs AFTER output is already on screen. Too late.
            zsh gives you NO hook that sits on a command's output stream.
```

Verified against multiple sources (zsh hooks docs, ChromaTerm internals). There is no zsh hook layer that intercepts output. Full stop.

### The approach that DOES work (and is proven)

**GLIMPS owns a PTY and runs your shell *inside* it.** Every byte your shell (and every command in it) writes goes through GLIMPS before it reaches the real screen. This is exactly how `script`, `tmux`, `asciinema`, and ChromaTerm work.

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
      |   command runs, writes output to PTY
      v
[ GLIMPS reads the raw output stream ]
      |   detect content type -> reformat -> re-emit ANSI
      v
[ back out to your real terminal screen ]
```

Two ways to deliver this, and we ship **both**:

1. **Whole-session mode (primary, robust):** GLIMPS launches your shell. You set GLIMPS as your terminal's "command" or wrap login. ALL output flows through it. This is how ChromaTerm gets interactive commands (vim, ssh) to keep working — the PTY makes them think they have a real terminal. The edge-case bypass list from your doc still applies (we detect interactive programs and pass their bytes through untouched).

2. **Per-command mode (opt-in, surgical):** A zsh ZLE widget rewrites your command line so `curl foo.com` actually runs as `glimps run -- curl foo.com`. Only that command is wrapped. Safer for users who don't want a session-wide layer, but fragile with complex pipelines/redirections — so it's the secondary path, not the default.

### What changes in the doc because of this

| Doc claim | Reality / fix |
|---|---|
| "Hooks into `preexec`" | ❌ Replace with "PTY session supervisor" (ChromaTerm model) |
| "Add one `source ...` line to `.zshrc`" | ⚠️ Real install = set GLIMPS as shell wrapper, or eval an init line that re-execs your shell under GLIMPS once |
| "Zero interception for interactive commands via allowlist" | ✅ Still works — but it's "pass PTY bytes straight through," not "skip the hook" |
| "Under 10ms latency" | ⚠️ Achievable for *batch* output, but a session-wide PTY adds a per-byte copy on EVERYTHING (including vim redraws). Target must be "imperceptible on interactive redraw," measured differently. Realistic but needs care. |

**Bottom line: the project is still very buildable. We just build the ChromaTerm-style PTY supervisor instead of the (impossible) hook design, and we add the structural reformatting + zero-config that ChromaTerm lacks. That gap IS our product.**

---

## 3. Honest competitive reality (so we know what we're up against)

| Tool | What it does | Stars (approx) | Why GLIMPS is different |
|---|---|---|---|
| `bat` | syntax-highlight files you explicitly open | ~59k | bat needs you to call it; GLIMPS is automatic & on live output |
| `jq` | pretty-print JSON you pipe to it | ~35k | manual pipe, JSON only |
| `glow` | render markdown you point it at | — | manual, markdown only |
| `delta` | better git diffs | — | git only |
| **ChromaTerm** | **PTY-wrap shell, regex-color all output** | **~210** | **closest analog. But: regex coloring only, hand-written YAML rules, no structural reformatting (won't pretty-print/indent JSON or HTML — just recolors existing text)** |
| `grc` | colorize known commands' output | — | per-command config, no auto-detect |

**The takeaway:**
- The PTY-wrap-the-shell model is *proven* (ChromaTerm). De-risked.
- ChromaTerm only has ~210 stars despite doing the hard part — which tells us **the hard part is not what makes a project popular.** What ChromaTerm lacks is exactly GLIMPS's pitch: *zero config* + *structural reformatting* (actually re-indenting JSON/HTML, not just recoloring) + *a beautiful out-of-the-box experience*.
- So our edge is **product/UX, not the PTY trick.** That also means the README, the demo GIF, and the "it just works the second you install it" moment matter more than any algorithm.

---

## 4. Recommended tech stack (concrete)

Core language: **Rust** (keep — the doc is right).

| Concern | Crate / choice | Notes |
|---|---|---|
| PTY management | `portable-pty` (from the wezterm project) | Battle-tested, cross-platform, the de-facto choice. Mac + Linux now, Windows later for free-ish. |
| Async I/O loop | `tokio` *or* plain threads + `mio` | Start with simple threads (reader thread + writer thread). Add async only if needed. Don't over-engineer v0. |
| JSON detect/format | `serde_json` (detect via parse-attempt) + custom pretty printer | Parse success = it's JSON. For embedded JSON blobs, scan for balanced `{...}` regions. |
| Color output | `anstyle` / `owo-colors` | Doc says `owo-colors` — fine. `anstyle` is lighter if we only need ANSI codes. |
| HTML detection | tag heuristics + `regex` | Doc is right: no full DOM parse. Just detect `<tag>` density and indent. |
| Log severity coloring | `regex` with `RegexSet` (multi-pattern, fast) | ERROR/WARN/INFO/DEBUG. RegexSet matches many patterns in one pass. |
| ANSI-already-present detection | `vte` parser or a cheap `\x1b[` scan | `vte` (also from wezterm/alacritty) properly parses escape sequences if we need correctness. |
| Binary detection | manual: null-byte + non-UTF8 scan of first 512 bytes | exactly as the doc says |
| Config file | `serde` + `toml` (`.glimpsrc`) | TOML over the doc's unspecified format — friendlier than YAML for users. |
| CLI args | `clap` | standard |
| Benchmarks | `criterion` | enforce the latency budget per release (doc's success metric) |
| Distribution | Homebrew formula + GitHub Releases (prebuilt binaries via `cargo-dist`) | `cargo-dist` auto-generates the Homebrew tap + CI release pipeline. Huge time saver. |

**Don't reach for:** a full terminal emulator engine, a GPU renderer, or an embedded AI model in v1. All scope creep.

---

## 5. What the doc is MISSING (gaps to close before building)

1. **The interception mechanism is wrong** (Section 2) — biggest gap.
2. **No story for the shell prompt itself.** When GLIMPS wraps the whole session, your prompt, autosuggestions, and syntax highlighting also flow through GLIMPS. We must NOT mangle those. Need a "prompt zone vs output zone" detector — typically via injecting invisible OSC marker sequences (`OSC 133` shell-integration markers, the same ones iTerm/VS Code use) so GLIMPS knows where a command's output begins and ends. **This is the real technical core of the project and the doc doesn't mention it at all.** It's also the key to your personal problem (telling input from output — Section 8).
3. **No latency model for interactive redraw.** "10ms for <100KB batch" is fine, but vim scrolling pushes bytes constantly through GLIMPS. Need a fast-path that does near-zero work when in a bypassed/interactive program.
4. **No "off switch" / panic story.** If GLIMPS ever corrupts a session, the user needs an instant escape hatch (env var `GLIMPS=0`, or a key to toggle). Critical for trust.
5. **Security/trust:** GLIMPS sits between the user and *all* their terminal output, including secrets, SSH sessions, password prompts. README must address: nothing is logged, nothing leaves the machine, password prompts (no-echo) are never touched. People will (rightly) be paranoid about a tool in this position.
6. **No test strategy for "top 50 commands."** The doc lists it as a success metric but no harness. We need a golden-file test corpus (recorded real outputs → expected formatted output).
7. **Mixed-content (your screenshot case) is hand-waved.** "Segment into up to 3 parts" is fine as scope, but the segmentation algorithm is the hardest formatting problem. I'd cut it from v1 entirely and ship JSON + logs + HTTP status first (the 80% case), add mixed-content in v0.3.

---

## 6. Corrected install experience

```bash
# Step 1 — install
brew install glimps

# Step 2 — enable (one line; this re-execs your shell under GLIMPS once per session)
echo 'eval "$(glimps init zsh)"' >> ~/.zshrc
```

`glimps init zsh` prints a tiny snippet that:
- checks if we're already inside a GLIMPS PTY (avoid infinite nesting),
- if not, re-execs the interactive shell inside the GLIMPS supervisor,
- installs the OSC-133 prompt markers so GLIMPS can tell command-input from command-output.

Still two steps. Still "restart terminal, done." But honest about the mechanism.

---

## 7. Build plan — phased milestones

I'd build it in this order. Each phase is independently demoable (important for momentum + GIFs).

### Phase 0 — Spike / de-risk (1–2 weeks)
- [ ] Bare PTY supervisor in Rust with `portable-pty`: launch zsh inside it, forward stdin/stdout/resize, do *nothing else*. Goal: a transparent passthrough that feels exactly like a normal terminal. **If this doesn't feel native, nothing else matters.** This is the make-or-break spike.
- [ ] Verify vim, ssh, htop, fzf all work untouched inside it.
- [ ] Inject + detect OSC-133 markers; prove we can isolate "this byte range is command output."

### Phase 1 — The first real win: JSON (2–3 weeks)
- [ ] Detect a full-buffer JSON output (parse-attempt) and pretty-print with colored keys/values.
- [ ] `isatty()` pipe-safety + ANSI-already-present pass-through + binary pass-through.
- [ ] Interactive-command bypass list.
- [ ] **This is the first "holy cow" demo:** `curl api.github.com/users/x` auto-pretty-printed with zero pipe.

### Phase 2 — Logs + HTTP + the separator (2 weeks)
- [ ] ERROR/WARN/INFO/DEBUG severity coloring (streaming, line-by-line).
- [ ] HTTP status code highlighting.
- [ ] **Command/output separator with timestamp + content-type badge** ← this is the part that solves YOUR problem. Prioritize it.

### Phase 3 — HTML + robustness (2–3 weeks)
- [ ] HTML tag detection + indentation (your long-HTML pain point).
- [ ] Large-output → streaming-mode switch (500KB threshold).
- [ ] Golden-file test corpus for top ~50 commands; benchmark harness with latency budget.

### Phase 4 — Polish + ship (2 weeks)
- [ ] `.glimpsrc` (TOML) config: bypass list, thresholds, color themes, on/off.
- [ ] `cargo-dist` → Homebrew tap + prebuilt binaries + CI.
- [ ] Killer README with an animated demo GIF (this is worth as much as a whole phase of code for stars).
- [ ] Launch posts (Show HN, r/commandline, r/rust, Lobsters).

### Later (post-v1)
Mixed-content segmentation, URL hyperlinking, error-line pinning, fish/bash, Windows, optional AI summarization.

**Realistic total to a launchable v1: ~10–12 focused weeks** solo. The spike (Phase 0) tells us in 2 weeks whether the whole thing is pleasant or a fight.

---

## 8. How this fixes YOUR actual problem

You said: *"I can't figure out the diff between input text and the output, especially when the output is too long in HTML."*

The features that solve this specifically:
1. **OSC-133 prompt markers + the command/output separator** (Phase 0 + Phase 2). A clear divider line with a timestamp between what you typed and what came back. This is *the* fix for "where does my command end and the output begin." It's also technically the backbone of the whole tool — so it's not a side feature, it's the core.
2. **Content-type badge** — a little `[HTML]` / `[JSON]` tag so you instantly know what you're looking at.
3. **HTML indentation** (Phase 3) — long HTML stops being one white wall of text and becomes a readable, indented tree.
4. **The `GLIMPS=0` off-switch** — when you don't want any of it, one env var and you're back to raw.

You also said you're "too lazy" to install other tools and want to fix it yourself rather than adopt something. Fair — but note: **ChromaTerm already does ~70% of the input/output-separation + coloring today, with zero code from you.** If your goal is *just to fix your own terminal this week*, I can set up ChromaTerm (or even a 20-line zsh + OSC-133 prompt-marker config) for you in an afternoon. If your goal is *to build and ship GLIMPS as a project*, that's the 10–12 week plan above. **These are two different goals — tell me which one you actually want** (Section 10).

---

## 9. Risks & honest concerns

- **100K stars is unlikely regardless of execution.** The space's best tool (bat) is at ~59k. Plan for a useful tool with a real (smaller) audience; treat 100K as a lottery ticket that good marketing buys you a chance at, not a target you engineer toward.
- **You sit between the user and everything they type** — including secrets and SSH. One corruption bug and people uninstall and never return. Trust is fragile here. This raises the quality bar a lot.
- **Latency on interactive redraw** is a real engineering risk (vim/htop pushing bytes through your loop). Must have a true zero-work fast path.
- **Rust + PTY + terminal escape sequences is genuinely hard** for a first big project. Doable, but it's intermediate-to-advanced, not beginner. Expect the escape-sequence handling (`vte`) to be the gnarliest part.
- **Maintenance burden:** a tool touching every command will get bug reports for every weird command/terminal combo. That's the cost of this category.

None of these are dealbreakers. They're the reasons it's worth doing — the moat is that it's hard and trust-sensitive.

---

## 10. Decision points — what I need from you

Pick so I can move:

1. **Goal:** (a) actually ship GLIMPS as a project [10–12 wks], or (b) just fix your own terminal readability this week [1 afternoon, possibly with ChromaTerm/a config]? Or (c) build GLIMPS but start by scratching your own itch first?
2. **If shipping:** do you want me to start with the **Phase 0 PTY spike** right now (a real, runnable Rust prototype that wraps your shell), so we find out in days whether it feels good?
3. **Name:** keep **GLIMPS**? (Check name availability on crates.io / Homebrew / GitHub before we commit — I can do that.)
4. **Scope for v1:** agree to cut mixed-content segmentation and URL hyperlinking from v1 (ship JSON + logs + HTTP + separator first)?

---

## Appendix — sources

- zsh hooks (preexec/precmd run before/after, cannot capture output): https://github.com/rothgar/mastering-zsh/blob/master/docs/config/hooks.md
- ChromaTerm — PTY-wraps your shell, works with interactive/SSH (proves the model): https://github.com/hSaria/ChromaTerm and https://github.com/tunnelsup/chromaterm
- `portable-pty` (Rust PTY crate): https://docs.rs/portable-pty/latest/portable_pty/
- PTY + OSC sequences in Rust (real-time capture pattern): https://developerlife.com/2025/08/10/pty-rust-osc-seq/
- Spawning a PTY in Rust ("broll" walkthrough): https://dev.to/mellorb/spawning-a-pty-in-rust-how-broll-captures-your-terminal-without-you-noticing-38mp
- `bat` (~59k stars, the popularity ceiling of this space): https://github.com/sharkdp/bat
- `jq` (~35k): https://github.com/jqlang/jq
- `grc` generic colouriser: https://github.com/garabik/grc
