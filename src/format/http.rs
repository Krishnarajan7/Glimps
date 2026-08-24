//! Buffered HTTP response formatter for `curl -i` / header+body output.
//!
//! The streaming line formatter already colors standalone `HTTP/1.1 404` lines.
//! This formatter handles the richer case: a response starts with a status line,
//! contains headers, then may contain a JSON/HTML body. It keeps detection tight
//! (`HTTP/` at the first non-whitespace byte opening a valid status line) so
//! ordinary text is not captured.

use super::theme::Theme;

/// Registry entry for buffered HTTP response formatting.
pub struct HttpResponse;

impl super::BufferedFormatter for HttpResponse {
    fn could_start(&self, head: &[u8]) -> bool {
        // The first line alone is the evidence, not the header block: curl's
        // stdout is line-buffered at a TTY, so the status line arrives in its
        // own flush and the headers routinely land a chunk later. Demanding a
        // `Name: value` line in the same chunk pushed every such response onto
        // the streaming path for good. Eagerness is safe by design — an
        // unconfirmed candidate is emitted verbatim at finalize.
        head.starts_with(b"HTTP/") && starts_like_status_line(head)
    }

    fn try_format(&self, bytes: &[u8], theme: &Theme) -> Option<Vec<u8>> {
        try_format(bytes, theme)
    }

    fn label(&self) -> &'static str {
        "HTTP"
    }

    fn needs_crlf(&self) -> bool {
        true
    }

    fn holds_across_stall(&self, buf: &[u8]) -> bool {
        // A *complete* valid status line is the evidence: nothing an
        // interactive prompt produces looks like `HTTP/2 200` followed by a
        // newline, and both gaps a real response has — status line to headers
        // (curl's next line-buffered write), header block to body (a network
        // round trip) — land after exactly that state. Without a hold the
        // 40 ms stall flush releases the run in one of those gaps, dropping it
        // to pass-through for the rest of the command.
        //
        // An *unterminated* first line never holds: a prompt that merely opens
        // with `HTTP/...` and waits for input must stay visible (liveness).
        //
        // Trimming matches `try_format` exactly, so holding a run always
        // implies it still has a real chance of formatting.
        let buf = trim_leading_newlines(buf);
        buf.starts_with(b"HTTP/") && complete_status_line(buf)
    }
}

/// Skip the leading CR/LF bytes [`try_format`] ignores — and only those, so a
/// run starting with a space (which `try_format` would reject) is never held.
fn trim_leading_newlines(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !matches!(byte, b'\r' | b'\n'))
        .unwrap_or(bytes.len());
    &bytes[start..]
}

/// Whether `head` (which starts with `HTTP/`) opens with something that is —
/// or could still grow into — a status line: `HTTP/<ver> <3-digit code> …`.
/// A first line already terminated by a newline must parse fully; a line still
/// arriving is judged on the prefix seen so far.
fn starts_like_status_line(head: &[u8]) -> bool {
    let (line, complete) = match head.iter().position(|&b| b == b'\n') {
        Some(end) => (&head[..end], true),
        None => (head, false),
    };
    let line = trim_trailing_crs(line);
    let mut fields = line
        .split(|&b| b == b' ' || b == b'\t')
        .filter(|field| !field.is_empty());
    let Some(version) = fields.next() else {
        return false;
    };
    if !version.starts_with(b"HTTP/") {
        return false;
    }
    let Some(code) = fields.next() else {
        // Only the version has arrived; plausible iff the line is still open.
        return !complete;
    };
    if !code.iter().all(u8::is_ascii_digit) {
        return false;
    }
    if code.len() == 3 {
        return true;
    }
    // A shorter code is a plausible prefix only while both the line and the
    // code field are still open.
    !complete && code.len() < 3 && fields.next().is_none()
}

/// Whether `bytes` (which starts with `HTTP/`) begins with a *complete*,
/// newline-terminated, valid status line.
fn complete_status_line(bytes: &[u8]) -> bool {
    let Some(end) = bytes.iter().position(|&b| b == b'\n') else {
        return false;
    };
    starts_like_status_line(&bytes[..=end])
}

