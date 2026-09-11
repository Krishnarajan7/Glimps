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
//! Install (PowerShell, experimental — see `docs/windows.md`):
//! ```text
//! glimps init pwsh | Out-String | Invoke-Expression   # near the TOP of $PROFILE
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
    # curl styles HTTP headers itself whenever stdout is a terminal. Those SGR
    # escapes split field names away from their values before GLIMPS can apply
    # semantic HTTP formatting. Disable only curl's native header styling, only
    # when `curl` is the ordinary external command; never replace a user's alias
    # or function. --no-styled-output has existed since curl 7.61.0, and the
    # help probe keeps older/custom curl builds untouched.
    if (( ! $+functions[curl] && ! $+aliases[curl] )) && \
       command curl --help all 2>/dev/null | command grep -q -- '--styled-output'; then
      curl() { command curl --no-styled-output "$@"; }
    fi
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
    # Let GLIMPS own curl's HTTP-header presentation. Preserve user-defined curl
    # aliases/functions and decline the wrapper on curl builds without support.
    if [ "$(type -t curl 2>/dev/null)" = "file" ] && \
       command curl --help all 2>/dev/null | command grep -q -- '--styled-output'; then
      curl() { command curl --no-styled-output "$@"; }
    fi
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

/// The PowerShell integration (pwsh 7 and Windows PowerShell 5.1). Experimental:
/// it mirrors the zsh marker contract exactly — private OSC 7337 + `133;C` when
/// a command starts, OSC 7339/7338 + `133;D;<exit>` + `133;A` at every prompt —
/// and the same metadata records. PowerShell has no `preexec`/`precmd`: the
/// PSReadLine entry point `PSConsoleHostReadLine` is wrapped for "command
/// submitted" and `prompt` for "command finished". Because `prompt` is a single
/// function that prompt engines (oh-my-posh, starship) replace outright, the
/// wrapper re-installs itself around whatever `prompt` is current before each
/// command, so the integration can sit at the top of `$PROFILE` like the others.
/// Written for 5.1 compatibility on purpose: no "`e" escapes, no `??`.
const PWSH_INIT: &str = r#"# GLIMPS shell integration for PowerShell — added by:
#   glimps init pwsh | Out-String | Invoke-Expression
# Place this near the TOP of $PROFILE so the rest of your profile runs once,
# inside GLIMPS — not twice. Safe to run repeatedly. Disabled entirely when
# GLIMPS=0. Windows support is experimental; see docs/windows.md.
if ($env:GLIMPS -ne '0') {
  if (-not $env:GLIMPS_ACTIVE) {
    # Outside a GLIMPS session: run this interactive shell inside the GLIMPS PTY
    # supervisor (once), then leave. PowerShell cannot exec, so this host waits
    # for the session and exits with its status. Guarded so it never fires for
    # non-console hosts, redirected I/O, or when glimps is not installed.
    if ($Host.Name -eq 'ConsoleHost' -and
        -not [Console]::IsInputRedirected -and -not [Console]::IsOutputRedirected -and
        (Get-Command glimps -ErrorAction SilentlyContinue)) {
      & glimps
      exit $LASTEXITCODE
    }
  } elseif (-not $Global:__glimps_integration_loaded) {
    # Inside a GLIMPS session: install OSC-133 markers so GLIMPS knows exactly
    # where command output begins (C) and ends (D), and never touches the prompt
    # or what you type.
    $Global:__glimps_integration_loaded = $true
    # Capture the private metadata capability, then remove it from the
    # environment inherited by ordinary child commands.
    $Global:__glimps_meta_path = $env:GLIMPS_META_PATH
    Remove-Item Env:\GLIMPS_META_PATH -ErrorAction SilentlyContinue
    # BOM-less UTF-8: the metadata file is parsed byte-for-byte and a BOM would
    # corrupt the first record.
    $Global:__glimps_utf8 = New-Object System.Text.UTF8Encoding($false)
    $Global:__glimps_ESC = [string][char]27
    $Global:__glimps_BEL = [string][char]7
    $Global:__glimps_NUL = [string][char]0

    # GLIMPS keeps the channel file open (read+write) for the whole session, so
    # the append must share read AND write. The convenience file helpers share
    # read only and would fail with a sharing violation on Windows.
    function Global:__glimps_meta([string]$record) {
      if ($Global:__glimps_meta_path) {
        try {
          $__glimps_stream = New-Object System.IO.FileStream($Global:__glimps_meta_path, [System.IO.FileMode]::Append, [System.IO.FileAccess]::Write, [System.IO.FileShare]::ReadWrite)
          try {
            $__glimps_bytes = $Global:__glimps_utf8.GetBytes($record)
            $__glimps_stream.Write($__glimps_bytes, 0, $__glimps_bytes.Length)
          } finally { $__glimps_stream.Dispose() }
        } catch { }
      }
    }

    # precmd: wrap whatever `prompt` is current. Re-run before each command so a
    # prompt engine loaded later in the profile is wrapped too. The wrapped
    # prompt's own text is returned unchanged — GLIMPS never rewrites a prompt.
    function Global:__glimps_install_prompt {
      __glimps_install_readline
      $current = $function:prompt
      if ($current -and $current.ToString().Contains('__glimps_prompt_hook')) { return }
      $Global:__glimps_prompt = $current
      function Global:prompt {
        # __glimps_prompt_hook — the first statement must read $? before anything
        # else runs and resets it.
        $__glimps_ok = $global:?
        Microsoft.PowerShell.Core\Set-StrictMode -Off
        $__glimps_exit = 0
        if (-not $__glimps_ok) {
          # A native command's status is in $LASTEXITCODE; a cmdlet/script error
          # (whose record points at this history entry) has no code — use 1.
          $__glimps_exit = 1
          $__glimps_history = Get-History -Count 1
          $__glimps_ps_error = $global:Error.Count -gt 0 -and $global:Error[0].InvocationInfo -and
            $__glimps_history -and $global:Error[0].InvocationInfo.HistoryId -eq $__glimps_history.Id
          if (-not $__glimps_ps_error -and $global:LASTEXITCODE -is [int] -and $global:LASTEXITCODE -ne 0) {
            $__glimps_exit = $global:LASTEXITCODE
          }
        }
        $__glimps_pwd = $ExecutionContext.SessionState.Path.CurrentLocation.Path
        __glimps_meta ('R' + $Global:__glimps_NUL + $__glimps_exit + $Global:__glimps_NUL + $__glimps_pwd + $Global:__glimps_NUL + $__glimps_exit + $Global:__glimps_NUL)
        # Private OSC 7339 carries the pipeline status (PowerShell has no pipestatus;
        # the exit code stands alone), 7338 the post-command working directory, then
        # the output end (133;D) and prompt start (133;A) markers.
        [Console]::Write($Global:__glimps_ESC + ']7339;' + $__glimps_exit + $Global:__glimps_BEL +
          $Global:__glimps_ESC + ']7338;' + $__glimps_pwd + $Global:__glimps_BEL +
          $Global:__glimps_ESC + ']133;D;' + $__glimps_exit + $Global:__glimps_BEL +
          $Global:__glimps_ESC + ']133;A' + $Global:__glimps_BEL)
        # Put $? back so the wrapped prompt sees the real status.
        if (-not $__glimps_ok) { Write-Error 'glimps: restoring last status' -ErrorAction Ignore }
        if ($Global:__glimps_prompt) { (@(& $Global:__glimps_prompt) -join '') } else { "PS $__glimps_pwd> " }
      }
    }

    # preexec: PSReadLine hands every submitted line through PSConsoleHostReadLine.
    # Send GLIMPS the command being run (private OSC 7337) so it can show a
    # colored command header and bypass interactive programs by name, then mark
    # the start of command output (OSC 133;C). Re-checked at every prompt like
    # `prompt` itself, because a later `Import-Module PSReadLine` redefines the
    # function. Without PSReadLine there is no hook, and GLIMPS stays in
    # pass-through.
    function Global:__glimps_install_readline {
      $current = $function:PSConsoleHostReadLine
      if (-not $current) { return }
      if ($current.ToString().Contains('__glimps_readline_hook')) { return }
      $Global:__glimps_readline = $current
      function Global:PSConsoleHostReadLine {
        # __glimps_readline_hook — the first statement must be the wrapped
        # reader, which captures $? for its own prompt coloring.
        $__glimps_line = $Global:__glimps_readline.Invoke()
        Microsoft.PowerShell.Core\Set-StrictMode -Off
        $__glimps_text = "$__glimps_line"
        if ($__glimps_text.Trim().Length -gt 0) {
          __glimps_meta ('C' + $Global:__glimps_NUL + $__glimps_text + $Global:__glimps_NUL)
          [Console]::Write($Global:__glimps_ESC + ']7337;' + $__glimps_text + $Global:__glimps_BEL + $Global:__glimps_ESC + ']133;C' + $Global:__glimps_BEL)
        }
        __glimps_install_prompt
        $__glimps_line
      }
    }
    __glimps_install_prompt
  }
}
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
        Some("pwsh" | "powershell") => {
            print!("{PWSH_INIT}");
            Ok(())
        }
        Some(other) => bail!(
            "glimps init: unsupported shell '{other}'. Supported: zsh, bash, pwsh.\n\
             Usage: glimps init <zsh|bash|pwsh>"
        ),
        None => bail!("glimps init: missing shell argument.\nUsage: glimps init <zsh|bash|pwsh>"),
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
    fn zsh_snippet_yields_curl_header_styling_to_glimps() {
        assert!(ZSH_INIT.contains("curl --no-styled-output"));
        assert!(ZSH_INIT.contains("! $+functions[curl] && ! $+aliases[curl]"));
        assert!(ZSH_INIT.contains("--help all"));
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
        assert!(print_init(Some("pwsh")).is_ok());
        assert!(print_init(Some("powershell")).is_ok());
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
    fn bash_snippet_yields_external_curl_header_styling_to_glimps() {
        assert!(BASH_INIT.contains("curl --no-styled-output"));
        assert!(BASH_INIT.contains("type -t curl"));
        assert!(BASH_INIT.contains("--help all"));
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
    #[test]
    fn pwsh_snippet_emits_the_same_marker_contract_as_zsh() {
        // Output start (C) with the command (7337), output end (D;<exit>) with
        // the cwd (7338) and pipeline status (7339), then prompt start (A).
        assert!(PWSH_INIT.contains("']7337;'"));
        assert!(PWSH_INIT.contains("']133;C'"));
        assert!(PWSH_INIT.contains("']7339;'"));
        assert!(PWSH_INIT.contains("']7338;'"));
        assert!(PWSH_INIT.contains("']133;D;'"));
        assert!(PWSH_INIT.contains("']133;A'"));
        // Metadata records use the byte layout `parse_records` expects.
        assert!(PWSH_INIT
            .contains("('C' + $Global:__glimps_NUL + $__glimps_text + $Global:__glimps_NUL)"));
        assert!(PWSH_INIT.contains(
            "('R' + $Global:__glimps_NUL + $__glimps_exit + $Global:__glimps_NUL + $__glimps_pwd + $Global:__glimps_NUL + $__glimps_exit + $Global:__glimps_NUL)"
        ));
    }

    #[test]
    fn pwsh_snippet_never_rewrites_the_prompt() {
        // No B marker (GLIMPS never touches the prompt zone) and the wrapped
        // prompt's own text is returned as-is.
        assert!(!PWSH_INIT.contains("133;B"));
        assert!(PWSH_INIT.contains("(@(& $Global:__glimps_prompt) -join '')"));
        assert!(PWSH_INIT.contains("$Global:__glimps_prompt = $current"));
    }

    #[test]
    fn pwsh_snippet_guards_nesting_and_off_switch() {
        assert!(PWSH_INIT.contains("if ($env:GLIMPS -ne '0')"));
        assert!(PWSH_INIT.contains("if (-not $env:GLIMPS_ACTIVE)"));
        assert!(PWSH_INIT.contains("elseif (-not $Global:__glimps_integration_loaded)"));
        assert!(PWSH_INIT.contains("Remove-Item Env:\\GLIMPS_META_PATH"));
        assert!(PWSH_INIT.contains("[Console]::IsOutputRedirected"));
    }

    #[test]
    fn pwsh_snippet_is_windows_powershell_5_compatible_and_bom_free() {
        // "`e" and "??" are pwsh 7-only; 5.1 is still the in-box shell.
        assert!(!PWSH_INIT.contains("`e"));
        assert!(!PWSH_INIT.contains("??"));
        // A BOM at the head of the metadata file would corrupt the first record.
        assert!(PWSH_INIT.contains("New-Object System.Text.UTF8Encoding($false)"));
        // The supervisor holds the channel open read+write; the append must
        // share both or Windows refuses it with a sharing violation.
        assert!(PWSH_INIT.contains("[System.IO.FileShare]::ReadWrite"));
        assert!(!PWSH_INIT.contains("AppendAllText"));
        // Both hooks re-check that they are still installed.
        assert!(PWSH_INIT.contains("Contains('__glimps_readline_hook')"));
        assert!(PWSH_INIT.contains("Contains('__glimps_prompt_hook')"));
        // Balanced braces: the snippet is consumed by Invoke-Expression whole.
        let opens = PWSH_INIT.matches('{').count();
        let closes = PWSH_INIT.matches('}').count();
        assert_eq!(opens, closes);
    }
}
