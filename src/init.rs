//! `glimps init <shell>` — prints the shell integration to source from an rc file.
//!
//! Install (zsh):
//! ```text
//! eval "$(glimps init zsh)"   # near the TOP of ~/.zshrc
//! ```
//! Install (bash):
//! ```text
//! eval "$(glimps init bash)"  # near the TOP of ~/.bashrc
//! ```
//!
//! The printed snippet does two jobs, depending on whether we're already inside
//! a GLIMPS session (`GLIMPS_ACTIVE`, set by the supervisor when it spawns the
//! shell — see `pty.rs`):
//!   * **Outside:** re-exec the interactive shell *inside* the GLIMPS PTY once,
//!     so all output flows through the supervisor (the ChromaTerm model).
//!   * **Inside:** install OSC-133 `precmd`/`preexec` hooks (zsh) or the
//!     equivalent `PROMPT_COMMAND` + `DEBUG`-trap hooks (bash) that emit the
//!     command-output markers GLIMPS needs to tell output from the prompt/input.
//!
//! **Placement matters.** The outside branch `exec`s the interactive shell
//! *inside* GLIMPS, and the re-exec'd shell then re-sources the same rc file. So
//! the line belongs near the TOP of the rc: everything above it runs in the
//! throwaway outer shell *and again* inside GLIMPS, while everything below it
//! runs only once (inside GLIMPS). Appending it to the end re-runs your whole rc
//! twice per session. (Login files like `.zprofile`/`.bash_profile` are not
//! re-run — the inner shell is interactive, and inherits the outer environment.)
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
# Place this near the TOP of ~/.zshrc (before plugin managers / prompt setup) so
# the rest of your zshrc runs once, inside GLIMPS — not twice.
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
    # Capture the private metadata capability in a non-exported variable, then
    # remove it from the environment inherited by ordinary child commands.
    typeset -g __glimps_meta_path="${GLIMPS_META_PATH-}"
    unset GLIMPS_META_PATH
    # zsh's partial-line indicator (the inverse "%") is emitted INSIDE the command
    # output zone, so it would make even no-output commands (cd, export) look like
    # they produced output and get a separator. GLIMPS owns the command/output
    # boundary now, so turn that indicator off.
    unsetopt prompt_sp prompt_cr 2>/dev/null
    autoload -Uz add-zsh-hook
    __glimps_precmd() {
      local __glimps_exit=$? __glimps_pipeline="${(j: :)pipestatus}"
      if [[ -n "$__glimps_meta_path" ]]; then
        print -rn -- $'R\0'"$__glimps_pipeline"$'\0'"$PWD"$'\0'"$__glimps_exit"$'\0' >> "$__glimps_meta_path" 2>/dev/null
      fi
      # Private OSC 7339 carries per-pipeline-stage statuses. This does not
      # enable pipefail or change shell behavior; GLIMPS only observes it.
      print -nr -- $'\e]7339;'"$__glimps_pipeline"$'\a'
      # Private OSC 7338 carries the post-command working directory. GLIMPS uses
      # it for command-specific UX like successful `cd` breadcrumbs; terminals
      # ignore unknown OSCs.
      print -nr -- $'\e]7338;'"$PWD"$'\a\e]133;D;'"${__glimps_exit}"$'\a\e]133;A\a'
    }
    __glimps_preexec() {
      if [[ -n "$__glimps_meta_path" ]]; then
        print -rn -- $'C\0'"$1"$'\0' >> "$__glimps_meta_path" 2>/dev/null
      fi
      # Send GLIMPS the command being run (private OSC 7337) so it can show a
      # colored command header and bypass interactive programs by name, then mark
      # the start of command output (OSC 133;C).
      print -nr -- $'\e]7337;'"$1"$'\a\e]133;C\a'
    }
    add-zsh-hook precmd __glimps_precmd
    add-zsh-hook preexec __glimps_preexec
  fi
fi
"#;

