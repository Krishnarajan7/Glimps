//! Streaming line colorizer — log severity + HTTP status.
//!
//! Unlike the buffered JSON/HTML formatters, this works **one complete line at a
//! time**, so it suits live/unbounded output (`tail -f`, `docker logs -f`). The
//! Formatter line-buffers across chunks and calls [`colorize_line`] on each
//! finished line.
//!
//! Precision first (charter invariant #2 — a false positive is worse than a miss):
//! * Severity matches only an **uppercase, word-delimited** level token near the
//!   start of the line (so prose like "an error occurred" is never colored).
//! * HTTP matches only lines beginning `HTTP/` with a real 3-digit status.
//!
//! On a match the whole line's content is wrapped in the level/class color (the
//! line ending is preserved outside the color). On no match, returns `None` so
//! the caller emits the line verbatim. With the plain theme the colors are empty,
//! so a "match" reproduces the input exactly — keeping the path byte-safe.

use super::theme::Theme;
use super::StreamingFormatter;

/// How far into a line we look for a severity token. Log formats put the level
/// up front (possibly after a timestamp); requiring it early avoids matching a
/// stray uppercase word deep in a message.
const SEVERITY_WINDOW: usize = 48;

/// Severity levels we recognize, longest-disambiguating handled by delimiting.
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

/// Colorize one line (which may include a trailing `\n` or `\r\n`). Asks each
/// streaming formatter in `formatters` (in order) for a color; the first that
/// claims the line wins. Returns the colored bytes, or `None` if no formatter
/// matched (the caller then emits the line verbatim).
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
        .find_map(|f| f.line_color(content, theme))?;

    let mut out = Vec::with_capacity(line.len() + color.len() + theme.reset.len());
    out.extend_from_slice(color.as_bytes());
    out.extend_from_slice(content);
    out.extend_from_slice(theme.reset.as_bytes());
    out.extend_from_slice(ending);
    Some(out)
}

/// Registry entry: HTTP status-line coloring. See [`StreamingFormatter`].
pub struct Http;
/// Registry entry: log-severity line coloring.
pub struct Logs;
/// Registry entry: stack-trace / panic highlighting.
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

/// Split a line into its content and its trailing newline (`""`, `"\n"`, or
/// `"\r\n"`), so coloring wraps only the content and leaves the ending intact.
fn split_line(line: &[u8]) -> (&[u8], &[u8]) {
    match line.strip_suffix(b"\n") {
        Some(rest) => {
            let content_len = rest.strip_suffix(b"\r").map_or(rest.len(), |r| r.len());
            line.split_at(content_len)
        }
        None => (line, &line[line.len()..]),
    }
}

/// Color for an HTTP status line (`HTTP/1.1 200 OK` → green, etc.), or `None`.
fn http_status_color(content: &[u8], theme: &Theme) -> Option<&'static str> {
    let c = ltrim(content);
    if !c.starts_with(b"HTTP/") {
        return None;
    }
    // The status code is the first whitespace-delimited 3-digit run after the
    // version token.
    let sp = c.iter().position(u8::is_ascii_whitespace)?;
    let rest = ltrim(&c[sp..]);
    let code = rest.get(..3)?;
    if !code.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if rest.get(3).is_some_and(u8::is_ascii_digit) {
        return None; // 4+ digit number, not a status code
    }
    match code[0] {
        b'2' => Some(theme.info),
        b'3' => Some(theme.debug),
        b'4' => Some(theme.warn),
        b'5' => Some(theme.error),
        _ => None,
    }
}

/// Color for a log-severity line, or `None`.
fn severity_color(content: &[u8], theme: &Theme) -> Option<&'static str> {
    let c = ltrim(content);
    let window = &c[..c.len().min(SEVERITY_WINDOW)];
    for (token, severity) in LEVELS {
        if contains_delimited(window, token) {
            return Some(severity.color(theme));
        }
    }
    None
}

