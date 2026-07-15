//! Shared byte-preserving line formatting primitives.

use super::super::theme::Theme;

/// Split a line into content and its trailing newline, if present.
pub(crate) fn split_line(line: &[u8]) -> (&[u8], &[u8]) {
    match line.strip_suffix(b"\n") {
        Some(rest) => {
            let content_len = rest.strip_suffix(b"\r").map_or(rest.len(), |r| r.len());
            line.split_at(content_len)
        }
        None => (line, &line[line.len()..]),
    }
}

pub(crate) fn colorize_size_path_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let words = word_spans(content);
    if words.len() < 2 {
        return None;
    }
    Some(colorize_words(content, ending, theme, |idx, _| match idx {
        0 => Some(theme.number),
        _ => Some(theme.key),
    }))
}

pub(crate) fn colorize_words<F>(
    content: &[u8],
    ending: &[u8],
    theme: &Theme,
    mut color: F,
) -> Vec<u8>
where
    F: FnMut(usize, &[u8]) -> Option<&'static str>,
{
    let spans = word_spans(content);
    let mut out = Vec::with_capacity(content.len() + ending.len() + spans.len() * 8);
    let mut cursor = 0;
    for (idx, (start, end)) in spans.into_iter().enumerate() {
        out.extend_from_slice(&content[cursor..start]);
        let word = &content[start..end];
        match color(idx, word) {
            Some(c) => {
                out.extend_from_slice(c.as_bytes());
                out.extend_from_slice(word);
                out.extend_from_slice(theme.reset.as_bytes());
            }
            None => out.extend_from_slice(word),
        }
        cursor = end;
    }
    out.extend_from_slice(&content[cursor..]);
    out.extend_from_slice(ending);
    out
}

pub(crate) fn word_spans(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if start < i {
            spans.push((start, i));
        }
    }
    spans
}

pub(crate) fn paint_whole(content: &[u8], ending: &[u8], color: &str, reset: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len() + ending.len() + color.len() + reset.len());
    out.extend_from_slice(color.as_bytes());
    out.extend_from_slice(content);
    out.extend_from_slice(reset.as_bytes());
    out.extend_from_slice(ending);
    out
}

pub(crate) fn paint_span(
    content: &[u8],
    ending: &[u8],
    start: usize,
    end: usize,
    color: &str,
    reset: &str,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len() + ending.len() + color.len() + reset.len());
    out.extend_from_slice(&content[..start]);
    out.extend_from_slice(color.as_bytes());
    out.extend_from_slice(&content[start..end]);
    out.extend_from_slice(reset.as_bytes());
    out.extend_from_slice(&content[end..]);
    out.extend_from_slice(ending);
    out
}

pub(crate) fn paint_bytes(out: &mut Vec<u8>, color: &str, bytes: &[u8], reset: &str) {
    out.extend_from_slice(color.as_bytes());
    out.extend_from_slice(bytes);
    out.extend_from_slice(reset.as_bytes());
}

pub(crate) fn trim_ascii_start(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    bytes
}

pub(crate) fn contains_ascii(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|w| w == needle)
}

pub(crate) fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

pub(crate) fn trim_ascii_end(mut bytes: &[u8]) -> &[u8] {
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}
