//! Filesystem and process command views.

use super::super::{cmdline, theme::Theme};
use super::{
    colorize_size_path_line, colorize_words, paint_bytes, paint_whole, split_line,
    trim_ascii_start, word_spans,
};

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

/// Color `whereis` results as a command label followed by typed locations.
/// Manual-page locations are distinguished from executable/source paths while
/// preserving the command's spacing and line ending exactly.
pub fn colorize_whereis_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let colon = content.iter().position(|byte| *byte == b':')?;
    let label = &content[..colon];
    if label.is_empty()
        || label.iter().any(u8::is_ascii_whitespace)
        || !label
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-' | b'.'))
    {
        return None;
    }

    let mut out = Vec::with_capacity(line.len() + 64);
    paint_bytes(&mut out, theme.key, label, theme.reset);
    paint_bytes(&mut out, theme.comment, b":", theme.reset);

    let locations = &content[colon + 1..];
    let words = word_spans(locations);
    let mut cursor = 0;
    for (start, end) in words {
        out.extend_from_slice(&locations[cursor..start]);
        let location = &locations[start..end];
        let color = if is_manual_page_path(location) {
            theme.keyword
        } else {
            theme.path
        };
        paint_bytes(&mut out, color, location, theme.reset);
        cursor = end;
    }
    out.extend_from_slice(&locations[cursor..]);
    out.extend_from_slice(ending);
    Some(out)
}

fn is_manual_page_path(path: &[u8]) -> bool {
    path.windows(4)
        .any(|window| window.eq_ignore_ascii_case(b"/man"))
}

/// Color one shell-history row: event number first, then the original command
/// using the same syntax colors as the GLIMPS command header.
pub fn colorize_history_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    colorize_numbered_command_line(line, theme)
}

/// Color an aggregated history-frequency row such as `uniq -c` emits.
pub fn colorize_history_count_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let number_start = content
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())?;
    let number_end = content[number_start..]
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .map(|offset| number_start + offset)
        .unwrap_or(content.len());
    if number_start == number_end || number_end == content.len() {
        return None;
    }

    let mut out = Vec::with_capacity(line.len() + theme.number.len() + theme.reset.len());
    out.extend_from_slice(&content[..number_start]);
    paint_bytes(
        &mut out,
        theme.number,
        &content[number_start..number_end],
        theme.reset,
    );
    // The command is a data value in this report, not command syntax. Keeping
    // it neutral prevents the cyan structural accent from filling a column.
    out.extend_from_slice(&content[number_end..]);
    out.extend_from_slice(ending);
    Some(out)
}

/// Color curl's built-in transfer meter without turning it into a rainbow.
/// Headers and time estimates are subdued, percentage fields carry numeric
/// meaning, completion is green, and current speed is readable content.
pub fn colorize_curl_progress_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    if content.contains(&b'\r') {
        let mut out = Vec::with_capacity(line.len() + 96);
        let mut start = 0;
        let mut matched = false;
        for (index, byte) in content.iter().copied().enumerate() {
            if byte != b'\r' {
                continue;
            }
            let segment = &content[start..index];
            if let Some(formatted) = colorize_curl_progress_segment(segment, b"", theme) {
                out.extend_from_slice(&formatted);
                matched = true;
            } else {
                out.extend_from_slice(segment);
            }
            out.push(b'\r');
            start = index + 1;
        }
        let segment = &content[start..];
        if let Some(formatted) = colorize_curl_progress_segment(segment, ending, theme) {
            out.extend_from_slice(&formatted);
            matched = true;
        } else {
            out.extend_from_slice(segment);
            out.extend_from_slice(ending);
        }
        return matched.then_some(out);
    }
    colorize_curl_progress_segment(content, ending, theme)
}

