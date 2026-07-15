//! Filesystem and process command views.

use super::super::theme::Theme;
use super::{colorize_size_path_line, colorize_words, paint_whole, split_line, word_spans};

/// Color one `find` output line as a path without filesystem lookups.
pub fn colorize_find_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let (content, ending) = split_line(line);
    if content.is_empty() || theme.reset.is_empty() {
        return None;
    }
    let text = std::str::from_utf8(content).ok()?;
    let mut out = Vec::with_capacity(line.len() + 48);
    let Some(last_slash) = text.rfind('/') else {
        out.extend_from_slice(theme.key.as_bytes());
        out.extend_from_slice(content);
        out.extend_from_slice(theme.reset.as_bytes());
        out.extend_from_slice(ending);
        return Some(out);
    };
    let (parent, leaf_with_slash) = text.split_at(last_slash);
    let leaf = &leaf_with_slash[1..];
    if !parent.is_empty() {
        out.extend_from_slice(theme.debug.as_bytes());
        out.extend_from_slice(parent.as_bytes());
        out.extend_from_slice(theme.reset.as_bytes());
    }
    out.extend_from_slice(theme.html_delim.as_bytes());
    out.push(b'/');
    out.extend_from_slice(theme.reset.as_bytes());
    out.extend_from_slice(theme.key.as_bytes());
    out.extend_from_slice(leaf.as_bytes());
    out.extend_from_slice(theme.reset.as_bytes());
    out.extend_from_slice(ending);
    Some(out)
}

/// Color one `ls` output line. Handles both long listings and simple
/// multi-column filename output without changing any visible text.
pub fn colorize_ls_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    if content.is_empty() {
        return None;
    }
    let text = std::str::from_utf8(content).ok()?;
    let trimmed = text.trim_start();
    if trimmed.starts_with("total ") {
        return Some(paint_whole(content, ending, theme.debug, theme.reset));
    }

    let words = word_spans(content);
    if words.is_empty() {
        return None;
    }
    let first = &content[words[0].0..words[0].1];
    let long_listing = looks_like_mode(first) && words.len() >= 8;
    let name_start = if long_listing { 8 } else { 0 };
    let long_name_is_hidden =
        long_listing && is_hidden_ls_name(&content[words[name_start].0..words[name_start].1]);
    let name_color = if long_name_is_hidden {
        theme.hidden
    } else {
        match first.first().copied() {
            Some(b'd') => theme.folder,
            Some(b'l') => theme.keyword,
            _ => theme.string,
        }
    };

    Some(colorize_words(content, ending, theme, |idx, word| {
        if long_listing {
            match idx {
                0 => Some(theme.debug),
                1 | 4 => Some(theme.number),
                2 | 3 | 5..=7 => Some(theme.comment),
                i if i >= name_start => Some(name_color),
                _ => None,
            }
        } else if word == b"->" {
            Some(theme.comment)
        } else if is_hidden_ls_name(word) {
            Some(theme.hidden)
        } else {
            Some(theme.key)
        }
    }))
}

fn is_hidden_ls_name(name: &[u8]) -> bool {
    name.starts_with(b".") && name != b"." && name != b".."
}

/// Color `du` output: size first, path after.
pub fn colorize_du_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    colorize_size_path_line(line, theme)
}

/// Color `df` output: header dimmed, numeric columns highlighted, high capacity
/// values warned.
pub fn colorize_df_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let words = word_spans(content);
    if words.is_empty() {
        return None;
    }
    let first = &content[words[0].0..words[0].1];
    if first == b"Filesystem" {
        return Some(paint_whole(content, ending, theme.debug, theme.reset));
    }
    Some(colorize_words(
        content,
        ending,
        theme,
        |idx, word| match idx {
            0 => Some(theme.key),
            1..=3 => Some(theme.number),
            4 if percent_value(word).is_some_and(|p| p >= 90) => Some(theme.warn),
            4 => Some(theme.number),
            _ => Some(theme.string),
        },
    ))
}

/// Color `ps` output: header dimmed, ids/resources highlighted, expensive rows
/// warned.
pub fn colorize_ps_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let words = word_spans(content);
    if words.is_empty() {
        return None;
    }
    let first = &content[words[0].0..words[0].1];
    if matches!(first, b"USER" | b"UID") {
        return Some(paint_whole(content, ending, theme.debug, theme.reset));
    }
    Some(colorize_words(
        content,
        ending,
        theme,
        |idx, word| match idx {
            0 => Some(theme.key),
            1 => Some(theme.number),
            2 | 3 if float_value(word).is_some_and(|v| v >= 50.0) => Some(theme.error),
            2 | 3 if float_value(word).is_some_and(|v| v >= 10.0) => Some(theme.warn),
            2 | 3 => Some(theme.number),
            4 | 5 => Some(theme.debug),
            6..=9 => Some(theme.comment),
            _ => Some(theme.muted),
        },
    ))
}

fn looks_like_mode(word: &[u8]) -> bool {
    word.len() >= 9
        && matches!(
            word.first(),
            Some(b'-' | b'd' | b'l' | b'b' | b'c' | b'p' | b's')
        )
        && word.iter().skip(1).take(9).all(|b| {
            matches!(
                b,
                b'r' | b'w' | b'x' | b'-' | b's' | b'S' | b't' | b'T' | b'@' | b'+'
            )
        })
}

fn percent_value(word: &[u8]) -> Option<u8> {
    let number = word.strip_suffix(b"%").unwrap_or(word);
    std::str::from_utf8(number).ok()?.parse().ok()
}

fn float_value(word: &[u8]) -> Option<f32> {
    std::str::from_utf8(word).ok()?.parse().ok()
}
