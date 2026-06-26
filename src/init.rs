//! `glimps init <shell>` — prints the shell integration to source from an rc file.
//!
//! Install (per GLIMPS-PLAN §6):
//! ```text
//! echo 'eval "$(glimps init zsh)"' >> ~/.zshrc
//! ```
//!
//! The printed snippet does two jobs, depending on whether we're already inside
//! a GLIMPS session (`GLIMPS_ACTIVE`, set by the supervisor when it spawns the
//! shell — see `pty.rs`):
//!   * **Outside:** re-exec the interactive shell *inside* the GLIMPS PTY once,
//!     so all output flows through the supervisor (the ChromaTerm model).
//!   * **Inside:** install OSC-133 `precmd`/`preexec` hooks that emit the
//!     command-output markers GLIMPS needs to tell output from the prompt/input.
//!
//! Both paths are no-ops when `GLIMPS=0`, so the off-switch reaches even the
//! enable shim. The snippet is safe to source repeatedly.

use anyhow::{bail, Result};

/// The zsh integration. Emits OSC-133 `C` (output start) and `D;<exit>` (output
/// end) — plus `A` (prompt start) for completeness — via `preexec`/`precmd`. It
/// deliberately does NOT touch `PROMPT` (no `B` marker): GLIMPS only ever acts on
/// the OUTPUT zone, and rewriting a user's prompt is exactly the kind of risk the
/// safety charter forbids.
const ZSH_INIT: &str = r#"# GLIMPS shell integration for zsh — added by: eval "$(glimps init zsh)"
# Safe to source repeatedly. Disabled entirely when GLIMPS=0.
if [[ "$GLIMPS" != "0" ]]; then
  if [[ -z "$GLIMPS_ACTIVE" ]]; then
    # Outside a GLIMPS session: re-exec this interactive shell inside the GLIMPS
    # PTY supervisor (once) so all command output flows through it. Guarded so it
    # never fires for non-interactive shells, non-terminals, or when uninstalled.
    if [[ -o interactive ]] && [[ -t 1 ]] && command -v glimps >/dev/null 2>&1; then
      exec glimps
    fi
  elif [[ -z "$__glimps_integration_loaded" ]]; then
    # Inside a GLIMPS session: install OSC-133 markers so GLIMPS knows exactly
    # where command output begins (C) and ends (D), and never touches the prompt
    # or what you type.
    __glimps_integration_loaded=1
    autoload -Uz add-zsh-hook
    __glimps_precmd() {
      local __glimps_exit=$?
      print -nr -- $'\e]133;D;'"${__glimps_exit}"$'\a\e]133;A\a'
    }
    __glimps_preexec() {
      print -nr -- $'\e]133;C\a'
    }
    add-zsh-hook precmd __glimps_precmd
    add-zsh-hook preexec __glimps_preexec
  fi
fi
"#;

/// Print the integration for `shell` to stdout (what `eval "$(...)"` consumes).
pub fn print_init(shell: Option<&str>) -> Result<()> {
    match shell {
        Some("zsh") => {
            print!("{ZSH_INIT}");
            Ok(())
        }
        Some(other) => bail!(
            "glimps init: unsupported shell '{other}'. Only 'zsh' is supported today.\n\
             Usage: glimps init zsh"
        ),
        None => bail!("glimps init: missing shell argument.\nUsage: glimps init zsh"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zsh_snippet_emits_output_markers() {
        // The C (output start) and D (output end) markers are the contract GLIMPS
        // relies on. Their byte sequences must be present.
        assert!(ZSH_INIT.contains(r"\e]133;C\a"), "missing C (output start)");
        assert!(
            ZSH_INIT.contains(r"\e]133;D;"),
            "missing D (output end + exit)"
        );
        assert!(ZSH_INIT.contains(r"\e]133;A\a"), "missing A (prompt start)");
    }

    #[test]
    fn zsh_snippet_never_touches_the_prompt() {
        // No PROMPT/PS1 mutation and no B marker — safety: we must not rewrite the
        // user's prompt.
        assert!(!ZSH_INIT.contains("PROMPT"));
        assert!(!ZSH_INIT.contains("PS1"));
        assert!(!ZSH_INIT.contains(r"\e]133;B\a"));
    }

    #[test]
    fn zsh_snippet_guards_nesting_and_off_switch() {
        assert!(
            ZSH_INIT.contains("GLIMPS_ACTIVE"),
            "must guard re-exec nesting"
        );
        assert!(
            ZSH_INIT.contains(r#""$GLIMPS" != "0""#),
            "must honor GLIMPS=0"
        );
        assert!(ZSH_INIT.contains("exec glimps"));
        assert!(
            ZSH_INIT.contains("command -v glimps"),
            "must not exec if uninstalled"
        );
    }

    #[test]
    fn print_init_rejects_unknown_and_missing_shell() {
        assert!(print_init(Some("fish")).is_err());
        assert!(print_init(None).is_err());
        assert!(print_init(Some("zsh")).is_ok());
    }
}
