#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/debug/glimps"
DOGFOOD_TMP=""

usage() {
  cat <<'EOF'
Usage: scripts/dogfood-macos.sh [check|session]

check     Build and run repo-local automated checks. Does not install anything.
session   Start an interactive GLIMPS-wrapped zsh using a temporary ZDOTDIR.

Neither mode edits ~/.zshrc, installs GLIMPS globally, or changes your login shell.
The session preserves HOME so Git credentials and desktop tools keep working.
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

  DOGFOOD_TMP="$(mktemp -d "${TMPDIR:-/tmp}/glimps-dogfood.XXXXXX")"
  cleanup() {
    if [[ -n "${DOGFOOD_TMP:-}" ]]; then
      rm -rf "$DOGFOOD_TMP"
      DOGFOOD_TMP=""
    fi
  }
  trap cleanup EXIT

  cat >"$DOGFOOD_TMP/.zshrc" <<EOF
export PROMPT='glimps-dogfood %~ %# '
eval "\$("$BIN" init zsh)"
EOF

  cat >"$DOGFOOD_TMP/.glimpsrc" <<'EOF'
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
  printf 'HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: sid=1\r\n\r\n{"ok":true}\n'
  printf 'Traceback (most recent call last):\n  File "app.py", line 7, in <module>\nValueError: broken config\n'
  printf 'name,age,active\nAda,37,true\n"Lovelace, Ada",12,false\n' > /tmp/glimps-users.csv
  cat /tmp/glimps-users.csv
  printf 'CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);\nSELECT * FROM users WHERE id = 42;\n' > /tmp/glimps-schema.sql
  cat /tmp/glimps-schema.sql
  sqlite3 -header -column :memory: 'CREATE TABLE users(id INTEGER, name TEXT, active TEXT); INSERT INTO users VALUES (1,"Ada","true"); SELECT * FROM users;'
  printf '{"level":"info","count":2}\n{"level":"error","ok":false}\n' > /tmp/glimps-events.jsonl
  cat /tmp/glimps-events.jsonl
  printf '// GLIMPS source sample\npub fn main() {\n    let answer = 42;\n    println!("ok");\n}\n' > /tmp/glimps-main.rs
  cat /tmp/glimps-main.rs
  printf '# deploy helper\ndef greet(name):\n    return f"hi {name}"\n' > /tmp/glimps-app.py
  head -20 /tmp/glimps-app.py
  cat README.md
  cat Cargo.toml
  cd docs
  cd ..
  ls -la
  du -sh src tests .
  df -h
  ps aux | head -5
  dig 360astra.io
  false
  find src -maxdepth 2 -type f
  git status --short
  git --no-pager log --oneline --decorate -5
  git branch -a
  git --no-pager diff --stat
  git --no-pager diff --numstat
  git --no-pager diff --name-status
  git --no-pager diff -- README.md
  man printf
  vim README.md
  printf 'A\x01\x02B'

Exit with: exit
EOF

  ZDOTDIR="$DOGFOOD_TMP" GLIMPSRC="$DOGFOOD_TMP/.glimpsrc" SHELL="$(command -v zsh)" "$BIN"
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
