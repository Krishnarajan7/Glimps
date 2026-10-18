//! Completion notices: tell the desktop when a long-running command finishes
//! (`[notify]` in `.glimpsrc`, opt-in).
//!
//! The notice is an escape sequence written to the real terminal at the
//! command's output end, in the same stream as the footer, so the terminal
//! turns it into a desktop notification (or a bell). Three rules keep it
//! quiet and private:
//!
//! - **No focus reporting.** GLIMPS never turns on `CSI ?1004h`: focus events
//!   arrive as input, so they would land in the shell's line editor and in
//!   full-screen programs as stray keystrokes. Presence is judged from typing
//!   instead (see [`InputActivity`]), and the terminal decides whether a
//!   notification shows while its window is focused.
//! - **Only commands you walked away from.** Shorter than the threshold,
//!   interrupted (Ctrl-C), interactive or full-screen (vim, ssh, a REPL you
//!   were typing into), or sensitive: no notice.
//! - **Name only.** The text names the program, and its subcommand only for
//!   tools known to take one (`cargo build`, `docker compose`): any other
//!   second word could be a private argument (`unlock hunter2`). Never later
//!   arguments and never any output: a
//!   notification may be kept by the desktop's notification history, which
//!   is outside GLIMPS's control (invariant #5).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::config::NotifyMethod;

/// Typing into the terminal this close to a command's end means the user was
/// there to see it finish.
pub(super) const PRESENCE_WINDOW: Duration = Duration::from_secs(10);

/// The longest command name a notice repeats, in chars.
const NAME_CAP: usize = 32;

/// Tools whose second word is a subcommand, so the notice may name it. For
/// anything else the second word could be a private argument.
const SUBCOMMAND_TOOLS: &[&str] = &[
    "apt",
    "apt-get",
    "bazel",
    "brew",
    "bun",
    "bundle",
    "cargo",
    "cmake",
    "composer",
    "conda",
    "deno",
    "dnf",
    "docker",
    "dotnet",
    "flutter",
    "gh",
    "git",
    "go",
    "gradle",
    "gradlew",
    "helm",
    "just",
    "kubectl",
    "make",
    "mix",
    "mvn",
    "nix",
    "npm",
    "pacman",
    "pip",
    "pip3",
    "pipenv",
    "pnpm",
    "podman",
    "poetry",
    "rails",
    "rake",
    "rustup",
    "swift",
    "task",
    "terraform",
    "tofu",
    "uv",
    "yarn",
    "yum",
    "zig",
];

/// The resolved escape sequence a notice is sent as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Backend {
    Osc9,
    Osc777,
    Osc99,
    Bell,
}

impl Backend {
    /// How `glimps doctor` names it.
    pub(crate) fn describe(self) -> &'static str {
        match self {
            Backend::Osc9 => "OSC 9",
            Backend::Osc777 => "OSC 777",
            Backend::Osc99 => "OSC 99",
            Backend::Bell => "the terminal bell",
        }
    }
}

/// Resolve the configured method against the terminal GLIMPS runs in. `env`
/// reads one environment variable (injected so tests do not touch the process
/// environment).
///
/// `auto` only chooses a desktop-notification sequence for terminals that are
/// known to show one, and rings the bell everywhere else. Under tmux or GNU
/// screen it always rings the bell: they drop unknown OSC sequences unless
/// passthrough is configured, but forward a bell to the outer terminal and
/// mark the window.
pub(crate) fn resolve(method: NotifyMethod, env: impl Fn(&str) -> Option<String>) -> Backend {
    match method {
        NotifyMethod::Osc9 => return Backend::Osc9,
        NotifyMethod::Osc777 => return Backend::Osc777,
        NotifyMethod::Osc99 => return Backend::Osc99,
        NotifyMethod::Bell => return Backend::Bell,
        NotifyMethod::Auto => {}
    }
    let term = env("TERM").unwrap_or_default();
    if env("TMUX").is_some() || term.starts_with("tmux") || term.starts_with("screen") {
        return Backend::Bell;
    }
    if matches!(
        env("TERM_PROGRAM").as_deref(),
        Some("iTerm.app" | "WezTerm" | "ghostty")
    ) {
        return Backend::Osc9;
    }
    if env("KITTY_WINDOW_ID").is_some() || term == "xterm-kitty" {
        return Backend::Osc99;
    }
    if term.starts_with("foot") {
        return Backend::Osc777;
    }
    Backend::Bell
}

/// How the command ended, as far as a notice cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    Success,
    /// The last stage succeeded but an earlier pipeline stage did not.
    StageFailed,
    Failed(i32),
}

/// The notice text: `✓ cargo build finished in 2m14s`.
pub(super) fn message(command: &[u8], outcome: Outcome, elapsed: &str) -> String {
    let label = label(command);
    match outcome {
        Outcome::Success => format!("\u{2713} {label} finished in {elapsed}"),
        Outcome::StageFailed => {
            format!("\u{26a0} {label} finished in {elapsed}, but a pipeline stage failed")
        }
        Outcome::Failed(code) => format!("\u{2717} {label} failed (exit {code}) after {elapsed}"),
    }
}

