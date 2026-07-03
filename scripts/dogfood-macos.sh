#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/debug/glimps"

usage() {
  cat <<'EOF'
Usage: scripts/dogfood-macos.sh [check|session]

check     Build and run repo-local automated checks. Does not install anything.
session   Start an interactive GLIMPS-wrapped zsh using a temporary ZDOTDIR.

Neither mode edits ~/.zshrc, installs GLIMPS globally, or changes your login shell.
EOF
}

require_macos_or_warn() {
  if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "warning: this dogfood helper is written for macOS; continuing anyway." >&2
  fi
}

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing required tool: $tool" >&2
    exit 1
  fi
}

run_check() {
  require_tool cargo
  require_tool zsh

  cd "$ROOT"
  cargo fmt --all -- --check
  cargo clippy --all-targets --all-features -- -D warnings
  cargo test --all --all-features
  cargo bench --no-run

  if command -v cargo-audit >/dev/null 2>&1; then
    cargo audit
  else
    echo "note: cargo-audit is not installed; skipping dependency advisory check." >&2
  fi
}

run_session() {
  require_tool cargo
  require_tool zsh

  cd "$ROOT"
  cargo build

  local tmp
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/glimps-dogfood.XXXXXX")"
  cleanup() {
    rm -rf "$tmp"
  }
  trap cleanup EXIT

  cat >"$tmp/.zshrc" <<EOF
export PROMPT='glimps-dogfood %~ %# '
eval "\$("$BIN" init zsh)"
EOF

  cat >"$tmp/.glimpsrc" <<'EOF'
enabled = true
color = true
separator = true
timestamp = true
EOF

  cat <<EOF
Starting a disposable GLIMPS session.

Try these commands:
  echo '{"alpha":1,"items":[2,3]}'
  printf 'INFO boot\nWARN disk\nERROR boom\n'
  printf 'HTTP/1.1 404 Not Found\n'
  git --no-pager diff -- README.md
  man printf
  vim README.md
  printf 'A\x01\x02B'

Exit with: exit
EOF

  ZDOTDIR="$tmp" HOME="$tmp" GLIMPSRC="$tmp/.glimpsrc" SHELL="$(command -v zsh)" "$BIN"
}

main() {
  require_macos_or_warn
  case "${1:-check}" in
    check) run_check ;;
    session) run_session ;;
    -h|--help|help) usage ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
}

main "$@"
