//! Networking and macOS system-report command views.

use super::super::theme::Theme;
use super::{
    colorize_words, contains_ascii, paint_bytes, paint_whole, split_line, trim_ascii,
    trim_ascii_start, word_spans,
};

/// Color `dig` and `nslookup` style DNS output.
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

/// Color `ifconfig` output. The command is a dense nested report, so we keep the
/// original layout and only mark interface headers, field names, addresses,
/// status words, and numeric values.
pub fn colorize_ifconfig_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let words = word_spans(content);
    if words.is_empty() {
        return None;
    }
    let first = &content[words[0].0..words[0].1];
    let offset = content.len() - trim_ascii_start(content).len();
    let header = offset == 0 && first.ends_with(b":");
    if !header && !is_ifconfig_field(first) {
        return None;
    }

    Some(colorize_words(content, ending, theme, |idx, word| {
        if idx == 0 {
            return Some(if header { theme.key } else { theme.keyword });
        }
        if ifconfig_status_is_up(word) {
            Some(theme.info)
        } else if ifconfig_status_is_down(word) || is_ifconfig_label(word) {
            Some(theme.comment)
        } else if looks_like_mac_address(word) || looks_like_ip_address(word) {
            Some(theme.path)
        } else if looks_like_ifconfig_number(word) {
            Some(theme.number)
        } else if word.starts_with(b"flags=") || word.starts_with(b"options=") {
            Some(theme.keyword)
        } else {
            Some(theme.string)
        }
    }))
}

fn is_ifconfig_field(word: &[u8]) -> bool {
    let word = word.strip_suffix(b":").unwrap_or(word);
    matches!(
        word,
        b"inet"
            | b"inet6"
            | b"ether"
            | b"media"
            | b"status"
            | b"nd6"
            | b"options"
            | b"options="
            | b"member"
            | b"groups"
            | b"vlan"
            | b"lladdr"
    ) || word.starts_with(b"options=")
}

fn is_ifconfig_label(word: &[u8]) -> bool {
    matches!(
        word.strip_suffix(b":").unwrap_or(word),
        b"mtu"
            | b"netmask"
            | b"broadcast"
            | b"prefixlen"
            | b"scopeid"
            | b"media"
            | b"status"
            | b"channel"
            | b"state"
    )
}

fn ifconfig_status_is_up(word: &[u8]) -> bool {
    matches!(
        word.strip_suffix(b",").unwrap_or(word),
        b"active" | b"associated" | b"RUNNING" | b"UP"
    )
}

fn ifconfig_status_is_down(word: &[u8]) -> bool {
    matches!(
        word.strip_suffix(b",").unwrap_or(word),
        b"inactive" | b"none" | b"down" | b"DOWN"
    )
}

fn looks_like_ifconfig_number(word: &[u8]) -> bool {
    (!word.is_empty() && word.iter().all(u8::is_ascii_digit))
        || word
            .strip_prefix(b"0x")
            .is_some_and(|hex| !hex.is_empty() && hex.iter().all(u8::is_ascii_hexdigit))
}

fn looks_like_ip_address(word: &[u8]) -> bool {
    let word = trim_word_punctuation(word);
    let has_dot = word.contains(&b'.');
    let has_colon = word.contains(&b':');
    if has_dot {
        return word
            .iter()
            .all(|b| b.is_ascii_digit() || matches!(*b, b'.' | b'/'));
    }
    has_colon
        && word
            .iter()
            .all(|b| b.is_ascii_hexdigit() || matches!(*b, b':' | b'%' | b'.'))
}

/// Color `scutil --dns` output.
pub fn colorize_scutil_dns_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    colorize_label_report_line(
        line,
        theme,
        &[b"DNS configuration".as_slice(), b"resolver ".as_slice()],
    )
}

/// Color `route get default` output.
pub fn colorize_route_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if let Some(out) = colorize_label_report_line(line, theme, &[]) {
        return Some(out);
    }
    colorize_route_or_netstat_table_line(line, theme)
}

/// Color `netstat -rn` routing tables.
pub fn colorize_netstat_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }
    if matches!(trimmed, b"Routing tables" | b"Internet:" | b"Internet6:") {
        return Some(paint_whole(content, ending, theme.key, theme.reset));
    }
    colorize_route_or_netstat_table_line(line, theme)
}

fn colorize_route_or_netstat_table_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let words = word_spans(content);
    if words.is_empty() {
        return None;
    }
    let first = &content[words[0].0..words[0].1];
    if matches!(first, b"Destination" | b"recvpipe") {
        return Some(paint_whole(content, ending, theme.debug, theme.reset));
    }
    Some(colorize_words(
        content,
        ending,
        theme,
        |idx, word| match idx {
            0 if looks_like_route_target(word) => Some(theme.key),
            1 if looks_like_route_target(word) => Some(theme.path),
            0 | 1 => Some(theme.string),
            2 if looks_like_route_flags(word) => Some(theme.keyword),
            2..=4 => Some(theme.comment),
            _ if looks_like_report_number(word) => Some(theme.number),
            _ if looks_like_interface_name(word) => Some(theme.key),
            _ => Some(theme.string),
        },
    ))
}

