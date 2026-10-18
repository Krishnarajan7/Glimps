//! Linux socket and firewall report views: `ss` socket tables and
//! `ufw status` rule tables.
//!
//! Both colorizers validate the shape of every line before painting it and
//! return `None` otherwise, so they are safe to try against lines that belong
//! to a neighbouring command in a `;`-chained list.

use super::super::theme::Theme;
use super::{colorize_spans, paint_bytes, paint_whole, split_line, trim_ascii, word_spans};

/// `ss` socket states, as printed in its `State` column.
const SS_STATES: &[&[u8]] = &[
    b"LISTEN",
    b"ESTAB",
    b"UNCONN",
    b"SYN-SENT",
    b"SYN-RECV",
    b"FIN-WAIT-1",
    b"FIN-WAIT-2",
    b"TIME-WAIT",
    b"CLOSE-WAIT",
    b"LAST-ACK",
    b"CLOSING",
    b"CLOSED",
];

/// `ss` `Netid` values, printed first when more than one socket family shows.
const SS_NETIDS: &[&[u8]] = &[
    b"tcp", b"udp", b"raw", b"u_str", b"u_dgr", b"u_seq", b"p_raw", b"p_dgr", b"nl", b"mptcp",
    b"sctp", b"tipc", b"v_str", b"v_dgr", b"xdp",
];

/// Color an `ss` socket table (`ss -tlnp`, `ss -tunap`, …).
///
/// The heading must start `State`/`Netid` and name `Recv-Q`; a row must be
/// `[netid] STATE recv-q send-q local peer [process…]` with numeric queues.
/// Anything else declines.
pub fn colorize_ss_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let spans = word_spans(content);
    let first = spans.first().map(|&(s, e)| &content[s..e])?;
    if matches!(first, b"State" | b"Netid") && super::contains_ascii(content, b"Recv-Q") {
        return Some(paint_whole(
            content,
            ending,
            theme.table_header,
            theme.reset,
        ));
    }

    let state_index = usize::from(SS_NETIDS.contains(&first));
    let word = |index: usize| spans.get(index).map(|&(s, e)| &content[s..e]);
    let state = word(state_index)?;
    if !SS_STATES.contains(&state) {
        return None;
    }
    let is_queue =
        |w: Option<&[u8]>| w.is_some_and(|w| !w.is_empty() && w.iter().all(u8::is_ascii_digit));
    if !is_queue(word(state_index + 1)) || !is_queue(word(state_index + 2)) {
        return None;
    }
    let local_index = state_index + 3;
    let peer_index = state_index + 4;
    word(peer_index)?;

    let mut out = Vec::with_capacity(content.len() + ending.len() + spans.len() * 24);
    let mut cursor = 0;
    for (index, &(start, end)) in spans.iter().enumerate() {
        out.extend_from_slice(&content[cursor..start]);
        let text = &content[start..end];
        if index > peer_index {
            // The process column (and `-e`/`-o` extras) runs to end of line;
            // paint it as one region so its own spacing is preserved.
            push_ss_process(&mut out, &content[start..], theme);
            cursor = content.len();
            break;
        }
        if index < state_index {
            paint_bytes(&mut out, theme.comment, text, theme.reset);
        } else if index == state_index {
            paint_bytes(&mut out, ss_state_color(state, theme), text, theme.reset);
        } else if index < local_index {
            let color = if text == b"0" {
                theme.comment
            } else {
                theme.number
            };
            paint_bytes(&mut out, color, text, theme.reset);
        } else {
            push_ss_address(&mut out, text, index == peer_index, theme);
        }
        cursor = end;
    }
    out.extend_from_slice(&content[cursor..]);
    out.extend_from_slice(ending);
    Some(out)
}

fn ss_state_color(state: &[u8], theme: &Theme) -> &'static str {
    match state {
        b"LISTEN" => theme.info,
        b"ESTAB" => theme.key,
        b"UNCONN" | b"CLOSED" => theme.comment,
        _ => theme.warn,
    }
}