/// Format the metadata returned by `curl -I` / `curl --head`. Only the status
/// code and genuinely useful value types receive strong colors; header names
/// use a quiet blue-gray and long policy values remain neutral.
pub fn colorize_curl_header_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let leading = content
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())?;
    let visible = &content[leading..];

    if visible.starts_with(b"HTTP/") {
        let spans = word_spans(visible);
        let (version_start, version_end) = *spans.first()?;
        let (code_start, code_end) = *spans.get(1)?;
        let code = &visible[code_start..code_end];
        if code.len() != 3 || !code.iter().all(u8::is_ascii_digit) {
            return None;
        }
        let status_color = match code[0] {
            b'2' => theme.info,
            b'3' => theme.debug,
            b'4' => theme.warn,
            b'5' => theme.error,
            _ => theme.muted,
        };
        let mut out = Vec::with_capacity(line.len() + 32);
        out.extend_from_slice(&content[..leading + version_start]);
        paint_bytes(
            &mut out,
            theme.muted,
            &visible[version_start..version_end],
            theme.reset,
        );
        out.extend_from_slice(&visible[version_end..code_start]);
        paint_bytes(&mut out, status_color, code, theme.reset);
        // The reason phrase is readable content, not another status badge.
        out.extend_from_slice(&visible[code_end..]);
        out.extend_from_slice(ending);
        return Some(out);
    }

    let colon = visible.iter().position(|byte| *byte == b':')?;
    let name = &visible[..colon];
    if name.is_empty()
        || !name
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
    {
        return None;
    }

    let value_start = colon
        + 1
        + visible[colon + 1..]
            .iter()
            .take_while(|byte| byte.is_ascii_whitespace())
            .count();
    let value = &visible[value_start..];
    let value_color =
        if name.eq_ignore_ascii_case(b"content-type") || name.eq_ignore_ascii_case(b"location") {
            Some(theme.string)
        } else if name.eq_ignore_ascii_case(b"date")
            || name.eq_ignore_ascii_case(b"last-modified")
            || name.eq_ignore_ascii_case(b"expires")
        {
            Some(theme.debug)
        } else if !value.is_empty()
            && value
                .iter()
                .all(|byte| byte.is_ascii_digit() || byte.is_ascii_whitespace())
        {
            Some(theme.number)
        } else {
            None
        };

    let mut out = Vec::with_capacity(line.len() + 32);
    out.extend_from_slice(&content[..leading]);
    paint_bytes(&mut out, theme.muted, name, theme.reset);
    paint_bytes(&mut out, theme.html_delim, b":", theme.reset);
    out.extend_from_slice(&visible[colon + 1..value_start]);
    if let Some(color) = value_color {
        paint_bytes(&mut out, color, value, theme.reset);
    } else {
        out.extend_from_slice(value);
    }
    out.extend_from_slice(ending);
    Some(out)
}

fn colorize_curl_progress_segment(content: &[u8], ending: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let spans = word_spans(content);
    let words = spans
        .iter()
        .map(|(start, end)| &content[*start..*end])
        .collect::<Vec<_>>();
    if words.is_empty() {
        return None;
    }

    let first_header = words.first() == Some(&b"%".as_slice())
        && words.contains(&b"Total".as_slice())
        && words.contains(&b"Received".as_slice())
        && words.contains(&b"Speed".as_slice());
    let second_header = words.first() == Some(&b"Dload".as_slice())
        && words.contains(&b"Upload".as_slice())
        && words.contains(&b"Spent".as_slice())
        && words.contains(&b"Left".as_slice());
    if first_header || second_header {
        return Some(paint_whole(content, ending, theme.debug, theme.reset));
    }

    if words.len() < 12
        || !words[0].iter().all(u8::is_ascii_digit)
        || !words[2].iter().all(u8::is_ascii_digit)
        || !words[4].iter().all(u8::is_ascii_digit)
    {
        return None;
    }

    Some(colorize_words(
        content,
        ending,
        theme,
        |index, word| match index {
            0 if word == b"100" => Some(theme.info),
            0 | 2 | 4 => Some(theme.number),
            8..=10 => Some(theme.debug),
            11 => Some(theme.string),
            _ => None,
        },
    ))
}

