//! Streaming JSON-lines detection and token coloring.

use super::super::theme::Theme;
use super::{paint_bytes, split_line, trim_ascii, trim_ascii_end, trim_ascii_start};
use serde_json::Value;

/// Color one complete JSON-lines row without reflowing it.
pub fn colorize_json_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let (content, ending) = split_line(line);
    if content.is_empty() || theme.reset.is_empty() {
        return None;
    }
    if !is_json_line_content(content) {
        return None;
    }
    let trimmed = trim_ascii(content);
    let mut out = Vec::with_capacity(line.len() + 64);
    let prefix_len = content.len() - trim_ascii_start(content).len();
    let suffix_len = content.len() - trim_ascii_end(content).len();
    out.extend_from_slice(&content[..prefix_len]);
    colorize_json_tokens(&mut out, trimmed, theme);
    out.extend_from_slice(&content[content.len() - suffix_len..]);
    out.extend_from_slice(ending);
    Some(out)
}

/// Whether a complete line is one JSON object/array. Used by the formatter core
/// to avoid buffering JSON-lines streams as one impossible giant JSON document.
pub fn is_json_line(line: &[u8]) -> bool {
    let (content, _) = split_line(line);
    is_json_line_content(content)
}

fn is_json_line_content(content: &[u8]) -> bool {
    let trimmed = trim_ascii(content);
    if !matches!(
        (trimmed.first(), trimmed.last()),
        (Some(b'{'), Some(b'}')) | (Some(b'['), Some(b']'))
    ) {
        return false;
    }
    serde_json::from_slice::<Value>(trimmed).is_ok()
}

fn colorize_json_tokens(out: &mut Vec<u8>, bytes: &[u8], theme: &Theme) {
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let end = json_string_end(bytes, i);
                let color = if next_json_non_ws(bytes, end) == Some(b':') {
                    theme.key
                } else {
                    theme.string
                };
                paint_bytes(out, color, &bytes[i..end], theme.reset);
                i = end;
            }
            b'-' | b'0'..=b'9' => {
                let end = json_number_end(bytes, i);
                paint_bytes(out, theme.number, &bytes[i..end], theme.reset);
                i = end;
            }
            b't' if bytes[i..].starts_with(b"true") => {
                paint_bytes(out, theme.keyword, &bytes[i..i + 4], theme.reset);
                i += 4;
            }
            b'f' if bytes[i..].starts_with(b"false") => {
                paint_bytes(out, theme.keyword, &bytes[i..i + 5], theme.reset);
                i += 5;
            }
            b'n' if bytes[i..].starts_with(b"null") => {
                paint_bytes(out, theme.keyword, &bytes[i..i + 4], theme.reset);
                i += 4;
            }
            b'{' | b'}' | b'[' | b']' | b':' | b',' => {
                paint_bytes(out, theme.html_delim, &bytes[i..i + 1], theme.reset);
                i += 1;
            }
            _ => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
}

fn json_string_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

fn next_json_non_ws(bytes: &[u8], start: usize) -> Option<u8> {
    bytes
        .iter()
        .skip(start)
        .copied()
        .find(|b| !b.is_ascii_whitespace())
}

fn json_number_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && matches!(bytes[i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
        i += 1;
    }
    i
}
