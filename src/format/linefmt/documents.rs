//! Man-page, Markdown, and configuration-file views.

use super::super::theme::Theme;
use super::{
    paint_bytes, paint_span, paint_whole, split_line, trim_ascii, trim_ascii_end, trim_ascii_start,
    CodeLanguage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownEmbeddedLanguage {
    Code(CodeLanguage),
    Html,
}

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

/// Color one `whatis`/`apropos` result while preserving its aligned text.
///
/// Results have the shape `name(section), alias(section) - description`. Some
/// shell built-in entries contain hundreds of aliases on one physical line, so
/// this parser streams spans into the result without splitting or wrapping it.
pub fn colorize_man_index_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let separator = content.windows(3).position(|window| window == b" - ")?;
    let entries = &content[..separator];
    let description = &content[separator + 3..];
    if description.is_empty() || !valid_man_index_entries(entries) {
        return None;
    }

    let mut out = Vec::with_capacity(line.len() + 96);
    let mut index = 0;
    while index < entries.len() {
        let start = index;
        match entries[index] {
            byte if byte.is_ascii_whitespace() => {
                index += 1;
                while index < entries.len() && entries[index].is_ascii_whitespace() {
                    index += 1;
                }
                out.extend_from_slice(&entries[start..index]);
            }
            b'(' => {
                paint_bytes(&mut out, theme.html_delim, b"(", theme.reset);
                index += 1;
                let section_start = index;
                while index < entries.len() && entries[index] != b')' {
                    index += 1;
                }
                paint_bytes(
                    &mut out,
                    theme.number,
                    &entries[section_start..index],
                    theme.reset,
                );
                paint_bytes(&mut out, theme.html_delim, b")", theme.reset);
                index += 1;
            }
            b',' => {
                index += 1;
                while index < entries.len() && entries[index].is_ascii_whitespace() {
                    index += 1;
                }
                paint_bytes(
                    &mut out,
                    theme.html_delim,
                    &entries[start..index],
                    theme.reset,
                );
            }
            _ => {
                index += 1;
                while index < entries.len() && !matches!(entries[index], b'(' | b',') {
                    index += 1;
                }
                paint_bytes(&mut out, theme.key, &entries[start..index], theme.reset);
            }
        }
    }
    paint_bytes(&mut out, theme.html_delim, b" - ", theme.reset);
    paint_bytes(&mut out, theme.muted, description, theme.reset);
    out.extend_from_slice(ending);
    Some(out)
}

fn valid_man_index_entries(entries: &[u8]) -> bool {
    let mut saw_entry = false;
    for entry in entries.split(|byte| *byte == b',') {
        let entry = trim_ascii(entry);
        let Some(open) = entry.iter().rposition(|byte| *byte == b'(') else {
            return false;
        };
        if !entry.ends_with(b")") || open == 0 || open + 2 >= entry.len() {
            return false;
        }
        let section = &entry[open + 1..entry.len() - 1];
        if !section
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'+' | b'-'))
        {
            return false;
        }
        saw_entry = true;
    }
    saw_entry
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

pub fn markdown_fence_language(line: &[u8]) -> Option<Option<MarkdownEmbeddedLanguage>> {
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

/// Color Apache per-directory configuration without pretending it is HTML.
///
/// `.htaccess` combines directive records, XML-shaped section containers,
/// regular expressions, environment variables, URLs, MIME types, and flag
/// lists. Known token shapes receive restrained semantic colors; regexes and
/// free-form arguments remain untouched. Selection is filename-gated, so
/// ordinary Apache-looking output cannot be claimed accidentally.
pub fn colorize_htaccess_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with(b"#") {
        return Some(paint_whole(content, ending, theme.comment, theme.reset));
    }

    let offset = content.len() - trimmed.len();
    let mut out = Vec::with_capacity(line.len() + 96);
    out.extend_from_slice(&content[..offset]);

    if trimmed.starts_with(b"<") {
        colorize_apache_section(trimmed, ending, theme, &mut out)?;
        return Some(out);
    }

    let directive_end = trimmed
        .iter()
        .position(u8::is_ascii_whitespace)
        .unwrap_or(trimmed.len());
    let directive = &trimmed[..directive_end];
    if directive.is_empty()
        || !directive
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
    {
        return None;
    }
    paint_bytes(&mut out, theme.html_name, directive, theme.reset);
    colorize_apache_arguments(&trimmed[directive_end..], theme, &mut out);
    out.extend_from_slice(ending);
    Some(out)
}

