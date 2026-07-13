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
use serde_json::Value;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeLanguage {
    CLike,
    Css,
    Go,
    Java,
    JavaScript,
    Kotlin,
    Php,
    Python,
    Ruby,
    Rust,
    Shell,
    Swift,
    TypeScript,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitView {
    Branch,
    DiffStat,
    Log,
    ShortStatus,
    Status,
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

/// Color common CLI diagnostic lines before command-specific formatters get a
/// chance to make them look like normal output. PTYs merge stdout/stderr into one
/// stream, so we infer conservatively from familiar tool wording.
pub fn colorize_cli_diagnostic_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }

    if is_usage_line(trimmed) {
        return Some(paint_whole(content, ending, theme.warn, theme.reset));
    }
    if is_cli_error_line(trimmed) {
        return Some(paint_whole(content, ending, theme.error, theme.reset));
    }
    None
}

/// Color common Git command output: status, short status, branches, and
/// `log --oneline` style lines. This keeps Git's layout untouched and only wraps
/// high-signal tokens such as branch names, hashes, status codes, and paths.
pub fn colorize_git_line(line: &[u8], theme: &Theme, view: GitView) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    match view {
        GitView::Branch => colorize_git_branch_line(line, theme),
        GitView::DiffStat => colorize_git_diff_stat_line(line, theme),
        GitView::Log => colorize_git_log_line(line, theme),
        GitView::ShortStatus => colorize_git_short_status_line(line, theme),
        GitView::Status => colorize_git_status_line(line, theme),
    }
}

