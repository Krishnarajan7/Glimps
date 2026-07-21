//! Man-page, Markdown, and configuration-file views.

use super::super::theme::Theme;
use super::{
    paint_bytes, paint_span, paint_whole, split_line, trim_ascii, trim_ascii_end, trim_ascii_start,
    CodeLanguage,
};

/// Clean and lightly format classic man-page overstrike output.
pub fn format_man_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let (content, ending) = split_line(line);
    let had_overstrike = content.contains(&b'\x08');
    let cleaned = if had_overstrike {
        clean_overstrike(content)
    } else {
        content.to_vec()
    };
    if cleaned.is_empty() {
        return had_overstrike.then(|| ending.to_vec());
    }

    let heading = is_man_heading(&cleaned);
    if !had_overstrike && (!heading || theme.reset.is_empty()) {
        return None;
    }
    let color = if heading { theme.key } else { theme.string };
    let mut out = Vec::with_capacity(cleaned.len() + ending.len() + 16);
    out.extend_from_slice(color.as_bytes());
    out.extend_from_slice(&cleaned);
    out.extend_from_slice(theme.reset.as_bytes());
    out.extend_from_slice(ending);
    Some(out)
}

/// Color Markdown output from project-file commands such as `cat README.md`.
/// This is intentionally visual-only: no wrapping, rendering, or byte changes.
pub fn colorize_markdown_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    if content.is_empty() {
        return None;
    }
    let trimmed = trim_ascii_start(content);
    if trimmed.starts_with(b"#") && heading_marker_len(trimmed).is_some() {
        return Some(paint_whole(content, ending, theme.key, theme.reset));
    }
    if trimmed.starts_with(b">") {
        return Some(paint_whole(content, ending, theme.comment, theme.reset));
    }
    if is_markdown_rule(trimmed) {
        return Some(paint_whole(content, ending, theme.comment, theme.reset));
    }
    if markdown_list_marker_len(trimmed).is_some() {
        return paint_markdown_inline(content, ending, theme, Some((trimmed, theme.warn)));
    }
    if let Some((start, end)) = fenced_code_span(content) {
        return Some(paint_span(
            content,
            ending,
            start,
            end,
            theme.keyword,
            theme.reset,
        ));
    }
    paint_markdown_inline(content, ending, theme, None)
}

pub fn markdown_fence_language(line: &[u8]) -> Option<Option<CodeLanguage>> {
    let (content, _) = split_line(line);
    let trimmed = trim_ascii_start(content);
    let fence = if trimmed.starts_with(b"```") {
        b"```"
    } else if trimmed.starts_with(b"~~~") {
        b"~~~"
    } else {
        return None;
    };
    let rest = trim_ascii(&trimmed[fence.len()..]);
    if rest.is_empty() {
        return Some(None);
    }
    let lang = rest
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(**b, b'+' | b'#' | b'-' | b'_'))
        .copied()
        .collect::<Vec<_>>();
    Some(markdown_code_language(&lang))
}

/// Color YAML / TOML / INI / dotenv-style config lines.
pub fn colorize_config_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }
    if matches!(trimmed.first(), Some(b'#' | b';')) {
        return Some(paint_whole(content, ending, theme.comment, theme.reset));
    }
    if trimmed.starts_with(b"[") && trimmed.contains(&b']') {
        return Some(paint_whole(content, ending, theme.keyword, theme.reset));
    }
    if trimmed.starts_with(b"- ") {
        return Some(paint_prefix(content, ending, theme, trimmed, theme.warn));
    }
    if let Some(idx) = key_value_separator(trimmed) {
        let offset = content.len() - trimmed.len();
        let key_end = offset + idx;
        let sep_start = key_end;
        let sep_end = sep_start + 1;
        let key_raw = &content[offset..key_end];
        let key_core = trim_ascii_end(key_raw);
        let mut out = Vec::with_capacity(content.len() + ending.len() + 48);
        out.extend_from_slice(&content[..offset]);
        out.extend_from_slice(theme.key.as_bytes());
        out.extend_from_slice(key_core);
        out.extend_from_slice(theme.reset.as_bytes());
        out.extend_from_slice(&content[offset + key_core.len()..sep_start]);
        out.extend_from_slice(theme.html_delim.as_bytes());
        out.extend_from_slice(&content[sep_start..sep_end]);
        out.extend_from_slice(theme.reset.as_bytes());
        let value = &content[sep_end..];
        if !trim_ascii(value).is_empty() {
            out.extend_from_slice(color_for_config_value(value, theme).as_bytes());
            out.extend_from_slice(value);
            out.extend_from_slice(theme.reset.as_bytes());
        } else {
            out.extend_from_slice(value);
        }
        out.extend_from_slice(ending);
        return Some(out);
    }
    None
}

