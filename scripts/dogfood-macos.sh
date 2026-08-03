#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/debug/glimps"
DOGFOOD_TMP=""
DOGFOOD_RESTART_STATUS=75

usage() {
  cat <<'EOF'
Usage: scripts/dogfood-macos.sh [check|session]

check     Build and run repo-local automated checks. Does not install anything.
session   Start an interactive GLIMPS-wrapped zsh using a temporary ZDOTDIR.

Neither mode edits ~/.zshrc, installs GLIMPS globally, or changes your login shell.
The session preserves HOME so Git credentials and desktop tools keep working.
Dogfood command history is persisted separately under the user's state directory.
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

  local state_root="${XDG_STATE_HOME:-$HOME/.local/state}/glimps"
  local history_file="${GLIMPS_DOGFOOD_HISTFILE:-$state_root/dogfood_history}"
  local history_parent
  history_parent="$(dirname "$history_file")"
  mkdir -p "$history_parent"
  touch "$history_file"
  chmod 600 "$history_file"

  DOGFOOD_TMP="$(mktemp -d "${TMPDIR:-/tmp}/glimps-dogfood.XXXXXX")"
  local restart_cwd_file="$DOGFOOD_TMP/restart-cwd"
  cleanup() {
    if [[ -n "${DOGFOOD_TMP:-}" ]]; then
      rm -rf "$DOGFOOD_TMP"
      DOGFOOD_TMP=""
    fi
  }
  trap cleanup EXIT

  cat >"$DOGFOOD_TMP/.zshrc" <<'EOF'
export PROMPT='glimps-dogfood %~ %# '
autoload -Uz compinit
compinit -u -d "$GLIMPS_DOGFOOD_TMP/.zcompdump"
setopt auto_menu complete_in_word
zstyle ':completion:*' menu select

# Keep dogfood history independent from the user's normal zsh history while
# preserving it across wrapper updates and future dogfood sessions.
export HISTFILE="$GLIMPS_DOGFOOD_HISTFILE"
HISTSIZE=200000
SAVEHIST=200000
setopt append_history inc_append_history

# Rebuild and request a controlled wrapper restart. The outer dogfood launcher
# recognizes the reserved status, starts the new binary, and restores $PWD.
glimps-update() {
  local resume_cwd="$PWD"
  if ! (
    builtin cd -- "$GLIMPS_DOGFOOD_ROOT" &&
    command cargo build
  ); then
    print -u2 -- 'glimps-update: build failed; continuing with the current formatter.'
    return 1
  fi
  builtin fc -AI "$HISTFILE" 2>/dev/null || true
  print -rn -- "$resume_cwd" >| "$GLIMPS_DOGFOOD_RESTART_CWD_FILE" || return 1
  print -- 'glimps-update: build complete; restarting GLIMPS and preserving history...'
  builtin exit "$GLIMPS_DOGFOOD_RESTART_STATUS"
}

eval "$("$GLIMPS_DOGFOOD_BIN" init zsh)"
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

After changing GLIMPS source, update this session with: glimps-update
Dogfood history persists at: $history_file
EOF

  local session_cwd="$ROOT"
  local session_status=0
  while true; do
    cd "$session_cwd"
    set +e
    ZDOTDIR="$DOGFOOD_TMP" \
      GLIMPSRC="$DOGFOOD_TMP/.glimpsrc" \
      SHELL="$(command -v zsh)" \
      GLIMPS_DOGFOOD_TMP="$DOGFOOD_TMP" \
      GLIMPS_DOGFOOD_ROOT="$ROOT" \
      GLIMPS_DOGFOOD_BIN="$BIN" \
      GLIMPS_DOGFOOD_HISTFILE="$history_file" \
      GLIMPS_DOGFOOD_RESTART_CWD_FILE="$restart_cwd_file" \
      GLIMPS_DOGFOOD_RESTART_STATUS="$DOGFOOD_RESTART_STATUS" \
      "$BIN"
    session_status=$?
    set -e

    if [[ "$session_status" -ne "$DOGFOOD_RESTART_STATUS" ]]; then
      break
    fi
    if [[ -f "$restart_cwd_file" ]]; then
      local requested_cwd
      requested_cwd="$(<"$restart_cwd_file")"
      if [[ -d "$requested_cwd" ]]; then
        session_cwd="$requested_cwd"
      else
        echo "warning: update resume directory no longer exists; using $ROOT" >&2
        session_cwd="$ROOT"
      fi
    fi
    echo "Restarting with the updated GLIMPS binary..."
  done
  return "$session_status"
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
