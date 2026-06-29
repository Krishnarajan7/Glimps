//! Command-line colorizer — turns a captured command into a syntax-colored
//! header, and extracts the command name for name-based bypass.
//!
//! This is a *best-effort visual* tokenizer, not a shell parser: it colors the
//! command name, quoted strings, flags, and operators, and leaves anything it's
//! unsure about in the default color. It is **byte-safe** — [`render`] only
//! inserts color escapes around the original bytes, never drops or reorders them,
//! so with the plain theme the output is byte-identical to the input.

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

/// Render `cmd` as a syntax-colored line: command name, `"quoted strings"`,
/// `-flags`, and operators get colors; everything else stays default. Preserves
/// every input byte (only color escapes are inserted).
pub fn render(cmd: &[u8], theme: &Theme) -> Vec<u8> {
    let mut out = Vec::with_capacity(cmd.len() * 2);
    let mut i = 0;
    // Whether the next bare word is a command name (true at start and right after
    // a command separator like `|`/`;`/`&`).
    let mut expect_command = true;

    while i < cmd.len() {
        let b = cmd[i];
        if b.is_ascii_whitespace() {
            let start = i;
            while i < cmd.len() && cmd[i].is_ascii_whitespace() {
                i += 1;
            }
            out.extend_from_slice(&cmd[start..i]); // whitespace: verbatim
        } else if b == b'"' || b == b'\'' {
            let start = i;
            i += 1;
            while i < cmd.len() && cmd[i] != b {
                // In double quotes, a backslash escapes the next byte.
                if b == b'"' && cmd[i] == b'\\' && i + 1 < cmd.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < cmd.len() {
                i += 1; // include the closing quote
            }
            paint(&mut out, theme.string, &cmd[start..i], theme.reset);
        } else if is_operator(b) {
            let start = i;
            while i < cmd.len() && is_operator(cmd[i]) {
                i += 1;
            }
            let op = &cmd[start..i];
            paint(&mut out, theme.comment, op, theme.reset);
            // A command separator starts a fresh command (so `ls | grep` colors
            // both names); redirects (`>`/`<`) do not.
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
                theme.key // command name
            } else if word.first() == Some(&b'-') {
                theme.warn // flag
            } else {
                "" // argument / path: default color
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
        assert!(s.contains("\x1b[36mcurl\x1b[0m")); // command -> cyan
        assert!(s.contains("\x1b[33m-s\x1b[0m")); // flag -> yellow
        assert!(s.contains("\x1b[32m\"https://x.com\"\x1b[0m")); // string -> green
    }

    #[test]
    fn colors_both_sides_of_a_pipe() {
        let s = rendered(b"ls | grep foo");
        assert!(s.contains("\x1b[36mls\x1b[0m"));
        assert!(s.contains("\x1b[36mgrep\x1b[0m")); // grep is a command name too
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

    proptest::proptest! {
        /// Byte-safety: with the plain theme, coloring is the identity — no input
        /// byte is ever dropped, reordered, or altered; and it never panics.
        #[test]
        fn prop_plain_render_is_identity(cmd: Vec<u8>) {
            proptest::prop_assert_eq!(render(&cmd, &Theme::plain()), cmd);
        }

        /// first_word never panics on arbitrary input.
        #[test]
        fn prop_first_word_never_panics(cmd: Vec<u8>) {
            let _ = first_word(&cmd);
        }
    }
}