/// Color a source-code line from reader commands (`cat`, `head`, `tail`, `sed`).
/// This is intentionally a lightweight visual lexer: it keeps layout intact,
/// avoids parser dependencies on the hot path, and passes through lines it cannot
/// improve.
pub fn colorize_code_line(line: &[u8], theme: &Theme, lang: CodeLanguage) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    if content.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(content.len() + ending.len() + 64);
    let mut i = 0;
    let mut colored_any = false;
    while i < content.len() {
        let b = content[i];
        if let Some(end) = code_line_comment_end(content, i, lang) {
            paint_bytes(&mut out, theme.comment, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if let Some(end) = code_block_comment_end(content, i, lang) {
            paint_bytes(&mut out, theme.comment, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if let Some(end) = code_string_end(content, i, lang) {
            paint_bytes(&mut out, theme.string, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if starts_code_number(content, i) {
            let end = code_number_end(content, i);
            paint_bytes(&mut out, theme.number, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if is_code_ident_start(b) {
            let end = code_ident_end(content, i);
            let word = &content[i..end];
            if is_code_keyword(word, lang) {
                paint_bytes(&mut out, theme.keyword, word, theme.reset);
                colored_any = true;
            } else if looks_like_code_constant(word) {
                paint_bytes(&mut out, theme.number, word, theme.reset);
                colored_any = true;
            } else if looks_like_function_call(content, end) {
                paint_bytes(&mut out, theme.key, word, theme.reset);
                colored_any = true;
            } else {
                out.extend_from_slice(word);
            }
            i = end;
        } else if is_code_punctuation(b, lang) {
            paint_bytes(&mut out, theme.html_delim, &content[i..i + 1], theme.reset);
            colored_any = true;
            i += 1;
        } else {
            out.push(b);
            i += 1;
        }
    }
    if !colored_any {
        return None;
    }
    out.extend_from_slice(ending);
    Some(out)
}

/// Color one complete JSON-lines row without reflowing it. Unlike the buffered
/// JSON formatter this preserves single-line shape, which keeps `tail -f` /
/// event streams responsive and readable.
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

/// Color one `find` output line as a path. This is command-aware and intentionally
/// conservative: it does not try to stat paths or infer file types, it just makes
/// parent segments, separators, and leaf names easier to scan.
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

/// Color `dig` / `nslookup` style DNS output.
pub fn colorize_dns_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with(b";;") || trimmed.starts_with(b"; <<>>") {
        let color = if contains_ascii(trimmed, b"SECTION:") {
            theme.keyword
        } else {
            theme.debug
        };
        return Some(paint_whole(content, ending, color, theme.reset));
    }
    if trimmed.starts_with(b";") {
        return Some(paint_whole(content, ending, theme.comment, theme.reset));
    }
    let words = word_spans(content);
    if words.len() < 4 {
        return None;
    }
    Some(colorize_words(
        content,
        ending,
        theme,
        |idx, word| match idx {
            0 => Some(theme.key),
            1 if word.iter().all(u8::is_ascii_digit) => Some(theme.number),
            2 => Some(theme.comment),
            3 => Some(theme.keyword),
            _ => Some(theme.key),
        },
    ))
}

/// Color macOS `networksetup` inventory output. These commands are common when
/// debugging Wi-Fi, but their output is a label/value report rather than logs or
/// a table, so a small dedicated formatter keeps it readable without reflowing.
pub fn colorize_networksetup_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.iter().all(|&b| b == b'=') {
        return Some(paint_whole(content, ending, theme.debug, theme.reset));
    }

    if let Some(label_len) = networksetup_heading_label_len(trimmed) {
        return Some(paint_networksetup_label_value(
            content, ending, trimmed, label_len, theme,
        ));
    }

    let offset = content.len() - trimmed.len();
    if offset > 0 {
        return Some(paint_networksetup_indented_value(
            content, ending, offset, theme,
        ));
    }

    None
}

fn networksetup_heading_label_len(trimmed: &[u8]) -> Option<usize> {
    const LABELS: &[&[u8]] = &[
        b"Preferred networks on ",
        b"Hardware Port:",
        b"Device:",
        b"Ethernet Address:",
        b"VLAN Configurations",
    ];
    LABELS
        .iter()
        .find_map(|label| trimmed.starts_with(label).then_some(label.len()))
}

fn paint_networksetup_label_value(
    content: &[u8],
    ending: &[u8],
    trimmed: &[u8],
    label_len: usize,
    theme: &Theme,
) -> Vec<u8> {
    let offset = content.len() - trimmed.len();
    let label_end = offset + label_len.min(trimmed.len());
    let mut out = Vec::with_capacity(content.len() + ending.len() + 48);
    out.extend_from_slice(&content[..offset]);
    paint_bytes(
        &mut out,
        theme.key,
        &content[offset..label_end],
        theme.reset,
    );
    if label_end < content.len() {
        let value = &content[label_end..];
        let value_color = if trim_ascii_start(value).starts_with(b"Wi-Fi")
            || looks_like_mac_address(trim_ascii(value))
        {
            theme.path
        } else {
            theme.string
        };
        paint_bytes(&mut out, value_color, value, theme.reset);
    }
    out.extend_from_slice(ending);
    out
}

fn paint_networksetup_indented_value(
    content: &[u8],
    ending: &[u8],
    offset: usize,
    theme: &Theme,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len() + ending.len() + 24);
    out.extend_from_slice(&content[..offset]);
    let value = &content[offset..];
    let color = if value.starts_with(b".") && value != b"." && value != b".." {
        theme.hidden
    } else {
        theme.string
    };
    paint_bytes(&mut out, color, value, theme.reset);
    out.extend_from_slice(ending);
    out
}

/// Clean and lightly format `man` / help output. This understands the classic
/// overstrike form (`N\bN`, `_\bx`) emitted by man pages when no pager handles
/// bold/underline.
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

/// Color CSV/TSV rows without changing any cell or delimiter bytes. CSV splitting
/// understands quoted cells well enough to avoid treating commas inside quotes as
/// delimiters; malformed rows fall back to ordinary pass-through.
pub fn colorize_delimited_line(
    line: &[u8],
    theme: &Theme,
    delimiter: u8,
    is_header: bool,
) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    if content.is_empty() {
        return None;
    }
    let spans = delimited_spans(content, delimiter)?;
    if spans.len() < 2 {
        return None;
    }

    let mut out = Vec::with_capacity(content.len() + ending.len() + spans.len() * 12);
    let mut cursor = 0;
    for (idx, (start, end)) in spans.iter().copied().enumerate() {
        if cursor < start {
            out.extend_from_slice(theme.html_delim.as_bytes());
            out.extend_from_slice(&content[cursor..start]);
            out.extend_from_slice(theme.reset.as_bytes());
        }
        let cell = &content[start..end];
        let color = if is_header {
            theme.key
        } else {
            color_for_delimited_cell(cell, theme)
        };
        out.extend_from_slice(color.as_bytes());
        out.extend_from_slice(cell);
        out.extend_from_slice(theme.reset.as_bytes());
        cursor = end;
        if idx + 1 == spans.len() && cursor < content.len() {
            out.extend_from_slice(theme.html_delim.as_bytes());
            out.extend_from_slice(&content[cursor..]);
            out.extend_from_slice(theme.reset.as_bytes());
            cursor = content.len();
        }
    }
    out.extend_from_slice(&content[cursor..]);
    out.extend_from_slice(ending);
    Some(out)
}

/// Color common database CLI result tables (`psql`, `sqlite3`, `mysql`/MariaDB)
/// without reflowing columns. Borders and row-count/status lines are dimmed;
/// header cells are keyed; data cells reuse the CSV/TSV value palette.
pub fn colorize_sql_result_line(line: &[u8], theme: &Theme, header_hint: bool) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    if content.is_empty() {
        return None;
    }
    let trimmed = trim_ascii(content);
    if trimmed.is_empty() {
        return None;
    }
    if is_sql_table_rule(trimmed) || is_sql_result_meta(trimmed) {
        return Some(paint_whole(content, ending, theme.debug, theme.reset));
    }
    if content.contains(&b'|') {
        return colorize_pipe_table_line(content, ending, theme, header_hint);
    }
    if content.contains(&b'\t') {
        return colorize_delimited_line(
            line,
            theme,
            b'\t',
            header_hint && looks_sql_header_row(content, b'\t'),
        );
    }
    let spans = whitespace_table_spans(content)?;
    if spans.len() < 2 {
        return None;
    }
    let is_header = header_hint && looks_header_spans(content, &spans);
    Some(colorize_spanned_cells(
        content, ending, &spans, theme, is_header,
    ))
}

/// Color SQL query text from `.sql` reader commands. This is a small visual lexer:
/// it highlights comments, quoted strings, numbers, punctuation, and common SQL
/// keywords without changing query layout or trying to pretty-print SQL.
pub fn colorize_sql_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    if content.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(content.len() + ending.len() + 64);
    let mut i = 0;
    let mut colored_any = false;
    while i < content.len() {
        let b = content[i];
        if i + 1 < content.len() && content[i] == b'-' && content[i + 1] == b'-' {
            paint_sql(&mut out, theme.comment, &content[i..], theme.reset);
            colored_any = true;
            i = content.len();
        } else if i + 1 < content.len() && content[i] == b'/' && content[i + 1] == b'*' {
            let end = find_sql_block_comment_end(&content[i + 2..])
                .map(|rel| i + 4 + rel)
                .unwrap_or(content.len());
            paint_sql(&mut out, theme.comment, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if b == b'\'' || b == b'"' {
            let end = sql_quoted_end(content, i, b);
            paint_sql(&mut out, theme.string, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if b.is_ascii_digit()
            || (matches!(b, b'-' | b'+') && content.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            let end = sql_number_end(content, i);
            paint_sql(&mut out, theme.number, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if is_sql_ident_start(b) {
            let end = sql_ident_end(content, i);
            let word = &content[i..end];
            if is_sql_keyword(word) {
                paint_sql(&mut out, theme.keyword, word, theme.reset);
                colored_any = true;
            } else {
                out.extend_from_slice(word);
            }
            i = end;
        } else if is_sql_punctuation(b) {
            paint_sql(&mut out, theme.html_delim, &content[i..i + 1], theme.reset);
            colored_any = true;
            i += 1;
        } else {
            out.push(b);
            i += 1;
        }
    }
    if !colored_any {
        return None;
    }
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

fn colorize_size_path_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
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

fn colorize_words<F>(content: &[u8], ending: &[u8], theme: &Theme, mut color: F) -> Vec<u8>
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

fn word_spans(bytes: &[u8]) -> Vec<(usize, usize)> {
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

fn paint_whole(content: &[u8], ending: &[u8], color: &str, reset: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len() + ending.len() + color.len() + reset.len());
    out.extend_from_slice(color.as_bytes());
    out.extend_from_slice(content);
    out.extend_from_slice(reset.as_bytes());
    out.extend_from_slice(ending);
    out
}

fn paint_span(
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

fn color_for_delimited_cell(cell: &[u8], theme: &Theme) -> &'static str {
    let trimmed = trim_ascii(cell);
    if trimmed.is_empty() {
        theme.comment
    } else if trimmed.first().is_some_and(|&b| b == b'"' || b == b'\'') {
        theme.string
    } else if looks_numeric(trimmed) {
        theme.number
    } else if matches!(
        lower_ascii(trimmed).as_slice(),
        b"true" | b"false" | b"null" | b"nil" | b"na" | b"n/a"
    ) {
        theme.keyword
    } else {
        theme.string
    }
}

fn delimited_spans(content: &[u8], delimiter: u8) -> Option<Vec<(usize, usize)>> {
    if delimiter == b'\t' {
        return split_unquoted(content, delimiter, false);
    }
    split_unquoted(content, delimiter, true)
}

fn split_unquoted(content: &[u8], delimiter: u8, csv_quotes: bool) -> Option<Vec<(usize, usize)>> {
    let mut spans = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let mut in_quotes = false;
    while i < content.len() {
        let b = content[i];
        if csv_quotes && b == b'"' {
            if in_quotes && content.get(i + 1) == Some(&b'"') {
                i += 2;
                continue;
            }
            in_quotes = !in_quotes;
            i += 1;
            continue;
        }
        if !in_quotes && b == delimiter {
            spans.push((start, i));
            start = i + 1;
        }
        i += 1;
    }
    if in_quotes {
        return None;
    }
    spans.push((start, content.len()));
    Some(spans)
}

fn colorize_pipe_table_line(
    content: &[u8],
    ending: &[u8],
    theme: &Theme,
    header_hint: bool,
) -> Option<Vec<u8>> {
    let spans = split_unquoted(content, b'|', false)?;
    if spans.len() < 2 {
        return None;
    }
    let is_header = header_hint && looks_header_spans(content, &spans);
    Some(colorize_spanned_cells(
        content, ending, &spans, theme, is_header,
    ))
}

fn colorize_spanned_cells(
    content: &[u8],
    ending: &[u8],
    spans: &[(usize, usize)],
    theme: &Theme,
    is_header: bool,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len() + ending.len() + spans.len() * 12);
    let mut cursor = 0;
    for &(start, end) in spans {
        if cursor < start {
            out.extend_from_slice(theme.html_delim.as_bytes());
            out.extend_from_slice(&content[cursor..start]);
            out.extend_from_slice(theme.reset.as_bytes());
        }
        let cell = &content[start..end];
        if cell.is_empty() {
            out.extend_from_slice(cell);
        } else {
            let color = if is_header {
                theme.key
            } else {
                color_for_delimited_cell(cell, theme)
            };
            out.extend_from_slice(color.as_bytes());
            out.extend_from_slice(cell);
            out.extend_from_slice(theme.reset.as_bytes());
        }
        cursor = end;
    }
    out.extend_from_slice(&content[cursor..]);
    out.extend_from_slice(ending);
    out
}

fn whitespace_table_spans(content: &[u8]) -> Option<Vec<(usize, usize)>> {
    let mut spans = Vec::new();
    let mut start = None;
    let mut i = 0;
    while i < content.len() {
        if content[i].is_ascii_whitespace() {
            let gap_start = i;
            while i < content.len() && content[i].is_ascii_whitespace() {
                i += 1;
            }
            if i - gap_start >= 2 {
                if let Some(s) = start.take() {
                    spans.push((s, gap_start));
                }
            } else if start.is_none() && i < content.len() {
                start = Some(gap_start);
            }
        } else {
            if start.is_none() {
                start = Some(i);
            }
            i += 1;
        }
    }
    if let Some(s) = start {
        spans.push((s, content.len()));
    }
    (spans.len() >= 2).then_some(spans)
}

fn looks_sql_header_row(content: &[u8], delimiter: u8) -> bool {
    delimited_spans(content, delimiter).is_some_and(|spans| looks_header_spans(content, &spans))
}

fn looks_header_spans(content: &[u8], spans: &[(usize, usize)]) -> bool {
    let mut meaningful = 0;
    for &(start, end) in spans {
        let cell = trim_ascii(&content[start..end]);
        if cell.is_empty() {
            continue;
        }
        meaningful += 1;
        if !looks_like_header_cell(cell) {
            return false;
        }
    }
    meaningful >= 2
}

fn looks_like_header_cell(cell: &[u8]) -> bool {
    !cell.is_empty()
        && !looks_numeric(cell)
        && !matches!(
            lower_ascii(cell).as_slice(),
            b"true" | b"false" | b"t" | b"f" | b"null" | b"nil"
        )
        && cell.iter().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(b, b'_' | b'-' | b'.' | b' ' | b'/' | b'(' | b')' | b'%')
        })
        && cell.iter().any(u8::is_ascii_alphabetic)
}

fn is_sql_table_rule(trimmed: &[u8]) -> bool {
    let allowed = trimmed
        .iter()
        .all(|b| matches!(b, b'+' | b'-' | b'=' | b' '));
    allowed
        && trimmed
            .iter()
            .filter(|&&b| matches!(b, b'-' | b'='))
            .count()
            >= 3
        && (trimmed.contains(&b'+') || trimmed.len() >= 3)
}

fn is_sql_result_meta(trimmed: &[u8]) -> bool {
    if trimmed.starts_with(b"(")
        && trimmed.ends_with(b")")
        && (trimmed.ends_with(b" row)") || trimmed.ends_with(b" rows)"))
    {
        return true;
    }
    if trimmed.starts_with(b"Time: ") || trimmed.starts_with(b"Query OK") {
        return true;
    }
    matches!(
        upper_ascii(trimmed).as_slice(),
        b"BEGIN"
            | b"COMMIT"
            | b"ROLLBACK"
            | b"CREATE TABLE"
            | b"CREATE INDEX"
            | b"CREATE VIEW"
            | b"DROP TABLE"
            | b"DROP INDEX"
            | b"ALTER TABLE"
    ) || starts_with_sql_command_tag(trimmed)
}

fn starts_with_sql_command_tag(trimmed: &[u8]) -> bool {
    const TAGS: &[&[u8]] = &[
        b"INSERT ", b"UPDATE ", b"DELETE ", b"SELECT ", b"COPY ", b"MOVE ", b"FETCH ",
    ];
    let upper = upper_ascii(trimmed);
    TAGS.iter().any(|tag| upper.starts_with(tag))
}

fn looks_numeric(bytes: &[u8]) -> bool {
    let mut seen_digit = false;
    for (idx, &b) in bytes.iter().enumerate() {
        match b {
            b'0'..=b'9' => seen_digit = true,
            b'.' | b'_' | b',' | b'%' => {}
            b'-' | b'+' if idx == 0 => {}
            _ => return false,
        }
    }
    seen_digit
}

fn lower_ascii(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(u8::to_ascii_lowercase).collect()
}

fn paint_sql(out: &mut Vec<u8>, color: &str, bytes: &[u8], reset: &str) {
    out.extend_from_slice(color.as_bytes());
    out.extend_from_slice(bytes);
    out.extend_from_slice(reset.as_bytes());
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

fn paint_bytes(out: &mut Vec<u8>, color: &str, bytes: &[u8], reset: &str) {
    out.extend_from_slice(color.as_bytes());
    out.extend_from_slice(bytes);
    out.extend_from_slice(reset.as_bytes());
}

fn colorize_git_diff_stat_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if let Some(formatted) = colorize_git_log_line(line, theme) {
        return Some(formatted);
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii(content);
    if trimmed.is_empty() {
        return None;
    }
    if is_git_diff_stat_summary(trimmed) {
        return Some(colorize_git_stat_summary(content, ending, theme));
    }
    if content.contains(&b'|') {
        return colorize_git_pipe_stat_line(content, ending, theme);
    }
    if content.contains(&b'\t') {
        return colorize_git_tab_stat_line(content, ending, theme);
    }
    if let Some((code_len, color)) = git_name_status_code(trimmed, theme) {
        let offset = content.len() - trim_ascii_start(content).len();
        let after_code = trim_ascii_start(&trimmed[code_len..]);
        let gap_len = trimmed[code_len..].len() - after_code.len();
        let mut out = Vec::with_capacity(line.len() + 48);
        out.extend_from_slice(&content[..offset]);
        paint_bytes(&mut out, color, &trimmed[..code_len], theme.reset);
        paint_bytes(
            &mut out,
            theme.html_delim,
            &trimmed[code_len..code_len + gap_len],
            theme.reset,
        );
        paint_git_pathish(&mut out, after_code, theme);
        out.extend_from_slice(ending);
        return Some(out);
    }
    None
}

fn colorize_git_pipe_stat_line(content: &[u8], ending: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let pipe = content.iter().position(|&b| b == b'|')?;
    let path = &content[..pipe];
    let rest = &content[pipe + 1..];
    let mut out = Vec::with_capacity(content.len() + ending.len() + 64);
    paint_bytes(&mut out, theme.key, path, theme.reset);
    paint_bytes(&mut out, theme.html_delim, b"|", theme.reset);
    colorize_git_stat_tail(&mut out, rest, theme);
    out.extend_from_slice(ending);
    Some(out)
}

fn colorize_git_tab_stat_line(content: &[u8], ending: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let spans = split_unquoted(content, b'\t', false)?;
    if spans.len() < 2 {
        return None;
    }
    let first = &content[spans[0].0..spans[0].1];
    if spans.len() >= 3 && is_numstat_count(first) {
        let second = &content[spans[1].0..spans[1].1];
        let mut out = Vec::with_capacity(content.len() + ending.len() + 64);
        paint_bytes(&mut out, theme.info, first, theme.reset);
        paint_bytes(&mut out, theme.html_delim, b"\t", theme.reset);
        paint_bytes(&mut out, theme.error, second, theme.reset);
        paint_bytes(&mut out, theme.html_delim, b"\t", theme.reset);
        paint_git_pathish(&mut out, &content[spans[2].0..], theme);
        out.extend_from_slice(ending);
        return Some(out);
    }
    if let Some((_, color)) = git_name_status_code(first, theme) {
        let mut out = Vec::with_capacity(content.len() + ending.len() + 48);
        paint_bytes(&mut out, color, first, theme.reset);
        paint_bytes(&mut out, theme.html_delim, b"\t", theme.reset);
        paint_git_pathish(&mut out, &content[spans[1].0..], theme);
        out.extend_from_slice(ending);
        return Some(out);
    }
    None
}

fn colorize_git_stat_summary(content: &[u8], ending: &[u8], theme: &Theme) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len() + ending.len() + 64);
    let mut i = 0;
    while i < content.len() {
        let b = content[i];
        if b.is_ascii_digit() {
            let end = content[i..]
                .iter()
                .position(|c| !c.is_ascii_digit())
                .map_or(content.len(), |rel| i + rel);
            paint_bytes(&mut out, theme.number, &content[i..end], theme.reset);
            i = end;
        } else if content[i..].starts_with(b"insertions(+)")
            || content[i..].starts_with(b"insertion(+)")
        {
            let len = if content[i..].starts_with(b"insertions(+)") {
                b"insertions(+)".len()
            } else {
                b"insertion(+)".len()
            };
            paint_bytes(&mut out, theme.info, &content[i..i + len], theme.reset);
            i += len;
        } else if content[i..].starts_with(b"deletions(-)")
            || content[i..].starts_with(b"deletion(-)")
        {
            let len = if content[i..].starts_with(b"deletions(-)") {
                b"deletions(-)".len()
            } else {
                b"deletion(-)".len()
            };
            paint_bytes(&mut out, theme.error, &content[i..i + len], theme.reset);
            i += len;
        } else {
            out.push(b);
            i += 1;
        }
    }
    out.extend_from_slice(ending);
    out
}

fn colorize_git_stat_tail(out: &mut Vec<u8>, bytes: &[u8], theme: &Theme) {
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_digit() {
            let end = bytes[i..]
                .iter()
                .position(|c| !c.is_ascii_digit())
                .map_or(bytes.len(), |rel| i + rel);
            paint_bytes(out, theme.number, &bytes[i..end], theme.reset);
            i = end;
        } else if b == b'+' {
            let end = bytes[i..]
                .iter()
                .position(|&c| c != b'+')
                .map_or(bytes.len(), |rel| i + rel);
            paint_bytes(out, theme.info, &bytes[i..end], theme.reset);
            i = end;
        } else if b == b'-' {
            let end = bytes[i..]
                .iter()
                .position(|&c| c != b'-')
                .map_or(bytes.len(), |rel| i + rel);
            paint_bytes(out, theme.error, &bytes[i..end], theme.reset);
            i = end;
        } else if matches!(b, b',' | b' ' | b'\t') {
            paint_bytes(out, theme.html_delim, &bytes[i..i + 1], theme.reset);
            i += 1;
        } else {
            out.push(b);
            i += 1;
        }
    }
}

fn is_git_diff_stat_summary(trimmed: &[u8]) -> bool {
    contains_ascii(trimmed, b" changed")
        && (contains_ascii(trimmed, b"insertion") || contains_ascii(trimmed, b"deletion"))
}

fn is_numstat_count(bytes: &[u8]) -> bool {
    bytes == b"-" || bytes.iter().all(u8::is_ascii_digit)
}

fn git_name_status_code(trimmed: &[u8], theme: &Theme) -> Option<(usize, &'static str)> {
    let first = *trimmed.first()?;
    let len = if first == b'R' || first == b'C' {
        1 + trimmed[1..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    } else {
        1
    };
    let color = match first {
        b'A' => theme.info,
        b'D' => theme.error,
        b'M' | b'T' => theme.warn,
        b'R' | b'C' => theme.keyword,
        b'U' => theme.warn,
        _ => return None,
    };
    if trimmed.get(len).is_none_or(u8::is_ascii_whitespace) {
        Some((len, color))
    } else {
        None
    }
}

fn colorize_git_branch_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let (content, ending) = split_line(line);
    if content.is_empty() {
        return None;
    }
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }
    let offset = content.len() - trimmed.len();
    let mut out = Vec::with_capacity(line.len() + 32);
    out.extend_from_slice(&content[..offset]);
    if trimmed.starts_with(b"*") {
        paint_bytes(&mut out, theme.info, b"*", theme.reset);
        if trimmed.get(1).is_some_and(u8::is_ascii_whitespace) {
            out.push(trimmed[1]);
            paint_bytes(
                &mut out,
                theme.key,
                trim_ascii_start(&trimmed[2..]),
                theme.reset,
            );
        } else {
            paint_bytes(&mut out, theme.key, &trimmed[1..], theme.reset);
        }
    } else if trimmed.starts_with(b"remotes/") {
        paint_bytes(&mut out, theme.debug, b"remotes/", theme.reset);
        paint_bytes(
            &mut out,
            theme.key,
            &trimmed[b"remotes/".len()..],
            theme.reset,
        );
    } else {
        paint_bytes(&mut out, theme.key, trimmed, theme.reset);
    }
    out.extend_from_slice(ending);
    Some(out)
}

fn colorize_git_log_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }
    let offset = content.len() - trimmed.len();
    if let Some(rest) = trimmed.strip_prefix(b"commit ") {
        let hash_len = rest.iter().take_while(|b| is_hex_byte(**b)).count();
        if hash_len < 7 {
            return None;
        }
        let mut out = Vec::with_capacity(line.len() + 32);
        out.extend_from_slice(&content[..offset]);
        paint_bytes(&mut out, theme.keyword, b"commit", theme.reset);
        out.push(b' ');
        paint_bytes(&mut out, theme.number, &rest[..hash_len], theme.reset);
        out.extend_from_slice(&rest[hash_len..]);
        out.extend_from_slice(ending);
        return Some(out);
    }
    let hash_len = trimmed.iter().take_while(|b| is_hex_byte(**b)).count();
    if !(7..=40).contains(&hash_len) || !trimmed.get(hash_len).is_some_and(u8::is_ascii_whitespace)
    {
        return None;
    }
    let mut out = Vec::with_capacity(line.len() + 48);
    out.extend_from_slice(&content[..offset]);
    paint_bytes(&mut out, theme.number, &trimmed[..hash_len], theme.reset);
    let mut cursor = hash_len;
    out.extend_from_slice(&trimmed[cursor..cursor + 1]);
    cursor += 1;
    if trimmed.get(cursor) == Some(&b'(') {
        if let Some(close) = trimmed[cursor..].iter().position(|&b| b == b')') {
            let end = cursor + close + 1;
            paint_bytes(&mut out, theme.key, &trimmed[cursor..end], theme.reset);
            cursor = end;
            if trimmed.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                out.push(trimmed[cursor]);
                cursor += 1;
            }
        }
    }
    out.extend_from_slice(&trimmed[cursor..]);
    out.extend_from_slice(ending);
    Some(out)
}

