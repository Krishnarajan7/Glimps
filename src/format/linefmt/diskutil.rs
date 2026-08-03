//! macOS `diskutil info` report formatting.

use super::super::theme::Theme;
use super::{paint_bytes, split_line, trim_ascii, trim_ascii_end, trim_ascii_start};

/// Color one field from the aligned label/value report emitted by
/// `diskutil info`. Whitespace and every visible byte are preserved.
pub fn colorize_diskutil_info_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    let colon = trimmed.iter().position(|byte| *byte == b':')?;
    let label_raw = &trimmed[..colon];
    let label = trim_ascii_end(label_raw);
    if !valid_label(label) {
        return None;
    }

    let offset = content.len() - trimmed.len();
    let colon_at = offset + colon;
    let after_colon = &content[colon_at + 1..];
    let value = trim_ascii_start(after_colon);
    let gap = after_colon.len() - value.len();

    let mut out = Vec::with_capacity(line.len() + 80);
    out.extend_from_slice(&content[..offset]);
    paint_bytes(&mut out, theme.key, label, theme.reset);
    out.extend_from_slice(&label_raw[label.len()..]);
    paint_bytes(&mut out, theme.html_delim, b":", theme.reset);
    out.extend_from_slice(&after_colon[..gap]);
    if !value.is_empty() {
        paint_diskutil_value(&mut out, label, value, theme);
    }
    out.extend_from_slice(ending);
    Some(out)
}

fn valid_label(label: &[u8]) -> bool {
    !label.is_empty()
        && label.iter().all(|byte| {
            byte.is_ascii_alphanumeric()
                || byte.is_ascii_whitespace()
                || matches!(*byte, b'(' | b')' | b'-' | b'/' | b'#')
        })
}

fn paint_diskutil_value(out: &mut Vec<u8>, label: &[u8], value: &[u8], theme: &Theme) {
    let lower_label = label.iter().map(u8::to_ascii_lowercase).collect::<Vec<_>>();
    let core = trim_ascii(value);

    if matches!(lower_label.as_slice(), b"device node" | b"mount point") {
        paint_bytes(out, theme.path, value, theme.reset);
    } else if core == b"Yes" {
        paint_bytes(out, theme.info, value, theme.reset);
    } else if core == b"No" {
        paint_bytes(out, theme.muted, value, theme.reset);
    } else if lower_label.ends_with(b"status") {
        let color = if contains_ascii_case_insensitive(core, b"fail") {
            theme.error
        } else if contains_ascii_case_insensitive(core, b"verified") {
            theme.info
        } else {
            theme.warn
        };
        paint_bytes(out, color, value, theme.reset);
    } else if contains_bytes(&lower_label, b"uuid") {
        paint_bytes(out, theme.number, value, theme.reset);
    } else if contains_bytes(&lower_label, b"file system")
        || matches!(
            lower_label.as_slice(),
            b"partition type" | b"type (bundle)" | b"protocol" | b"media type"
        )
    {
        paint_bytes(out, theme.keyword, value, theme.reset);
    } else if is_numeric_report_field(&lower_label) {
        paint_numeric_expression(out, value, theme);
    } else {
        paint_bytes(out, theme.string, value, theme.reset);
    }
}

fn is_numeric_report_field(label: &[u8]) -> bool {
    [b"size".as_slice(), b"space", b"offset", b"count"]
        .iter()
        .any(|needle| label.windows(needle.len()).any(|part| part == *needle))
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|part| part == needle)
}

fn paint_numeric_expression(out: &mut Vec<u8>, value: &[u8], theme: &Theme) {
    let mut index = 0;
    while index < value.len() {
        let start = index;
        let color = if value[index].is_ascii_digit() {
            index += 1;
            while index < value.len()
                && (value[index].is_ascii_digit() || matches!(value[index], b'.' | b',' | b'%'))
            {
                index += 1;
            }
            theme.number
        } else if matches!(value[index], b'(' | b')') {
            index += 1;
            theme.html_delim
        } else {
            index += 1;
            while index < value.len()
                && !value[index].is_ascii_digit()
                && !matches!(value[index], b'(' | b')')
            {
                index += 1;
            }
            theme.string
        };
        paint_bytes(out, color, &value[start..index], theme.reset);
    }
}

fn contains_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
    })
}