fn trim_trailing_crs(line: &[u8]) -> &[u8] {
    let end = line
        .iter()
        .rposition(|&b| b != b'\r')
        .map_or(0, |last| last + 1);
    &line[..end]
}

pub fn try_format(bytes: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(bytes).ok()?;
    // Keep the trailing blank line: for a HEAD response it is the header/body
    // separator, even though there is intentionally no body after it.
    let text = text.trim_start_matches(['\r', '\n']);
    if !text.starts_with("HTTP/") {
        return None;
    }

    let mut rest = text;
    let mut out = String::with_capacity(text.len() + text.len() / 5);
    let mut formatted_any = false;

    while let Some((head, body)) = split_header_body(rest) {
        let mut lines = head.lines();
        let status = lines.next()?.trim_end_matches('\r');
        if !valid_status_line(status) {
            return None;
        }
        if formatted_any {
            out.push('\n');
        }
        render_status(&mut out, status, theme);
        for line in lines {
            render_header(&mut out, line.trim_end_matches('\r'), theme);
        }

        let body = body.trim_start_matches(['\r', '\n']);
        if body.starts_with("HTTP/") {
            rest = body;
            formatted_any = true;
            continue;
        }
        if !body.is_empty() {
            out.push('\n');
            render_body(&mut out, body.as_bytes(), theme);
        }
        formatted_any = true;
        break;
    }

    formatted_any.then(|| out.into_bytes())
}

/// Split the header block from the body at the first blank line.
///
/// Tolerates any number of CRs before each LF. The bytes GLIMPS sees are not
/// the bytes `curl` wrote: the inner PTY has `ONLCR` on, so it expands curl's
/// own `\r\n` to `\r\r\n` and the separator arrives as `\r\r\n\r\r\n`. Matching
/// only the literal `\r\n\r\n` / `\n\n` forms means every real terminal response
/// is declined while a response read from a file formats fine.
fn split_header_body(text: &str) -> Option<(&str, &str)> {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(offset) = bytes
        .get(from..)
        .and_then(|rest| rest.iter().position(|&byte| byte == b'\n'))
    {
        // `end_of_headers` terminates the last header line; the blank line that
        // follows it is CRs only, then its own LF at `blank`.
        let end_of_headers = from + offset;
        let mut blank = end_of_headers + 1;
        while bytes.get(blank) == Some(&b'\r') {
            blank += 1;
        }
        // Every byte scanned here is ASCII, so both indices are char boundaries.
        if bytes.get(blank) == Some(&b'\n') {
            return Some((&text[..end_of_headers], &text[blank + 1..]));
        }
        from = end_of_headers + 1;
    }
    None
}

fn valid_status_line(line: &str) -> bool {
    let mut parts = line.split_whitespace();
    let Some(version) = parts.next() else {
        return false;
    };
    let Some(code) = parts.next() else {
        return false;
    };
    version.starts_with("HTTP/")
        && code.len() == 3
        && code.as_bytes().iter().all(u8::is_ascii_digit)
}

fn render_status(out: &mut String, line: &str, theme: &Theme) {
    let mut parts = line.splitn(3, char::is_whitespace);
    let version = parts.next().unwrap_or("");
    let code = parts.next().unwrap_or("");
    let reason = parts.next().unwrap_or("").trim_start();
    let color = status_color(code.as_bytes(), theme);

    paint(out, theme.muted, theme.reset, version);
    out.push(' ');
    paint(out, color, theme.reset, code);
    if !reason.is_empty() {
        out.push(' ');
        out.push_str(reason);
    }
    out.push('\n');
}

fn render_header(out: &mut String, line: &str, theme: &Theme) {
    let Some((name, value)) = line.split_once(':') else {
        paint(out, theme.comment, theme.reset, line);
        out.push('\n');
        return;
    };
    paint(out, header_name_color(name, theme), theme.reset, name);
    paint(out, theme.html_delim, theme.reset, ":");
    out.push(' ');
    paint(
        out,
        header_value_color(name, theme),
        theme.reset,
        value.trim(),
    );
    out.push('\n');
}

