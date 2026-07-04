//! Buffered HTTP response formatter for `curl -i` / header+body output.
//!
//! The streaming line formatter already colors standalone `HTTP/1.1 404` lines.
//! This formatter handles the richer case: a response starts with a status line,
//! contains headers, then may contain a JSON/HTML body. It keeps detection tight
//! (`HTTP/` at the first non-whitespace byte plus a complete header block) so
//! ordinary text is not captured.

use super::theme::Theme;

/// Registry entry for buffered HTTP response formatting.
pub struct HttpResponse;

impl super::BufferedFormatter for HttpResponse {
    fn could_start(&self, head: &[u8]) -> bool {
        head.starts_with(b"HTTP/") && looks_like_headers(head)
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
}

fn looks_like_headers(bytes: &[u8]) -> bool {
    bytes
        .split(|&b| b == b'\n')
        .skip(1)
        .any(|line| line.iter().position(|&b| b == b':').is_some_and(|i| i > 0))
}

pub fn try_format(bytes: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(bytes).ok()?;
    let text = text.trim_matches(|c| c == '\r' || c == '\n');
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

fn split_header_body(text: &str) -> Option<(&str, &str)> {
    if let Some(i) = text.find("\r\n\r\n") {
        return Some((&text[..i], &text[i + 4..]));
    }
    text.find("\n\n").map(|i| (&text[..i], &text[i + 2..]))
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

    paint(out, theme.comment, theme.reset, version);
    out.push(' ');
    paint(out, color, theme.reset, code);
    if !reason.is_empty() {
        out.push(' ');
        paint(out, color, theme.reset, reason);
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
    if eq(name, "location") {
        theme.keyword
    } else if eq(name, "set-cookie") {
        theme.warn
    } else if eq(name, "content-type") || eq(name, "content-length") {
        theme.info
    } else {
        theme.key
    }
}

fn header_value_color(name: &str, theme: &Theme) -> &'static str {
    if eq(name, "location") {
        theme.keyword
    } else if eq(name, "set-cookie") {
        theme.warn
    } else {
        theme.string
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
    fn formats_redirect_chain() {
        let input = b"HTTP/1.1 301 Moved\r\nLocation: https://x.test\r\n\r\nHTTP/2 200 OK\r\nContent-Type: text/html\r\n\r\n<p>hi</p>";
        let out = String::from_utf8(try_format(input, &Theme::plain()).unwrap()).unwrap();
        assert!(out.contains("HTTP/1.1 301 Moved\nLocation: https://x.test"));
        assert!(out.contains("HTTP/2 200 OK\nContent-Type: text/html"));
        assert!(out.contains("HTML body\n<p>\n  hi\n</p>"));
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
        assert!(s.contains("\x1b[33m404\x1b[0m"));
        assert!(s.contains("\x1b[35mHTML body\x1b[0m"));
    }
}
