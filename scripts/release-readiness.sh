#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STRICT=0
OPTIONAL_MISSING=0

usage() {
  cat <<'EOF'
Usage: scripts/release-readiness.sh [--strict] [--tag vX.Y.Z]

Runs the repo-local gates that should pass before cutting a public beta:
  - cargo fmt --all -- --check
  - cargo clippy --all-targets --all-features -- -D warnings
  - cargo test --all --all-features
  - cargo bench --no-run
  - website lint, typecheck, build, and production dependency audit
  - release config sanity checks
  - (with --tag) Cargo.toml version matches the release tag

Options:
  --strict         missing optional tools (cargo-audit, dist) fail the script
  --tag vX.Y.Z     assert Cargo.toml's package version equals X.Y.Z before you
                   tag. cargo-dist rejects a tag whose version doesn't match a
                   workspace package version, so run this before every release
                   tag to catch a forgotten version bump early.

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

EXPECT_TAG=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --strict)
      STRICT=1
      shift
      ;;
    --tag)
      EXPECT_TAG="${2:-}"
      if [[ -z "$EXPECT_TAG" ]]; then
        echo "--tag requires an argument, e.g. --tag v0.1.0" >&2
        exit 2
      fi
      shift 2
      ;;
    --tag=*)
      EXPECT_TAG="${1#--tag=}"
      shift
      ;;
    -h|--help|help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done

require_tool cargo
require_tool npm

cd "$ROOT"

# The package version cargo-dist will match the release tag against.
cargo_version() {
  grep -m1 -E '^version = "' "$ROOT/Cargo.toml" | sed -E 's/^version = "(.+)"/\1/' || true
}

# Guard the most common first-release failure: the git tag and the Cargo.toml
# version must agree or `dist` errors ("no package with version X.Y.Z").
if [[ -n "$EXPECT_TAG" ]]; then
  want="${EXPECT_TAG#v}"
  have="$(cargo_version)"
  if [[ "$want" != "$have" ]]; then
    echo "release version mismatch:" >&2
    echo "  Cargo.toml version = ${have:-<unreadable>}" >&2
    echo "  release tag        = $EXPECT_TAG (expects package version $want)" >&2
    echo "  cargo-dist requires them to match. Bump Cargo.toml 'version' to $want," >&2
    echo "  commit, then tag $EXPECT_TAG." >&2
    exit 1
  fi
  echo "version check: Cargo.toml $have matches tag $EXPECT_TAG"
fi

cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all --all-features
cargo bench --no-run

(
  cd site
  npm ci --ignore-scripts
  npm run lint
  npm run typecheck
  npm run build
  npm audit --omit=dev --audit-level=high
)

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
check_contains .github/workflows/release.yml 'contents: read' "read-only default release permissions"
check_contains .github/workflows/release.yml 'attestations: write' "release attestation permission"
check_contains .github/workflows/release.yml 'actions/attest@' "release artifact attestation"
check_contains dist-workspace.toml 'allow-dirty = ["ci"]' "intentional hardened cargo-dist workflow override"
check_contains SECURITY.md 'Report a Vulnerability Privately' "private security reporting policy"
check_contains .github/CODEOWNERS '/src/pty.rs' "PTY code ownership"

while IFS= read -r action; do
  if [[ ! "$action" =~ @[0-9a-f]{40}$ ]]; then
    echo "release config check failed: mutable or malformed action reference '$action'" >&2
    exit 1
  fi
done < <(
  grep -hE '^[[:space:]]*(-[[:space:]]+)?uses:' .github/workflows/*.yml \
    | sed -E 's/.*uses:[[:space:]]*([^[:space:]#]+).*/\1/'
)

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
