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

/// Color one WHOIS record line without painting long registry prose.
///
/// WHOIS clients and registries expose many flag combinations, but their
/// useful output converges on a stable `field: value` shape.  Formatting that
/// shape keeps full responses and filtered pipelines consistent while leaving
/// organization names, remarks, and postal addresses neutral and readable.
pub fn colorize_whois_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with(b"%") || trimmed.starts_with(b"#") || trimmed.starts_with(b">>>") {
        return Some(paint_whole(content, ending, theme.comment, theme.reset));
    }

    let colon = trimmed.iter().position(|byte| *byte == b':')?;
    let field = &trimmed[..colon];
    if field.is_empty()
        || field.first().is_some_and(u8::is_ascii_whitespace)
        || field.last().is_some_and(u8::is_ascii_whitespace)
        || !field.iter().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b'/' | b' ')
        })
    {
        return None;
    }

    let value_offset = colon
        + 1
        + trimmed[colon + 1..]
            .iter()
            .take_while(|byte| byte.is_ascii_whitespace())
            .count();
    let value = &trimmed[value_offset..];
    let value_color = whois_value_color(field, value, theme);

    let leading_len = content.len() - trimmed.len();
    let mut out = Vec::with_capacity(line.len() + 64);
    out.extend_from_slice(&content[..leading_len]);
    // Labels are scaffolding, not the content the user came to inspect. Keep
    // the whole left column quiet so repeated WHOIS records remain scannable.
    paint_bytes(&mut out, theme.comment, field, theme.reset);
    paint_bytes(&mut out, theme.html_delim, b":", theme.reset);
    out.extend_from_slice(&trimmed[colon + 1..value_offset]);
    if let Some(color) = value_color {
        paint_bytes(&mut out, color, value, theme.reset);
    } else {
        out.extend_from_slice(value);
    }
    out.extend_from_slice(ending);
    Some(out)
}

const WHOIS_CONTACT_FIELDS: &[&[u8]] = &[b"abuse-mailbox", b"abuse-email", b"e-mail", b"email"];

const WHOIS_NETWORK_FIELDS: &[&[u8]] = &[
    b"inetnum",
    b"inet6num",
    b"netrange",
    b"cidr",
    b"netname",
    b"route",
    b"route6",
    b"origin",
    b"aut-num",
    b"as-name",
    b"nserver",
    b"name server",
    b"whois",
    b"referralserver",
    b"registrar whois server",
];

const WHOIS_IDENTITY_FIELDS: &[&[u8]] = &[b"orgname", b"org-name", b"organization", b"registrar"];

const WHOIS_DATE_FIELDS: &[&[u8]] = &[
    b"created",
    b"creation date",
    b"regdate",
    b"updated",
    b"updated date",
    b"changed",
    b"last-modified",
    b"expires",
    b"expiry date",
    b"registry expiry date",
    b"registrar registration expiration date",
    b"registration time",
    b"expiration time",
];

fn whois_value_color<'a>(field: &[u8], value: &[u8], theme: &'a Theme) -> Option<&'a str> {
    if whois_field_is(field, WHOIS_CONTACT_FIELDS)
        || whois_field_is(field, WHOIS_NETWORK_FIELDS)
        || whois_field_is(field, WHOIS_IDENTITY_FIELDS)
    {
        return Some(theme.string);
    }
    if whois_field_is(field, WHOIS_DATE_FIELDS) {
        return Some(theme.debug);
    }
    if field.eq_ignore_ascii_case(b"country") {
        return Some(theme.muted);
    }
    if field.eq_ignore_ascii_case(b"status") {
        if whois_contains_ignore_ascii_case(value, b"revoked")
            || whois_contains_ignore_ascii_case(value, b"blocked")
            || whois_contains_ignore_ascii_case(value, b"denied")
        {
            return Some(theme.error);
        }
        if whois_contains_ignore_ascii_case(value, b"reserved")
            || whois_contains_ignore_ascii_case(value, b"inactive")
            || whois_contains_ignore_ascii_case(value, b"pending")
        {
            return Some(theme.warn);
        }
        if whois_contains_ignore_ascii_case(value, b"active")
            || whois_contains_ignore_ascii_case(value, b"allocated")
            || whois_contains_ignore_ascii_case(value, b"assigned")
            || whois_contains_ignore_ascii_case(value, b"ok")
        {
            return Some(theme.info);
        }
        return Some(theme.string);
    }
    None
}