fn colorize_apache_section(
    trimmed: &[u8],
    ending: &[u8],
    theme: &Theme,
    out: &mut Vec<u8>,
) -> Option<()> {
    let close = trimmed.iter().rposition(|byte| *byte == b'>')?;
    if close + 1 != trimmed.len() {
        return None;
    }
    let name_start = if trimmed.get(1) == Some(&b'/') { 2 } else { 1 };
    let name_end = trimmed[name_start..close]
        .iter()
        .position(u8::is_ascii_whitespace)
        .map_or(close, |index| name_start + index);
    let name = &trimmed[name_start..name_end];
    if name.is_empty()
        || !name
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
    {
        return None;
    }

    paint_bytes(out, theme.html_delim, &trimmed[..name_start], theme.reset);
    paint_bytes(out, theme.html_name, name, theme.reset);
    colorize_apache_arguments(&trimmed[name_end..close], theme, out);
    paint_bytes(out, theme.html_delim, b">", theme.reset);
    out.extend_from_slice(ending);
    Some(())
}

fn colorize_apache_arguments(arguments: &[u8], theme: &Theme, out: &mut Vec<u8>) {
    let mut index = 0;
    while index < arguments.len() {
        if arguments[index].is_ascii_whitespace() {
            let start = index;
            while index < arguments.len() && arguments[index].is_ascii_whitespace() {
                index += 1;
            }
            out.extend_from_slice(&arguments[start..index]);
            continue;
        }

        if matches!(arguments[index], b'\'' | b'"') {
            let quote = arguments[index];
            let start = index;
            index += 1;
            while index < arguments.len() {
                if arguments[index] == b'\\' && index + 1 < arguments.len() {
                    index += 2;
                } else if arguments[index] == quote {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
            paint_bytes(out, theme.string, &arguments[start..index], theme.reset);
            continue;
        }

        if arguments[index..].starts_with(b"%{") {
            let start = index;
            index += 2;
            while index < arguments.len() && arguments[index] != b'}' {
                index += 1;
            }
            if index < arguments.len() {
                index += 1;
            }
            paint_bytes(out, theme.key, &arguments[start..index], theme.reset);
            continue;
        }

        if arguments[index] == b'[' {
            if let Some(relative_end) = arguments[index..].iter().position(|byte| *byte == b']') {
                let end = index + relative_end + 1;
                colorize_apache_flags(&arguments[index..end], theme, out);
                index = end;
                continue;
            }
        }

        let start = index;
        while index < arguments.len() && !arguments[index].is_ascii_whitespace() {
            index += 1;
        }
        let token = &arguments[start..index];
        if let Some(color) = apache_value_color(token, theme) {
            paint_bytes(out, color, token, theme.reset);
        } else {
            out.extend_from_slice(token);
        }
    }
}

fn colorize_apache_flags(flags: &[u8], theme: &Theme, out: &mut Vec<u8>) {
    paint_bytes(out, theme.html_delim, b"[", theme.reset);
    let inner = &flags[1..flags.len() - 1];
    for (index, part) in inner.split(|byte| *byte == b',').enumerate() {
        if index > 0 {
            paint_bytes(out, theme.html_delim, b",", theme.reset);
        }
        if let Some(equals) = part.iter().position(|byte| *byte == b'=') {
            paint_bytes(out, theme.html_attr, &part[..equals], theme.reset);
            paint_bytes(out, theme.html_delim, b"=", theme.reset);
            let value = &part[equals + 1..];
            let color = if value.iter().all(u8::is_ascii_digit) {
                theme.number
            } else {
                theme.string
            };
            paint_bytes(out, color, value, theme.reset);
        } else {
            paint_bytes(out, theme.html_attr, part, theme.reset);
        }
    }
    paint_bytes(out, theme.html_delim, b"]", theme.reset);
}

fn apache_value_color(token: &[u8], theme: &Theme) -> Option<&'static str> {
    if token.is_empty() {
        return None;
    }
    if token.iter().all(u8::is_ascii_digit) {
        return Some(theme.number);
    }
    if matches!(
        token,
        b"On" | b"Off" | b"on" | b"off" | b"all" | b"denied" | b"granted" | b"Allow" | b"Deny"
    ) || (token.len() > 1 && token.iter().all(|byte| byte.is_ascii_uppercase()))
    {
        return Some(theme.keyword);
    }
    if token.starts_with(b"http://")
        || token.starts_with(b"https://")
        || token.starts_with(b"mod_")
        || (token.contains(&b'/')
            && token.iter().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(*byte, b'/' | b'+' | b'-' | b'.')
            }))
    {
        return Some(theme.string);
    }
    None
}