fn colorize_numbered_command_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let number_start = content
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())?;
    let number_end = content[number_start..]
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .map(|offset| number_start + offset)
        .unwrap_or(content.len());
    if number_start == number_end || number_end == content.len() {
        return None;
    }
    let command_start = content[number_end..]
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .map(|offset| number_end + offset)?;

    let mut out = Vec::with_capacity(line.len() + 64);
    out.extend_from_slice(&content[..number_start]);
    paint_bytes(
        &mut out,
        theme.number,
        &content[number_start..number_end],
        theme.reset,
    );
    out.extend_from_slice(&content[number_end..command_start]);
    out.extend_from_slice(&cmdline::render(&content[command_start..], theme));
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
    let long_name = long_listing.then(|| &content[words[name_start].0..words[name_start].1]);
    let arrow_idx = if long_listing {
        words
            .iter()
            .enumerate()
            .skip(name_start)
            .find_map(|(idx, (start, end))| (&content[*start..*end] == b"->").then_some(idx))
    } else {
        None
    };
    let name_color = if long_name.is_some_and(is_dot_ls_entry) {
        theme.comment
    } else if long_name.is_some_and(is_hidden_ls_name) {
        theme.hidden
    } else {
        match first.first().copied() {
            Some(b'd') => theme.folder,
            Some(b'l') => theme.keyword,
            Some(b'-') if is_executable_mode(first) => theme.info,
            _ => theme.string,
        }
    };

    Some(colorize_words(content, ending, theme, |idx, word| {
        if long_listing {
            match idx {
                0 => Some(theme.debug),
                1 | 4 => Some(theme.number),
                2 | 3 | 5..=7 => Some(theme.comment),
                i if arrow_idx == Some(i) => Some(theme.comment),
                i if arrow_idx.is_some_and(|arrow| i > arrow) => Some(theme.path),
                i if i >= name_start => Some(name_color),
                _ => None,
            }
        } else if is_dot_ls_entry(word) || word == b"->" {
            Some(theme.comment)
        } else if is_hidden_ls_name(word) {
            Some(theme.hidden)
        } else if word.ends_with(b"/") {
            Some(theme.folder)
        } else if word.ends_with(b"*") {
            Some(theme.info)
        } else if word.ends_with(b"@") {
            Some(theme.keyword)
        } else {
            Some(theme.string)
        }
    }))
}

fn is_dot_ls_entry(name: &[u8]) -> bool {
    matches!(name, b"." | b"..")
}

fn is_hidden_ls_name(name: &[u8]) -> bool {
    name.starts_with(b".") && name != b"." && name != b".."
}

fn is_executable_mode(mode: &[u8]) -> bool {
    mode.iter()
        .skip(1)
        .take(9)
        .any(|byte| matches!(*byte, b'x' | b's' | b't'))
}

/// Color `du` output: size first, path after.
pub fn colorize_du_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    colorize_size_path_line(line, theme)
}

/// Color the label/value report emitted by macOS `GetFileInfo`.
pub fn colorize_getfileinfo_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    let colon = trimmed.iter().position(|byte| *byte == b':')?;
    let label = &trimmed[..colon];
    if label.is_empty()
        || !label
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b' ' | b'_' | b'-'))
    {
        return None;
    }

    let offset = content.len() - trimmed.len();
    let value_start = offset + colon + 1;
    let value = &content[value_start..];
    let value_color = match ascii_lower(label).as_slice() {
        b"file" | b"directory" => theme.path,
        b"created" | b"modified" => theme.number,
        b"attributes" | b"type" | b"creator" => theme.keyword,
        _ => theme.string,
    };

    let mut out = Vec::with_capacity(line.len() + 40);
    out.extend_from_slice(&content[..offset]);
    paint_bytes(&mut out, theme.key, label, theme.reset);
    paint_bytes(&mut out, theme.html_delim, b":", theme.reset);
    if !value.is_empty() {
        paint_bytes(&mut out, value_color, value, theme.reset);
    }
    out.extend_from_slice(ending);
    Some(out)
}

/// Color macOS `xattr -l` extended-attribute names and their values.
pub fn colorize_xattr_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }

    if let Some(colon) = trimmed.iter().position(|byte| *byte == b':') {
        let name = &trimmed[..colon];
        if !name.is_empty()
            && name
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-'))
        {
            let offset = content.len() - trimmed.len();
            let value_start = offset + colon + 1;
            let value = &content[value_start..];
            let color = if looks_like_hex_value(value) {
                theme.number
            } else {
                theme.string
            };
            let mut out = Vec::with_capacity(line.len() + 40);
            out.extend_from_slice(&content[..offset]);
            paint_bytes(&mut out, theme.key, name, theme.reset);
            paint_bytes(&mut out, theme.html_delim, b":", theme.reset);
            if !value.is_empty() {
                paint_bytes(&mut out, color, value, theme.reset);
            }
            out.extend_from_slice(ending);
            return Some(out);
        }
    }

    Some(paint_whole(
        content,
        ending,
        if looks_like_hex_value(trimmed) {
            theme.number
        } else {
            theme.string
        },
        theme.reset,
    ))
}

fn ascii_lower(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(u8::to_ascii_lowercase).collect()
}

