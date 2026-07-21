//! Command-line helpers for the ▌ header: neutralize control bytes so a captured
//! command is safe inside GLIMPS's own chrome, and extract the command name for
//! name-based bypass.
//!
//! Both operate on the raw captured command bytes and are **byte-safe**. The
//! header renderer only inserts SGR color escapes around safe display bytes, and
//! the sanitizer only drops or replaces control bytes (all single-byte ASCII),
//! never splitting a multibyte UTF-8 sequence.

use super::theme::Theme;

/// Wrapper commands to look past when finding the "real" command for bypass
/// (`sudo vim`, `env X=1 less`, …).
const WRAPPERS: &[&str] = &[
    "sudo", "env", "command", "nohup", "time", "doas", "exec", "builtin", "stdbuf",
];

/// Shell operator bytes (best-effort; used only for coloring).
fn is_operator(b: u8) -> bool {
    matches!(b, b'|' | b'&' | b';' | b'<' | b'>' | b'(' | b')')
}

/// Render `cmd` as a syntax-colored line: command names, quoted strings, flags,
/// and operators get colors; everything else stays default. Preserves every
/// input byte, only inserting color escapes.
pub fn render(cmd: &[u8], theme: &Theme) -> Vec<u8> {
    let mut out = Vec::with_capacity(cmd.len() * 2);
    let mut i = 0;
    let mut expect_command = true;

    while i < cmd.len() {
        let b = cmd[i];
        if b.is_ascii_whitespace() {
            let start = i;
            while i < cmd.len() && cmd[i].is_ascii_whitespace() {
                i += 1;
            }
            out.extend_from_slice(&cmd[start..i]);
        } else if b == b'"' || b == b'\'' {
            let start = i;
            i += 1;
            while i < cmd.len() && cmd[i] != b {
                if b == b'"' && cmd[i] == b'\\' && i + 1 < cmd.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < cmd.len() {
                i += 1;
            }
            paint(&mut out, theme.string, &cmd[start..i], theme.reset);
        } else if is_operator(b) {
            let start = i;
            while i < cmd.len() && is_operator(cmd[i]) {
                i += 1;
            }
            let op = &cmd[start..i];
            paint(&mut out, theme.comment, op, theme.reset);
            if op.iter().any(|&c| matches!(c, b'|' | b';' | b'&')) {
                expect_command = true;
            }
        } else {
            let start = i;
            while i < cmd.len()
                && !cmd[i].is_ascii_whitespace()
                && cmd[i] != b'"'
                && cmd[i] != b'\''
                && !is_operator(cmd[i])
            {
                i += 1;
            }
            let word = &cmd[start..i];
            let color = if expect_command {
                expect_command = false;
                theme.key
            } else if word.first() == Some(&b'-') {
                theme.warn
            } else {
                ""
            };
            paint(&mut out, color, word, theme.reset);
        }
    }
    out
}

/// The command's name (basename), looking past wrapper commands and env
/// assignments, for name-based bypass. `sudo vim` -> `vim`, `/usr/bin/less` ->
/// `less`. `None` if the command is empty or not valid UTF-8.
pub fn first_word(cmd: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(cmd).ok()?;
    for word in text.split_whitespace() {
        // Skip wrappers, flags, and `VAR=value` env assignments to reach the real
        // command being run.
        if WRAPPERS.contains(&word) || word.starts_with('-') || is_env_assignment(word) {
            continue;
        }
        let base = word.rsplit('/').next().unwrap_or(word);
        if base.is_empty() {
            continue;
        }
        return Some(base.to_string());
    }
    None
}

/// `VAR=value` style env assignment (a `=` before any `/`, with a name-ish lhs).
fn is_env_assignment(word: &str) -> bool {
    match word.split_once('=') {
        Some((lhs, _)) => {
            !lhs.is_empty()
                && !lhs.contains('/')
                && lhs.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        }
        None => false,
    }
}

/// Neutralize control bytes that are unsafe inside a single-line, GLIMPS-authored
/// display line (the `▌` command header, the `moved to <cwd>` breadcrumb, the
/// `command failed: <cmd>` footer). A captured command or cwd can carry raw
/// `ESC`/C0 controls — planted by a forged marker or a maliciously named
/// file/dir — which, emitted verbatim inside GLIMPS's own trusted-looking
/// chrome, would move the cursor, clear the screen, or inject an OSC-8 link
/// (BUG #2). Any run of control bytes (`0x00..=0x1F`, which includes ESC, CR,
/// LF and TAB — a header is one line — plus DEL `0x7F`) collapses to a single
/// space, keeping the visible text readable and on one line.
///
/// Byte-safe on arbitrary input: control bytes are all single-byte ASCII, so
/// dropping them can never split a multibyte UTF-8 sequence, and it never
/// panics. The result is additionally guaranteed to be valid UTF-8: GLIMPS-
/// authored chrome must never emit invalid sequences (invariant #4), and a
/// forged marker or hostile filename can carry them — invalid bytes become
/// U+FFFD. This must ONLY be applied to GLIMPS-generated chrome — never to
/// pass-through output.
pub(crate) fn sanitize_display(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut in_control_run = false;
    for &b in bytes {
        if b <= 0x1F || b == 0x7F {
            // Collapse a run of control bytes into one space.
            if !in_control_run {
                out.push(b' ');
                in_control_run = true;
            }
        } else {
            out.push(b);
            in_control_run = false;
        }
    }
    match String::from_utf8_lossy(&out) {
        // Already valid — no copy was made, return the buffer as-is.
        std::borrow::Cow::Borrowed(_) => out,
        // Invalid sequences were replaced with U+FFFD.
        std::borrow::Cow::Owned(fixed) => fixed.into_bytes(),
    }
}

fn paint(out: &mut Vec<u8>, color: &str, text: &[u8], reset: &str) {
    out.extend_from_slice(color.as_bytes());
    out.extend_from_slice(text);
    out.extend_from_slice(reset.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(cmd: &[u8]) -> String {
        String::from_utf8(render(cmd, &Theme::default_colored())).unwrap()
    }

    #[test]
    fn colors_command_string_and_flag() {
        let s = rendered(br#"curl -s "https://x.com""#);
        assert!(s.contains("\x1b[36mcurl\x1b[0m"));
        assert!(s.contains("\x1b[38;5;220m-s\x1b[0m"));
        assert!(s.contains("\x1b[38;5;117m\"https://x.com\"\x1b[0m"));
    }

    #[test]
    fn colors_both_sides_of_a_pipe() {
        let s = rendered(b"ls | grep foo");
        assert!(s.contains("\x1b[36mls\x1b[0m"));
        assert!(s.contains("\x1b[36mgrep\x1b[0m"));
    }

    #[test]
    fn plain_theme_is_byte_identical() {
        for cmd in [
            &b"echo hello"[..],
            br#"curl -s "a b" | jq '.x'"#,
            b"VAR=1 sudo -i vim /etc/hosts",
            b"git commit -m 'msg with spaces'",
            b"",
        ] {
            assert_eq!(render(cmd, &Theme::plain()), cmd);
        }
    }

    #[test]
    fn first_word_basenames_and_skips_wrappers() {
        assert_eq!(first_word(b"vim file").as_deref(), Some("vim"));
        assert_eq!(first_word(b"/usr/bin/less x").as_deref(), Some("less"));
        assert_eq!(first_word(b"sudo vim /etc/hosts").as_deref(), Some("vim"));
        assert_eq!(first_word(b"sudo -i htop").as_deref(), Some("htop"));
        assert_eq!(first_word(b"env FOO=bar ssh host").as_deref(), Some("ssh"));
        assert_eq!(first_word(b"  git   status ").as_deref(), Some("git"));
        assert_eq!(first_word(b"").as_deref(), None);
        assert_eq!(first_word(b"sudo").as_deref(), None); // only a wrapper
    }

    #[test]
    fn sanitize_display_strips_controls_keeps_text() {
        // Each control byte (ESC, C0, DEL) is neutralized; a run collapses to
        // one space. Only the control bytes go: the `[2J` printables of a CSI
        // survive, but with their ESC gone they can no longer act.
        assert_eq!(sanitize_display(b"clear\x1b[2J"), b"clear [2J".to_vec());
        assert_eq!(
            sanitize_display(b"a\x00\x07\x1b\x7fb"), // run of controls -> one space
            b"a b".to_vec()
        );
        assert_eq!(
            sanitize_display(b"line1\r\nline2"), // CR/LF are controls -> one space
            b"line1 line2".to_vec()
        );
        // Normal printable text (incl. multibyte UTF-8) is untouched.
        assert_eq!(sanitize_display(b"echo hi"), b"echo hi".to_vec());
        assert_eq!("café ▌".as_bytes(), sanitize_display("café ▌".as_bytes()));
        // Invalid UTF-8 (forged marker / hostile filename) becomes U+FFFD
        // instead of leaking into GLIMPS-authored chrome.
        assert_eq!(
            sanitize_display(b"echo \xff\xfe hi"),
            "echo \u{fffd}\u{fffd} hi".as_bytes().to_vec()
        );
    }

    proptest::proptest! {
        /// first_word never panics on arbitrary input.
        #[test]
        fn prop_first_word_never_panics(cmd: Vec<u8>) {
            let _ = first_word(&cmd);
        }

        /// Byte-safety: with the plain theme, coloring is the identity.
        #[test]
        fn prop_plain_render_is_identity(cmd: Vec<u8>) {
            proptest::prop_assert_eq!(render(&cmd, &Theme::plain()), cmd);
        }

        /// sanitize_display never panics, never emits a control byte, and its
        /// output is always valid UTF-8 (invariant #4 — GLIMPS chrome must
        /// never inject invalid sequences, whatever a forged marker carried).
        #[test]
        fn prop_sanitize_display_removes_all_controls(bytes: Vec<u8>) {
            let out = sanitize_display(&bytes);
            proptest::prop_assert!(out.iter().all(|&b| b > 0x1F && b != 0x7F));
            proptest::prop_assert!(std::str::from_utf8(&out).is_ok());
        }
    }
}