/// Color dotenv assignments without changing or summarizing their values.
///
/// Real `.env` files commonly contain credentials, so this formatter is only a
/// visual pass: keys, delimiters, typed values, and comments receive ANSI spans,
/// while the original bytes remain recoverable exactly after stripping ANSI.
pub fn colorize_dotenv_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with(b"#") {
        return Some(paint_whole(content, ending, theme.comment, theme.reset));
    }

    let offset = content.len() - trimmed.len();
    let (export, assignment) = if let Some(rest) = trimmed.strip_prefix(b"export ") {
        (true, rest)
    } else {
        (false, trimmed)
    };
    let equals = assignment.iter().position(|byte| *byte == b'=')?;
    let key = trim_ascii_end(&assignment[..equals]);
    if !valid_dotenv_key(key) {
        return None;
    }
    let after_equals = &assignment[equals + 1..];
    let comment = dotenv_comment_start(after_equals);
    let value = &after_equals[..comment.unwrap_or(after_equals.len())];
    let inline_comment = comment.map(|index| &after_equals[index..]);

    let mut out = Vec::with_capacity(line.len() + 80);
    out.extend_from_slice(&content[..offset]);
    if export {
        paint_bytes(&mut out, theme.keyword, b"export", theme.reset);
        out.push(b' ');
    }
    paint_bytes(&mut out, theme.key, key, theme.reset);
    out.extend_from_slice(&assignment[key.len()..equals]);
    paint_bytes(&mut out, theme.html_delim, b"=", theme.reset);
    if !value.is_empty() {
        paint_bytes(
            &mut out,
            color_for_config_value(value, theme),
            value,
            theme.reset,
        );
    }
    if let Some(comment) = inline_comment {
        paint_bytes(&mut out, theme.comment, comment, theme.reset);
    }
    out.extend_from_slice(ending);
    Some(out)
}

fn valid_dotenv_key(key: &[u8]) -> bool {
    key.first()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
        && key
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'.' | b'-'))
}

fn dotenv_comment_start(value: &[u8]) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    for (index, byte) in value.iter().copied().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if byte == b'\\' && quote == Some(b'"') {
            escaped = true;
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            if quote == Some(byte) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(byte);
            }
            continue;
        }
        if byte == b'#' && quote.is_none() && (index == 0 || value[index - 1].is_ascii_whitespace())
        {
            return Some(index);
        }
    }
    None
}