fn looks_like_hex_value(value: &[u8]) -> bool {
    let trimmed = trim_ascii_start(value);
    !trimmed.is_empty()
        && trimmed
            .iter()
            .all(|byte| byte.is_ascii_hexdigit() || byte.is_ascii_whitespace())
}

pub fn colorize_kubectl_pods_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let spans = word_spans(content);
    if spans.len() < 5 {
        return None;
    }
    let cols: Vec<&[u8]> = spans.iter().map(|(s, e)| &content[*s..*e]).collect();

    if cols[0] == b"NAME"
        && cols[1] == b"READY"
        && cols[2] == b"STATUS"
        && cols[3] == b"RESTARTS"
        && cols[4] == b"AGE"
    {
        return Some(paint_whole(content, ending, theme.debug, theme.reset));
    }
    if ready_color(cols[1], theme).is_none() || pod_status_color(cols[2], theme).is_none() {
        return None;
    }

    Some(colorize_words(
        content,
        ending,
        theme,
        |idx, word| match idx {
            0 => Some(theme.key),
            1 => ready_color(word, theme),
            2 => pod_status_color(word, theme),
            3 => Some(theme.number),
            4 => Some(theme.debug),
            _ => None,
        },
    ))
}

fn pod_status_color(status: &[u8], theme: &Theme) -> Option<&'static str> {
    match status {
        b"Running" => Some(theme.info),
        b"Pending" | b"ImagePullBackOff" => Some(theme.warn),
        b"CrashLoopBackOff" | b"Error" => Some(theme.error),
        _ => None,
    }
}

fn ready_color(ready: &[u8], theme: &Theme) -> Option<&'static str> {
    let slash = ready.iter().position(|&b| b == b'/')?;
    let current = parse_u16(&ready[..slash])?;
    let total = parse_u16(&ready[slash + 1..])?;

    Some(if current == total && total > 0 {
        theme.info
    } else {
        theme.warn
    })
}

fn parse_u16(bytes: &[u8]) -> Option<u16> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

/// Color `df` output by storage meaning rather than treating every number alike.
/// The capacity column anchors the schema, so multi-word filesystem names such
/// as macOS `map auto_home` do not shift the remaining columns.
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
        return Some(colorize_words(
            content,
            ending,
            theme,
            |_idx, word| match word {
                b"Filesystem" => Some(theme.key),
                b"Used" | b"iused" => Some(theme.warn),
                b"Avail" | b"Available" | b"ifree" => Some(theme.info),
                b"Capacity" | b"Use%" | b"%iused" => Some(theme.keyword),
                b"Mounted" | b"on" => Some(theme.path),
                _ => Some(theme.muted),
            },
        ));
    }

    let percentages = words
        .iter()
        .enumerate()
        .filter_map(|(idx, (start, end))| {
            percent_value(&content[*start..*end]).map(|value| (idx, value))
        })
        .collect::<Vec<_>>();
    let (capacity_idx, _) = *percentages.first()?;
    if capacity_idx < 3 {
        return None;
    }
    let size_idx = capacity_idx - 3;
    let used_idx = capacity_idx - 2;
    let available_idx = capacity_idx - 1;
    let inode_percent_idx = percentages.get(1).map(|(idx, _)| *idx);
    let mount_idx = inode_percent_idx.map_or(capacity_idx + 1, |idx| idx + 1);

    Some(colorize_words(content, ending, theme, |idx, word| {
        if idx < size_idx {
            Some(theme.key)
        } else if idx == size_idx {
            Some(theme.muted)
        } else if idx == used_idx {
            Some(theme.warn)
        } else if idx == available_idx {
            Some(theme.info)
        } else if idx == capacity_idx || inode_percent_idx == Some(idx) {
            percent_value(word).map(|value| storage_pressure_color(value, theme))
        } else if inode_percent_idx.is_some_and(|inode_idx| idx + 2 == inode_idx) {
            Some(theme.warn)
        } else if inode_percent_idx.is_some_and(|inode_idx| idx + 1 == inode_idx) {
            Some(theme.info)
        } else if idx >= mount_idx {
            Some(theme.path)
        } else {
            Some(theme.string)
        }
    }))
}

fn storage_pressure_color(percent: f64, theme: &Theme) -> &'static str {
    if percent >= 90.0 {
        theme.error
    } else if percent >= 70.0 {
        theme.warn
    } else {
        theme.info
    }
}

