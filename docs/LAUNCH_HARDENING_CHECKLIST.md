# GLIMPS Launch Hardening Checklist

This is the working checklist for taking GLIMPS from "good on our machines" to
"safe for strangers to try." Keep this file honest: mark an item done only after
it is verified on the current code.

The checklist is intentionally plain. It is here to prevent launch excitement
from turning into install surprises.

Legend: todo / in progress / done

## 0. Current State

- done: Competitive product gap analysis added in
  `docs/COMPETITIVE_PRODUCT_GAP_ANALYSIS.md`.
- done: Core PTY supervisor wraps an interactive shell.
- done: OSC-133 zoning keeps prompt/input separate from command output.
- done: Command headers, badges, JSON, HTML, logs, HTTP status, diffs, and stack
  trace highlighting exist.
- done: Formatter safety net covers golden files, property tests, corpus tests,
  and PTY integration tests.
- done: `anyhow` lower bound is pinned to the RustSec-patched `1.0.103` release.
- done: `portable-pty` is upgraded to `0.9.0`, removing the unmaintained
  `serial` transitive dependency.
- done: Linux/macOS Rust CI, RustSec audit, and website quality CI are green on
  consecutive `main` commits.
- done: Website dependencies are reduced to the packages used by the static
  application; clean install, lint, typecheck, build, and npm audit are gated.
- done: `glimps doctor` provides read-only installation and runtime diagnostics.
- in progress: Public beta dogfood on a clean Mac outside this development
  machine. Use `docs/FRESH_MAC_DOGFOOD.md` and `scripts/dogfood-macos.sh`.
- todo: Release/tap flow verified from an actual version tag.

## 1. Security And Dependency Hygiene

- done: Patch `RUSTSEC-2026-0190` by resolving `anyhow >= 1.0.103`.
- done: Resolve `RUSTSEC-2017-0008` by upgrading `portable-pty` to `0.9.0`,
  which removes the transitive `serial` dependency.
- todo: Run `cargo audit` before every public release.
- todo: Keep the no-telemetry promise enforceable: do not add logging, analytics,
  crash upload, or persistent capture of terminal contents.
- todo: Add a release checklist item that confirms no new dependency performs
  network I/O in normal GLIMPS runtime.
- done: Add `SECURITY.md`, `CODE_OF_CONDUCT.md`, and trust-boundary CODEOWNERS.
- done: Pin GitHub Actions to full commits, disable ordinary checkout credential
  persistence, and default workflow permissions to read-only.
- done: Generate signed GitHub provenance attestations for release artifacts.
- done: Add Dependabot monitoring for Cargo, npm, and GitHub Actions.
- todo: Enable GitHub private vulnerability reporting and a protected `main`
  ruleset in repository settings.

## 2. Fresh-Machine Dogfood

- done: Add a non-invasive dogfood helper that does not install GLIMPS globally
  or edit `~/.zshrc`.
- todo: Test on a clean Apple Silicon Mac.
- todo: Test on an Intel Mac, if available.
- todo: Verify Terminal.app.
- todo: Verify iTerm2.
- todo: Verify Ghostty.
- todo: Verify with popular zsh setups:
  - Oh My Zsh
  - Starship
  - Powerlevel10k
  - zsh-autosuggestions
  - zsh-syntax-highlighting
- todo: Verify commands that should be bypassed or pass through cleanly:
  - `vim`, `nvim`, `less`, `ssh`, `tmux`, `fzf`, `top`, `htop`
- todo: Verify lifecycle behavior repeatedly:
  - `exit`
  - Ctrl-D
  - Ctrl-C during foreground command
  - terminal resize
  - SIGTERM to GLIMPS

## 3. Release And Install

- todo: Create or verify `Krishnarajan7/homebrew-tap`.
- todo: Add the required GitHub secret for Homebrew publishing.
- done: Add a repo-local release-readiness preflight script that does not
  install GLIMPS globally or start an interactive session.
- done: Add a public-beta release runbook for preflight, dogfood, demo,
  Homebrew, release candidate, public tag, and rollback.
- todo: Run the release workflow from a test tag.
- todo: Confirm generated macOS arm64, macOS x64, Linux arm64, and Linux x64
  artifacts.
