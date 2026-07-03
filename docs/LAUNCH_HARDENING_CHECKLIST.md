# GLIMPS Launch Hardening Checklist

This is the working checklist for taking GLIMPS from a strong local beta to a
public beta that strangers can install without us hand-holding them. Keep this
file honest: mark an item done only after it is verified on the current code.

Legend: todo / in progress / done

## 0. Current State

- done: Core PTY supervisor wraps an interactive shell.
- done: OSC-133 zoning keeps prompt/input separate from command output.
- done: Command headers, badges, JSON, HTML, logs, HTTP status, diffs, and stack
  trace highlighting exist.
- done: Formatter safety net covers golden files, property tests, corpus tests,
  and PTY integration tests.
- done: `anyhow` lower bound is pinned to the RustSec-patched `1.0.103` release.
- done: `portable-pty` is upgraded to `0.9.0`, removing the unmaintained
  `serial` transitive dependency.
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
  - `vim`, `nvim`, `less`, `man`, `ssh`, `tmux`, `fzf`, `top`, `htop`
- todo: Verify lifecycle behavior repeatedly:
  - `exit`
  - Ctrl-D
  - Ctrl-C during foreground command
  - terminal resize
  - SIGTERM to GLIMPS

## 3. Release And Install

- todo: Create or verify `Krishnarajan7/homebrew-tap`.
- todo: Add the required GitHub secret for Homebrew publishing.
- todo: Run the release workflow from a test tag.
- todo: Confirm generated macOS arm64, macOS x64, Linux arm64, and Linux x64
  artifacts.
- todo: Confirm the shell installer puts `glimps` in the expected location.
- todo: Do not advertise `brew install glimps` until the tap works end-to-end.
- todo: Add a short rollback/uninstall note to the release notes.

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

- todo: Generate and commit the demo GIF from `demo/glimps.tape`.
- done: Add a short "known beta limits" section to the README.
- todo: Add examples for JSON, HTML, logs, HTTP, diff, and stack traces.
- todo: Make README claims match shipped install reality.
- todo: Keep zsh-only support explicit until bash/fish integration lands.

## 6. Later, Not Public-Beta Blocking

- todo: bash support.
- todo: fish support.
- todo: mixed-content segmentation, such as log lines containing JSON.
- todo: OSC 8 URL hyperlinking.
- todo: Windows support, only after Unix behavior is boringly stable.