fn colorize_git_short_status_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let (content, ending) = split_line(line);
    if content.len() < 2 {
        return None;
    }
    if let Some(rest) = content.strip_prefix(b"## ") {
        let mut out = Vec::with_capacity(line.len() + 32);
        paint_bytes(&mut out, theme.debug, b"## ", theme.reset);
        paint_git_branch_meta(&mut out, rest, theme);
        out.extend_from_slice(ending);
        return Some(out);
    }
    if !is_git_short_status_code(&content[..2]) {
        return None;
    }
    let mut path_start = 2;
    while content.get(path_start).is_some_and(u8::is_ascii_whitespace) {
        path_start += 1;
    }
    if path_start >= content.len() {
        return None;
    }
    let mut out = Vec::with_capacity(line.len() + 32);
    let color = git_short_status_color(&content[..2], theme);
    paint_bytes(&mut out, color, &content[..2], theme.reset);
    if path_start > 2 {
        paint_bytes(
            &mut out,
            theme.html_delim,
            &content[2..path_start],
            theme.reset,
        );
    }
    paint_git_pathish(&mut out, &content[path_start..], theme);
    out.extend_from_slice(ending);
    Some(out)
}

fn colorize_git_status_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }
    if looks_like_short_git_status(content) || content.starts_with(b"## ") {
        return colorize_git_short_status_line(line, theme);
    }
    if let Some(branch) = trimmed.strip_prefix(b"On branch ") {
        return Some(colorize_prefix_value_line(
            content,
            ending,
            content.len() - trimmed.len(),
            b"On branch ",
            branch,
            theme,
        ));
    }
    if let Some(branch) = trimmed.strip_prefix(b"HEAD detached at ") {
        return Some(colorize_prefix_value_line(
            content,
            ending,
            content.len() - trimmed.len(),
            b"HEAD detached at ",
            branch,
            theme,
        ));
    }
    if is_git_status_heading(trimmed) {
        return Some(paint_whole(content, ending, theme.keyword, theme.reset));
    }
    if is_git_status_advice(trimmed) {
        return Some(paint_whole(content, ending, theme.debug, theme.reset));
    }
    if is_git_clean_line(trimmed) {
        return Some(paint_whole(content, ending, theme.info, theme.reset));
    }
    if let Some((label_len, color)) = git_status_label(trimmed, theme) {
        let offset = content.len() - trimmed.len();
        let after_label = trim_ascii_start(&trimmed[label_len..]);
        let gap_len = trimmed[label_len..].len() - after_label.len();
        let mut out = Vec::with_capacity(line.len() + 48);
        out.extend_from_slice(&content[..offset]);
        paint_bytes(&mut out, color, &trimmed[..label_len], theme.reset);
        paint_bytes(
            &mut out,
            theme.html_delim,
            &trimmed[label_len..label_len + gap_len],
            theme.reset,
        );
        paint_git_pathish(&mut out, after_label, theme);
        out.extend_from_slice(ending);
        return Some(out);
    }
    None
}