/// `cargo build`, `make`, or `command` when no name can be read. The name is
/// whatever the user ran, so it is reduced to a conservative character set:
/// it must not be able to end the escape sequence it travels in or split the
/// `OSC 777` fields, whatever the shell hook sent.
fn label(command: &[u8]) -> String {
    let Some((name, subcommand)) = super::cmdline::name_and_subcommand(command) else {
        return "command".to_string();
    };
    let mut label: String = name
        .chars()
        .take(NAME_CAP)
        .map(|c| {
            if c.is_alphanumeric() || "._+-@:".contains(c) {
                c
            } else {
                '?'
            }
        })
        .collect();
    if name.chars().count() > NAME_CAP {
        label.push('\u{2026}'); // …
    }
    if let Some(subcommand) = subcommand.filter(|_| SUBCOMMAND_TOOLS.contains(&name.as_str())) {
        label.push(' ');
        label.push_str(&subcommand);
    }
    label
}

/// The bytes that deliver `body` through `backend`. Control characters are
/// dropped from the body here as a last line of defense, so no body can end
/// the sequence early whatever built it.
pub(super) fn sequence(backend: Backend, body: &str) -> Vec<u8> {
    let body: String = body.chars().filter(|c| !c.is_control()).collect();
    let (prefix, body) = match backend {
        Backend::Bell => return vec![0x07],
        Backend::Osc9 => ("\x1b]9;", body),
        Backend::Osc777 => ("\x1b]777;notify;GLIMPS;", body.replace(';', ",")),
        Backend::Osc99 => ("\x1b]99;;", body),
    };
    let mut out = Vec::with_capacity(prefix.len() + body.len() + 2);
    out.extend_from_slice(prefix.as_bytes());
    out.extend_from_slice(body.as_bytes());
    out.extend_from_slice(b"\x1b\\");
    out
}

/// When the user last typed into the session. The stdin forwarder touches it
/// on every read; the formatter reads it at a command's end to tell "walked
/// away" from "was typing into it" (a REPL, a prompt that needed an answer).
/// It holds a time only: never the bytes.
#[derive(Debug)]
pub struct InputActivity {
    base: Instant,
    /// Milliseconds since `base`, plus one; `0` means no input yet.
    last: AtomicU64,
}

impl InputActivity {
    pub fn new() -> Self {
        InputActivity {
            base: Instant::now(),
            last: AtomicU64::new(0),
        }
    }

    /// Record that input arrived now.
    pub fn touch(&self) {
        let millis = u64::try_from(self.base.elapsed().as_millis()).unwrap_or(u64::MAX - 1);
        self.last.store(millis.saturating_add(1), Ordering::Relaxed);
    }

    /// When input last arrived, if ever.
    pub(super) fn last(&self) -> Option<Instant> {
        let stored = self.last.load(Ordering::Relaxed);
        let millis = stored.checked_sub(1)?;
        self.base.checked_add(Duration::from_millis(millis))
    }
}