- todo: Confirm the shell installer puts `glimps` in the expected location.
- todo: Do not advertise `brew install glimps` until the tap works end-to-end.
- todo: Add a short rollback/uninstall note to the release notes.
- todo: Verify each downloaded artifact with `gh attestation verify` during the
  release-candidate run.

## 4. Performance

- todo: Refresh Criterion baselines after the current formatter architecture.
- todo: Record target budgets for:
  - plain pass-through
  - plain text with streaming line colorizers
  - JSON formatting
  - diff formatting
  - stack trace formatting
- todo: Add a manual stress run with large JSON, long logs, binary blobs, and TUI
  redraw-heavy commands.
- todo: Keep the unit latency guard for pathological regressions.

## 5. Product Polish

- in progress: Close the P0 product gaps from
  `docs/COMPETITIVE_PRODUCT_GAP_ANALYSIS.md`.
- done: Add semantic HTML colors for delimiters, element names, attributes,
  quoted values, and raw CSS/JS/title text.
- done: Add a repo-local demo rendering script for `demo/glimps.tape`.
- todo: Generate and commit the demo GIF from `demo/glimps.tape`.
- done: Add a short "known beta limits" section to the README.
- done: Add examples for JSON, HTML, logs, HTTP, diff, and stack traces.
- done: Add a curl/HTTP response formatter that separates status line, headers,
  cookies, redirects, and JSON/HTML body formatting.
- done: Improve TUI UX with a safe entry breadcrumb, without coloring inside
  full-screen apps.
- todo: Add explicit TUI exit breadcrumbs when they can be emitted without
  racing the returning prompt.
- done: Add a small clean-exit farewell message after interactive sessions.
- done: Add successful `cd` breadcrumbs using the shell-reported post-command
  working directory.
- done: Add command-aware `find` path coloring for easier scanning.
- done: Add more command-aware formatters for high-signal commands such as
  `ls`, `du`, `df`, `ps`, and `dig`, while keeping pass-through as the default.
- done: Add man/help rendering that works with pager behavior instead of
  dumping unreadable plain output.
- done: Add command duration and exit-code display when the shell reports an
  exit code.
- done: Add failure summary for non-zero command exits.
- done: Add command-aware Markdown rendering for `cat README.md` / docs output.
- done: Add command-aware YAML/TOML/INI/dotenv config formatting for reader
  commands.
- done: Add command-aware CSV/TSV table coloring for reader commands.
- done: Add command-aware SQL query coloring for reader commands.
- done: Add JSON-lines support for streaming logs and API output.
- done: Add source-code syntax coloring for common extensions in reader
  commands.
- done: Add SQL result/table coloring for common database CLI output.
- done: Add Git status, branch, and log coloring for common developer workflows.
- done: Add Git diff stat, numstat, and name-status coloring.
- done: Make README claims match shipped install reality.
- done: Keep zsh-only support explicit until bash/fish integration lands.

## 6. Contributor Growth

- done: Add `CONTRIBUTING.md` with architecture map and formatter rules.
- done: Add GitHub issue templates for bugs, formatter requests, good-first
  formatter tasks, and docs/release work.
- done: Add a pull request template with safety and verification prompts.
- todo: Add labels for `good first issue`, `formatter`, `command-aware`,
  `safety`, `docs`, and `release`.
- done: Draft 10 scoped beginner issue specs from the competitive gap analysis
  in `docs/GOOD_FIRST_ISSUES.md`.
- todo: Create live GitHub issues from `docs/GOOD_FIRST_ISSUES.md` after labels
  exist.
- done: Add a formatter design guide for contributors.
- done: Add a Code of Conduct, private security-reporting policy, CODEOWNERS,
  compatibility matrix, and known-issues page.

## 7. Later, Not Public-Beta Blocking

- done: bash support (beta; raw `DEBUG` trap replacement remains a documented
  compatibility caveat).
- todo: fish support.
- todo: mixed-content segmentation, such as log lines containing JSON.
- todo: OSC 8 URL hyperlinking.
- todo: Windows support, only after Unix behavior is boringly stable.
