# Changelog

All notable changes to GLIMPS are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Table column headers recolored** — the header row of a result or CSV/TSV
  table (mysql/psql/sqlite `desc`, `select`, delimited files) is now bold
  white instead of cyan, so column names read clearly as headers and no
  longer compete with the sky-blue string values below them. JSON keys and
  command names keep their cyan. Applies on every platform (one shared theme).
- **Remote commands over ssh** — `ssh host cmd` (no `-t`, no `-N`) is no
  longer treated as an interactive session: the remote command's output gets
  the same treatment the local command would, including its command view
  (`ssh host df -h` is the `df` view) and the failure footer's decode. Plain
  sessions (`ssh host`, `-t`) stay untouched, and a remote command that reads
  secrets (`ssh host cat .env`) stays raw pass-through. A leading `!` no
  longer hides the real command from the classifier.

- **Report lines** — after every other formatter has declined, a plain
  `Label: value` line (`timedatectl`, `hostnamectl`, `sw_vers`) or
  `KEY=value` line (`env`) gets its field name painted and its separator
  dimmed; the value is never touched. Off with `reports = false` under
  `[formatters]`.

- **Homebrew listings** — `brew services list` rows are colored (name, the
  `started`/`none`/`error` status, user, launchd plist or systemd unit path)
  and `brew list --versions`, `brew outdated`, `brew leaves`, `brew tap`,
  `brew deps --tree` and `brew uses` get formula names, versions and
  comparators told apart. Homebrew's own colors (`==>` headings, the green
  `started`, red `Error:`) pass through untouched; `install`, `upgrade`,
  `info`, `doctor`, `--json` output and `brew help` are left as they were.

- **Database shell awareness** — inside an interactive `mysql`/`psql`/`sqlite3`
  session every result table is understood structurally, not just the first:
  header rows are keyed under each table's top rule (including `desc`'s `Null`
  column and single-column `show tables`), multi-line SQL cells such as
  `SHOW CREATE TABLE` are syntax-colored (keywords, backtick identifiers,
  numbers, strings), the client's own status lines (`13 rows in set`,
  `Empty set`, `Database changed`, `Query OK`, `Bye`) are dimmed, and
  `ERROR 1064 (42000):` prefixes are painted red with the message left
  readable. The prompt and the echo of what you type pass through as-is.
  The `status` report reads as a report, not a table: the client banner
  keeps its version gold, each `Label:` takes a lavender label colour with
  a dim colon, and values are painted by kind (numbers, paths, text) — the
  same for the `Threads: 3  Questions: 68 …` summary line.

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

- **Expression column headers are keyed** — a bare `select coalesce(a,'x')`
  names the column with the literal expression, so the header cell holds
  commas, quotes and parens. That row sits directly under the top rule, which
  proves it is the header, so it now takes the bold-white header colour
  instead of sharing the sky-blue value colour of the data below it.
- **Typed SQL is no longer recoloured** — an echoed database-shell prompt or
  continuation (`mysql> …`, `    -> …`, `sqlite> …`, psql `db=> …`) is left
  exactly as typed. An indented continuation line such as
  `->            col AS alias` used to open a two-space gap that read as a
  two-column table and got cell colour; GLIMPS now declines all prompt/echo
  lines, and result tables keep their colouring.
- **A wide row no longer disables an interactive SQL session** — one result
  line longer than `line_cap` (64 KiB: a big `TEXT` cell, `SHOW CREATE TABLE`,
  `GROUP_CONCAT`) used to latch the whole `mysql`/`psql`/`sqlite3` run to
  pass-through, so every table printed afterwards rendered plain until you
  exited the shell. The long line now degrades only itself and formatting
  resumes on the next line.
- **`glimps doctor` inside a dogfood session** — the integration check
  recognises the rc line however the binary is spelled (`glimps init zsh`,
  a full path, or a variable such as the dogfood rc's
  `"$GLIMPS_DOGFOOD_BIN" init zsh`) instead of failing on the literal text.
  The session check now names the binary that actually runs the session
  (exported as `GLIMPS_BIN`) when `glimps` on PATH is a different install,
  and `scripts/dogfood-macos.sh` puts the repo build first on PATH so the
  doctor you run inside it is the one you just built.
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