/// Color for a stack-trace / panic line, or `None`. High-precision patterns only,
/// so ordinary output is never mistaken for a trace (invariant #2):
///   * Rust panic header (`thread '…' panicked at …`) and Python traceback header
///     (`Traceback (most recent call last):`) — error color;
///   * Python frame lines (`File "…", line N`) — dim;
///   * exception-type lines (`ValueError: …`, `pkg.mod.SomeError: …`) — error.
fn stacktrace_color(content: &[u8], theme: &Theme) -> Option<&'static str> {
    // Rust panic header (at column 0).
    if content.starts_with(b"thread '") && window_contains(content, b"panicked at") {
        return Some(theme.error);
    }
    let t = ltrim(content);
    // Python traceback header.
    if t.starts_with(b"Traceback (most recent call last):") {
        return Some(theme.error);
    }
    // Python frame: `File "<path>", line <n>` (indented under the header).
    if t.starts_with(b"File \"") && window_contains(t, b"\", line ") {
        return Some(theme.debug);
    }
    // Exception-type line: a dotted CamelCase identifier ending in
    // Error/Exception/Warning/Interrupt, at column 0, followed by `:`.
    if is_exception_line(content) {
        return Some(theme.error);
    }
    None
}

/// Whether `content` is a Python-style exception line: it begins at column 0 with
/// a dotted identifier (`pkg.mod.Name`) whose last segment ends in a known
/// exception suffix, immediately followed by `:`. Precise enough that prose like
/// `Note: see above` or `http://x: y` does not match.
fn is_exception_line(content: &[u8]) -> bool {
    // Must start at column 0 with an uppercase-or-lowercase identifier char (a
    // dotted module path may be lowercase), but NOT whitespace.
    let Some(colon) = content.iter().position(|&b| b == b':') else {
        return false;
    };
    let token = &content[..colon];
    // A dotted identifier with no leading/trailing dot (so `.Error` / `Error.`
    // don't qualify) and only identifier bytes.
    if token.is_empty()
        || token.first() == Some(&b'.')
        || token.last() == Some(&b'.')
        || token.iter().any(|b| !is_ident(*b))
    {
        return false;
    }
    // The final dotted segment is the class name; require it to start uppercase
    // and end in a recognized suffix.
    let class = token.rsplit(|&b| b == b'.').next().unwrap_or(token);
    if !class.first().is_some_and(u8::is_ascii_uppercase) {
        return false;
    }
    const SUFFIXES: &[&[u8]] = &[b"Error", b"Exception", b"Warning", b"Interrupt"];
    SUFFIXES.iter().any(|s| class.ends_with(s))
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
}