/// Color `lsof -i` connection rows.
pub fn colorize_lsof_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let words = word_spans(content);
    if words.is_empty() {
        return None;
    }
    let first = &content[words[0].0..words[0].1];
    if first == b"COMMAND" {
        return Some(paint_whole(content, ending, theme.debug, theme.reset));
    }
    Some(colorize_words(
        content,
        ending,
        theme,
        |idx, word| match idx {
            0 => Some(theme.key),
            1 => Some(theme.number),
            2 => Some(theme.string),
            3 | 4 => Some(theme.keyword),
            _ if looks_like_report_number(word) => Some(theme.number),
            _ if looks_like_socket_name(word) => Some(theme.path),
            _ => Some(theme.muted),
        },
    ))
}

/// Color `launchctl list` service rows.
pub fn colorize_launchctl_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let words = word_spans(content);
    if words.is_empty() {
        return None;
    }
    let first = &content[words[0].0..words[0].1];
    if first == b"PID" {
        return Some(paint_whole(content, ending, theme.debug, theme.reset));
    }
    Some(colorize_words(
        content,
        ending,
        theme,
        |idx, word| match idx {
            0 if word == b"-" => Some(theme.comment),
            0 if looks_like_report_number(word) => Some(theme.number),
            1 if word == b"0" => Some(theme.info),
            1 if word == b"-" => Some(theme.comment),
            1 if word.starts_with(b"-") || word != b"0" => Some(theme.warn),
            2 => Some(theme.key),
            _ => Some(theme.string),
        },
    ))
}

/// Color `pmset -g` power settings.
pub fn colorize_pmset_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    if let Some(out) = colorize_label_report_line(
        line,
        theme,
        &[
            b"System-wide power settings:".as_slice(),
            b"Currently in use:".as_slice(),
            b"Battery Power:".as_slice(),
            b"AC Power:".as_slice(),
        ],
    ) {
        return Some(out);
    }
    let (content, ending) = split_line(line);
    let words = word_spans(content);
    if words.is_empty() {
        return None;
    }
    Some(colorize_words(
        content,
        ending,
        theme,
        |idx, word| match idx {
            0 => Some(theme.key),
            _ if looks_like_report_number(word) => Some(theme.number),
            _ if matches!(word, b"on" | b"enabled" | b"true") => Some(theme.info),
            _ if matches!(word, b"off" | b"disabled" | b"false") => Some(theme.comment),
            _ => Some(theme.string),
        },
    ))
}

fn colorize_label_report_line(line: &[u8], theme: &Theme, headings: &[&[u8]]) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }
    if headings.iter().any(|heading| trimmed.starts_with(heading)) {
        return Some(paint_whole(content, ending, theme.key, theme.reset));
    }
    let colon = trimmed.iter().position(|&b| b == b':')?;
    let offset = content.len() - trimmed.len();
    let label_end = offset + colon + 1;
    Some(paint_report_label_value(content, ending, label_end, theme))
}

fn paint_report_label_value(
    content: &[u8],
    ending: &[u8],
    label_end: usize,
    theme: &Theme,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len() + ending.len() + 48);
    paint_bytes(&mut out, theme.keyword, &content[..label_end], theme.reset);
    if label_end < content.len() {
        let value = &content[label_end..];
        paint_bytes(
            &mut out,
            report_value_color(value, theme),
            value,
            theme.reset,
        );
    }
    out.extend_from_slice(ending);
    out
}

fn report_value_color(value: &[u8], theme: &Theme) -> &'static str {
    let trimmed = trim_ascii(value);
    if looks_like_mac_address(trimmed)
        || looks_like_ip_address(trimmed)
        || looks_like_interface_name(trimmed)
    {
        theme.path
    } else if looks_like_report_number(trimmed) {
        theme.number
    } else if ifconfig_status_is_up(trimmed) {
        theme.info
    } else if ifconfig_status_is_down(trimmed) {
        theme.comment
    } else {
        theme.string
    }
}

fn looks_like_route_target(word: &[u8]) -> bool {
    let word = trim_word_punctuation(word);
    word == b"default"
        || word == b"localhost"
        || word.contains(&b'.')
        || word.contains(&b':')
        || looks_like_interface_name(word)
}

fn looks_like_route_flags(word: &[u8]) -> bool {
    !word.is_empty()
        && word
            .iter()
            .all(|b| b.is_ascii_alphabetic() || matches!(*b, b'<' | b'>' | b',' | b'-'))
}

fn looks_like_interface_name(word: &[u8]) -> bool {
    let word = trim_word_punctuation(word);
    if word.len() < 3 || word.len() > 16 {
        return false;
    }
    let prefix_len = word.iter().take_while(|b| b.is_ascii_alphabetic()).count();
    prefix_len > 0 && prefix_len < word.len() && word[prefix_len..].iter().all(u8::is_ascii_digit)
}

fn looks_like_socket_name(word: &[u8]) -> bool {
    let word = trim_word_punctuation(word);
    word.contains(&b':') || word.contains(&b'.') || word.contains(&b'[') || word.contains(&b']')
}

fn looks_like_report_number(word: &[u8]) -> bool {
    let word = trim_word_punctuation(word);
    (!word.is_empty() && word.iter().all(u8::is_ascii_digit))
        || word
            .strip_prefix(b"0x")
            .is_some_and(|hex| !hex.is_empty() && hex.iter().all(u8::is_ascii_hexdigit))
}

fn trim_word_punctuation(mut word: &[u8]) -> &[u8] {
    while matches!(word.last(), Some(b',' | b';' | b')')) {
        word = &word[..word.len() - 1];
    }
    while matches!(word.first(), Some(b'(')) {
        word = &word[1..];
    }
    word
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
