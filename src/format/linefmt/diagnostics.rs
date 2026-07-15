//! Conservative command-line diagnostic detection.

use super::super::theme::Theme;
use super::{paint_whole, split_line, trim_ascii, trim_ascii_start};

/// Color common CLI diagnostic lines before command-specific formatters get a
/// chance to make them look like normal output. PTYs merge stdout/stderr into one
/// stream, so we infer conservatively from familiar tool wording.
pub fn colorize_cli_diagnostic_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }

    if is_usage_line(trimmed) {
        return Some(paint_whole(content, ending, theme.warn, theme.reset));
    }
    if is_cli_error_line(trimmed) {
        return Some(paint_whole(content, ending, theme.error, theme.reset));
    }
    None
}

fn is_usage_line(trimmed: &[u8]) -> bool {
    starts_with_ascii_ci(trimmed, b"usage:")
}

pub(crate) fn is_cli_error_line(trimmed: &[u8]) -> bool {
    let Some(colon) = trimmed.iter().position(|&b| b == b':') else {
        return false;
    };
    let tool = trim_ascii(&trimmed[..colon]);
    if tool.is_empty()
        || tool.len() > 64
        || !tool
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/' | b'+'))
    {
        return false;
    }
    let message = trim_ascii_start(&trimmed[colon + 1..]);
    contains_ascii_ci(message, b"illegal option")
        || contains_ascii_ci(message, b"invalid option")
        || contains_ascii_ci(message, b"unknown option")
        || contains_ascii_ci(message, b"unrecognized option")
        || contains_ascii_ci(message, b"no such file or directory")
        || contains_ascii_ci(message, b"permission denied")
}

fn starts_with_ascii_ci(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && haystack[..needle.len()].eq_ignore_ascii_case(needle)
}

fn contains_ascii_ci(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && needle.len() <= haystack.len()
        && haystack
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}