fn colorize_prefix_value_line(
    content: &[u8],
    ending: &[u8],
    offset: usize,
    prefix: &[u8],
    value: &[u8],
    theme: &Theme,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len() + ending.len() + 24);
    out.extend_from_slice(&content[..offset]);
    paint_bytes(&mut out, theme.debug, prefix, theme.reset);
    paint_git_branch_meta(&mut out, value, theme);
    out.extend_from_slice(ending);
    out
}

fn paint_git_branch_meta(out: &mut Vec<u8>, bytes: &[u8], theme: &Theme) {
    if let Some(pos) = find_ascii(bytes, b"...") {
        paint_bytes(out, theme.key, &bytes[..pos], theme.reset);
        paint_bytes(out, theme.html_delim, b"...", theme.reset);
        let rest = &bytes[pos + 3..];
        if let Some(meta_start) = rest.iter().position(|&b| b == b'[') {
            let branch = trim_ascii_end(&rest[..meta_start]);
            paint_bytes(out, theme.key, branch, theme.reset);
            if meta_start > branch.len() {
                out.extend_from_slice(&rest[branch.len()..meta_start]);
            }
            paint_bytes(out, theme.warn, &rest[meta_start..], theme.reset);
        } else {
            paint_bytes(out, theme.key, rest, theme.reset);
        }
    } else if let Some(meta_start) = bytes.iter().position(|&b| b == b'[') {
        let branch = trim_ascii_end(&bytes[..meta_start]);
        paint_bytes(out, theme.key, branch, theme.reset);
        if meta_start > branch.len() {
            out.extend_from_slice(&bytes[branch.len()..meta_start]);
        }
        paint_bytes(out, theme.warn, &bytes[meta_start..], theme.reset);
    } else {
        paint_bytes(out, theme.key, bytes, theme.reset);
    }
}