impl Default for InputActivity {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether input in the last [`PRESENCE_WINDOW`] of a command (and after it
/// started, so the Enter that launched it does not count) shows the user was
/// at the terminal when it finished.
pub(super) fn user_was_present(
    last_input: Option<Instant>,
    started: Instant,
    now: Instant,
) -> bool {
    last_input.is_some_and(|input| {
        input >= started && now.saturating_duration_since(input) < PRESENCE_WINDOW
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn explicit_methods_ignore_the_environment() {
        let env = env_of(&[("TERM_PROGRAM", "iTerm.app"), ("TMUX", "/tmp/x")]);
        assert_eq!(resolve(NotifyMethod::Osc9, &env), Backend::Osc9);
        assert_eq!(resolve(NotifyMethod::Osc777, &env), Backend::Osc777);
        assert_eq!(resolve(NotifyMethod::Osc99, &env), Backend::Osc99);
        assert_eq!(resolve(NotifyMethod::Bell, &env), Backend::Bell);
    }

    #[test]
    fn auto_picks_known_terminals_and_rings_the_bell_elsewhere() {
        let auto = |pairs: &[(&str, &str)]| resolve(NotifyMethod::Auto, env_of(pairs));
        assert_eq!(auto(&[("TERM_PROGRAM", "iTerm.app")]), Backend::Osc9);
        assert_eq!(auto(&[("TERM_PROGRAM", "WezTerm")]), Backend::Osc9);
        assert_eq!(auto(&[("TERM_PROGRAM", "ghostty")]), Backend::Osc9);
        assert_eq!(auto(&[("TERM", "xterm-kitty")]), Backend::Osc99);
        assert_eq!(auto(&[("KITTY_WINDOW_ID", "1")]), Backend::Osc99);
        assert_eq!(auto(&[("TERM", "foot")]), Backend::Osc777);
        assert_eq!(auto(&[("TERM", "foot-extra")]), Backend::Osc777);
        // Unknown or unsupported terminals get the bell, never a guess.
        assert_eq!(auto(&[("TERM_PROGRAM", "Apple_Terminal")]), Backend::Bell);
        assert_eq!(auto(&[("TERM_PROGRAM", "vscode")]), Backend::Bell);
        assert_eq!(auto(&[("TERM", "xterm-256color")]), Backend::Bell);
        assert_eq!(auto(&[]), Backend::Bell);
        // Multiplexers drop unknown OSC sequences; the bell gets through.
        assert_eq!(
            auto(&[
                ("TERM_PROGRAM", "iTerm.app"),
                ("TMUX", "/tmp/tmux-501/default,1,0")
            ]),
            Backend::Bell
        );
        assert_eq!(
            auto(&[("TERM_PROGRAM", "ghostty"), ("TERM", "screen-256color")]),
            Backend::Bell
        );
        assert_eq!(auto(&[("TERM", "tmux-256color")]), Backend::Bell);
    }

    #[test]
    fn messages_name_the_command_and_its_outcome() {
        assert_eq!(
            message(b"cargo build --release", Outcome::Success, "2m14s"),
            "\u{2713} cargo build finished in 2m14s"
        );
        assert_eq!(
            message(b"npm test -- --token=abc", Outcome::Failed(1), "45.2s"),
            "\u{2717} npm test failed (exit 1) after 45.2s"
        );
        assert_eq!(
            message(b"make | tee log", Outcome::StageFailed, "31.0s"),
            "\u{26a0} make finished in 31.0s, but a pipeline stage failed"
        );
        assert_eq!(
            message(b"", Outcome::Success, "1m00s"),
            "\u{2713} command finished in 1m00s"
        );
    }

    #[test]
    fn labels_never_carry_arguments_or_sequence_breakers() {
        assert_eq!(label(b"sleep 40"), "sleep");
        assert_eq!(
            label(b"curl -H 'Authorization: Bearer x' https://a"),
            "curl"
        );
        // Only known tools name a subcommand; any other second word could
        // be private.
        assert_eq!(label(b"./deploy.sh prod"), "deploy.sh");
        assert_eq!(label(b"unlock hunter"), "unlock");
        assert_eq!(label(b"ssh prod-db uptime"), "ssh");
        assert_eq!(label(b"docker compose up -d"), "docker compose");
        assert_eq!(label(b"sudo make install"), "make install");
        // A name holding a BEL, ESC, `;` or `$(…)` cannot reach the sequence.
        assert_eq!(label(b"a\x07b;c x"), "a?b?c");
        assert_eq!(label(b"\x1b]52;c;x"), "??52?c?x");
        assert_eq!(label(b"$(id)"), "??id?");
        assert_eq!(label("caf\u{e9} run".as_bytes()), "caf\u{e9}");
        let long = "x".repeat(40);
        assert_eq!(
            label(long.as_bytes()),
            format!("{}\u{2026}", "x".repeat(32))
        );
    }

    #[test]
    fn sequences_are_well_formed() {
        assert_eq!(sequence(Backend::Osc9, "done"), b"\x1b]9;done\x1b\\");
        assert_eq!(
            sequence(Backend::Osc777, "a;b"),
            b"\x1b]777;notify;GLIMPS;a,b\x1b\\"
        );
        assert_eq!(sequence(Backend::Osc99, "done"), b"\x1b]99;;done\x1b\\");
        assert_eq!(sequence(Backend::Bell, "done"), b"\x07");
    }

    #[test]
    fn sequence_bodies_cannot_end_the_sequence_early() {
        for backend in [Backend::Osc9, Backend::Osc777, Backend::Osc99] {
            let out = sequence(backend, "x\x07y\x1b]52;c;Zg==\x1b\\z\u{9c}\u{85}");
            let inner = &out[..out.len() - 2];
            assert!(
                !inner[1..].iter().any(|b| matches!(b, 0x07 | 0x1b)),
                "{backend:?}: {out:?}"
            );
            assert!(!String::from_utf8(out)
                .unwrap()
                .contains(['\u{9c}', '\u{85}']));
        }
    }

    #[test]
    fn input_activity_records_only_a_time() {
        let activity = InputActivity::new();
        assert_eq!(activity.last(), None);
        let before = Instant::now();
        activity.touch();
        let last = activity.last().unwrap();
        // Millisecond resolution: within a millisecond of the real time.
        assert!(last + Duration::from_millis(1) >= before);
        assert!(last <= Instant::now());
    }

    #[test]
    fn presence_counts_only_recent_input_after_the_start() {
        let started = Instant::now();
        let now = started + Duration::from_secs(60);
        assert!(!user_was_present(None, started, now));
        // Typed 3 s before the end: present (a REPL `exit`, an answered prompt).
        assert!(user_was_present(
            Some(now - Duration::from_secs(3)),
            started,
            now
        ));
        // Typed early on, then walked away.
        assert!(!user_was_present(
            Some(started + Duration::from_secs(5)),
            started,
            now
        ));
        assert!(!user_was_present(Some(now - PRESENCE_WINDOW), started, now));
        // The Enter that launched the command does not count, even when the
        // command was short.
        let short_end = started + Duration::from_secs(2);
        assert!(!user_was_present(
            started.checked_sub(Duration::from_millis(5)),
            started,
            short_end
        ));
    }
}