/// The bash integration. bash has no native `preexec`/`precmd`, so the same
/// OSC-133 markers are emitted via a `DEBUG` trap (fires before each command,
/// like `preexec`) plus `PROMPT_COMMAND` (fires before each prompt, like
/// `precmd`). Like the zsh snippet it never rewrites `PS1` and emits no `B`
/// marker — GLIMPS only ever acts on the OUTPUT zone.
///
/// The `DEBUG` trap fires before *every* top-level command, so an "armed" flag
/// (set last in `PROMPT_COMMAND`, cleared when it fires) makes the output-start
/// markers land exactly once per command line — never for the prompt commands
/// themselves or during completion. The captured command is the full history
/// line (not `$BASH_COMMAND`, which is only the first pipeline stage), any
/// pre-existing `DEBUG` trap is chained (not clobbered), and a `PROMPT_COMMAND`
/// array (bash 5.1+) is preserved. bash 3.2 compatible (the macOS system bash):
/// no required arrays, no non-POSIX tests.
const BASH_INIT: &str = r#"# GLIMPS shell integration for bash — added by: eval "$(glimps init bash)"
# Place this near the TOP of ~/.bashrc (before plugin managers / prompt setup) so
# the rest of your bashrc runs once, inside GLIMPS — not twice.
# Safe to source repeatedly. Disabled entirely when GLIMPS=0.
if [ "$GLIMPS" != "0" ]; then
  if [ -z "$GLIMPS_ACTIVE" ]; then
    # Outside a GLIMPS session: re-exec this interactive shell inside the GLIMPS
    # PTY supervisor (once) so all command output flows through it. Guarded so it
    # never fires for non-interactive shells, non-terminals, or when uninstalled.
    case "$-" in
      *i*)
        if [ -t 1 ] && command -v glimps >/dev/null 2>&1; then
          exec glimps
        fi
        ;;
    esac
  elif [ -z "$__glimps_integration_loaded" ]; then
    # Inside a GLIMPS session: install OSC-133 markers so GLIMPS knows exactly
    # where command output begins (C) and ends (D), and never touches the prompt
    # or what you type. bash has no preexec/precmd, so we use a DEBUG trap (before
    # each command) + PROMPT_COMMAND (before each prompt).
    __glimps_integration_loaded=1
    # Keep the channel capability private to this shell. Child commands must not
    # inherit the path used to author trusted GLIMPS metadata.
    __glimps_meta_path=${GLIMPS_META_PATH-}
    unset GLIMPS_META_PATH
    __glimps_armed=""
    __glimps_debug_exit=0
    __glimps_debug_pipeline=0
    # Preserve any DEBUG trap set before us (mcfly, a hand-rolled trap, …) so we
    # chain it instead of clobbering it. Tools loaded AFTER us that build on
    # bash-preexec chain us the same way; a tool that installs a raw DEBUG trap
    # below the glimps line would override us — put the glimps line last in that
    # case (bash integration is beta; see the README).
    __glimps_prev_debug=""
    case "$(trap -p DEBUG)" in
      "") ;;
      *) __glimps_prev_debug=$(trap -p DEBUG | sed "s/^trap -- '//;s/' DEBUG\$//;s/'\\\\''/'/g") ;;
    esac
    __glimps_preexec() {
      # Preserve $? and PIPESTATUS before any hook work clobbers them. This is
      # especially important before PROMPT_COMMAND: bash runs DEBUG before the
      # prompt hooks, so this is the last clean moment to observe a pipeline.
      __glimps_debug_exit=$? __glimps_debug_pipeline="${PIPESTATUS[*]}"
      local __glimps_ec=$__glimps_debug_exit
      # Chain any pre-existing DEBUG trap first — never silently disable it.
      [ -n "$__glimps_prev_debug" ] && eval "$__glimps_prev_debug"
      # DEBUG fires before every top-level command. Emit the output-start markers
      # only once per command line (when armed), never for our own hooks or during
      # completion.
      if [ -n "$__glimps_armed" ] && [ -z "$COMP_LINE" ] && [ -z "$READLINE_LINE" ]; then
        case "$BASH_COMMAND" in
          __glimps_*) ;;
          *)
            __glimps_armed=""
            # Full command line (bash 3.2): the latest history entry, minus its
            # leading index. $BASH_COMMAND is only the FIRST pipeline stage, which
            # would mis-name `cmd | less` and defeat bypass-by-name for a program
            # downstream of a pipe. Fall back to $BASH_COMMAND if history is off.
            local __glimps_cmd
            __glimps_cmd=$(HISTTIMEFORMAT= builtin history 1 2>/dev/null)
            __glimps_cmd=${__glimps_cmd#*[0-9]  }
            [ -z "$__glimps_cmd" ] && __glimps_cmd=$BASH_COMMAND
            if [ -n "$__glimps_meta_path" ]; then
              printf 'C\0%s\0' "$__glimps_cmd" >> "$__glimps_meta_path" 2>/dev/null
            fi
            # Private OSC 7337 carries the command (colored header + bypass by
            # name); OSC 133;C marks the start of command output.
            printf '\033]7337;%s\007\033]133;C\007' "$__glimps_cmd"
            ;;
        esac
      fi
      return $__glimps_ec
    }
    __glimps_precmd() {
      # Runs FIRST in PROMPT_COMMAND: capture the just-finished command's exit
      # status before anything else clobbers $?.
      local __glimps_exit=$__glimps_debug_exit __glimps_pipeline="$__glimps_debug_pipeline"
      if [ -n "$__glimps_meta_path" ]; then
        printf 'R\0%s\0%s\0%s\0' "$__glimps_pipeline" "$PWD" "$__glimps_exit" >> "$__glimps_meta_path" 2>/dev/null
      fi
      # Private OSC 7339 carries per-pipeline-stage statuses. This observes the
      # shell's pipeline result without enabling pipefail or changing behavior.
      printf '\033]7339;%s\007' "$__glimps_pipeline"
      # Private OSC 7338 carries the post-command cwd (cd breadcrumbs); 133;D ends
      # the output zone (+ exit code); 133;A starts the next prompt.
      printf '\033]7338;%s\007\033]133;D;%s\007\033]133;A\007' "$PWD" "$__glimps_exit"
    }
    __glimps_arm() {
      # Runs LAST in PROMPT_COMMAND, after all prompt work: only now arm preexec,
      # so the DEBUG trap fires for your next command, not the prompt commands.
      __glimps_armed=1
    }
    trap '__glimps_preexec' DEBUG
    # Install precmd first + arm last. PROMPT_COMMAND is usually a string, but
    # bash 5.1+ allows an array — handle both so we never collapse a user's array
    # (which would drop their other prompt hooks).
    case "$(declare -p PROMPT_COMMAND 2>/dev/null)" in
      "declare -a"*)
        PROMPT_COMMAND=(__glimps_precmd "${PROMPT_COMMAND[@]}" __glimps_arm)
        ;;
      *)
        case ";$PROMPT_COMMAND;" in
          *";__glimps_precmd;"*) ;;
          *) PROMPT_COMMAND="__glimps_precmd;${PROMPT_COMMAND:+$PROMPT_COMMAND;}__glimps_arm" ;;
        esac
        ;;
    esac
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
        Some("bash") => {
            print!("{BASH_INIT}");
            Ok(())
        }
        Some(other) => bail!(
            "glimps init: unsupported shell '{other}'. Supported: zsh, bash.\n\
             Usage: glimps init <zsh|bash>"
        ),
        None => bail!("glimps init: missing shell argument.\nUsage: glimps init <zsh|bash>"),
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
            ZSH_INIT.contains(r"\e]7337;"),
            "missing command-capture marker"
        );
        assert!(ZSH_INIT.contains(r"\e]7338;"), "missing cwd marker");
        assert!(
            ZSH_INIT.contains(r"\e]7339;"),
            "missing pipeline-status marker"
        );
        assert!(
            ZSH_INIT.contains(r"\e]133;D;"),
            "missing D (output end + exit)"
        );
        assert!(ZSH_INIT.contains(r"\e]133;A\a"), "missing A (prompt start)");
        assert!(ZSH_INIT.contains("unset GLIMPS_META_PATH"));
        assert!(ZSH_INIT.contains("$'C\\0'"));
        assert!(ZSH_INIT.contains("$'R\\0'"));
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
    fn zsh_snippet_disables_partial_line_indicator() {
        // zsh's prompt_sp "%" lands in the output zone and would make no-output
        // commands look like they produced output; we disable it (GLIMPS owns the
        // boundary). Keep this so the no-output-separator fix can't regress.
        assert!(ZSH_INIT.contains("unsetopt prompt_sp"));
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
        assert!(print_init(Some("bash")).is_ok());
    }

    #[test]
    fn bash_snippet_emits_output_markers() {
        // The same C/D/A/command/cwd marker contract as zsh — GLIMPS relies on
        // these regardless of shell.
        assert!(
            BASH_INIT.contains(r"\033]133;C\007"),
            "missing C (output start)"
        );
        assert!(
            BASH_INIT.contains(r"\033]7337;"),
            "missing command-capture marker"
        );
        assert!(BASH_INIT.contains(r"\033]7338;"), "missing cwd marker");
        assert!(
            BASH_INIT.contains(r"\033]7339;"),
            "missing pipeline-status marker"
        );
        assert!(
            BASH_INIT.contains(r"\033]133;D;"),
            "missing D (output end + exit)"
        );
        assert!(
            BASH_INIT.contains(r"\033]133;A\007"),
            "missing A (prompt start)"
        );
        assert!(BASH_INIT.contains("unset GLIMPS_META_PATH"));
        assert!(BASH_INIT.contains(r"printf 'C\0%s\0'"));
        assert!(BASH_INIT.contains(r"printf 'R\0%s\0%s\0%s\0'"));
    }

    #[test]
    fn bash_snippet_never_touches_the_prompt() {
        // No PS1 mutation and no B marker: we must not rewrite the user's prompt.
        // (PROMPT_COMMAND is a hook variable, not the prompt string — prepending
        // to it is the standard, safe bash mechanism and does not change PS1.)
        assert!(!BASH_INIT.contains("PS1"));
        assert!(!BASH_INIT.contains(r"\033]133;B\007"));
    }

    #[test]
    fn bash_snippet_guards_nesting_and_off_switch() {
        assert!(
            BASH_INIT.contains("GLIMPS_ACTIVE"),
            "must guard re-exec nesting"
        );
        assert!(
            BASH_INIT.contains(r#""$GLIMPS" != "0""#),
            "must honor GLIMPS=0"
        );
        assert!(BASH_INIT.contains("exec glimps"));
        assert!(
            BASH_INIT.contains("command -v glimps"),
            "must not exec if uninstalled"
        );
    }

    #[test]
    fn bash_snippet_installs_debug_and_prompt_command_hooks() {
        // The DEBUG trap is bash's preexec; PROMPT_COMMAND is its precmd. Both are
        // required for GLIMPS to find the command/output boundary in bash.
        assert!(BASH_INIT.contains("trap '__glimps_preexec' DEBUG"));
        assert!(BASH_INIT.contains("PROMPT_COMMAND="));
        // precmd must run FIRST (capture $? before it's clobbered); arm LAST.
        assert!(BASH_INIT.contains("__glimps_precmd;"));
        assert!(BASH_INIT.contains("__glimps_arm"));
        // Idempotency guard so repeated sourcing can't stack the hook.
        assert!(BASH_INIT.contains(r#"*";__glimps_precmd;"*"#));
    }

    #[test]
    fn bash_snippet_is_robust_to_other_tools_and_pipelines() {
        // Chains (not clobbers) any pre-existing DEBUG trap.
        assert!(
            BASH_INIT.contains("trap -p DEBUG"),
            "must capture an existing DEBUG trap to chain it"
        );
        assert!(
            BASH_INIT.contains(r#"eval "$__glimps_prev_debug""#),
            "must invoke the chained DEBUG trap"
        );
        // Captures the FULL command line (history), not just the first pipeline
        // stage — so `cmd | less` bypasses `less`, not `cmd`.
        assert!(
            BASH_INIT.contains("builtin history 1"),
            "must capture the full command line from history"
        );
        assert!(
            BASH_INIT.contains(r#"__glimps_debug_pipeline="${PIPESTATUS[*]}""#),
            "must capture pipeline stage statuses before prompt work clobbers them"
        );
        // Preserves a PROMPT_COMMAND array (bash 5.1+) instead of collapsing it.
        assert!(
            BASH_INIT.contains(r#""declare -a"*"#),
            "must handle an array-typed PROMPT_COMMAND"
        );
        assert!(
            BASH_INIT.contains(r#"("${PROMPT_COMMAND[@]}")"#)
                || BASH_INIT.contains(r#"${PROMPT_COMMAND[@]}"#)
        );
    }
}
