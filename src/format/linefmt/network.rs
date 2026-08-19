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

/// Color live `ping` output and its final statistics without buffering.
pub fn colorize_ping_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii_start(content);
    if trimmed.is_empty() {
        return None;
    }

    if contains_ascii_case_insensitive(trimmed, b"unreachable")
        || contains_ascii_case_insensitive(trimmed, b"unknown host")
        || contains_ascii_case_insensitive(trimmed, b"cannot resolve")
    {
        return Some(paint_whole(content, ending, theme.error, theme.reset));
    }
    if contains_ascii_case_insensitive(trimmed, b"timeout")
        || contains_ascii_case_insensitive(trimmed, b"no answer yet")
    {
        return Some(paint_whole(content, ending, theme.warn, theme.reset));
    }

    let words = word_spans(content);
    let first = words.first().map(|(start, end)| &content[*start..*end])?;
    if first == b"PING" {
        return Some(colorize_words(content, ending, theme, |idx, word| {
            if idx == 0 {
                Some(theme.keyword)
            } else if looks_like_ping_address(word) {
                Some(theme.path)
            } else if ping_plain_number(word).is_some() {
                Some(theme.number)
            } else if matches!(word, b"data" | b"bytes" | b"of") {
                Some(theme.muted)
            } else {
                Some(theme.key)
            }
        }));
    }

    if contains_ascii(trimmed, b"bytes from") {
        return Some(colorize_words(content, ending, theme, |idx, word| {
            if idx == 0 && ping_plain_number(word).is_some() {
                Some(theme.number)
            } else if matches!(word, b"bytes" | b"from" | b"ms") {
                Some(theme.muted)
            } else if looks_like_ping_address(word) {
                Some(theme.path)
            } else if let Some(latency) = ping_latency(word) {
                Some(latency_color(latency, theme))
            } else if word.starts_with(b"icmp_seq=") || word.starts_with(b"ttl=") {
                Some(theme.number)
            } else {
                Some(theme.string)
            }
        }));
    }

    if trimmed.starts_with(b"---") && contains_ascii(trimmed, b"ping statistics") {
        return Some(colorize_words(content, ending, theme, |idx, word| {
            if idx == 0 || word == b"---" {
                Some(theme.html_delim)
            } else if idx == 1 {
                Some(theme.key)
            } else {
                Some(theme.muted)
            }
        }));
    }

    if contains_ascii(trimmed, b"packets transmitted") && contains_ascii(trimmed, b"packet loss") {
        return Some(colorize_words(content, ending, theme, |_idx, word| {
            if let Some(loss) = ping_percent(word) {
                Some(loss_color(loss, theme))
            } else if ping_plain_number(word).is_some() {
                Some(theme.number)
            } else {
                Some(theme.muted)
            }
        }));
    }

    if (trimmed.starts_with(b"round-trip ") || trimmed.starts_with(b"rtt "))
        && trimmed.contains(&b'=')
    {
        return Some(colorize_words(content, ending, theme, |_idx, word| {
            if word == b"=" {
                Some(theme.html_delim)
            } else if word == b"ms" {
                Some(theme.muted)
            } else if ping_latency_group(word) {
                Some(theme.number)
            } else {
                Some(theme.key)
            }
        }));
    }
    None
}

fn looks_like_ping_address(word: &[u8]) -> bool {
    let word = word.strip_suffix(b":").unwrap_or(word);
    looks_like_ip_address(word)
        || word
            .strip_prefix(b"(")
            .and_then(|value| value.strip_suffix(b")"))
            .is_some_and(looks_like_ip_address)
}

fn ping_plain_number(word: &[u8]) -> Option<f64> {
    let word = word
        .strip_suffix(b",")
        .or_else(|| word.strip_suffix(b":"))
        .unwrap_or(word);
    std::str::from_utf8(word).ok()?.parse().ok()
}

fn ping_latency(word: &[u8]) -> Option<f64> {
    let value = word
        .strip_prefix(b"time=")
        .or_else(|| word.strip_prefix(b"time<"))?;
    let value = value.strip_suffix(b"ms").unwrap_or(value);
    std::str::from_utf8(value).ok()?.parse().ok()
}