/// Whether `needle` occurs anywhere in `haystack`.
fn window_contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Whether `token` appears in `haystack` as a whole word (not flanked by
/// `[A-Za-z0-9_]`), so `ERROR` matches in `[ERROR]` / `ERROR:` but not inside
/// `MYERROR` or `ERROR_CODE`.
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

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn ltrim(mut bytes: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = bytes {
        if first.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All streaming formatters enabled, in priority order.
    fn all() -> [&'static dyn StreamingFormatter; 3] {
        [&Http, &Logs, &StackTrace]
    }

    fn colored(line: &[u8]) -> Option<String> {
        colorize_line(line, &Theme::default_colored(), &all())
            .map(|v| String::from_utf8(v).unwrap())
    }

    #[test]
    fn colors_uppercase_severity_lines() {
        assert!(colored(b"ERROR: boom\n").unwrap().starts_with("\x1b[31m"));
        assert!(colored(b"[WARN] low disk\n")
            .unwrap()
            .starts_with("\x1b[33m"));
        assert!(colored(b"2026-01-01 INFO started\n")
            .unwrap()
            .starts_with("\x1b[32m"));
        assert!(colored(b"DEBUG x=1\n").unwrap().starts_with("\x1b[2m"));
    }

    #[test]
    fn preserves_content_and_line_ending() {
        let out = colorize_line(b"ERROR: boom\r\n", &Theme::default_colored(), &all()).unwrap();
        assert_eq!(out, b"\x1b[31mERROR: boom\x1b[0m\r\n");
        let out = colorize_line(b"ERROR: boom\n", &Theme::default_colored(), &all()).unwrap();
        assert_eq!(out, b"\x1b[31mERROR: boom\x1b[0m\n");
        // No trailing newline is fine too.
        let out = colorize_line(b"ERROR: boom", &Theme::default_colored(), &all()).unwrap();
        assert_eq!(out, b"\x1b[31mERROR: boom\x1b[0m");
    }

    #[test]
    fn http_status_lines_color_by_class() {
        assert!(colored(b"HTTP/1.1 200 OK\n")
            .unwrap()
            .starts_with("\x1b[32m"));
        assert!(colored(b"HTTP/2 301 Moved\n")
            .unwrap()
            .starts_with("\x1b[2m"));
        assert!(colored(b"HTTP/1.1 404 Not Found\n")
            .unwrap()
            .starts_with("\x1b[33m"));
        assert!(colored(b"HTTP/1.1 500 Boom\n")
            .unwrap()
            .starts_with("\x1b[31m"));
    }

    #[test]
    fn does_not_color_prose_or_lowercase() {
        // Lowercase and mid-sentence words must NOT trigger (precision).
        assert!(colored(b"an error occurred while connecting\n").is_none());
        assert!(colored(b"No errors found.\n").is_none());
        // Level beyond the SEVERITY_WINDOW (50-char prefix, then ERROR) is ignored.
        assert!(
            colored(b"this is a long informational sentence with no level... ERROR\n").is_none()
        );
        assert!(colored(b"MYERROR is a variable\n").is_none()); // not delimited
        assert!(colored(b"ERROR_CODE = 5\n").is_none()); // underscore = word char
        assert!(colored(b"plain text line\n").is_none());
        assert!(colored(b"\n").is_none());
    }

    #[test]
    fn does_not_color_non_status_http_like_lines() {
        assert!(colored(b"HTTP/1.1 banana\n").is_none()); // no numeric code
        assert!(colored(b"HTTP/1.1 2000 weird\n").is_none()); // 4-digit
        assert!(colored(b"GET /api HTTP/1.1\n").is_none()); // doesn't start with HTTP/
    }

    #[test]
    fn plain_theme_is_byte_identical_on_match() {
        // The whole point that keeps the streaming path byte-safe: with no colors,
        // a "matched" line reproduces the input exactly.
        let line = b"ERROR: boom\r\n";
        assert_eq!(colorize_line(line, &Theme::plain(), &all()).unwrap(), line);
    }

    #[test]
    fn highlights_stack_traces() {
        // Rust panic header, Python traceback header + frame + exception line.
        assert!(colored(b"thread 'main' panicked at src/main.rs:42:10:\n")
            .unwrap()
            .starts_with("\x1b[31m")); // red
        assert!(colored(b"Traceback (most recent call last):\n")
            .unwrap()
            .starts_with("\x1b[31m"));
        assert!(colored(b"  File \"app.py\", line 10, in <module>\n")
            .unwrap()
            .starts_with("\x1b[2m")); // dim frame
        assert!(colored(b"ValueError: bad input\n")
            .unwrap()
            .starts_with("\x1b[31m"));
        assert!(colored(b"json.decoder.JSONDecodeError: Expecting value\n")
            .unwrap()
            .starts_with("\x1b[31m"));
    }

    #[test]
    fn does_not_mistake_prose_for_a_stack_trace() {
        // Precision: ordinary "Word: ..." lines and paths must NOT be colored.
        assert!(colored(b"Note: see the docs above\n").is_none());
        assert!(colored(b"http://example.com: reachable\n").is_none());
        assert!(colored(b"TODO: refactor this later\n").is_none());
        assert!(colored(b"Summary: 3 passed\n").is_none());
        assert!(colored(b"at the store I bought milk\n").is_none());
        // "error: ..." lowercase is a cargo/compiler style, handled by logs not here;
        // it must not be caught as an exception line (lowercase class).
        assert!(colored(b"error: could not compile\n").is_none());
        // Leading/trailing dot tokens are not valid exception identifiers.
        assert!(colored(b".Error: leading dot\n").is_none());
        assert!(colored(b"Error.: trailing dot\n").is_none());
    }

    #[test]
    fn stacktrace_gate_off_disables_it() {
        // With StackTrace not in the registry, a panic header is left alone.
        let without_trace: [&dyn StreamingFormatter; 2] = [&Http, &Logs];
        assert!(colorize_line(
            b"thread 'main' panicked at x:1:1:\n",
            &Theme::default_colored(),
            &without_trace,
        )
        .is_none());
    }

    proptest::proptest! {
        /// Never panics, and with the plain theme any match is byte-identical to
        /// the input (so the streaming path can't corrupt user bytes).
        #[test]
        fn prop_plain_is_identity_and_never_panics(line: Vec<u8>) {
            let fmts: [&dyn StreamingFormatter; 3] = [&Http, &Logs, &StackTrace];
            if let Some(out) = colorize_line(&line, &Theme::plain(), &fmts) {
                proptest::prop_assert_eq!(out, line);
            }
        }
    }
}
