//! Streaming HTTP, log-severity, and stack-trace formatters.

use super::super::theme::Theme;
use super::super::StreamingFormatter;

/// How far into a line we look for a severity token. Log formats put the level
/// up front (possibly after a timestamp); requiring it early avoids matching a
/// stray uppercase word deep in a message.
const SEVERITY_WINDOW: usize = 48;

const LEVELS: &[(&[u8], Severity)] = &[
    (b"ERROR", Severity::Error),
    (b"EXCEPTION", Severity::Error),
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

    fn format_line(&self, content: &[u8], ending: &[u8], theme: &Theme) -> Option<Vec<u8>> {
        colorize_log_line(content, ending, theme)
            .or_else(|| colorize_ssh_auth_line(content, ending, theme))
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
    formatters
        .iter()
        .find_map(|formatter| formatter.format_line(content, ending, theme))
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
    severity_match(window).map(|(_, _, severity)| severity.color(theme))
}

fn colorize_log_line(content: &[u8], ending: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let trimmed = ltrim(content);
    let leading = content.len() - trimmed.len();
    let window = &trimmed[..trimmed.len().min(SEVERITY_WINDOW)];
    let (start, len, severity) = severity_match(window)?;
    let level_start = leading + start;
    let level_end = level_start + len;

    let mut out = Vec::with_capacity(content.len() + ending.len() + 3 * theme.reset.len() + 32);
    if level_start > 0 {
        out.extend_from_slice(theme.debug.as_bytes());
        out.extend_from_slice(&content[..level_start]);
        out.extend_from_slice(theme.reset.as_bytes());
    }
    out.extend_from_slice(severity.color(theme).as_bytes());
    out.extend_from_slice(&content[level_start..level_end]);
    out.extend_from_slice(theme.reset.as_bytes());
    out.extend_from_slice(&content[level_end..]);
    out.extend_from_slice(ending);
    Some(out)
}

#[derive(Clone, Copy)]
struct ColorSpan {
    start: usize,
    end: usize,
    color: &'static str,
}

/// Color conventional OpenSSH authentication records even though they do not
/// carry an explicit `ERROR`/`INFO` level. The timestamp and `sshd[pid]:`
/// envelope make this deliberately narrow; prose containing "Failed password"
/// must not become an error log by accident.
fn colorize_ssh_auth_line(content: &[u8], ending: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let words = auth_word_spans(content);
    let (timestamp_end, host_index, service_index) = auth_envelope(&words, content)?;
    let (service_start, service_end) = words[service_index];
    let service = &content[service_start..service_end];
    let pid = sshd_pid_range(service)?;

    let message_start = content[service_end..]
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())?
        + service_end;
    let message = &content[message_start..];
    let (event_len, event_color) = ssh_auth_event(message, theme)?;

    let mut spans = vec![
        ColorSpan {
            start: words[0].0,
            end: timestamp_end,
            color: theme.debug,
        },
        ColorSpan {
            start: words[host_index].0,
            end: words[host_index].1,
            color: theme.muted,
        },
        ColorSpan {
            start: service_start,
            end: service_start + pid.0,
            color: theme.debug,
        },
        ColorSpan {
            start: service_start + pid.0,
            end: service_start + pid.1,
            color: theme.number,
        },
        ColorSpan {
            start: service_start + pid.1,
            end: service_end,
            color: theme.debug,
        },
        ColorSpan {
            start: message_start,
            end: message_start + event_len,
            color: event_color,
        },
    ];
    add_ssh_auth_subject_spans(message, message_start, theme, &mut spans);
    render_color_spans(content, ending, theme, &mut spans)
}

fn auth_envelope(words: &[(usize, usize)], content: &[u8]) -> Option<(usize, usize, usize)> {
    if words.len() < 3 {
        return None;
    }
    let word = |index: usize| &content[words[index].0..words[index].1];
    if looks_like_iso_timestamp(word(0)) {
        return Some((words[0].1, 1, 2));
    }
    if words.len() >= 5
        && looks_like_syslog_month(word(0))
        && word(1).iter().all(u8::is_ascii_digit)
        && looks_like_clock(word(2))
    {
        return Some((words[2].1, 3, 4));
    }
    None
}

fn looks_like_iso_timestamp(word: &[u8]) -> bool {
    word.len() >= 19
        && word.get(4) == Some(&b'-')
        && word.get(7) == Some(&b'-')
        && matches!(word.get(10), Some(b'T' | b't' | b' '))
        && word.get(13) == Some(&b':')
        && word.get(16) == Some(&b':')
        && [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
            .iter()
            .all(|index| word[*index].is_ascii_digit())
}

fn looks_like_syslog_month(word: &[u8]) -> bool {
    const MONTHS: &[&[u8]] = &[
        b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov",
        b"Dec",
    ];
    MONTHS.contains(&word)
}

fn looks_like_clock(word: &[u8]) -> bool {
    word.len() == 8
        && word[2] == b':'
        && word[5] == b':'
        && word
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 2 | 5) || byte.is_ascii_digit())
}

fn sshd_pid_range(service: &[u8]) -> Option<(usize, usize)> {
    let pid_start = b"sshd[".len();
    let pid_end = service.strip_prefix(b"sshd[")?.strip_suffix(b"]:")?.len() + pid_start;
    (pid_end > pid_start && service[pid_start..pid_end].iter().all(u8::is_ascii_digit))
        .then_some((pid_start, pid_end))
}