fn paint_git_pathish(out: &mut Vec<u8>, bytes: &[u8], theme: &Theme) {
    if let Some(pos) = find_ascii(bytes, b" -> ") {
        paint_bytes(out, theme.key, &bytes[..pos], theme.reset);
        paint_bytes(out, theme.html_delim, &bytes[pos..pos + 4], theme.reset);
        paint_bytes(out, theme.key, &bytes[pos + 4..], theme.reset);
    } else {
        paint_bytes(out, theme.key, bytes, theme.reset);
    }
}

fn looks_like_short_git_status(content: &[u8]) -> bool {
    content.len() >= 3
        && is_git_short_status_code(&content[..2])
        && content.get(2).is_some_and(u8::is_ascii_whitespace)
}

fn is_git_short_status_code(code: &[u8]) -> bool {
    code.len() == 2
        && (code == b"??"
            || code == b"!!"
            || code
                .iter()
                .all(|b| matches!(b, b' ' | b'M' | b'A' | b'D' | b'R' | b'C' | b'U' | b'T')))
        && code.iter().any(|&b| b != b' ')
}

fn git_short_status_color(code: &[u8], theme: &Theme) -> &'static str {
    if code == b"??" {
        theme.string
    } else if code == b"!!" {
        theme.debug
    } else if code.contains(&b'D') {
        theme.error
    } else if code.contains(&b'U') {
        theme.warn
    } else if code.contains(&b'A') {
        theme.info
    } else if code.contains(&b'R') || code.contains(&b'C') {
        theme.keyword
    } else {
        theme.warn
    }
}

fn is_git_status_heading(trimmed: &[u8]) -> bool {
    matches!(
        trimmed,
        b"Changes to be committed:"
            | b"Changes not staged for commit:"
            | b"Untracked files:"
            | b"Unmerged paths:"
            | b"Stash entries:"
    )
}

fn is_git_status_advice(trimmed: &[u8]) -> bool {
    trimmed.starts_with(b"(use ")
        || trimmed.starts_with(b"(no changes added")
        || trimmed.starts_with(b"Your branch ")
}

fn is_git_clean_line(trimmed: &[u8]) -> bool {
    trimmed.starts_with(b"nothing to commit") || trimmed.starts_with(b"working tree clean")
}

fn git_status_label(trimmed: &[u8], theme: &Theme) -> Option<(usize, &'static str)> {
    const LABELS: &[(&[u8], GitStatusKind)] = &[
        (b"modified:", GitStatusKind::Modified),
        (b"new file:", GitStatusKind::Added),
        (b"deleted:", GitStatusKind::Deleted),
        (b"renamed:", GitStatusKind::Renamed),
        (b"copied:", GitStatusKind::Renamed),
        (b"typechange:", GitStatusKind::Modified),
        (b"both modified:", GitStatusKind::Conflict),
        (b"both added:", GitStatusKind::Conflict),
        (b"both deleted:", GitStatusKind::Conflict),
        (b"added by us:", GitStatusKind::Conflict),
        (b"added by them:", GitStatusKind::Conflict),
        (b"deleted by us:", GitStatusKind::Conflict),
        (b"deleted by them:", GitStatusKind::Conflict),
    ];
    LABELS
        .iter()
        .find(|(label, _)| trimmed.starts_with(label))
        .map(|(label, kind)| (label.len(), git_status_kind_color(*kind, theme)))
}