fn whois_field_is(field: &[u8], candidates: &[&[u8]]) -> bool {
    candidates
        .iter()
        .any(|candidate| field.eq_ignore_ascii_case(candidate))
}

fn whois_contains_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DfColumn {
    start: usize,
    role: DfColumnRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DfColumnRole {
    Filesystem,
    Type,
    Total,
    Used,
    Available,
    Capacity,
    Inodes,
    InodesUsed,
    InodesFree,
    InodesCapacity,
    Mount,
}

/// Color `df` using the schema printed by this invocation. This covers BSD and
/// GNU layouts and remains correct when `-h`, `-H`, `-k`, `-m`, `-i`, `-P`,
/// `-T`, or compatible combined/custom-output flags add, remove or reorder
/// columns. Byte positions—not whitespace token counts—also preserve macOS's
/// multi-word `map auto_home` filesystem name.
pub fn colorize_df_line(
    line: &[u8],
    theme: &Theme,
    columns: &mut Vec<DfColumn>,
) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let words = word_spans(content);
    if words.is_empty() {
        return None;
    }
    if let Some(schema) = df_header_schema(content, &words) {
        *columns = schema;
        return Some(colorize_words(content, ending, theme, |_idx, word| {
            df_header_color(word, theme)
        }));
    }

    if columns.len() >= 2 {
        return colorize_df_row(content, ending, theme, columns);
    }

    // Conservative fallback for a row received without its header (or a
    // localized/unknown header). This retains the previous useful behavior.
    colorize_df_row_by_percentages(content, ending, theme, &words)
}

fn df_header_schema(content: &[u8], words: &[(usize, usize)]) -> Option<Vec<DfColumn>> {
    let first = words.first().map(|(start, end)| &content[*start..*end])?;
    if !matches!(first, b"Filesystem" | b"Source") {
        return None;
    }
    let mut columns = Vec::new();
    for (start, end) in words.iter().copied() {
        let word = &content[start..end];
        let role = match word {
            b"Filesystem" | b"Source" => DfColumnRole::Filesystem,
            b"Type" | b"Fstype" => DfColumnRole::Type,
            b"Size" | b"512-blocks" | b"1024-blocks" | b"1K-blocks" | b"1M-blocks" => {
                DfColumnRole::Total
            }
            b"Used" => DfColumnRole::Used,
            b"Avail" | b"Available" => DfColumnRole::Available,
            b"Capacity" | b"Use%" | b"Pcent" => DfColumnRole::Capacity,
            b"Inodes" => DfColumnRole::Inodes,
            b"iused" | b"IUsed" => DfColumnRole::InodesUsed,
            b"ifree" | b"IFree" => DfColumnRole::InodesFree,
            b"%iused" | b"IUse%" | b"IPcent" => DfColumnRole::InodesCapacity,
            b"Mounted" | b"File" | b"Target" => DfColumnRole::Mount,
            // `on` is the second word of the single `Mounted on` label.
            _ => continue,
        };
        columns.push(DfColumn { start, role });
    }
    (columns.len() >= 2
        && columns
            .iter()
            .any(|column| column.role == DfColumnRole::Mount))
    .then_some(columns)
}

fn df_header_color(word: &[u8], theme: &Theme) -> Option<&'static str> {
    match word {
        b"Filesystem" | b"Source" => Some(theme.muted),
        b"Type" | b"Fstype" => Some(theme.keyword),
        b"Used" | b"iused" | b"IUsed" => Some(theme.warn),
        b"Avail" | b"Available" | b"ifree" | b"IFree" => Some(theme.info),
        b"Capacity" | b"Use%" | b"Pcent" | b"%iused" | b"IUse%" | b"IPcent" => Some(theme.keyword),
        b"Mounted" | b"on" | b"File" | b"Target" => Some(theme.path),
        _ => Some(theme.muted),
    }
}