fn ssh_auth_event(message: &[u8], theme: &Theme) -> Option<(usize, &'static str)> {
    const FAILED: &[&[u8]] = &[
        b"Failed password",
        b"Failed publickey",
        b"Invalid user",
        b"authentication failure",
    ];
    const ACCEPTED: &[&[u8]] = &[
        b"Accepted password",
        b"Accepted publickey",
        b"Accepted keyboard-interactive",
    ];
    if let Some(event) = FAILED.iter().find(|event| message.starts_with(event)) {
        return Some((event.len(), theme.error));
    }
    ACCEPTED
        .iter()
        .find(|event| message.starts_with(event))
        .map(|event| (event.len(), theme.info))
}

fn add_ssh_auth_subject_spans(
    message: &[u8],
    offset: usize,
    theme: &Theme,
    spans: &mut Vec<ColorSpan>,
) {
    let Some(from) = find_bytes(message, b" from ") else {
        return;
    };
    let before_from = &message[..from];
    let subject_start = if let Some(marker) = find_bytes(before_from, b" for invalid user ") {
        let invalid_start = marker + b" for ".len();
        let invalid_end = invalid_start + b"invalid user".len();
        spans.push(ColorSpan {
            start: offset + invalid_start,
            end: offset + invalid_end,
            color: theme.warn,
        });
        Some(marker + b" for invalid user ".len())
    } else {
        find_bytes(before_from, b" for ").map(|marker| marker + b" for ".len())
    };
    if let Some(subject_start) = subject_start {
        if subject_start < from {
            spans.push(ColorSpan {
                start: offset + subject_start,
                end: offset + from,
                color: theme.string,
            });
        }
    }

    let address_start = from + b" from ".len();
    let port = find_bytes(&message[address_start..], b" port ")
        .map(|relative| address_start + relative)
        .unwrap_or(message.len());
    if address_start < port {
        spans.push(ColorSpan {
            start: offset + address_start,
            end: offset + port,
            color: theme.path,
        });
    }
    if port < message.len() {
        let port_start = port + b" port ".len();
        let port_end = message[port_start..]
            .iter()
            .position(u8::is_ascii_whitespace)
            .map(|relative| port_start + relative)
            .unwrap_or(message.len());
        if port_start < port_end && message[port_start..port_end].iter().all(u8::is_ascii_digit) {
            spans.push(ColorSpan {
                start: offset + port_start,
                end: offset + port_end,
                color: theme.number,
            });
        }
    }
}

fn render_color_spans(
    content: &[u8],
    ending: &[u8],
    theme: &Theme,
    spans: &mut [ColorSpan],
) -> Option<Vec<u8>> {
    spans.sort_unstable_by_key(|span| span.start);
    let mut cursor = 0;
    let mut out = Vec::with_capacity(content.len() + ending.len() + spans.len() * 12);
    for span in spans {
        if span.start < cursor || span.start >= span.end || span.end > content.len() {
            return None;
        }
        out.extend_from_slice(&content[cursor..span.start]);
        out.extend_from_slice(span.color.as_bytes());
        out.extend_from_slice(&content[span.start..span.end]);
        out.extend_from_slice(theme.reset.as_bytes());
        cursor = span.end;
    }
    out.extend_from_slice(&content[cursor..]);
    out.extend_from_slice(ending);
    Some(out)
}

fn auth_word_spans(content: &[u8]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut index = 0;
    while index < content.len() {
        while index < content.len() && content[index].is_ascii_whitespace() {
            index += 1;
        }
        let start = index;
        while index < content.len() && !content[index].is_ascii_whitespace() {
            index += 1;
        }
        if start < index {
            spans.push((start, index));
        }
    }
    spans
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    (!needle.is_empty() && needle.len() <= haystack.len()).then(|| {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    })?
}

fn severity_match(window: &[u8]) -> Option<(usize, usize, Severity)> {
    LEVELS.iter().find_map(|(token, severity)| {
        find_severity(window, token).map(|start| (start, token.len(), *severity))
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
        .any(|(token, _)| find_severity(window, token).is_some())
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

/// Match conventional uppercase levels everywhere in the leading log window,
/// while accepting title/lowercase variants only in a structured level slot.
/// This recognizes `2026-08-05 09:00:09 Error ...` and `[error] ...` without
/// painting ordinary prose such as `an error occurred`.
fn find_severity(haystack: &[u8], token: &[u8]) -> Option<usize> {
    if token.is_empty() || token.len() > haystack.len() {
        return None;
    }
    for start in 0..=haystack.len() - token.len() {
        let candidate = &haystack[start..start + token.len()];
        if !candidate.eq_ignore_ascii_case(token) {
            continue;
        }
        let before_ok = start == 0 || !is_word(haystack[start - 1]);
        let after = start + token.len();
        let after_ok = after == haystack.len() || !is_word(haystack[after]);
        if !before_ok || !after_ok {
            continue;
        }
        if candidate == token || structured_severity_prefix(&haystack[..start]) {
            return Some(start);
        }
    }
    None
}

fn structured_severity_prefix(prefix: &[u8]) -> bool {
    let prefix = ltrim(prefix);
    if prefix.is_empty() {
        return true;
    }

    // T and Z are permitted in an ISO-8601 timestamp, but arbitrary words are
    // not. Requiring a digit keeps prose like `The Error ...` out.
    let has_alpha = prefix.iter().any(u8::is_ascii_alphabetic);
    if !has_alpha {
        return true;
    }
    let has_digit = prefix.iter().any(u8::is_ascii_digit);
    has_digit
        && prefix
            .iter()
            .all(|byte| !byte.is_ascii_alphabetic() || matches!(byte, b'T' | b't' | b'Z' | b'z'))
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