fn ping_percent(word: &[u8]) -> Option<f64> {
    let value = word.strip_suffix(b",").unwrap_or(word).strip_suffix(b"%")?;
    std::str::from_utf8(value).ok()?.parse().ok()
}

fn ping_latency_group(word: &[u8]) -> bool {
    word.contains(&b'/')
        && word
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(*byte, b'.' | b'/'))
}

fn latency_color(latency_ms: f64, theme: &Theme) -> &'static str {
    if latency_ms < 100.0 {
        theme.info
    } else if latency_ms < 250.0 {
        theme.warn
    } else {
        theme.error
    }
}

fn loss_color(loss_percent: f64, theme: &Theme) -> &'static str {
    if loss_percent == 0.0 {
        theme.info
    } else if loss_percent < 25.0 {
        theme.warn
    } else {
        theme.error
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

/// Color an `lsof` open-file table: every invocation from bare `lsof` to
/// `-i`/`-p`/`+D` prints the same fixed-width shape, read from its own heading.
pub fn colorize_lsof_line(
    line: &[u8],
    theme: &Theme,
    columns: &mut Vec<LsofColumn>,
) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let words = word_spans(content);
    if words.is_empty() {
        return None;
    }
    if let Some(schema) = lsof_header_schema(content, &words) {
        *columns = schema;
        // Each heading takes its own column's color, the way the other two
        // schema-learning views do (`colorize_df_line`, `colorize_ps_line`), so
        // a heading is visually tied to the column it names. Painting the whole
        // row one color made it indistinguishable from the COMMAND value below
        // it, which shares `theme.key`. The empty word keeps the value-dependent
        // branches out of the decision, as `ps` does.
        let roles = columns.iter().map(|column| column.role).collect::<Vec<_>>();
        return Some(colorize_words(content, ending, theme, |index, _| {
            let role = roles.get(index).copied().unwrap_or(LsofColumnRole::Unknown);
            lsof_role_color(role, b"", theme)
        }));
    }
    colorize_lsof_row(content, ending, theme, &words, columns)
}

/// Byte span and semantic role of one heading in the table `lsof` printed for
/// the current invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LsofColumn {
    start: usize,
    end: usize,
    role: LsofColumnRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LsofColumnRole {
    Command,
    Pid,
    User,
    Fd,
    Type,
    Device,
    Size,
    Node,
    Name,
    Unknown,
}

/// Widest schema accepted. Every row is matched against every column, so an
/// absurdly wide "heading" would make each later line cost O(words × columns).
/// A real `lsof` table has at most a dozen columns; this only rules out a line
/// that reached the heading test by accident or by construction.
const LSOF_MAX_COLUMNS: usize = 64;

/// Learn the schema from an `lsof` heading row.
///
/// Flags reshape the table — `-R` adds PPID, `-K` adds TID, `+L` adds NLINK,
/// `-o` renames SIZE/OFF — so the heading is read rather than assumed. A row is
/// only a schema if it starts at COMMAND and ends at NAME, which is what every
/// `lsof` table does and what arbitrary whitespace-aligned output does not.
fn lsof_header_schema(content: &[u8], words: &[(usize, usize)]) -> Option<Vec<LsofColumn>> {
    let (first_start, first_end) = *words.first()?;
    if &content[first_start..first_end] != b"COMMAND" || words.len() > LSOF_MAX_COLUMNS {
        return None;
    }
    let columns = words
        .iter()
        .map(|(start, end)| LsofColumn {
            start: *start,
            end: *end,
            role: lsof_header_role(&content[*start..*end]),
        })
        .collect::<Vec<_>>();
    let ends_at_name = columns
        .last()
        .is_some_and(|column| column.role == LsofColumnRole::Name);
    let has_pid = columns
        .iter()
        .any(|column| column.role == LsofColumnRole::Pid);
    (ends_at_name && has_pid && columns.len() >= 4).then_some(columns)
}