#[derive(Clone, Copy)]
enum GitStatusKind {
    Added,
    Conflict,
    Deleted,
    Modified,
    Renamed,
}

fn git_status_kind_color(kind: GitStatusKind, theme: &Theme) -> &'static str {
    match kind {
        GitStatusKind::Added => theme.info,
        GitStatusKind::Conflict => theme.warn,
        GitStatusKind::Deleted => theme.error,
        GitStatusKind::Modified => theme.warn,
        GitStatusKind::Renamed => theme.keyword,
    }
}

fn is_hex_byte(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

fn find_ascii(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    needle.len().checked_sub(1).filter(|_| !needle.is_empty())?;
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn code_line_comment_end(bytes: &[u8], start: usize, lang: CodeLanguage) -> Option<usize> {
    match lang {
        CodeLanguage::Python | CodeLanguage::Ruby | CodeLanguage::Shell => {
            (bytes[start] == b'#').then_some(bytes.len())
        }
        CodeLanguage::Php => (bytes[start] == b'#'
            || (bytes[start] == b'/' && bytes.get(start + 1) == Some(&b'/')))
        .then_some(bytes.len()),
        CodeLanguage::Css => None,
        _ => (bytes[start] == b'/' && bytes.get(start + 1) == Some(&b'/')).then_some(bytes.len()),
    }
}

fn code_block_comment_end(bytes: &[u8], start: usize, lang: CodeLanguage) -> Option<usize> {
    if !matches!(
        lang,
        CodeLanguage::CLike
            | CodeLanguage::Css
            | CodeLanguage::Go
            | CodeLanguage::Java
            | CodeLanguage::JavaScript
            | CodeLanguage::Kotlin
            | CodeLanguage::Php
            | CodeLanguage::Rust
            | CodeLanguage::Swift
            | CodeLanguage::TypeScript
    ) {
        return None;
    }
    if bytes[start] != b'/' || bytes.get(start + 1) != Some(&b'*') {
        return None;
    }
    let end = find_sql_block_comment_end(&bytes[start + 2..])
        .map(|rel| start + 4 + rel)
        .unwrap_or(bytes.len());
    Some(end)
}

fn code_string_end(bytes: &[u8], start: usize, lang: CodeLanguage) -> Option<usize> {
    let quote = bytes[start];
    if quote == b'\'' && lang == CodeLanguage::Rust {
        return rust_char_literal_end(bytes, start);
    }
    if quote == b'\'' || quote == b'"' || (quote == b'`' && code_supports_backticks(lang)) {
        return Some(quoted_code_end(bytes, start, quote));
    }
    None
}

fn rust_char_literal_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let mut visible = 0;
    while i < bytes.len() && visible <= 8 {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            visible += 1;
            continue;
        }
        if bytes[i] == b'\'' {
            return Some(i + 1);
        }
        if bytes[i].is_ascii_whitespace() {
            return None;
        }
        i += 1;
        visible += 1;
    }
    None
}

fn code_supports_backticks(lang: CodeLanguage) -> bool {
    matches!(
        lang,
        CodeLanguage::Go
            | CodeLanguage::JavaScript
            | CodeLanguage::Shell
            | CodeLanguage::TypeScript
    )
}

fn quoted_code_end(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            return i + 1;
        }
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
        } else {
            i += 1;
        }
    }
    bytes.len()
}

fn starts_code_number(bytes: &[u8], start: usize) -> bool {
    let b = bytes[start];
    if b == b'.' {
        return bytes.get(start + 1).is_some_and(u8::is_ascii_digit)
            && start
                .checked_sub(1)
                .and_then(|idx| bytes.get(idx))
                .is_none_or(|prev| !is_code_ident_continue(*prev));
    }
    b.is_ascii_digit()
        && start
            .checked_sub(1)
            .and_then(|idx| bytes.get(idx))
            .is_none_or(|prev| !is_code_ident_continue(*prev))
}

fn code_number_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    if bytes.get(i) == Some(&b'0') && matches!(bytes.get(i + 1), Some(b'x' | b'X' | b'b' | b'B')) {
        i += 2;
    }
    while i < bytes.len()
        && matches!(
            bytes[i],
            b'0'..=b'9'
                | b'a'..=b'f'
                | b'A'..=b'F'
                | b'.'
                | b'_'
                | b'x'
                | b'X'
                | b'+'
                | b'-'
        )
    {
        i += 1;
    }
    i
}

fn is_code_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || matches!(b, b'_' | b'$')
}

fn is_code_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$')
}

fn code_ident_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && is_code_ident_continue(bytes[i]) {
        i += 1;
    }
    i
}

fn looks_like_code_constant(word: &[u8]) -> bool {
    let trimmed = word.strip_prefix(b"$").unwrap_or(word);
    trimmed.len() > 1
        && trimmed
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
        && trimmed.iter().any(u8::is_ascii_uppercase)
}

fn looks_like_function_call(bytes: &[u8], mut end: usize) -> bool {
    while end < bytes.len() && bytes[end].is_ascii_whitespace() {
        end += 1;
    }
    bytes.get(end) == Some(&b'(')
}

fn is_code_punctuation(b: u8, lang: CodeLanguage) -> bool {
    matches!(
        b,
        b'{' | b'}'
            | b'['
            | b']'
            | b'('
            | b')'
            | b','
            | b';'
            | b':'
            | b'.'
            | b'='
            | b'+'
            | b'-'
            | b'*'
            | b'/'
            | b'%'
            | b'<'
            | b'>'
            | b'!'
            | b'?'
            | b'&'
            | b'|'
            | b'^'
            | b'~'
            | b'@'
    ) || (lang == CodeLanguage::Shell && matches!(b, b'$'))
}

fn is_code_keyword(word: &[u8], lang: CodeLanguage) -> bool {
    match lang {
        CodeLanguage::Rust => is_rust_keyword(word),
        CodeLanguage::Python => is_python_keyword(word),
        CodeLanguage::Shell => is_shell_keyword(word),
        CodeLanguage::JavaScript => is_javascript_keyword(word),
        CodeLanguage::TypeScript => is_typescript_keyword(word),
        CodeLanguage::Go => is_go_keyword(word),
        CodeLanguage::Java => is_java_keyword(word),
        CodeLanguage::Kotlin => is_kotlin_keyword(word),
        CodeLanguage::Swift => is_swift_keyword(word),
        CodeLanguage::Ruby => is_ruby_keyword(word),
        CodeLanguage::Php => is_php_keyword(word),
        CodeLanguage::Css => is_css_keyword(word),
        CodeLanguage::CLike => is_c_like_keyword(word),
    }
}

fn is_rust_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"as"
            | b"async"
            | b"await"
            | b"break"
            | b"const"
            | b"continue"
            | b"crate"
            | b"dyn"
            | b"else"
            | b"enum"
            | b"extern"
            | b"false"
            | b"fn"
            | b"for"
            | b"if"
            | b"impl"
            | b"in"
            | b"let"
            | b"loop"
            | b"match"
            | b"mod"
            | b"move"
            | b"mut"
            | b"pub"
            | b"ref"
            | b"return"
            | b"self"
            | b"Self"
            | b"static"
            | b"struct"
            | b"super"
            | b"trait"
            | b"true"
            | b"type"
            | b"unsafe"
            | b"use"
            | b"where"
            | b"while"
    )
}