/// Semantic roles learned from the header emitted by this particular `ps`
/// invocation. BSD `ps`, `ps aux`, POSIX `ps -ef`, and custom `-o` layouts all
/// use different column orders, so fixed numeric positions are not reliable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PsColumnRole {
    User,
    Id,
    Cpu,
    Memory,
    Size,
    Tty,
    State,
    Start,
    Time,
    Priority,
    Command,
    Unknown,
}

/// Color `ps` output using the roles declared by its header. The discovered
/// layout is retained by `Formatter` for the remaining rows of this command.
pub fn colorize_ps_line(
    line: &[u8],
    theme: &Theme,
    columns: &mut Vec<PsColumnRole>,
) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let words = word_spans(content);
    if words.is_empty() {
        return None;
    }
    if let Some(header) = ps_header_roles(content, &words) {
        *columns = header;
        return Some(colorize_words(content, ending, theme, |idx, _| {
            Some(ps_role_color(
                columns.get(idx).copied().unwrap_or(PsColumnRole::Unknown),
                b"",
                theme,
            ))
        }));
    }

    if !columns.is_empty() {
        return Some(colorize_words(content, ending, theme, |idx, word| {
            let role = columns.get(idx).copied().unwrap_or(PsColumnRole::Unknown);
            Some(ps_role_color(role, word, theme))
        }));
    }

    // Headerless output (`ps -o pid=,...`) cannot declare a trustworthy schema.
    // Retain the conservative legacy fallback rather than guessing from values.
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

fn ps_header_roles(content: &[u8], words: &[(usize, usize)]) -> Option<Vec<PsColumnRole>> {
    let roles = words
        .iter()
        .map(|(start, end)| ps_header_role(&content[*start..*end]))
        .collect::<Vec<_>>();
    let recognized = roles
        .iter()
        .filter(|role| **role != PsColumnRole::Unknown)
        .count();
    (recognized >= 2 && roles.contains(&PsColumnRole::Id)).then_some(roles)
}

fn ps_header_role(word: &[u8]) -> PsColumnRole {
    match word {
        b"USER" | b"UID" | b"RUSER" | b"EUSER" | b"LOGNAME" => PsColumnRole::User,
        b"PID" | b"PPID" | b"PGID" | b"SID" | b"TPGID" | b"LWP" | b"NLWP" => PsColumnRole::Id,
        b"%CPU" | b"CPU" | b"C" | b"CP" => PsColumnRole::Cpu,
        b"%MEM" | b"MEM" => PsColumnRole::Memory,
        b"VSZ" | b"VIRT" | b"RSS" | b"RSZ" | b"SZ" => PsColumnRole::Size,
        b"TTY" | b"TT" => PsColumnRole::Tty,
        b"STAT" | b"STATE" | b"S" => PsColumnRole::State,
        b"START" | b"STARTED" | b"LSTART" | b"STIME" => PsColumnRole::Start,
        b"TIME" | b"ETIME" | b"ELAPSED" | b"TIME+" => PsColumnRole::Time,
        b"PRI" | b"NI" | b"NICE" | b"RTPRIO" | b"PSR" => PsColumnRole::Priority,
        b"COMMAND" | b"CMD" | b"COMM" | b"ARGS" | b"COMMAND_NAME" => PsColumnRole::Command,
        _ => PsColumnRole::Unknown,
    }
}

fn ps_role_color(role: PsColumnRole, word: &[u8], theme: &Theme) -> &'static str {
    match role {
        PsColumnRole::User => theme.string,
        PsColumnRole::Id | PsColumnRole::Size => theme.number,
        PsColumnRole::Cpu | PsColumnRole::Memory => match float_value(word) {
            Some(value) if value >= 50.0 => theme.error,
            Some(value) if value >= 10.0 => theme.warn,
            _ => theme.muted,
        },
        PsColumnRole::Tty => theme.path,
        PsColumnRole::State => theme.keyword,
        PsColumnRole::Start | PsColumnRole::Time | PsColumnRole::Priority => theme.comment,
        PsColumnRole::Command => theme.folder,
        PsColumnRole::Unknown => theme.muted,
    }
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

fn percent_value(word: &[u8]) -> Option<f64> {
    let number = word.strip_suffix(b"%").unwrap_or(word);
    std::str::from_utf8(number).ok()?.parse().ok()
}

fn float_value(word: &[u8]) -> Option<f32> {
    std::str::from_utf8(word).ok()?.parse().ok()
}