fn colorize_df_row(
    content: &[u8],
    ending: &[u8],
    theme: &Theme,
    columns: &[DfColumn],
) -> Option<Vec<u8>> {
    let words = word_spans(content);
    let mount_start = columns
        .iter()
        .find(|column| column.role == DfColumnRole::Mount)?
        .start;
    let mount_word = words.iter().position(|(start, _)| *start >= mount_start)?;
    let middle_roles = columns
        .iter()
        .filter_map(|column| {
            (!matches!(column.role, DfColumnRole::Filesystem | DfColumnRole::Mount))
                .then_some(column.role)
        })
        .collect::<Vec<_>>();
    if mount_word <= middle_roles.len() {
        return None;
    }
    // Work backwards from the mount column. Unlike header byte boundaries,
    // token order remains stable when a large right-aligned number extends to
    // the left of its short label (`ifree` is the common macOS example).
    let first_middle_word = mount_word - middle_roles.len();
    Some(colorize_words(content, ending, theme, |index, word| {
        let role = if index < first_middle_word {
            DfColumnRole::Filesystem
        } else if index < mount_word {
            middle_roles[index - first_middle_word]
        } else {
            DfColumnRole::Mount
        };
        Some(df_value_color(role, word, theme))
    }))
}

fn df_value_color(role: DfColumnRole, value: &[u8], theme: &Theme) -> &'static str {
    match role {
        DfColumnRole::Filesystem => theme.muted,
        DfColumnRole::Type => theme.keyword,
        DfColumnRole::Total | DfColumnRole::Inodes => theme.muted,
        DfColumnRole::Used | DfColumnRole::InodesUsed => theme.warn,
        DfColumnRole::Available | DfColumnRole::InodesFree => theme.info,
        DfColumnRole::Capacity | DfColumnRole::InodesCapacity => percent_value(value)
            .map(|percent| storage_pressure_color(percent, theme))
            .unwrap_or(theme.muted),
        DfColumnRole::Mount => theme.path,
    }
}

fn colorize_df_row_by_percentages(
    content: &[u8],
    ending: &[u8],
    theme: &Theme,
    words: &[(usize, usize)],
) -> Option<Vec<u8>> {
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
    if let Some(separator) = colorize_ps_section_separator(content, ending, theme) {
        return Some(separator);
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
        let roles = ps_row_roles(content, &words, columns)?;
        return Some(colorize_words(content, ending, theme, |idx, word| {
            let role = roles.get(idx).copied().unwrap_or(PsColumnRole::Unknown);
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

fn colorize_ps_section_separator(content: &[u8], ending: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let trimmed = content.trim_ascii();
    let middle = trimmed
        .strip_prefix(b"=====")?
        .strip_suffix(b"=====")?
        .trim_ascii();
    if middle.is_empty() || !middle.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(colorize_words(content, ending, theme, |idx, _| match idx {
        1 => Some(theme.number),
        _ => Some(theme.comment),
    }))
}

fn ps_row_roles(
    content: &[u8],
    words: &[(usize, usize)],
    columns: &[PsColumnRole],
) -> Option<Vec<PsColumnRole>> {
    let first_id = columns.iter().position(|role| *role == PsColumnRole::Id)?;
    let id_word = words
        .get(first_id)
        .map(|(start, end)| &content[*start..*end])?;
    if !id_word.iter().all(u8::is_ascii_digit) {
        return None;
    }

    let mut roles = Vec::with_capacity(words.len());
    let mut word_index = 0;
    for (column_index, role) in columns.iter().copied().enumerate() {
        if word_index >= words.len() {
            return None;
        }
        if role == PsColumnRole::Command && column_index + 1 == columns.len() {
            roles.push(PsColumnRole::Command);
            roles.resize(words.len(), PsColumnRole::Unknown);
            break;
        }
        if role == PsColumnRole::Start && looks_like_ps_long_start(content, words, word_index) {
            roles.extend(std::iter::repeat_n(PsColumnRole::Start, 5));
            word_index += 5;
        } else {
            roles.push(role);
            word_index += 1;
        }
    }
    (roles.len() == words.len()).then_some(roles)
}

fn looks_like_ps_long_start(content: &[u8], words: &[(usize, usize)], start: usize) -> bool {
    let Some(parts) = words.get(start..start + 5) else {
        return false;
    };
    let part = |index: usize| &content[parts[index].0..parts[index].1];
    part(0).len() == 3
        && part(0).iter().all(u8::is_ascii_alphabetic)
        && part(1).len() == 3
        && part(1).iter().all(u8::is_ascii_alphabetic)
        && part(2).iter().all(u8::is_ascii_digit)
        && part(3).contains(&b':')
        && part(4).len() == 4
        && part(4).iter().all(u8::is_ascii_digit)
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
