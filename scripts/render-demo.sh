#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="$ROOT/target/debug"
GIF="$ROOT/demo/glimps.gif"

usage() {
  cat <<'EOF'
Usage: scripts/render-demo.sh

Builds the repo-local debug binary, puts target/debug at the front of PATH for
this process only, then renders demo/glimps.tape with VHS.

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
  "")
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac

require_tool cargo
require_tool zsh
require_tool vhs

cd "$ROOT"
cargo build --bin glimps

PATH="$BIN_DIR:$PATH" vhs demo/glimps.tape

if [[ ! -s "$GIF" ]]; then
  echo "demo render did not produce a non-empty $GIF" >&2
  exit 1
fi

echo "Rendered $GIF"