fn render_body(out: &mut String, body: &[u8], theme: &Theme) {
    if let Some(formatted) = super::json::try_format(body, theme) {
        paint(out, theme.keyword, theme.reset, "JSON body");
        out.push('\n');
        out.push_str(&String::from_utf8_lossy(&formatted));
    } else if let Some(formatted) = super::html::try_format(body, theme) {
        paint(out, theme.keyword, theme.reset, "HTML body");
        out.push('\n');
        out.push_str(&String::from_utf8_lossy(&formatted));
    } else {
        out.push_str(&String::from_utf8_lossy(body));
    }
}

fn status_color(code: &[u8], theme: &Theme) -> &'static str {
    match code.first().copied() {
        Some(b'2') => theme.info,
        Some(b'3') => theme.debug,
        Some(b'4') => theme.warn,
        Some(b'5') => theme.error,
        _ => theme.comment,
    }
}

fn header_name_color(name: &str, theme: &Theme) -> &'static str {
    let _ = name;
    theme.muted
}

fn header_value_color(name: &str, theme: &Theme) -> &'static str {
    if eq(name, "content-type") || eq(name, "location") {
        theme.string
    } else if eq(name, "date") || eq(name, "last-modified") || eq(name, "expires") {
        theme.debug
    } else if eq(name, "content-length") || eq(name, "retry-after") || eq(name, "age") {
        theme.number
    } else {
        // Long CSP, cache and vary policies are already dense. Keeping their
        // values neutral makes the colored field name useful without creating
        // a wall of one accent color.
        ""
    }
}

