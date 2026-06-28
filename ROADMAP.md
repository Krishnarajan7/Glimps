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
- ☑ Command/output **separator line + timestamp** (the core fix for input-vs-output)
- ☑ Content-type **badge** (`[JSON]` / `[HTML]` / `[LOG]`)
- ☑ JSON detect + pretty-print (colored keys/values)
- ☑ HTML detect + indentation
- ☑ `GLIMPS=0` off-switch
- ☑ `glimps init zsh` one-line enable

Exit criteria: you use it daily for a week without turning it off.

## v0.2 — Safety & breadth (still local / early testers)
Goal: it never gets in the way, on any command.

- ◐ Interactive bypass (vim, htop, less, fzf, …) — pass through untouched
      (done via alternate-screen detection; name-based cases like `ssh` still TODO)
- ◐ Binary output pass-through (NUL-byte detection done; non-UTF8 scan TODO)
- ◐ ANSI-already-present pass-through (a control byte ends/declines a buffered run;
      no explicit whole-run gate yet)
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
- ◐ Golden-file test corpus (JSON/HTML goldens + ~14 common-command byte-safety
      fixtures; extend toward the full top-50)
- ☑ `criterion` benchmarks + enforced latency budget in CI (latency-budget test
      runs on Linux+macOS; benches kept compiling via `cargo bench --no-run`)
- ☑ Color themes — no-color / minimal mode via `color = false`

Exit criteria: CI green on Linux + macOS; latency budget held.

## v1.0 — Public launch
Goal: `brew install glimps`, a README that sells it, and a demo GIF that spreads.

- ◐ Homebrew formula + prebuilt binaries via `cargo-dist` + release CI
      (configured: `dist-workspace.toml` + `.github/workflows/release.yml` build
      macOS+Linux binaries, a shell installer, and a Homebrew formula on version
      tags. To go live: create the `glimps-sh/homebrew-tap` repo + token secret,
      bump the version, and push a `vX.Y.Z` tag.)
- ◐ Polished README (done) with an animated before/after demo GIF (still to record)
- ☑ Docs: install, config, safety/privacy statement, uninstall (README + `.glimpsrc.example`)
- ☑ Hardened: 36-fixture command corpus (ANSI/Unicode/overstrike/tables/control-only/
      empty) all byte-preserved; password-prompt test; fuzz sweep over text+ANSI
      (10k cases) + arbitrary-bytes/arbitrary-config property tests prove no panic
      and no byte loss. (Corpus can keep growing toward a literal top-50.)
- ☐ Launch: Show HN, r/commandline, r/rust, Lobsters

Exit criteria: a stranger installs it in <30s and it "just works."

## v1.x — Scale & reach (post-launch, demand-driven)
- ☐ Mixed-content segmentation (the multi-format screenshot case)
- ☐ URL highlighting / clickable hyperlinks (OSC 8)
- ☐ Error-line pinning / summary panel for long output
- ☐ bash + fish support
- ☐ Windows support (PTY + ANSI differences)
- ☐ More formatters via the `add-formatter` skill: YAML, CSV, SQL, stack traces, diffs

## v2.0 — Ambition (only if v1 has real traction)
- ☐ Optional, **local/offline**, opt-in AI output summarization (privacy-preserving)
- ☐ Plugin API for community formatters
- ☐ Per-project / per-directory config profiles

---

## Notes
- Anything that can't be turned off, or that risks corrupting output, does not ship — see
  the invariants in `CLAUDE.md`.
- Cut without guilt if v1 slips: mixed-content segmentation and URL hyperlinking are the
  first things to defer. JSON + logs + HTTP + the separator is the 80% that matters.
