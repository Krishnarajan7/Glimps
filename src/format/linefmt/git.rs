//! Git command output views.

use super::super::theme::Theme;
use super::{
    contains_ascii, paint_bytes, paint_whole, split_line, split_unquoted, trim_ascii,
    trim_ascii_end, trim_ascii_start,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitView {
    Branch,
    DiffStat,
    Log,
    ShortStatus,
    Status,
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