fn eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn paint(out: &mut String, color: &str, reset: &str, text: &str) {
    out.push_str(color);
    out.push_str(text);
    out.push_str(reset);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_headers_and_json_body() {
        let input = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-Trace: abc\r\n\r\n{\"ok\":true}";
        let out = String::from_utf8(try_format(input, &Theme::plain()).unwrap()).unwrap();
        assert_eq!(
            out,
            "HTTP/1.1 200 OK\nContent-Type: application/json\nX-Trace: abc\n\nJSON body\n{\n  \"ok\": true\n}"
        );
    }

    #[test]
    fn formats_headers_only_response() {
        let input = b"HTTP/2 200\r\nContent-Type: text/html\r\nContent-Length: 559\r\n\r\n";
        let out = String::from_utf8(try_format(input, &Theme::plain()).unwrap()).unwrap();
        assert_eq!(
            out,
            "HTTP/2 200\nContent-Type: text/html\nContent-Length: 559\n"
        );
    }

    #[test]
    fn formats_redirect_chain() {
        let input = b"HTTP/1.1 301 Moved\r\nLocation: https://x.test\r\n\r\nHTTP/2 200 OK\r\nContent-Type: text/html\r\n\r\n<p>hi</p>";
        let out = String::from_utf8(try_format(input, &Theme::plain()).unwrap()).unwrap();
        assert!(out.contains("HTTP/1.1 301 Moved\nLocation: https://x.test"));
        assert!(out.contains("HTTP/2 200 OK\nContent-Type: text/html"));
        assert!(out.contains("HTML body\n<p>\n  hi\n</p>"));
    }

    #[test]
    fn status_line_evidence_matches_real_first_chunks() {
        // Complete first lines, with every CR shape the PTY produces.
        assert!(starts_like_status_line(b"HTTP/2 200 \r\r\n"));
        assert!(starts_like_status_line(
            b"HTTP/1.1 301 Moved Permanently\r\n"
        ));
        assert!(starts_like_status_line(b"HTTP/1.1 404 Not Found\nmore"));
        // Still-arriving first lines: plausible prefixes are accepted...
        assert!(starts_like_status_line(b"HTTP/"));
        assert!(starts_like_status_line(b"HTTP/1.1 "));
        assert!(starts_like_status_line(b"HTTP/1.1 20"));
        assert!(starts_like_status_line(b"HTTP/1.1 200 OK"));
        // ...but not text that can no longer become a status line.
        assert!(!starts_like_status_line(b"HTTP/2 is the successor"));
        assert!(!starts_like_status_line(b"HTTP/1.1 20 OK"));
        assert!(!starts_like_status_line(b"HTTP/2\r\n"));
        assert!(!starts_like_status_line(b"HTTP/1.1 20\n"));
        assert!(!starts_like_status_line(b"HTTP/1.1 2000 OK\n"));
    }

    #[test]
    fn only_a_terminated_status_line_holds_across_a_stall() {
        use super::super::BufferedFormatter;
        assert!(HttpResponse.holds_across_stall(b"HTTP/2 200 \r\r\n"));
        assert!(HttpResponse.holds_across_stall(b"HTTP/1.1 200 OK\r\nServer: x\r\n"));
        assert!(!HttpResponse.holds_across_stall(b"HTTP/1.1 200 OK"));
        assert!(!HttpResponse.holds_across_stall(b"HTTP/2 waiting\n"));
    }

    #[test]
    fn declines_incomplete_headers() {
        assert!(try_format(b"HTTP/1.1 200 OK\nContent-Type: text/html", &Theme::plain()).is_none());
        assert!(try_format(b"plain text\n\nbody", &Theme::plain()).is_none());
    }

    #[test]
    fn colored_output_is_valid_utf8() {
        let input = b"HTTP/1.1 404 Not Found\nContent-Type: text/html\n\n<p>nope</p>";
        let out = try_format(input, &Theme::default_colored()).unwrap();
        let s = std::str::from_utf8(&out).unwrap();
        assert!(s.contains("\x1b[38;5;220m404\x1b[0m"));
        assert!(s.contains("\x1b[35mHTML body\x1b[0m"));
    }

    // ---- property tests ----------------------------------------------------

    proptest::proptest! {
        /// `try_format` never panics on arbitrary bytes, and when it claims a
        /// run its output is valid UTF-8. The function slices a `&str` at byte
        /// indices found by scanning for LF/CR, so a boundary mistake would be
        /// a panic rather than a wrong answer — exactly what this pins.
        #[test]
        fn prop_try_format_never_panics_and_emits_utf8(bytes: Vec<u8>) {
            if let Some(out) = try_format(&bytes, &Theme::plain()) {
                proptest::prop_assert!(std::str::from_utf8(&out).is_ok());
            }
        }

        /// The same, over inputs shaped like real responses, so the property
        /// exercises the split/render path instead of declining almost always.
        /// Line endings are drawn from every form a PTY can produce, including
        /// the `\r\r\n` that ONLCR expansion creates.
        #[test]
        fn prop_response_shaped_input_never_panics(
            eol in proptest::sample::select(vec!["\n", "\r\n", "\r\r\n", "\r\r\r\n"]),
            code in 0u32..2000,
            headers in proptest::collection::vec("[ -~]{0,40}", 0..6),
            body in "[ -~\n]{0,120}",
        ) {
            let mut input = format!("HTTP/1.1 {code} Status{eol}");
            for header in &headers {
                input.push_str(header);
                input.push_str(eol);
            }
            input.push_str(eol);
            input.push_str(&body);
            if let Some(out) = try_format(input.as_bytes(), &Theme::plain()) {
                proptest::prop_assert!(std::str::from_utf8(&out).is_ok());
            }
        }

        /// Splitting is agnostic to how many CRs precede each LF: the same
        /// response with any of the PTY line endings renders identically once
        /// the endings themselves are normalized.
        #[test]
        fn prop_cr_padding_does_not_change_the_render(
            code in 100u32..600,
            value in "[ -~]{0,30}",
        ) {
            let render = |eol: &str| {
                let input =
                    format!("HTTP/1.1 {code} OK{eol}X-Test: {value}{eol}{eol}plain body");
                try_format(input.as_bytes(), &Theme::plain())
            };
            let baseline = render("\r\n");
            for eol in ["\n", "\r\r\n", "\r\r\r\n"] {
                proptest::prop_assert_eq!(render(eol), baseline.clone());
            }
        }
    }
}