fn is_python_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"and"
            | b"as"
            | b"assert"
            | b"async"
            | b"await"
            | b"break"
            | b"class"
            | b"continue"
            | b"def"
            | b"del"
            | b"elif"
            | b"else"
            | b"except"
            | b"False"
            | b"finally"
            | b"for"
            | b"from"
            | b"global"
            | b"if"
            | b"import"
            | b"in"
            | b"is"
            | b"lambda"
            | b"None"
            | b"nonlocal"
            | b"not"
            | b"or"
            | b"pass"
            | b"raise"
            | b"return"
            | b"True"
            | b"try"
            | b"while"
            | b"with"
            | b"yield"
    )
}

fn is_shell_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"case"
            | b"do"
            | b"done"
            | b"elif"
            | b"else"
            | b"esac"
            | b"export"
            | b"fi"
            | b"for"
            | b"function"
            | b"if"
            | b"in"
            | b"local"
            | b"readonly"
            | b"return"
            | b"select"
            | b"set"
            | b"then"
            | b"until"
            | b"while"
    )
}

fn is_javascript_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"await"
            | b"async"
            | b"break"
            | b"case"
            | b"catch"
            | b"class"
            | b"const"
            | b"continue"
            | b"debugger"
            | b"default"
            | b"delete"
            | b"do"
            | b"else"
            | b"export"
            | b"extends"
            | b"false"
            | b"finally"
            | b"for"
            | b"from"
            | b"function"
            | b"if"
            | b"import"
            | b"in"
            | b"instanceof"
            | b"let"
            | b"new"
            | b"null"
            | b"of"
            | b"return"
            | b"super"
            | b"switch"
            | b"this"
            | b"throw"
            | b"true"
            | b"try"
            | b"typeof"
            | b"undefined"
            | b"var"
            | b"void"
            | b"while"
            | b"with"
            | b"yield"
    )
}

fn is_typescript_keyword(word: &[u8]) -> bool {
    is_javascript_keyword(word)
        || matches!(
            word,
            b"any"
                | b"boolean"
                | b"declare"
                | b"enum"
                | b"implements"
                | b"interface"
                | b"keyof"
                | b"namespace"
                | b"never"
                | b"number"
                | b"private"
                | b"protected"
                | b"public"
                | b"readonly"
                | b"string"
                | b"type"
                | b"unknown"
        )
}

fn is_go_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"break"
            | b"case"
            | b"chan"
            | b"const"
            | b"continue"
            | b"default"
            | b"defer"
            | b"else"
            | b"fallthrough"
            | b"false"
            | b"for"
            | b"func"
            | b"go"
            | b"goto"
            | b"if"
            | b"import"
            | b"interface"
            | b"map"
            | b"nil"
            | b"package"
            | b"range"
            | b"return"
            | b"select"
            | b"struct"
            | b"switch"
            | b"true"
            | b"type"
            | b"var"
    )
}

fn is_java_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"abstract"
            | b"assert"
            | b"boolean"
            | b"break"
            | b"byte"
            | b"case"
            | b"catch"
            | b"char"
            | b"class"
            | b"const"
            | b"continue"
            | b"default"
            | b"do"
            | b"double"
            | b"else"
            | b"enum"
            | b"extends"
            | b"false"
            | b"final"
            | b"finally"
            | b"float"
            | b"for"
            | b"if"
            | b"implements"
            | b"import"
            | b"instanceof"
            | b"int"
            | b"interface"
            | b"long"
            | b"new"
            | b"null"
            | b"package"
            | b"private"
            | b"protected"
            | b"public"
            | b"return"
            | b"short"
            | b"static"
            | b"strictfp"
            | b"super"
            | b"switch"
            | b"synchronized"
            | b"this"
            | b"throw"
            | b"throws"
            | b"transient"
            | b"true"
            | b"try"
            | b"void"
            | b"volatile"
            | b"while"
    )
}

fn is_kotlin_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"as"
            | b"break"
            | b"class"
            | b"continue"
            | b"data"
            | b"do"
            | b"else"
            | b"false"
            | b"for"
            | b"fun"
            | b"if"
            | b"in"
            | b"interface"
            | b"is"
            | b"null"
            | b"object"
            | b"package"
            | b"return"
            | b"super"
            | b"this"
            | b"throw"
            | b"true"
            | b"try"
            | b"typealias"
            | b"val"
            | b"var"
            | b"when"
            | b"while"
    )
}

fn is_swift_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"as"
            | b"associatedtype"
            | b"break"
            | b"case"
            | b"catch"
            | b"class"
            | b"continue"
            | b"default"
            | b"defer"
            | b"do"
            | b"else"
            | b"enum"
            | b"extension"
            | b"false"
            | b"for"
            | b"func"
            | b"guard"
            | b"if"
            | b"import"
            | b"in"
            | b"init"
            | b"inout"
            | b"let"
            | b"nil"
            | b"private"
            | b"protocol"
            | b"public"
            | b"return"
            | b"self"
            | b"static"
            | b"struct"
            | b"super"
            | b"switch"
            | b"throw"
            | b"true"
            | b"try"
            | b"typealias"
            | b"var"
            | b"where"
            | b"while"
    )
}

fn is_ruby_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"alias"
            | b"and"
            | b"begin"
            | b"break"
            | b"case"
            | b"class"
            | b"def"
            | b"defined?"
            | b"do"
            | b"else"
            | b"elsif"
            | b"end"
            | b"ensure"
            | b"false"
            | b"for"
            | b"if"
            | b"in"
            | b"module"
            | b"next"
            | b"nil"
            | b"not"
            | b"or"
            | b"redo"
            | b"rescue"
            | b"retry"
            | b"return"
            | b"self"
            | b"super"
            | b"then"
            | b"true"
            | b"undef"
            | b"unless"
            | b"until"
            | b"when"
            | b"while"
            | b"yield"
    )
}

fn is_php_keyword(word: &[u8]) -> bool {
    matches!(
        lower_ascii(word).as_slice(),
        b"abstract"
            | b"and"
            | b"array"
            | b"as"
            | b"break"
            | b"callable"
            | b"case"
            | b"catch"
            | b"class"
            | b"clone"
            | b"const"
            | b"continue"
            | b"declare"
            | b"default"
            | b"die"
            | b"do"
            | b"echo"
            | b"else"
            | b"elseif"
            | b"empty"
            | b"endfor"
            | b"endforeach"
            | b"endif"
            | b"endswitch"
            | b"endwhile"
            | b"extends"
            | b"false"
            | b"final"
            | b"finally"
            | b"for"
            | b"foreach"
            | b"function"
            | b"global"
            | b"if"
            | b"implements"
            | b"include"
            | b"instanceof"
            | b"interface"
            | b"isset"
            | b"namespace"
            | b"new"
            | b"null"
            | b"or"
            | b"print"
            | b"private"
            | b"protected"
            | b"public"
            | b"require"
            | b"return"
            | b"static"
            | b"switch"
            | b"throw"
            | b"trait"
            | b"true"
            | b"try"
            | b"unset"
            | b"use"
            | b"var"
            | b"while"
            | b"xor"
    )
}

