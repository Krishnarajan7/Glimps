# GLIMPS — Versioned Roadmap

The releasing strategy is deliberate: **first make it solve YOUR exact problem end-to-end
(v0.1), then widen it into the full product (v1.0), then scale.** Each version is a thing a
user can actually install and benefit from — no half-states shipped.

Legend: ☐ todo ◐ in progress ☑ done

---

## v0.1 — "Scratch the itch" (private / your machine)
Goal: **you can tell input from output, and long JSON/HTML is readable.** Mac + zsh only.
This is the version that fixes the problem you started with.

- ☑ Phase 0 spike: transparent PTY supervisor (shell runs inside GLIMPS, feels native)
- ☑ OSC-133 prompt markers + detection → know where command OUTPUT starts/ends
      (zone scanner + `glimps init zsh` precmd/preexec marker emission)
- ☑ **Colored command header + timestamp** (THE core fix for input-vs-output): the
      typed command is captured (preexec OSC marker) and shown syntax-colored
      before its output — command name / strings / flags. Falls back to a dim rule
      when no command is captured.
- ☑ Content-type **badge** (`[JSON]` / `[HTML]` / `[LOG]`)
- ☑ JSON detect + pretty-print (colored keys/values)
- ☑ HTML detect + indentation
- ☑ `GLIMPS=0` off-switch
- ☑ `glimps init zsh` one-line enable

Exit criteria: you use it daily for a week without turning it off.

## v0.2 — Safety & breadth (still local / early testers)
Goal: it never gets in the way, on any command.

- ☑ Interactive bypass (vim, htop, less, fzf, ssh, …) — pass through untouched
      (alternate-screen detection + name-based bypass via the captured command;
      bypass list is configurable in `.glimpsrc`)
- ☑ Binary output pass-through (NUL, other non-text C0/DEL control bytes, AND
      invalid-UTF8 detection — a multibyte char split across chunks is NOT
      misclassified; binary streams verbatim with no header/badge/color)
- ☑ ANSI-already-present pass-through (the scanner routes every escape byte to
      `Pass`, finalizing the run, so Output never contains ESC; JSON-with-ESC falls
      back to verbatim, and the streaming colorizer never re-colors a line that
      already carries ANSI or binary)
- ☑ `isatty()` pipe-safety (formatting off when stdout isn't a terminal)
- ☑ Log severity coloring (ERROR/WARN/INFO/DEBUG), streaming line-by-line
- ☑ HTTP status code highlighting
- ☑ Streaming mode for unbounded output (`tail -f`, `docker logs -f`)
      (plain text streams line-by-line; only complete lines colored; long lines capped)

Exit criteria: zero interference on the top 50 common commands.

## v0.3 — Robustness & config
Goal: trustworthy enough to hand to strangers.

- ◐ `.glimpsrc` (TOML): thresholds, enable/disable per type, color/separator/
      timestamp toggles, master switch (done). Name-based bypass list still TODO
      (needs command-name capture).
- ☑ Large-output streaming switch (buffer/line caps → verbatim past threshold)
- ☑ Golden-file test corpus (JSON/HTML goldens + **50** common-command byte-safety
      fixtures — incl. already-ANSI git color/graph, jq -C colored JSON, 256-color,
      CR progress bars, top/lsof/netstat tables, emoji/CJK, binary; all byte-preserved)
- ☑ `criterion` benchmarks + enforced latency budget in CI (latency-budget test
      runs on Linux+macOS; benches kept compiling via `cargo bench --no-run`)
- ☑ Color themes — no-color / minimal mode via `color = false`

Exit criteria: CI green on Linux + macOS; latency budget held.

## v1.0 — Public launch
Goal: `brew install glimps`, a README that sells it, and a demo GIF that spreads.

- ◐ Homebrew formula + prebuilt binaries via `cargo-dist` + release CI
      (configured: `dist-workspace.toml` + `.github/workflows/release.yml` build
      macOS+Linux binaries, a shell installer, and a Homebrew formula on version
      tags. To go live: create the `Krishnarajan7/homebrew-tap` repo + token secret,
      bump the version, and push a `vX.Y.Z` tag.)
- ◐ Polished README (done) with an animated before/after demo GIF — tooling done
      (`demo/glimps.tape` + `demo/README.md`, self-contained VHS script); the GIF
      itself just needs `vhs demo/glimps.tape` run on a machine with VHS installed
- ☑ Docs: install, config, safety/privacy statement, uninstall (README + `.glimpsrc.example`)
- ☑ Hardened: 52-fixture command corpus (ANSI/Unicode/overstrike/tables/control-only/
      empty/binary/already-ANSI) all byte-preserved; password-prompt test; fuzz sweep
      over text+ANSI (10k cases) + arbitrary-bytes/arbitrary-config property tests
      prove no panic and no byte loss; **end-to-end PTY integration tests**
      (`tests/pty_integration.rs`) drive the real binary through a pseudo-terminal to
      pin exit/Ctrl-D/SIGTERM termination and live JSON-format / binary-passthrough.
- ☐ Launch: Show HN, r/commandline, r/rust, Lobsters

Exit criteria: a stranger installs it in <30s and it "just works."

## v1.x — Scale & reach (post-launch, demand-driven)
- ☐ Mixed-content segmentation (the multi-format screenshot case)
- ☐ URL highlighting / clickable hyperlinks (OSC 8)
- ☐ Error-line pinning / summary panel for long output
- ☐ bash + fish support
- ☐ Windows support (PTY + ANSI differences)
- ◐ More formatters via the `add-formatter` skill: **diffs ☑** (unified-diff
      coloring, hunk-anchored detection) and **stack traces ☑** (Rust panics +
      Python tracebacks, streaming); YAML, CSV, SQL still ☐

## v2.0 — Ambition (only if v1 has real traction)
- ☐ Optional, **local/offline**, opt-in AI output summarization (privacy-preserving)
- ☐ Plugin API for community formatters
- ☐ Per-project / per-directory config profiles

---

## Notes
- Anything that can't be turned off, or that risks corrupting output, does not ship — see
  the invariants in `docs/SAFETY_INVARIANTS.md`.
- Public-beta hardening work is tracked in `docs/LAUNCH_HARDENING_CHECKLIST.md`.
- Cut without guilt if v1 slips: mixed-content segmentation and URL hyperlinking are the
  first things to defer. JSON + logs + HTTP + the separator is the 80% that matters.