/// `host:port` with the host and port painted apart. A wildcard side
/// (`*`, `0.0.0.0`, `[::]`) of a peer reads as "anyone", so it stays dim.
fn push_ss_address(out: &mut Vec<u8>, text: &[u8], peer: bool, theme: &Theme) {
    let Some(colon) = text.iter().rposition(|&b| b == b':') else {
        paint_bytes(out, theme.path, text, theme.reset);
        return;
    };
    let (host, port) = (&text[..colon], &text[colon + 1..]);
    let wildcard_host = matches!(host, b"*" | b"0.0.0.0" | b"[::]");
    let host_color = if peer && wildcard_host {
        theme.comment
    } else {
        theme.path
    };
    if !host.is_empty() {
        paint_bytes(out, host_color, host, theme.reset);
    }
    paint_bytes(out, theme.comment, b":", theme.reset);
    if !port.is_empty() {
        let port_color = if port == b"*" {
            theme.comment
        } else {
            theme.number
        };
        paint_bytes(out, port_color, port, theme.reset);
    }
}

/// `users:(("nginx",pid=1100043,fd=6),…)`: quoted program names stand out,
/// `key=` labels and numbers are told apart, punctuation recedes.
fn push_ss_process(out: &mut Vec<u8>, text: &[u8], theme: &Theme) {
    let mut index = 0;
    while index < text.len() {
        let byte = text[index];
        if byte == b'"' {
            let close = text[index + 1..]
                .iter()
                .position(|&b| b == b'"')
                .map_or(text.len(), |offset| index + 1 + offset + 1);
            paint_bytes(out, theme.key, &text[index..close], theme.reset);
            index = close;
        } else if byte.is_ascii_digit() {
            let end = run_end(text, index, |b| b.is_ascii_digit());
            paint_bytes(out, theme.number, &text[index..end], theme.reset);
            index = end;
        } else if byte.is_ascii_alphabetic() {
            let end = run_end(text, index, |b| {
                b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
            });
            paint_bytes(out, theme.label, &text[index..end], theme.reset);
            index = end;
        } else if byte.is_ascii_whitespace() {
            let end = run_end(text, index, |b| b.is_ascii_whitespace());
            out.extend_from_slice(&text[index..end]);
            index = end;
        } else {
            let end = run_end(text, index, |b| {
                !(b == b'"' || b.is_ascii_alphanumeric() || b.is_ascii_whitespace())
            });
            paint_bytes(out, theme.comment, &text[index..end], theme.reset);
            index = end;
        }
    }
}

/// End of the run starting at `start` whose bytes satisfy `keep` (at least
/// one byte, so callers always advance).
fn run_end(text: &[u8], start: usize, keep: impl Fn(u8) -> bool) -> usize {
    text[start + 1..]
        .iter()
        .position(|&b| !keep(b))
        .map_or(text.len(), |offset| start + 1 + offset)
}

const UFW_ACTIONS: &[&[u8]] = &[b"ALLOW", b"DENY", b"REJECT", b"LIMIT"];

/// Color `ufw status` / `ufw status verbose` / `ufw status numbered`.
pub fn colorize_ufw_line(line: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii(content);
    if trimmed.is_empty() {
        return None;
    }
    if let Some(out) = colorize_ufw_report_line(content, ending, theme) {
        return Some(out);
    }
    let spans = word_spans(content);
    let words: Vec<&[u8]> = spans.iter().map(|&(s, e)| &content[s..e]).collect();
    if words.first() == Some(&b"To".as_slice())
        && words.contains(&b"Action".as_slice())
        && words.last() == Some(&b"From".as_slice())
    {
        return Some(paint_whole(
            content,
            ending,
            theme.table_header,
            theme.reset,
        ));
    }
    if trimmed.iter().all(|&b| b == b'-' || b == b' ') && trimmed.contains(&b'-') {
        return Some(paint_whole(content, ending, theme.comment, theme.reset));
    }

    // A rule row: the action sits in its own padded column, never first.
    let action_index = words
        .iter()
        .position(|word| UFW_ACTIONS.contains(word))
        .filter(|&index| index > 0)?;
    let action_start = spans[action_index].0;
    if !content[..action_start].ends_with(b"  ") {
        return None;
    }
    let action_color = match words[action_index] {
        b"ALLOW" => theme.info,
        b"LIMIT" => theme.warn,
        _ => theme.error,
    };
    let mut in_comment = false;
    Some(colorize_spans(
        content,
        ending,
        &spans,
        theme,
        |index, word| {
            if word.starts_with(b"#") {
                in_comment = true;
            }
            if in_comment || is_ufw_annotation(word) {
                return Some(theme.comment);
            }
            Some(match index {
                _ if index == action_index => action_color,
                _ if index < action_index => {
                    if word.starts_with(b"[") || word.ends_with(b"]") {
                        theme.comment
                    } else {
                        theme.key
                    }
                }
                _ if index == action_index + 1 && matches!(word, b"IN" | b"OUT" | b"FWD") => {
                    theme.comment
                }
                _ if word == b"on" => theme.comment,
                _ => theme.string,
            })
        },
    ))
}

