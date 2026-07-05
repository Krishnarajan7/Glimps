#!/usr/bin/env bash
#
# create-issues.sh — create the GLIMPS good-first-issue set on GitHub from the
# body files in .github/good-first-issues/.
#
# Each source file starts with two metadata lines:
#   <!-- title: ... -->
#   <!-- labels: a,b,c -->
# ...followed by the Markdown issue body.
#
# Behavior:
#   - Idempotent by title: an issue whose exact title already exists is skipped,
#     so re-running won't create duplicates.
#   - The "Start contributing to GLIMPS" issue (00-start-here.md) is pinned.
#   - Run setup-labels.sh FIRST so the labels exist.
#
# Requires an authenticated GitHub CLI (see setup-labels.sh header).
#
# Usage:
#   scripts/gh/setup-labels.sh            # once, first
#   scripts/gh/create-issues.sh           # create the issues
#   scripts/gh/create-issues.sh --dry-run # print what would be created
#   REPO=Krishnarajan7/Glimps scripts/gh/create-issues.sh
set -euo pipefail

DRY_RUN=0
[ "${1:-}" = "--dry-run" ] && DRY_RUN=1

REPO="${REPO:-}"
repo_args=()
[ -n "$REPO" ] && repo_args=(--repo "$REPO")

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ISSUE_DIR="$(cd "$SCRIPT_DIR/../../.github/good-first-issues" && pwd)"
PIN_FILE="00-start-here.md"

if [ "$DRY_RUN" -eq 0 ]; then
  if ! command -v gh >/dev/null 2>&1; then
    echo "error: GitHub CLI (gh) not found. Install it, then 'gh auth login'." >&2
    exit 1
  fi
  if ! gh auth status >/dev/null 2>&1; then
    echo "error: gh is not authenticated. Run 'gh auth login' first." >&2
    exit 1
  fi
fi

# Snapshot existing issue titles once, for de-duplication.
existing_titles=""
if [ "$DRY_RUN" -eq 0 ]; then
  existing_titles="$(gh issue list --state all --limit 500 --json title \
    --jq '.[].title' "${repo_args[@]}" 2>/dev/null || true)"
fi

meta() { # meta <field> <file>
  sed -n -E "s/^<!-- $1:[[:space:]]*(.*[^[:space:]])[[:space:]]*-->\$/\1/p" "$2" | head -1
}

created=0 skipped=0
for file in "$ISSUE_DIR"/*.md; do
  [ -e "$file" ] || continue
  title="$(meta title "$file")"
  labels_csv="$(meta labels "$file")"
  # Body = everything after the two metadata lines, leading blanks trimmed.
  body="$(grep -vE '^<!-- (title|labels):.*-->$' "$file" | awk 'NF{p=1} p')"

  if [ -z "$title" ]; then
    echo "  ! $(basename "$file"): no title metadata, skipping" >&2
    continue
  fi

  # Build repeated --label args from the CSV.
  label_args=()
  IFS=',' read -ra _labels <<<"$labels_csv"
  for l in "${_labels[@]}"; do
    l="$(echo "$l" | awk '{$1=$1};1')" # trim
    [ -n "$l" ] && label_args+=(--label "$l")
  done

  if [ "$DRY_RUN" -eq 1 ]; then
    echo "would create: [$labels_csv] $title"
    continue
  fi

  # De-dupe on exact title match.
  if printf '%s\n' "$existing_titles" | grep -Fxq "$title"; then
    echo "  = skip (exists): $title"
    skipped=$((skipped + 1))
    continue
  fi

  tmp="$(mktemp)"
  printf '%s\n' "$body" >"$tmp"
  url="$(gh issue create --title "$title" --body-file "$tmp" \
    "${label_args[@]}" "${repo_args[@]}")"
  rm -f "$tmp"
  echo "  ✓ created: $title"
  echo "      $url"
  created=$((created + 1))

  # Pin the welcome issue.
  if [ "$(basename "$file")" = "$PIN_FILE" ]; then
    num="${url##*/}"
    node_id="$(gh issue view "$num" --json id --jq .id "${repo_args[@]}")"
    if gh api graphql \
        -f query='mutation($id:ID!){pinIssue(input:{issueId:$id}){issue{number}}}' \
        -f id="$node_id" >/dev/null 2>&1; then
      echo "      📌 pinned"
    else
      echo "      (could not pin automatically — pin it from the issue page:" >&2
      echo "       open the issue → right sidebar → Pin issue)" >&2
    fi
  fi
done

if [ "$DRY_RUN" -eq 1 ]; then
  echo "Dry run complete. Re-run without --dry-run to create them."
else
  echo "Done. Created $created, skipped $skipped."
fi
