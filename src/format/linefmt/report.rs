//! Last-resort field coloring for plain report output.
//!
//! Many tools print `Label: value` lines (`timedatectl`, `hostnamectl`,
//! `sw_vers`, `brew config`) or `KEY=value` lines (`env`, `printenv`). No
//! view claims them and no streaming formatter matches them, so they reach
//! the terminal as a wall of one colour. This pass runs only after every
//! other formatter has declined, and paints just the field name and its
//! separator — the value is never touched, so nothing a reader copies out
//! is changed and there is nothing to get wrong about the value's kind.

use super::super::theme::Theme;
use super::{paint_bytes, split_line, trim_ascii_start};

/// Longest field name that still reads as a label rather than a sentence.
const MAX_LABEL_LEN: usize = 40;
/// A label is a few words; a sentence that happens to end in a colon is not.
const MAX_LABEL_WORDS: usize = 4;
const MAX_KEY_LEN: usize = 64;

/// Paint the `Label:` or `KEY=` prefix of one line, declining anything else.
pub fn colorize_report_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let lead = content.len() - trim_ascii_start(content).len();
    let body = &content[lead..];
    let (name_end, separator_len) = label_prefix(body).or_else(|| key_prefix(body))?;
    let mut out = Vec::with_capacity(line.len() + 32);
    out.extend_from_slice(&content[..lead]);
    paint_bytes(&mut out, theme.label, &body[..name_end], theme.reset);
    paint_bytes(
        &mut out,
        theme.html_delim,
        &body[name_end..name_end + separator_len],
        theme.reset,
    );
    out.extend_from_slice(&body[name_end + separator_len..]);
    out.extend_from_slice(ending);
    Some(out)
}

/// `Local time: …`, `System clock synchronized: yes`, `macOS: 26.1`. The
/// label starts with a letter, is a few plain words, and the colon is
/// followed by whitespace and a value — so `https://…`, `12:30`, `key:value`
/// and a bare `Example usage:` heading never match. A single all-lowercase
/// word is a tool naming itself (`rg: notes: unsupported encoding`), and log
/// severities belong to the log pass.
fn label_prefix(body: &[u8]) -> Option<(usize, usize)> {
    let colon = body.iter().position(|&b| b == b':')?;
    let label = &body[..colon];
    if label.is_empty()
        || label.len() > MAX_LABEL_LEN
        || !label[0].is_ascii_alphabetic()
        || label.last().is_some_and(u8::is_ascii_whitespace)
        || !label.iter().all(|&b| {
            b.is_ascii_alphanumeric() || matches!(b, b' ' | b'.' | b'-' | b'_' | b'/' | b'(' | b')')
        })
        || label.split(|&b| b == b' ').count() > MAX_LABEL_WORDS
        || is_log_severity(label)
        || (!label.contains(&b' ') && label.iter().all(|b| !b.is_ascii_uppercase()))
    {
        return None;
    }
    let value = trim_ascii_start(&body[colon + 1..]);
    if value.len() == body.len() - colon - 1 || value.is_empty() {
        return None;
    }
    Some((colon, 1))
}

/// `GOOGLE_CLIENT_ID=`, `PATH=/usr/bin`: a shell identifier straight from
/// column zero, followed by `=`.
fn key_prefix(body: &[u8]) -> Option<(usize, usize)> {
    let equals = body.iter().position(|&b| b == b'=')?;
    let key = &body[..equals];
    if key.is_empty()
        || key.len() > MAX_KEY_LEN
        || !(key[0].is_ascii_alphabetic() || key[0] == b'_')
        || !key.iter().all(|&b| b.is_ascii_alphanumeric() || b == b'_')
        || body.get(equals + 1) == Some(&b'=')
    {
        return None;
    }
    Some((equals, 1))
}

fn is_log_severity(label: &[u8]) -> bool {
    let lower = label.to_ascii_lowercase();
    matches!(
        lower.as_slice(),
        b"error"
            | b"warning"
            | b"warn"
            | b"info"
            | b"debug"
            | b"trace"
            | b"fatal"
            | b"notice"
            | b"note"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted(line: &str) -> Option<String> {
        colorize_report_line(line.as_bytes(), &Theme::default_colored())
            .map(|out| String::from_utf8(out).expect("valid utf-8"))
    }

    #[test]
    fn labels_and_keys_are_painted_and_values_left_alone() {
        assert_eq!(
            painted("               Local time: Sun 2026-09-13 13:58:40 UTC\n").as_deref(),
            Some("               \x1b[38;5;183mLocal time\x1b[0m\x1b[2m:\x1b[0m Sun 2026-09-13 13:58:40 UTC\n")
        );
        assert_eq!(
            painted("System clock synchronized: yes\r\n").as_deref(),
            Some("\x1b[38;5;183mSystem clock synchronized\x1b[0m\x1b[2m:\x1b[0m yes\r\n")
        );
        assert_eq!(
            painted("GOOGLE_CLIENT_SECRET=<set>\n").as_deref(),
            Some("\x1b[38;5;183mGOOGLE_CLIENT_SECRET\x1b[0m\x1b[2m=\x1b[0m<set>\n")
        );
        assert_eq!(
            painted("macOS: 26.1-arm64\n").as_deref(),
            Some("\x1b[38;5;183mmacOS\x1b[0m\x1b[2m:\x1b[0m 26.1-arm64\n")
        );
    }

    #[test]
    fn sentences_urls_times_and_severities_are_declined() {
        for line in [
            "https://example.com/path\n",
            "13:58:40\n",
            "key:value\n",
            "ERROR: boom\n",
            "Note: read this first\n",
            "Kernel Version:\n",
            "Example usage:\n",
            "rg: notes/archive: unsupported encoding\n",
            "zsh: command not found: foo\n",
            "This sentence ends with a colon and is long: yes\n",
            "1st thing: x\n",
            "a == b\n",
            "==> heading\n",
            "-flag=1\n",
            "some words then KEY=value\n",
            "\n",
        ] {
            assert_eq!(painted(line), None, "{line:?}");
        }
    }

    #[test]
    fn plain_theme_declines() {
        assert_eq!(
            colorize_report_line(b"Local time: now\n", &Theme::plain()),
            None
        );
    }
}
