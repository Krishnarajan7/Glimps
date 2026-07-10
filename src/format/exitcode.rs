//! Exit-code translation — the dictionary behind the command status footer.
//!
//! A shell reports *what* a command returned but never *why* it matters:
//! `137` is meaningless until someone tells you it is `128 + SIGKILL`. This
//! module turns an exit code into a class (how loud the footer should be),
//! a verb, and an optional one-line human decode.
//!
//! Two rules keep this honest:
//! - **Decode, don't diagnose.** We state facts the code implies ("SIGKILL:
//!   force-killed, often out of memory"), never invented causes. An unknown
//!   code gets no story at all.
//! - **Only portable signals.** Exit codes `128+N` name signal `N`, but the
//!   numbering differs between macOS and Linux for a few signals (7/10/12 —
//!   SIGBUS lives at 10 on macOS and 7 on Linux). We only decode codes whose
//!   meaning is identical on both platforms; the rest stay raw numbers.

/// How the footer should treat the exit. This is the "Ctrl-C is not red"
/// rule: a user action must never be styled like a failure, or the red
/// footer trains people to ignore it (alarm fatigue kills the feature).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitClass {
    /// Exit 0. Quiet by design.
    Success,
    /// A user or system action, not a defect: Ctrl-C, SIGTERM, SIGPIPE.
    /// Rendered dim/neutral, never red, never "command failed".
    Notice,
    /// A real failure. Rendered loud (red) with the decode when known.
    Failure,
}

/// The decoded meaning of one exit code.
#[derive(Debug, Clone, Copy)]
pub struct ExitStatus {
    pub class: ExitClass,
    /// Footer verb: "done", "failed", "interrupted", "killed", …
    pub verb: &'static str,
    /// Optional human decode appended after an em-dash. `None` for codes
    /// with no portable, factual story (plain `exit 1`, unknown codes).
    pub explain: Option<&'static str>,
}

/// Translate an exit code into its footer rendering.
pub fn describe(code: i32) -> ExitStatus {
    match code {
        0 => status(ExitClass::Success, "done", None),
        126 => failure("failed", "found but not executable (permission?)"),
        127 => failure("failed", "command not found on PATH"),
        // 128+N = terminated by signal N (portable subset only; see module doc).
        //
        // Notice vs Failure for signals hinges on who acted: a deliberate
        // keystroke at this terminal (Ctrl-C, Ctrl-\) or a polite external
        // stop (SIGTERM) is acknowledged dimly; red would scold the user for
        // their own action. SIGHUP is the asymmetric one: unlike SIGTERM it
        // is rarely sent to a foreground command on purpose — if the terminal
        // truly closed nobody sees this footer, so the *visible* case is an
        // unexpected death (e.g. a `kill -HUP` meant as "reload" that killed
        // an app which never handled it). That earns Failure.
        129 => failure(
            "hung up",
            "SIGHUP: unhandled hangup (terminal closed or kill -HUP)",
        ),
        130 => notice("interrupted", "Ctrl-C, not an error"),
        131 => notice("quit", "SIGQUIT: Ctrl-\\"),
        132 => failure("crashed", "SIGILL: illegal instruction"),
        133 => failure("crashed", "SIGTRAP: trace/breakpoint trap"),
        134 => failure("aborted", "SIGABRT: abort() or failed assertion"),
        136 => failure("crashed", "SIGFPE: arithmetic error (divide by zero?)"),
        137 => failure("killed", "SIGKILL: force-killed, often out of memory"),
        139 => failure("crashed", "SIGSEGV: segmentation fault"),
        141 => notice(
            "ended",
            "SIGPIPE: reader closed the pipe early (often benign)",
        ),
        142 => failure("timed out", "SIGALRM: timer expired"),
        143 => notice("terminated", "SIGTERM: asked to stop"),
        // Everything else — including plain exit 1/2 and the codes whose
        // signal numbering is platform-dependent — is a failure with no story.
        _ => status(ExitClass::Failure, "failed", None),
    }
}

const fn status(class: ExitClass, verb: &'static str, explain: Option<&'static str>) -> ExitStatus {
    ExitStatus {
        class,
        verb,
        explain,
    }
}

const fn failure(verb: &'static str, explain: &'static str) -> ExitStatus {
    status(ExitClass::Failure, verb, Some(explain))
}

const fn notice(verb: &'static str, explain: &'static str) -> ExitStatus {
    status(ExitClass::Notice, verb, Some(explain))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_quiet_success() {
        let s = describe(0);
        assert_eq!(s.class, ExitClass::Success);
        assert_eq!(s.verb, "done");
        assert!(s.explain.is_none());
    }

    #[test]
    fn plain_failures_get_no_invented_story() {
        for code in [1, 2, 3, 42, 100, 255] {
            let s = describe(code);
            assert_eq!(s.class, ExitClass::Failure, "exit {code}");
            assert_eq!(s.verb, "failed", "exit {code}");
            assert!(s.explain.is_none(), "exit {code} must stay raw");
        }
    }

    #[test]
    fn env_problems_are_decoded() {
        assert_eq!(describe(127).explain, Some("command not found on PATH"));
        assert_eq!(
            describe(126).explain,
            Some("found but not executable (permission?)")
        );
        assert_eq!(describe(126).class, ExitClass::Failure);
    }

    #[test]
    fn user_actions_are_notices_not_failures() {
        // Deliberate keystrokes (Ctrl-C, Ctrl-\), a polite external stop
        // (SIGTERM), and pipeline shape (SIGPIPE) — never red. SIGHUP is
        // deliberately NOT here: see the class rationale in `describe`.
        for code in [130, 131, 141, 143] {
            assert_eq!(describe(code).class, ExitClass::Notice, "exit {code}");
        }
        assert_eq!(describe(130).verb, "interrupted");
        assert_eq!(describe(143).verb, "terminated");
    }

    #[test]
    fn crashes_are_loud_failures() {
        for code in [129, 132, 133, 134, 136, 137, 139, 142] {
            let s = describe(code);
            assert_eq!(s.class, ExitClass::Failure, "exit {code}");
            assert!(s.explain.is_some(), "exit {code} should be decoded");
        }
        assert_eq!(describe(137).verb, "killed");
        assert_eq!(describe(139).verb, "crashed");
    }

    #[test]
    fn platform_divergent_signal_codes_stay_raw() {
        // 135/138/140 decode to different signals on macOS vs Linux
        // (SIGBUS/SIGEMT/SIGUSR1/SIGSYS shuffle) — refusing to guess is the
        // honest move. Same for out-of-range and negative codes.
        for code in [135, 138, 140, 200, 256, -1, i32::MIN, i32::MAX] {
            let s = describe(code);
            assert_eq!(s.class, ExitClass::Failure, "exit {code}");
            assert!(s.explain.is_none(), "exit {code} must not tell a story");
        }
    }
}