fn is_css_keyword(word: &[u8]) -> bool {
    matches!(
        lower_ascii(word).as_slice(),
        b"absolute"
            | b"auto"
            | b"block"
            | b"border-box"
            | b"center"
            | b"flex"
            | b"fixed"
            | b"grid"
            | b"hidden"
            | b"important"
            | b"inline"
            | b"none"
            | b"relative"
            | b"solid"
            | b"static"
            | b"sticky"
            | b"transparent"
    )
}

fn is_c_like_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"auto"
            | b"bool"
            | b"break"
            | b"case"
            | b"char"
            | b"class"
            | b"const"
            | b"constexpr"
            | b"continue"
            | b"default"
            | b"delete"
            | b"do"
            | b"double"
            | b"else"
            | b"enum"
            | b"extern"
            | b"false"
            | b"float"
            | b"for"
            | b"if"
            | b"inline"
            | b"int"
            | b"long"
            | b"namespace"
            | b"new"
            | b"nullptr"
            | b"private"
            | b"protected"
            | b"public"
            | b"return"
            | b"short"
            | b"signed"
            | b"sizeof"
            | b"static"
            | b"struct"
            | b"switch"
            | b"template"
            | b"this"
            | b"true"
            | b"typedef"
            | b"typename"
            | b"union"
            | b"unsigned"
            | b"using"
            | b"void"
            | b"volatile"
            | b"while"
    )
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

fn find_sql_block_comment_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(2).position(|w| w == b"*/")
}

fn sql_quoted_end(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            if bytes.get(i + 1) == Some(&quote) {
                i += 2;
                continue;
            }
            return i + 1;
        }
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
        } else {
            i += 1;
        }
    }
    bytes.len()
}

fn sql_number_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    if matches!(bytes.get(i), Some(b'-' | b'+')) {
        i += 1;
    }
    while i < bytes.len() && (bytes[i].is_ascii_digit() || matches!(bytes[i], b'.' | b'_')) {
        i += 1;
    }
    i
}

fn is_sql_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn sql_ident_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    i
}

fn is_sql_punctuation(b: u8) -> bool {
    matches!(
        b,
        b',' | b';' | b'(' | b')' | b'.' | b'=' | b'*' | b'+' | b'-' | b'/' | b'<' | b'>'
    )
}

fn is_sql_keyword(word: &[u8]) -> bool {
    matches!(
        upper_ascii(word).as_slice(),
        b"ADD"
            | b"ALTER"
            | b"AND"
            | b"AS"
            | b"ASC"
            | b"BEGIN"
            | b"BETWEEN"
            | b"BY"
            | b"CASE"
            | b"CHECK"
            | b"COMMIT"
            | b"CONSTRAINT"
            | b"CREATE"
            | b"CROSS"
            | b"DELETE"
            | b"DESC"
            | b"DISTINCT"
            | b"DROP"
            | b"ELSE"
            | b"END"
            | b"EXISTS"
            | b"FALSE"
            | b"FOREIGN"
            | b"FROM"
            | b"FULL"
            | b"GROUP"
            | b"HAVING"
            | b"IF"
            | b"IN"
            | b"INDEX"
            | b"INNER"
            | b"INSERT"
            | b"INTO"
            | b"IS"
            | b"JOIN"
            | b"KEY"
            | b"LEFT"
            | b"LIKE"
            | b"LIMIT"
            | b"NOT"
            | b"NULL"
            | b"ON"
            | b"OR"
            | b"ORDER"
            | b"OUTER"
            | b"PRIMARY"
            | b"REFERENCES"
            | b"RETURNING"
            | b"RIGHT"
            | b"ROLLBACK"
            | b"SELECT"
            | b"SET"
            | b"TABLE"
            | b"THEN"
            | b"TRUE"
            | b"UNION"
            | b"UNIQUE"
            | b"UPDATE"
            | b"VALUES"
            | b"WHEN"
            | b"WHERE"
            | b"WITH"
    )
}

fn upper_ascii(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(u8::to_ascii_uppercase).collect()
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

fn looks_like_mac_address(word: &[u8]) -> bool {
    word.len() == 17
        && word.iter().enumerate().all(|(idx, b)| {
            if idx % 3 == 2 {
                *b == b':'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

fn trim_ascii_start(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    bytes
}

fn is_usage_line(trimmed: &[u8]) -> bool {
    starts_with_ascii_ci(trimmed, b"usage:")
}

pub(crate) fn is_cli_error_line(trimmed: &[u8]) -> bool {
    let Some(colon) = trimmed.iter().position(|&b| b == b':') else {
        return false;
    };
    let tool = trim_ascii(&trimmed[..colon]);
    if tool.is_empty()
        || tool.len() > 64
        || !tool
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/' | b'+'))
    {
        return false;
    }
    let message = trim_ascii_start(&trimmed[colon + 1..]);
    contains_ascii_ci(message, b"illegal option")
        || contains_ascii_ci(message, b"invalid option")
        || contains_ascii_ci(message, b"unknown option")
        || contains_ascii_ci(message, b"unrecognized option")
        || contains_ascii_ci(message, b"no such file or directory")
        || contains_ascii_ci(message, b"permission denied")
}

fn starts_with_ascii_ci(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && haystack[..needle.len()].eq_ignore_ascii_case(needle)
}

fn contains_ascii_ci(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && needle.len() <= haystack.len()
        && haystack
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}

fn contains_ascii(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|w| w == needle)
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

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn trim_ascii_end(mut bytes: &[u8]) -> &[u8] {
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
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

/// Whether a line carries an ERROR-class severity token (`ERROR`/`FATAL`/
/// `CRITICAL`, delimited) in its leading window. Shared with error-line
/// pinning (`pin.rs`) so the pin and the streaming colorizer always agree on
/// what an error log line is.
pub(crate) fn is_error_log_line(content: &[u8]) -> bool {
    let c = ltrim(content);
    let window = &c[..c.len().min(SEVERITY_WINDOW)];
    LEVELS
        .iter()
        .filter(|(_, severity)| matches!(severity, Severity::Error))
        .any(|(token, _)| contains_delimited(window, token))
}

/// Whether `content` is a Python-style exception line: it begins at column 0 with
/// a dotted identifier (`pkg.mod.Name`) whose last segment ends in a known
/// exception suffix, immediately followed by `:`. Precise enough that prose like
/// `Note: see above` or `http://x: y` does not match. Shared with `pin.rs`.
pub(crate) fn is_exception_line(content: &[u8]) -> bool {
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

/// Left-trim ASCII whitespace. Shared with `pin.rs` so the pin and the line
/// colorizers agree on what "the start of a line" means.
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
            .starts_with("\x1b[38;5;220m"));
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
            .starts_with("\x1b[38;5;220m"));
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
    fn cli_diagnostics_color_errors_and_usage() {
        let theme = Theme::default_colored();
        assert_eq!(
            colorize_cli_diagnostic_line(b"find: illegal option -- m\n", &theme).unwrap(),
            b"\x1b[31mfind: illegal option -- m\x1b[0m\n"
        );
        assert_eq!(
            colorize_cli_diagnostic_line(b"usage: find path ... [expression]\n", &theme).unwrap(),
            b"\x1b[38;5;220musage: find path ... [expression]\x1b[0m\n"
        );
        assert!(colorize_cli_diagnostic_line(b"this usage is documented here\n", &theme).is_none());
        assert!(
            colorize_cli_diagnostic_line(b"http://example.com: no such page\n", &theme).is_none()
        );
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