fn lsof_header_role(word: &[u8]) -> LsofColumnRole {
    match word {
        b"COMMAND" | b"TASKCMD" => LsofColumnRole::Command,
        b"PID" | b"PPID" | b"TID" | b"PGID" => LsofColumnRole::Pid,
        b"USER" => LsofColumnRole::User,
        b"FD" => LsofColumnRole::Fd,
        b"TYPE" => LsofColumnRole::Type,
        b"DEVICE" => LsofColumnRole::Device,
        b"SIZE" | b"OFF" | b"SIZE/OFF" | b"OFFSET" | b"NLINK" => LsofColumnRole::Size,
        b"NODE" | b"INODE" => LsofColumnRole::Node,
        b"NAME" => LsofColumnRole::Name,
        _ => LsofColumnRole::Unknown,
    }
}

/// Color one table row against the learned schema.
///
/// Values are matched to headings by byte position rather than by word index,
/// because rows for kernel objects (KQUEUE, NPOLICY, PSXSEM) leave DEVICE,
/// SIZE/OFF and NODE empty, which shifts every later word. NAME is the one
/// column that may contain spaces, so every word from its start byte onward is
/// treated as part of it — each is still painted individually (a NAME with a
/// space gets a reset between the pieces), but none can be mistaken for a
/// value in an earlier column.
fn colorize_lsof_row(
    content: &[u8],
    ending: &[u8],
    theme: &Theme,
    words: &[(usize, usize)],
    columns: &[LsofColumn],
) -> Option<Vec<u8>> {
    // No schema means no coloring, and that is load-bearing: it is the whole
    // reason `lsof -t` (bare pids), `-v` and `-h` stay verbatim, since none of
    // them print a heading. `df` and `ps` do fall back to guessing from value
    // shapes when their schema is empty — do NOT copy that here.
    let name_start = columns
        .last()
        .filter(|column| column.role == LsofColumnRole::Name)?
        .start;
    // Only a row carrying a process id under the PID heading is data. Warning
    // continuations ("Output information may be incomplete.") and any output
    // that drifted out of alignment decline here rather than being recolored.
    // This runs before the roles vector is built so a declined line pays for a
    // short scan instead of a full mapping plus an allocation.
    let pid_index = words
        .iter()
        .position(|span| lsof_role_at(columns, *span, name_start) == LsofColumnRole::Pid)?;
    let (pid_start, pid_end) = *words.get(pid_index)?;
    let pid = content.get(pid_start..pid_end)?;
    if !pid.iter().all(u8::is_ascii_digit) {
        return None;
    }
    // `roles` is indexed by the same word order `colorize_words` walks, because
    // both come from `word_spans` over this same `content`.
    let roles = words
        .iter()
        .map(|span| lsof_role_at(columns, *span, name_start))
        .collect::<Vec<_>>();
    Some(colorize_words(content, ending, theme, |index, word| {
        let role = roles.get(index).copied().unwrap_or(LsofColumnRole::Unknown);
        lsof_role_color(role, word, theme)
    }))
}

/// The heading a value sits under. Numeric columns are right-aligned and text
/// columns left-aligned, but `lsof` sizes each column to its widest value, so a
/// value always overlaps its own heading. The widest overlap wins for the rare
/// value that outgrows its column and reaches into a neighbor.
fn lsof_role_at(
    columns: &[LsofColumn],
    (start, end): (usize, usize),
    name_start: usize,
) -> LsofColumnRole {
    if start >= name_start {
        return LsofColumnRole::Name;
    }
    let mut best = (0usize, LsofColumnRole::Unknown);
    for column in columns {
        // Columns come from `word_spans`, so they are sorted by `start`: once a
        // heading begins at or after this word ends, no later one can overlap.
        // A single line may be up to `limits.line_cap` bytes, so this keeps the
        // per-row cost proportional to the columns actually near each word.
        if column.start >= end {
            break;
        }
        let overlap = end.min(column.end).saturating_sub(start.max(column.start));
        if overlap > best.0 {
            best = (overlap, column.role);
        }
    }
    best.1
}

