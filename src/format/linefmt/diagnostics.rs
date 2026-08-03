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

/// Message fragments that mark a `tool: …` line as a diagnostic rather than
/// ordinary output. Deliberately short and unambiguous: painting real output
/// red is worse than leaving a diagnostic uncolored, so a fragment earns its
/// place only if it is an errno string or a getopt message no tool would emit
/// on success.
const ERROR_FRAGMENTS: [&[u8]; 9] = [
    b"illegal option",
    b"invalid option",
    b"unknown option",
    b"unrecognized option",
    b"no such file or directory",
    b"permission denied",
    // macOS reports TCC-protected paths (~/Desktop, ~/Documents, ~/Downloads,
    // iCloud Drive) as EPERM, not EACCES. Without this, `find: .: Operation
    // not permitted` misses the diagnostic pass and falls through to the
    // `find` path view, which paints the whole line in the filename color —
    // a hard failure rendered to look like a result.
    b"operation not permitted",
    b"not a directory",
    b"is a directory",
];

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
    ERROR_FRAGMENTS
        .iter()
        .any(|fragment| contains_ascii_ci(message, fragment))
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