/// `(v6)`, `(out)`, `(log)`, `(log-all)` qualifiers.
fn is_ufw_annotation(word: &[u8]) -> bool {
    matches!(word, b"(v6)" | b"(out)" | b"(log)" | b"(log-all)")
}

/// The `Label: value` lines above the rule table.
fn colorize_ufw_report_line(content: &[u8], ending: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    const LABELS: &[&[u8]] = &[b"Status:", b"Logging:", b"Default:", b"New profiles:"];
    let label = LABELS.iter().find(|label| content.starts_with(label))?;
    let value = &content[label.len()..];
    let mut out = Vec::with_capacity(content.len() + ending.len() + 48);
    paint_bytes(&mut out, theme.label, label, theme.reset);
    if *label == b"Status:" {
        let color = match trim_ascii(value) {
            b"active" => theme.info,
            b"inactive" => theme.warn,
            _ => theme.string,
        };
        let lead = value.len() - super::trim_ascii_start(value).len();
        out.extend_from_slice(&value[..lead]);
        paint_bytes(&mut out, color, &value[lead..], theme.reset);
    } else {
        let spans = word_spans(value);
        out.extend_from_slice(&colorize_spans(value, b"", &spans, theme, |_, word| {
            let bare = word.strip_suffix(b",").unwrap_or(word);
            Some(match bare {
                b"allow" => theme.info,
                b"deny" | b"reject" => theme.error,
                b"limit" => theme.warn,
                _ if bare.starts_with(b"(") => theme.comment,
                _ => theme.string,
            })
        }));
    }
    out.extend_from_slice(ending);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_text(out: &[u8]) -> Vec<u8> {
        let mut plain = Vec::new();
        let mut index = 0;
        while index < out.len() {
            if out[index] == 0x1b {
                while index < out.len() && out[index] != b'm' {
                    index += 1;
                }
                index += 1;
                continue;
            }
            plain.push(out[index]);
            index += 1;
        }
        plain
    }

    fn ss(line: &[u8]) -> Option<Vec<u8>> {
        colorize_ss_line(line, &Theme::default_colored())
    }

    fn ufw(line: &[u8]) -> Option<Vec<u8>> {
        colorize_ufw_line(line, &Theme::default_colored())
    }

    #[test]
    fn ss_rows_keep_every_byte_and_paint_columns() {
        let theme = Theme::default_colored();
        let line = b"LISTEN 0      511          0.0.0.0:80        0.0.0.0:*    users:((\"nginx\",pid=1100043,fd=6))\r\n";
        let out = ss(line).unwrap();
        assert_eq!(plain_text(&out), line);
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with(&format!("{}LISTEN{}", theme.info, theme.reset)));
        assert!(text.contains(&format!("{}511{}", theme.number, theme.reset)));
        assert!(text.contains(&format!("{}\"nginx\"{}", theme.key, theme.reset)));
        assert!(text.contains(&format!("{}1100043{}", theme.number, theme.reset)));
        assert!(text.ends_with("\r\n"));
    }

    #[test]
    fn ss_handles_ipv6_scoped_and_netid_rows() {
        for line in [
            b"LISTEN 0      4096   127.0.0.53%lo:53        0.0.0.0:*\n".as_slice(),
            b"LISTEN 0      511             [::]:443           [::]:*    users:((\"nginx\",pid=1,fd=8))\n",
            b"tcp   ESTAB  0      36     10.0.0.5:22    203.0.113.9:51514\n",
            b"LISTEN 0      511                *:3000             *:*    users:((\"PM2 v7.0.1: God\",pid=16,fd=20))\n",
        ] {
            let out = ss(line).unwrap_or_else(|| panic!("{}", String::from_utf8_lossy(line)));
            assert_eq!(plain_text(&out), line);
        }
        assert!(
            ss(b"State  Recv-Q Send-Q Local Address:Port  Peer Address:PortProcess\n").is_some()
        );
    }

    #[test]
    fn ss_declines_lines_that_are_not_socket_rows() {
        for line in [
            b"Status: active\n".as_slice(),
            b"22/tcp                     ALLOW       Anywhere\n",
            b"LISTEN to me carefully\n",
            b"/etc/nginx/proxy_params:3:proxy_set_header X-Forwarded-For $x;\n",
            b"PermitRootLogin yes\n",
            b"\n",
        ] {
            assert!(ss(line).is_none(), "{}", String::from_utf8_lossy(line));
        }
    }

    #[test]
    fn ufw_status_table_is_painted_and_byte_preserving() {
        let theme = Theme::default_colored();
        let lines: [&[u8]; 7] = [
            b"Status: active\r\n",
            b"To                         Action      From\r\n",
            b"--                         ------      ----\r\n",
            b"22/tcp                     ALLOW       Anywhere\r\n",
            b"443/tcp (v6)               DENY IN     Anywhere (v6)              # web\r\n",
            b"[ 1] 80/tcp                ALLOW IN    Anywhere\r\n",
            b"Default: deny (incoming), allow (outgoing), disabled (routed)\r\n",
        ];
        for line in lines {
            let out = ufw(line).unwrap_or_else(|| panic!("{}", String::from_utf8_lossy(line)));
            assert_eq!(plain_text(&out), line);
        }
        let status = String::from_utf8(ufw(lines[0]).unwrap()).unwrap();
        assert!(status.contains(&format!("{}active{}", theme.info, theme.reset)));
        let allow = String::from_utf8(ufw(lines[3]).unwrap()).unwrap();
        assert!(allow.contains(&format!("{}ALLOW{}", theme.info, theme.reset)));
        assert!(allow.contains(&format!("{}22/tcp{}", theme.key, theme.reset)));
        let deny = String::from_utf8(ufw(lines[4]).unwrap()).unwrap();
        assert!(deny.contains(&format!("{}DENY{}", theme.error, theme.reset)));
        assert!(deny.contains(&format!("{}(v6){}", theme.comment, theme.reset)));
    }

    #[test]
    fn ufw_declines_prose_and_other_commands_lines() {
        for line in [
            b"ALLOW everything please\n".as_slice(),
            b"we ALLOW it\n",
            b"LISTEN 0      511          0.0.0.0:80        0.0.0.0:*\n",
            b"/etc/nginx/sites-available/budgetbox:36:        proxy_set_header X\n",
            b"PermitRootLogin yes\n",
        ] {
            assert!(ufw(line).is_none(), "{}", String::from_utf8_lossy(line));
        }
    }

    #[test]
    fn plain_theme_declines() {
        let plain = Theme::plain();
        assert!(colorize_ss_line(b"LISTEN 0 1 a:1 b:*\n", &plain).is_none());
        assert!(colorize_ufw_line(b"Status: active\n", &plain).is_none());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn never_panics_and_only_inserts_escapes(line in proptest::collection::vec(any::<u8>(), 0..200)) {
            let theme = Theme::default_colored();
            for out in [colorize_ss_line(&line, &theme), colorize_ufw_line(&line, &theme)]
                .into_iter()
                .flatten()
            {
                // Coloring only adds bytes; the input survives as a subsequence.
                let mut rest = out.as_slice();
                for &byte in &line {
                    let found = rest.iter().position(|&b| b == byte);
                    prop_assert!(found.is_some());
                    rest = &rest[found.unwrap_or(0) + 1..];
                }
            }
        }
    }
}