fn lsof_role_color(role: LsofColumnRole, word: &[u8], theme: &Theme) -> Option<&'static str> {
    Some(match role {
        // Cyan reads as the row's identity. `folder` would be the match with
        // `ps`, but a DIR file type already claims it in this same row.
        LsofColumnRole::Command => theme.key,
        LsofColumnRole::Pid => theme.number,
        LsofColumnRole::User => theme.string,
        LsofColumnRole::Fd => lsof_fd_color(word, theme),
        LsofColumnRole::Type => lsof_type_color(word, theme),
        // A major,minor device number and an inode identify the same file the
        // NAME column already names, so they stay out of the way. A SIZE/OFF
        // value is a file offset, not a size, when it carries an offset prefix:
        // `0t` decimal, or `0x` hex under `-o`.
        LsofColumnRole::Device => theme.comment,
        LsofColumnRole::Size if word.starts_with(b"0t") || word.starts_with(b"0x") => theme.comment,
        LsofColumnRole::Size => theme.number,
        // NODE holds the protocol for a network file and an inode for the rest.
        LsofColumnRole::Node if word.iter().all(u8::is_ascii_digit) => theme.comment,
        LsofColumnRole::Node => theme.keyword,
        LsofColumnRole::Name => lsof_name_color(word, theme),
        // The mapper could not place THIS WORD under any heading. Only the word
        // is left bare — the rest of the row still colors, since the row already
        // proved itself by carrying a numeric pid under the PID heading, and one
        // stray value (a column the heading did not name) is not reason to drop
        // a whole row's coloring. Invariant #2 applied at word granularity:
        // uncolored beats guessed, and a drifted value stays visibly plain.
        LsofColumnRole::Unknown => return None,
    })
}

fn lsof_fd_color(word: &[u8], theme: &Theme) -> &'static str {
    match word {
        // A descriptor still open on an unlinked file — the reason a deleted
        // file can keep holding disk space.
        b"DEL" | b"del" => theme.warn,
        // lsof could not read the descriptor.
        b"err" | b"NOFD" | b"unk" => theme.error,
        // Process structure rather than an open file: these repeat on every row
        // of every process and are what makes bare `lsof` hard to read.
        b"cwd" | b"rtd" | b"txt" | b"mem" | b"mmap" | b"pd" | b"jld" | b"ltx" | b"tr" | b"v86" => {
            theme.comment
        }
        // A real descriptor with its access mode: `0u`, `3r`, `42uW`.
        _ if word.first().is_some_and(u8::is_ascii_digit) => theme.keyword,
        _ => theme.muted,
    }
}

fn lsof_type_color(word: &[u8], theme: &Theme) -> &'static str {
    match word {
        b"DIR" => theme.folder,
        // Communication endpoints — the rows people run `lsof` to find.
        b"IPv4" | b"IPv6" | b"inet" | b"unix" | b"sock" | b"netlink" | b"systm" | b"ndrv"
        | b"rte" | b"key" | b"PIPE" | b"FIFO" => theme.keyword,
        _ => theme.muted,
    }
}

fn lsof_name_color(word: &[u8], theme: &Theme) -> &'static str {
    if word.starts_with(b"(") && word.ends_with(b")") && word.len() > 2 {
        return lsof_state_color(&word[1..word.len() - 1], theme);
    }
    if word == b"->" {
        return theme.comment;
    }
    theme.path
}

/// The connection state or file condition `lsof` appends to a name.
fn lsof_state_color(state: &[u8], theme: &Theme) -> &'static str {
    match state {
        b"LISTEN" | b"ESTABLISHED" => theme.info,
        // Half-closed and transitional sockets: a pile of CLOSE_WAIT entries is
        // the classic descriptor leak. `deleted`/`revoked` mark a name that no
        // longer resolves to the file behind it.
        b"CLOSE_WAIT" | b"TIME_WAIT" | b"FIN_WAIT_1" | b"FIN_WAIT_2" | b"LAST_ACK" | b"CLOSING"
        | b"CLOSED" | b"SYN_SENT" | b"SYN_RCVD" | b"deleted" | b"revoked" => theme.warn,
        _ => theme.muted,
    }
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
