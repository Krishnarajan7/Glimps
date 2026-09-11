# Changelog

All notable changes to GLIMPS are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Database shell awareness** — inside an interactive `mysql`/`psql`/`sqlite3`
  session every result table is understood structurally, not just the first:
  header rows are keyed under each table's top rule (including `desc`'s `Null`
  column and single-column `show tables`), multi-line SQL cells such as
  `SHOW CREATE TABLE` are syntax-colored (keywords, backtick identifiers,
  numbers, strings), the client's own status lines (`13 rows in set`,
  `Empty set`, `Database changed`, `Query OK`, `Bye`) are dimmed, and
  `ERROR 1064 (42000):` prefixes are painted red with the message left
  readable. The prompt and the echo of what you type pass through as-is.

- **Windows (experimental, untested on real hardware)** — the crate builds
  and lints for `x86_64-pc-windows-msvc`: ConPTY through `portable-pty`,
  console VT input/output flags enabled for the session and restored exactly
  on exit and panic, window-size polling in place of `SIGWINCH`, `pwsh` /
  `powershell` as the default shell, `glimps init pwsh` shell integration
  with the same OSC-133 marker contract as zsh, `.exe`-suffixed command names
  resolving to the same views, a ConPTY byte-fidelity probe
  (`examples/pty_probe.rs`) and `scripts/dogfood-windows.ps1`. See
  `docs/windows.md` for the decision gate before this is offered for download.

### Fixed

- `mysql --version` (and other `<tool> --version` banners) are no longer
  painted as a result table; the version number and vendor note are colored
  instead.

## [0.1.0] - 2026-08-30

First public release. GLIMPS is a zero-config smart terminal output formatter:
it wraps your shell in a PTY it owns, finds the command/output boundary with
OSC-133 shell-integration markers, and reformats output it recognizes — never
your prompt, your typed input, or anything it isn't sure about.

### Added

- **PTY session supervisor** — runs your shell inside a PTY, with raw-mode
  restoration guaranteed on every exit path (including panics), SIGWINCH
  resize propagation, and clean termination-signal handling.
- **Command header** — a `▌` separator above each command's output repeating
  the syntax-colored command with an optional timestamp, so scrollback stays
  navigable.
- **Failure intelligence** — exit codes translated to plain language (`127` →
  command not found, `137` → SIGKILL/OOM), pipeline-stage failure warnings via
  `PIPESTATUS`, Ctrl-C reported as a notice rather than an error, and the
  actual error line pinned under the footer with `file:line` and scroll
  distance.
- **Content formatters** — JSON (pretty-printed, key order preserved), HTML
  trees, streaming log severity coloring, full HTTP responses (status,
  headers, cookies, redirects, body), unified diffs, stack traces (Rust,
  Python), Git status/branch/log/stat output, Markdown, YAML/TOML/INI/dotenv
  config files, `.gitignore`/`.gitleaksignore`, CSV/TSV/PSV tables, SQL files,
  JSON-lines streams, common source-code files, and database CLI result
  tables.
- **Command-aware views** — focused formatting for `cd`, `ls`, `find`, `du`,
  `df`, `ps`, `ping`, `dig`/`nslookup`, `grep`/`rg`, `lsof`, `ifconfig`,
  `netstat -rn`, `scutil --dns`, `networksetup`, `diskutil info`,
  `launchctl list`, `pmset -g`, `man`/`apropos`/`whatis`, `kubectl get pods`,
  and cargo build/test/check summaries.
- **Shell integration** — `glimps init zsh` (primary) and `glimps init bash`
  (beta, `DEBUG`-trap based with trap chaining) install the OSC-133 markers;
  one guarded line in your rc file.
- **`glimps setup`** — guided, consent-based install of the shell integration:
  shows the exact rc-file change, asks before touching anything, takes a
  timestamped backup, and writes atomically.
- **`glimps off` / `glimps on`** — pause and resume formatting instantly for
  the current session, signalled in-band over a private OSC that is
  provenance-guarded (only a real `glimps` command cycle can flip it — output
  from a `cat`, `curl`, or SSH remote cannot); no restart needed.
- **`glimps doctor`** — read-only diagnostics for binary, shell, rc-file
  integration, config, `PATH`, and TTY state; warns when the integration line
  sits below a plugin manager or prompt framework (the double-sourcing
  footgun), and when another shell integration that emits OSC-133 marks
  (iTerm2, Ghostty, Warp) is present.
- **Configuration** — optional `~/.glimpsrc` (TOML) with per-formatter
  toggles, color/separator/timestamp switches, and buffer limits; missing or
  broken config falls back to defaults. `theme = "light"` selects a palette
  tuned for light terminal backgrounds, and the `NO_COLOR` convention is
  honored (structure kept, escapes dropped).
- **Output-inflation guard** — if pretty-printing a document (JSON/HTML) would
  exceed `pretty_max_lines` (default 4000), the original bytes are shown
  instead; GLIMPS never floods scrollback and never hides output.
- **Safety guarantees** — pass-through by default when uncertain; never
  reformats binary output, already-colored output, non-TTY destinations,
  no-echo password prompts, or full-screen apps (vim/ssh/htop/less/fzf);
  secret-printing commands pass through raw; byte-safety enforced by property
  tests and a golden corpus; `GLIMPS=0` instant off-switch.
- **Privacy** — no telemetry, no network calls, no persistent logging of
  terminal contents. Ever.
- **Distribution** — prebuilt binaries for Apple Silicon and Intel macOS and
  Linux (aarch64/x86_64), a shell installer, a Homebrew tap
  (`Krishnarajan7/homebrew-tap`), and `cargo install glimps` from crates.io.

[Unreleased]: https://github.com/Krishnarajan7/Glimps/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Krishnarajan7/Glimps/releases/tag/v0.1.0
