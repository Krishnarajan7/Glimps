# GLIMPS

**Zero-config smart terminal output formatter.** GLIMPS runs your shell inside a PTY it
owns, auto-detects the content flowing past (JSON, HTML, logs, HTTP responses), and
reformats it with structure, color, and a clear command/output separator — no manual
piping, no flags.

> Status: **pre-alpha / in development.** Not yet released. macOS + zsh first.

## Why
Terminal output is a flat monochrome wall. You can't tell where your command ends and the
output begins, and long JSON/HTML is unreadable. Existing tools (`jq`, `bat`, `glow`) each
fix one format but require you to know what's coming and pipe it manually. GLIMPS does it
automatically, for everything, transparently.

## How it works (the honest version)
GLIMPS is a **PTY session supervisor**, like ChromaTerm/`script`/`tmux` — *not* a shell
hook. zsh hooks can't intercept command output; owning the PTY can. It uses OSC-133 markers
to know exactly where command output begins, so it never touches your prompt or input.
Full rationale: [`GLIMPS-PLAN.md`](./GLIMPS-PLAN.md).

## Repo map
| File | What |
|---|---|
| [`GLIMPS-PLAN.md`](./GLIMPS-PLAN.md) | R&D findings, feasibility, tech-stack rationale |
| [`ROADMAP.md`](./ROADMAP.md) | Versioned plan (v0.1 → v2.0) |
| `src/pty.rs` | The PTY supervisor (the heart) |
| `src/format/` | All output transforms (the one formatting seam) |
| `src/terminal.rs` | Raw-mode guard + size |

## Build & run (Phase 0 spike: transparent passthrough)
Requires Rust (`curl https://sh.rustup.rs -sSf | sh`).

```bash
cargo run            # launches your shell inside GLIMPS; type `exit` to leave
```

The spike does *only* transparent pass-through — it should feel exactly like a normal
terminal. That's the gate before any formatting is added.

## License
MIT — see [`LICENSE`](./LICENSE).