/// Color `.gitleaksignore` comments and fingerprints.
///
/// A fingerprint has the shape `<commit>:<path>:<rule-id>:<line>`. Paths may
/// themselves contain `:`, so parse the fixed fields from both ends rather than
/// blindly splitting into four pieces.
pub fn colorize_gitleaks_ignore_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with(b"#") {
        return Some(paint_whole(content, ending, theme.comment, theme.reset));
    }

    let first_colon = trimmed.iter().position(|&b| b == b':')?;
    let last_colon = trimmed.iter().rposition(|&b| b == b':')?;
    if first_colon == last_colon {
        return None;
    }
    let rule_colon = trimmed[..last_colon].iter().rposition(|&b| b == b':')?;
    if rule_colon == first_colon {
        return None;
    }

    let commit = &trimmed[..first_colon];
    let path = &trimmed[first_colon + 1..rule_colon];
    let rule = &trimmed[rule_colon + 1..last_colon];
    let line_number = &trimmed[last_colon + 1..];
    if !(7..=64).contains(&commit.len())
        || !commit.iter().all(u8::is_ascii_hexdigit)
        || path.is_empty()
        || rule.is_empty()
        || line_number.is_empty()
        || !line_number.iter().all(u8::is_ascii_digit)
    {
        return None;
    }

    let offset = content.len() - trimmed.len();
    let mut out = Vec::with_capacity(line.len() + 64);
    out.extend_from_slice(&content[..offset]);
    paint_bytes(&mut out, theme.number, commit, theme.reset);
    paint_bytes(&mut out, theme.html_delim, b":", theme.reset);
    paint_bytes(&mut out, theme.key, path, theme.reset);
    paint_bytes(&mut out, theme.html_delim, b":", theme.reset);
    paint_bytes(&mut out, theme.keyword, rule, theme.reset);
    paint_bytes(&mut out, theme.html_delim, b":", theme.reset);
    paint_bytes(&mut out, theme.number, line_number, theme.reset);
    out.extend_from_slice(ending);
    Some(out)
}

/// Color a `.gitignore` pattern without changing any of its bytes.
///
/// Gitignore syntax is deliberately kept distinct from generic configuration:
/// comments are muted, negation is highlighted, path separators stay subtle,
/// and glob operators stand out from the literal path around them.
pub fn colorize_gitignore_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    if content.is_empty() {
        return None;
    }
    if content.starts_with(b"#") {
        return Some(paint_whole(content, ending, theme.comment, theme.reset));
    }

    let mut out = Vec::with_capacity(line.len() + 64);
    let mut index = 0;
    if content.starts_with(b"!") {
        paint_bytes(&mut out, theme.keyword, b"!", theme.reset);
        index = 1;
    }

    while index < content.len() {
        let start = index;
        let color = match content[index] {
            b'\\' => {
                index = (index + 2).min(content.len());
                theme.string
            }
            b'*' => {
                while content.get(index) == Some(&b'*') {
                    index += 1;
                }
                theme.warn
            }
            b'?' => {
                index += 1;
                theme.warn
            }
            b'[' => {
                index += 1;
                while index < content.len() {
                    let byte = content[index];
                    index += 1;
                    if byte == b'\\' && index < content.len() {
                        index += 1;
                    } else if byte == b']' {
                        break;
                    }
                }
                theme.warn
            }
            b'/' => {
                index += 1;
                theme.html_delim
            }
            _ => {
                index += 1;
                while index < content.len()
                    && !matches!(content[index], b'\\' | b'*' | b'?' | b'[' | b'/')
                {
                    index += 1;
                }
                theme.string
            }
        };
        paint_bytes(&mut out, color, &content[start..index], theme.reset);
    }
    out.extend_from_slice(ending);
    Some(out)
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

fn markdown_code_language(lang: &[u8]) -> Option<MarkdownEmbeddedLanguage> {
    let lower = lang.to_ascii_lowercase();
    let code = match lower.as_slice() {
        b"html" | b"htm" => return Some(MarkdownEmbeddedLanguage::Html),
        b"bash" | b"sh" | b"shell" | b"zsh" | b"fish" | b"console" => CodeLanguage::Shell,
        b"rust" | b"rs" => CodeLanguage::Rust,
        b"python" | b"py" => CodeLanguage::Python,
        b"javascript" | b"js" | b"jsx" => CodeLanguage::JavaScript,
        b"typescript" | b"ts" | b"tsx" => CodeLanguage::TypeScript,
        b"go" => CodeLanguage::Go,
        b"java" => CodeLanguage::Java,
        b"kotlin" | b"kt" => CodeLanguage::Kotlin,
        b"swift" => CodeLanguage::Swift,
        b"ruby" | b"rb" => CodeLanguage::Ruby,
        b"php" => CodeLanguage::Php,
        b"css" | b"scss" | b"sass" => CodeLanguage::Css,
        b"c" | b"h" | b"cpp" | b"cc" | b"cxx" | b"hpp" => CodeLanguage::CLike,
        _ => return None,
    };
    Some(MarkdownEmbeddedLanguage::Code(code))
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
