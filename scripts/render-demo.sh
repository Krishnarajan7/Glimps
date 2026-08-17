#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="$ROOT/target/debug"

# Each entry is "<tape>:<expected gif>".
TAPES=(
  "demo/glimps.tape:demo/glimps.gif"
  "demo/failure.tape:demo/failure.gif"
)

usage() {
  cat <<'EOF'
Usage: scripts/render-demo.sh [tape ...]

Builds the repo-local debug binary, puts target/debug at the front of PATH for
this process only, then renders the demo tapes with VHS.

With no arguments it renders every tape:

  demo/glimps.tape   -> demo/glimps.gif    (formatting: header, JSON, logs)
  demo/failure.tape  -> demo/failure.gif   (failure intelligence)

Pass one or more tape paths to render only those.

Review each render against docs/VISUAL_EVIDENCE_CHECKLIST.md before committing it.

This does not install GLIMPS globally, edit ~/.zshrc, or change your login shell.
EOF
}

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing required tool: $tool" >&2
    exit 1
  fi
}

case "${1:-}" in
  -h|--help|help)
    usage
    exit 0
    ;;
esac

# An explicit tape list overrides the default set. The expected output is read
# from the tape's own `Output` directive, so the two can never drift.
if [[ $# -gt 0 ]]; then
  TAPES=()
  for tape in "$@"; do
    if [[ ! -f "$ROOT/$tape" ]]; then
      echo "no such tape: $tape" >&2
      exit 2
    fi
    out="$(awk '$1 == "Output" { print $2; exit }' "$ROOT/$tape")"
    if [[ -z "$out" ]]; then
      echo "tape has no Output directive: $tape" >&2
      exit 2
    fi
    TAPES+=("$tape:$out")
  done
fi

require_tool cargo
require_tool zsh
require_tool vhs
require_tool python3

cd "$ROOT"
cargo build --bin glimps

for entry in "${TAPES[@]}"; do
  tape="${entry%%:*}"
  gif="${entry##*:}"

  PATH="$BIN_DIR:$PATH" vhs "$tape"

  if [[ ! -s "$ROOT/$gif" ]]; then
    echo "demo render did not produce a non-empty $gif" >&2
    exit 1
  fi

  echo "Rendered $ROOT/$gif"
done
