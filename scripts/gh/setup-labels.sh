#!/usr/bin/env bash
#
# setup-labels.sh — create (or update) the GLIMPS issue label set.
#
# Idempotent: uses `gh label create --force`, so re-running updates the color
# and description of existing labels instead of failing. Safe to run again.
#
# Requires the GitHub CLI, authenticated for this repo:
#   brew install gh          # or: https://cli.github.com
#   gh auth login
#
# Usage:
#   scripts/gh/setup-labels.sh            # act on the repo of the current dir
#   REPO=Krishnarajan7/Glimps scripts/gh/setup-labels.sh
set -euo pipefail

REPO="${REPO:-}"
repo_args=()
[ -n "$REPO" ] && repo_args=(--repo "$REPO")

if ! command -v gh >/dev/null 2>&1; then
  echo "error: GitHub CLI (gh) not found. Install it, then 'gh auth login'." >&2
  echo "  brew install gh   (or see https://cli.github.com)" >&2
  exit 1
fi

if ! gh auth status >/dev/null 2>&1; then
  echo "error: gh is not authenticated. Run 'gh auth login' first." >&2
  exit 1
fi

# name|color(hex, no #)|description
labels=(
  "good first issue|7057ff|Small, scoped, newcomer-friendly task with clear acceptance criteria"
  "help wanted|008672|Maintainers would welcome a contributor picking this up"
  "formatter|1d76db|Adds or changes an output formatter"
  "command-aware|0e8a16|Formatting tied to a specific command's output shape"
  "safety|b60205|Touches a terminal-safety invariant; extra care and tests required"
  "docs|0075ca|Documentation, README, or demo work"
  "release|d4c5f9|Release, packaging, or install-path verification"
  "bug|d73a4a|GLIMPS formats or behaves incorrectly"
  "enhancement|a2eeef|New capability or improvement"
  "question|cc317c|Needs discussion or clarification before code"
  "pty-safety|5319e7|Touches PTY supervisor, raw mode, or OSC-133 — not a first issue"
)

echo "Creating/updating labels on ${REPO:-current repo}..."
for entry in "${labels[@]}"; do
  IFS='|' read -r name color desc <<<"$entry"
  gh label create "$name" \
    --color "$color" \
    --description "$desc" \
    --force \
    "${repo_args[@]}"
  echo "  ✓ $name"
done

echo "Done. ${#labels[@]} labels set."
