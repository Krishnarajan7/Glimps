#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STRICT=0
OPTIONAL_MISSING=0

usage() {
  cat <<'EOF'
Usage: scripts/release-readiness.sh [--strict]

Runs the repo-local gates that should pass before cutting a public beta:
  - cargo fmt --all -- --check
  - cargo clippy --all-targets --all-features -- -D warnings
  - cargo test --all --all-features
  - cargo bench --no-run
  - release config sanity checks

Optional tools:
  - cargo-audit, if installed, checks RustSec advisories
  - cargo-dist's `dist`, if installed, runs `dist plan`

By default, missing optional tools print warnings. With --strict, missing
optional tools fail the script.

This does not install GLIMPS globally, edit ~/.zshrc, or start a GLIMPS session.
EOF
}

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing required tool: $tool" >&2
    exit 1
  fi
}

optional_missing() {
  local tool="$1"
  OPTIONAL_MISSING=1
  echo "warning: optional release tool not found: $tool" >&2
  echo "         install it before an actual public release, or rerun without --strict for local dev." >&2
}

check_contains() {
  local file="$1"
  local needle="$2"
  local label="$3"

  if ! grep -Fq "$needle" "$file"; then
    echo "release config check failed: $label" >&2
    echo "  missing '$needle' in $file" >&2
    exit 1
  fi
}

case "${1:-}" in
  --strict)
    STRICT=1
    ;;
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

cd "$ROOT"

cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all --all-features
cargo bench --no-run

if command -v cargo-audit >/dev/null 2>&1; then
  cargo audit
else
  optional_missing cargo-audit
fi

check_contains dist-workspace.toml 'tap = "Krishnarajan7/homebrew-tap"' "Homebrew tap target"
check_contains dist-workspace.toml '"aarch64-apple-darwin"' "Apple Silicon artifact target"
check_contains dist-workspace.toml '"x86_64-apple-darwin"' "Intel Mac artifact target"
check_contains dist-workspace.toml '"aarch64-unknown-linux-gnu"' "Linux arm64 artifact target"
check_contains dist-workspace.toml '"x86_64-unknown-linux-gnu"' "Linux x64 artifact target"
check_contains .github/workflows/release.yml 'HOMEBREW_TAP_TOKEN' "Homebrew tap publishing secret"

if command -v dist >/dev/null 2>&1; then
  dist plan
else
  optional_missing "cargo-dist binary: dist"
fi

if [[ "$STRICT" == "1" && "$OPTIONAL_MISSING" == "1" ]]; then
  echo "strict release readiness failed because optional release tools are missing." >&2
  exit 1
fi

echo "Release-readiness preflight completed."
