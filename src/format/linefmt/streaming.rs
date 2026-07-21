//! Streaming HTTP, log-severity, and stack-trace formatters.

use super::super::theme::Theme;
use super::super::StreamingFormatter;

/// How far into a line we look for a severity token. Log formats put the level
/// up front (possibly after a timestamp); requiring it early avoids matching a
/// stray uppercase word deep in a message.
const SEVERITY_WINDOW: usize = 48;

const LEVELS: &[(&[u8], Severity)] = &[
    (b"ERROR", Severity::Error),
    (b"FATAL", Severity::Error),
    (b"CRITICAL", Severity::Error),
    (b"WARNING", Severity::Warn),
    (b"WARN", Severity::Warn),
    (b"INFO", Severity::Info),
    (b"NOTICE", Severity::Info),
    (b"DEBUG", Severity::Debug),
    (b"TRACE", Severity::Debug),
];

#[derive(Clone, Copy)]
enum Severity {
    Error,
    Warn,
    Info,
    Debug,
}

impl Severity {
    fn color(self, theme: &Theme) -> &'static str {
        match self {
            Severity::Error => theme.error,
            Severity::Warn => theme.warn,
            Severity::Info => theme.info,
            Severity::Debug => theme.debug,
        }
    }
}

/// Registry entry: HTTP status-line coloring.
pub struct Http;
/// Registry entry: log-severity line coloring.
pub struct Logs;
/// Registry entry: stack-trace and panic highlighting.
pub struct StackTrace;

impl StreamingFormatter for Http {
    fn line_color(&self, content: &[u8], theme: &Theme) -> Option<&'static str> {
        http_status_color(content, theme)
    }
}

impl StreamingFormatter for Logs {
    fn line_color(&self, content: &[u8], theme: &Theme) -> Option<&'static str> {
        severity_color(content, theme)
    }
}

impl StreamingFormatter for StackTrace {
    fn line_color(&self, content: &[u8], theme: &Theme) -> Option<&'static str> {
        stacktrace_color(content, theme)
    }
}

/// Colorize one complete line using the first registered streaming formatter
/// that claims it. The line ending remains outside the color sequence.
pub fn colorize_line(
    line: &[u8],
    theme: &Theme,
    formatters: &[&dyn StreamingFormatter],
) -> Option<Vec<u8>> {
    let (content, ending) = split_line(line);
    if content.is_empty() {
        return None;
    }
    let color = formatters
        .iter()
        .find_map(|formatter| formatter.line_color(content, theme))?;

    let mut out = Vec::with_capacity(line.len() + color.len() + theme.reset.len());
    out.extend_from_slice(color.as_bytes());
    out.extend_from_slice(content);
    out.extend_from_slice(theme.reset.as_bytes());
    out.extend_from_slice(ending);
    Some(out)
}

fn split_line(line: &[u8]) -> (&[u8], &[u8]) {
    match line.strip_suffix(b"\n") {
        Some(rest) => {
            let content_len = rest
                .strip_suffix(b"\r")
                .map_or(rest.len(), |rest| rest.len());
            line.split_at(content_len)
        }
        None => (line, &line[line.len()..]),
    }
}

fn http_status_color(content: &[u8], theme: &Theme) -> Option<&'static str> {
    let content = ltrim(content);
    if !content.starts_with(b"HTTP/") {
        return None;
    }
    let space = content.iter().position(u8::is_ascii_whitespace)?;
    let rest = ltrim(&content[space..]);
    let code = rest.get(..3)?;
    if !code.iter().all(u8::is_ascii_digit) || rest.get(3).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    match code[0] {
        b'2' => Some(theme.info),
        b'3' => Some(theme.debug),
        b'4' => Some(theme.warn),
        b'5' => Some(theme.error),
        _ => None,
    }
}

fn severity_color(content: &[u8], theme: &Theme) -> Option<&'static str> {
    let content = ltrim(content);
    let window = &content[..content.len().min(SEVERITY_WINDOW)];
    LEVELS.iter().find_map(|(token, severity)| {
        contains_delimited(window, token).then(|| severity.color(theme))
    })
}

fn stacktrace_color(content: &[u8], theme: &Theme) -> Option<&'static str> {
    if content.starts_with(b"thread '") && window_contains(content, b"panicked at") {
        return Some(theme.error);
    }
    let trimmed = ltrim(content);
    if trimmed.starts_with(b"Traceback (most recent call last):") {
        return Some(theme.error);
    }
    if trimmed.starts_with(b"File \"") && window_contains(trimmed, b"\", line ") {
        return Some(theme.debug);
    }
    is_exception_line(content).then_some(theme.error)
}

/// Whether a line carries an ERROR-class severity token in its leading window.
pub(crate) fn is_error_log_line(content: &[u8]) -> bool {
    let content = ltrim(content);
    let window = &content[..content.len().min(SEVERITY_WINDOW)];
    LEVELS
        .iter()
        .filter(|(_, severity)| matches!(severity, Severity::Error))
        .any(|(token, _)| contains_delimited(window, token))
}

/// Whether `content` is a precise Python-style exception line.
pub(crate) fn is_exception_line(content: &[u8]) -> bool {
    let Some(colon) = content.iter().position(|byte| *byte == b':') else {
        return false;
    };
    let token = &content[..colon];
    if token.is_empty()
        || token.first() == Some(&b'.')
        || token.last() == Some(&b'.')
        || token.iter().any(|byte| !is_ident(*byte))
    {
        return false;
    }
    let class = token.rsplit(|byte| *byte == b'.').next().unwrap_or(token);
    if !class.first().is_some_and(u8::is_ascii_uppercase) {
        return false;
    }
    const SUFFIXES: &[&[u8]] = &[b"Error", b"Exception", b"Warning", b"Interrupt"];
    SUFFIXES.iter().any(|suffix| class.ends_with(suffix))
}

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.'
}

fn window_contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn contains_delimited(haystack: &[u8], token: &[u8]) -> bool {
    if token.is_empty() || token.len() > haystack.len() {
        return false;
    }
    for start in 0..=haystack.len() - token.len() {
        if &haystack[start..start + token.len()] != token {
            continue;
        }
        let before_ok = start == 0 || !is_word(haystack[start - 1]);
        let after = start + token.len();
        let after_ok = after == haystack.len() || !is_word(haystack[after]);
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Left-trim ASCII whitespace. Shared with error pinning.
pub(crate) fn ltrim(mut bytes: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = bytes {
        if first.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    bytes
}