fn paint_prefix(
    content: &[u8],
    ending: &[u8],
    theme: &Theme,
    trimmed: &[u8],
    color: &str,
) -> Vec<u8> {
    let offset = content.len() - trimmed.len();
    let prefix_len = markdown_list_marker_len(trimmed).unwrap_or(2);
    let prefix_end = offset + prefix_len.min(trimmed.len());
    let mut out = Vec::with_capacity(content.len() + ending.len() + 16);
    out.extend_from_slice(&content[..offset]);
    out.extend_from_slice(color.as_bytes());
    out.extend_from_slice(&content[offset..prefix_end]);
    out.extend_from_slice(theme.reset.as_bytes());
    out.extend_from_slice(&content[prefix_end..]);
    out.extend_from_slice(ending);
    out
}

fn paint_markdown_inline(
    content: &[u8],
    ending: &[u8],
    theme: &Theme,
    prefix: Option<(&[u8], &str)>,
) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(content.len() + ending.len() + 96);
    let mut i = 0;
    let mut colored_any = false;
    if let Some((trimmed, color)) = prefix {
        let offset = content.len() - trimmed.len();
        let prefix_len = markdown_list_marker_len(trimmed).unwrap_or(2);
        let prefix_end = offset + prefix_len.min(trimmed.len());
        out.extend_from_slice(&content[..offset]);
        out.extend_from_slice(color.as_bytes());
        out.extend_from_slice(&content[offset..prefix_end]);
        out.extend_from_slice(theme.reset.as_bytes());
        i = prefix_end;
        colored_any = true;
    }

    while i < content.len() {
        if let Some(end) = markdown_html_comment_end(content, i) {
            paint_bytes(&mut out, theme.comment, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if let Some(end) = markdown_inline_code_end(content, i) {
            paint_bytes(&mut out, theme.keyword, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if let Some(end) = markdown_strong_end(content, i) {
            paint_bytes(&mut out, theme.string, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if let Some((label_end, url_end)) = markdown_link_end(content, i) {
            paint_bytes(&mut out, theme.string, &content[i..label_end], theme.reset);
            paint_bytes(
                &mut out,
                theme.debug,
                &content[label_end..url_end],
                theme.reset,
            );
            colored_any = true;
            i = url_end;
        } else {
            out.push(content[i]);
            i += 1;
        }
    }

    if !colored_any {
        return None;
    }
    out.extend_from_slice(ending);
    Some(out)
}

fn heading_marker_len(bytes: &[u8]) -> Option<usize> {
    let n = bytes.iter().take_while(|&&b| b == b'#').count();
    (1..=6)
        .contains(&n)
        .then_some(n)
        .filter(|_| bytes.get(n).is_some_and(u8::is_ascii_whitespace))
}

fn markdown_list_marker_len(bytes: &[u8]) -> Option<usize> {
    if matches!(bytes, [b'-' | b'*' | b'+', ws, ..] if ws.is_ascii_whitespace()) {
        return Some(2);
    }
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits > 0
        && digits <= 3
        && matches!(bytes.get(digits), Some(b'.' | b')'))
        && bytes.get(digits + 1).is_some_and(u8::is_ascii_whitespace)
    {
        return Some(digits + 2);
    }
    None
}

fn is_markdown_rule(bytes: &[u8]) -> bool {
    let compact = bytes
        .iter()
        .copied()
        .filter(|b| !b.is_ascii_whitespace())
        .collect::<Vec<_>>();
    compact.len() >= 3
        && compact
            .iter()
            .all(|&b| b == compact[0] && matches!(b, b'-' | b'*' | b'_'))
}

fn fenced_code_span(bytes: &[u8]) -> Option<(usize, usize)> {
    let trimmed = trim_ascii_start(bytes);
    let offset = bytes.len() - trimmed.len();
    (trimmed.starts_with(b"```") || trimmed.starts_with(b"~~~")).then_some((offset, bytes.len()))
}

fn markdown_inline_code_end(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'`') {
        return None;
    }
    let end_rel = bytes[start + 1..].iter().position(|&b| b == b'`')?;
    Some(start + 2 + end_rel)
}

fn markdown_strong_end(bytes: &[u8], start: usize) -> Option<usize> {
    let marker = match (bytes.get(start), bytes.get(start + 1)) {
        (Some(b'*'), Some(b'*')) => b"**",
        (Some(b'_'), Some(b'_')) => b"__",
        _ => return None,
    };
    let end_rel = bytes[start + 2..]
        .windows(2)
        .position(|window| window == marker)?;
    Some(start + 4 + end_rel)
}

fn markdown_link_end(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let close = bytes[start + 1..].iter().position(|&b| b == b']')? + start + 1;
    if bytes.get(close + 1) != Some(&b'(') {
        return None;
    }
    let url_close = bytes[close + 2..].iter().position(|&b| b == b')')? + close + 2;
    Some((close + 1, url_close + 1))
}

fn markdown_html_comment_end(bytes: &[u8], start: usize) -> Option<usize> {
    if !bytes[start..].starts_with(b"<!--") {
        return None;
    }
    let end_rel = bytes[start + 4..]
        .windows(3)
        .position(|window| window == b"-->")?;
    Some(start + 7 + end_rel)
}

fn markdown_code_language(lang: &[u8]) -> Option<CodeLanguage> {
    let lower = lang.to_ascii_lowercase();
    match lower.as_slice() {
        b"bash" | b"sh" | b"shell" | b"zsh" | b"fish" | b"console" => Some(CodeLanguage::Shell),
        b"rust" | b"rs" => Some(CodeLanguage::Rust),
        b"python" | b"py" => Some(CodeLanguage::Python),
        b"javascript" | b"js" | b"jsx" => Some(CodeLanguage::JavaScript),
        b"typescript" | b"ts" | b"tsx" => Some(CodeLanguage::TypeScript),
        b"go" => Some(CodeLanguage::Go),
        b"java" => Some(CodeLanguage::Java),
        b"kotlin" | b"kt" => Some(CodeLanguage::Kotlin),
        b"swift" => Some(CodeLanguage::Swift),
        b"ruby" | b"rb" => Some(CodeLanguage::Ruby),
        b"php" => Some(CodeLanguage::Php),
        b"css" | b"scss" | b"sass" => Some(CodeLanguage::Css),
        b"c" | b"h" | b"cpp" | b"cc" | b"cxx" | b"hpp" => Some(CodeLanguage::CLike),
        _ => None,
    }
}

fn key_value_separator(bytes: &[u8]) -> Option<usize> {
    let sep = bytes.iter().position(|&b| b == b'=' || b == b':')?;
    let key = trim_ascii(&bytes[..sep]);
    if key.is_empty() || key.iter().any(|&b| matches!(b, b'{' | b'}' | b'[' | b']')) {
        return None;
    }
    Some(sep)
}

fn color_for_config_value(value: &[u8], theme: &Theme) -> &'static str {
    let trimmed = trim_ascii_start(value);
    if trimmed
        .first()
        .is_some_and(|&b| b == b'"' || b == b'\'' || b == b'[')
    {
        theme.string
    } else if trimmed
        .first()
        .is_some_and(|b| b.is_ascii_digit() || *b == b'-')
    {
        theme.number
    } else if matches!(
        trimmed,
        b" true" | b" false" | b"true" | b"false" | b"null" | b"nil"
    ) {
        theme.keyword
    } else {
        theme.string
    }
}

fn clean_overstrike(content: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len());
    let mut i = 0;
    while i < content.len() {
        if i + 2 < content.len() && content[i + 1] == b'\x08' {
            let left = content[i];
            let right = content[i + 2];
            if left == right || left == b'_' {
                out.push(right);
                i += 3;
                continue;
            }
        }
        if content[i] != b'\x08' {
            out.push(content[i]);
        }
        i += 1;
    }
    out
}

fn is_man_heading(content: &[u8]) -> bool {
    let trimmed = trim_ascii(content);
    !trimmed.is_empty()
        && trimmed.len() <= 48
        && trimmed.iter().all(|b| {
            b.is_ascii_uppercase() || b.is_ascii_digit() || matches!(b, b' ' | b'-' | b'_')
        })
        && trimmed.iter().any(u8::is_ascii_uppercase)
}
