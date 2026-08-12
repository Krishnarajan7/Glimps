//! Delimited tables, database result tables, and SQL syntax views.

use super::super::theme::Theme;
use super::{paint_bytes, paint_whole, split_line, trim_ascii};

const DELIMITED_TABLE_WIDTH: usize = 100;
pub(crate) const AUTO_DELIMITER: u8 = 0;

/// Parse and reflow a complete CSV/TSV/PSV document.
///
/// Small datasets become aligned tables. Wide datasets become record blocks,
/// because forcing ten or twenty columns onto one terminal line merely moves
/// the original comma-separated wrapping problem into padded text. Every cell
/// remains visible; malformed or inconsistent input is declined entirely.
pub fn format_delimited_document(bytes: &[u8], theme: &Theme, delimiter: u8) -> Option<Vec<u8>> {
    let delimiter = resolve_document_delimiter(bytes, delimiter)?;
    let records = parse_delimited_document(bytes, delimiter)?;
    let (header, rows) = records.split_first()?;
    if header.len() < 2
        || rows.is_empty()
        || header.iter().any(|cell| trim_ascii(cell).is_empty())
        || rows.iter().any(|row| row.len() != header.len())
    {
        return None;
    }

    let widths = delimited_column_widths(header, rows)?;
    let total_width = widths.iter().sum::<usize>() + (widths.len() - 1) * 2;
    if total_width <= DELIMITED_TABLE_WIDTH
        && records
            .iter()
            .flatten()
            .all(|cell| cell.is_ascii() && !cell.contains(&b'\n'))
    {
        Some(render_delimited_table(header, rows, &widths, theme))
    } else {
        Some(render_delimited_records(header, rows, theme))
    }
}

fn resolve_document_delimiter(bytes: &[u8], requested: u8) -> Option<u8> {
    if requested != AUTO_DELIMITER {
        return matches!(requested, b',' | b';' | b'\t' | b'|').then_some(requested);
    }

    b",;\t"
        .iter()
        .copied()
        .filter_map(|candidate| {
            let records = parse_delimited_document(bytes, candidate)?;
            let (header, rows) = records.split_first()?;
            let valid = header.len() >= 2
                && !rows.is_empty()
                && header.iter().all(|cell| plausible_header_cell(cell))
                && rows.iter().all(|row| row.len() == header.len());
            valid.then_some((candidate, header.len()))
        })
        .max_by_key(|(_, columns)| *columns)
        .map(|(delimiter, _)| delimiter)
}

fn plausible_header_cell(cell: &[u8]) -> bool {
    let cell = trim_ascii(cell);
    !cell.is_empty()
        && cell.iter().any(u8::is_ascii_alphabetic)
        && cell.iter().all(|byte| {
            byte.is_ascii_alphanumeric()
                || byte.is_ascii_whitespace()
                || matches!(*byte, b'_' | b'-' | b'.' | b'/' | b'(' | b')')
        })
}

fn parse_delimited_document(bytes: &[u8], delimiter: u8) -> Option<Vec<Vec<Vec<u8>>>> {
    if !matches!(delimiter, b',' | b';' | b'\t' | b'|') || bytes.is_empty() {
        return None;
    }
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = Vec::new();
    let mut in_quotes = false;
    let mut quoted_field = false;
    let mut closed_quote = false;
    let mut record_started = false;
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'"' {
            if in_quotes {
                if bytes.get(i + 1) == Some(&b'"') {
                    field.push(b'"');
                    i += 2;
                    record_started = true;
                    continue;
                }
                in_quotes = false;
                closed_quote = true;
                i += 1;
                continue;
            }
            if field.is_empty() && !quoted_field {
                in_quotes = true;
                quoted_field = true;
                record_started = true;
                i += 1;
                continue;
            }
            // A quote in an unquoted field is not RFC 4180 CSV. Decline the
            // whole transform instead of presenting a subtly altered record.
            return None;
        }
        if !in_quotes && byte == delimiter {
            record.push(std::mem::take(&mut field));
            quoted_field = false;
            closed_quote = false;
            record_started = true;
            i += 1;
            continue;
        }
        if !in_quotes && matches!(byte, b'\r' | b'\n') {
            if byte == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            if record_started || !field.is_empty() || !record.is_empty() {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            quoted_field = false;
            closed_quote = false;
            record_started = false;
            i += 1;
            continue;
        }
        if closed_quote {
            // After a quoted field closes, only its delimiter, record ending,
            // or EOF is valid. Refuse permissive repair that could hide damage.
            return None;
        }
        field.push(byte);
        record_started = true;
        i += 1;
    }

    if in_quotes {
        return None;
    }
    if record_started || !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    (records.len() >= 2).then_some(records)
}

fn delimited_column_widths(header: &[Vec<u8>], rows: &[Vec<Vec<u8>>]) -> Option<Vec<usize>> {
    let mut widths = header
        .iter()
        .map(|cell| {
            std::str::from_utf8(cell)
                .ok()
                .map(str::chars)
                .map(Iterator::count)
        })
        .collect::<Option<Vec<_>>>()?;
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            let width = std::str::from_utf8(cell).ok()?.chars().count();
            widths[index] = widths[index].max(width);
        }
    }
    Some(widths)
}

fn render_delimited_table(
    header: &[Vec<u8>],
    rows: &[Vec<Vec<u8>>],
    widths: &[usize],
    theme: &Theme,
) -> Vec<u8> {
    let numeric = (0..header.len())
        .map(|column| {
            rows.iter().all(|row| {
                let value = trim_ascii(&row[column]);
                value.is_empty() || looks_numeric(value)
            })
        })
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    render_delimited_table_row(&mut out, header, widths, &numeric, theme, true);
    for (index, width) in widths.iter().copied().enumerate() {
        if index > 0 {
            out.extend_from_slice(b"  ");
        }
        paint_bytes(
            &mut out,
            theme.comment,
            "─".repeat(width).as_bytes(),
            theme.reset,
        );
    }
    out.push(b'\n');
    for row in rows {
        render_delimited_table_row(&mut out, row, widths, &numeric, theme, false);
    }
    out
}

fn render_delimited_table_row(
    out: &mut Vec<u8>,
    cells: &[Vec<u8>],
    widths: &[usize],
    numeric: &[bool],
    theme: &Theme,
    header: bool,
) {
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            out.extend_from_slice(b"  ");
        }
        let width = std::str::from_utf8(cell)
            .map(|text| text.chars().count())
            .unwrap_or(cell.len());
        let padding = widths[index].saturating_sub(width);
        if !header && numeric[index] {
            out.extend(std::iter::repeat_n(b' ', padding));
        }
        paint_delimited_value(out, cell, theme, header);
        if header || !numeric[index] {
            out.extend(std::iter::repeat_n(b' ', padding));
        }
    }
    out.push(b'\n');
}

fn render_delimited_records(header: &[Vec<u8>], rows: &[Vec<Vec<u8>>], theme: &Theme) -> Vec<u8> {
    let label_width = header.iter().map(Vec::len).max().unwrap_or(0);
    let mut out = Vec::new();
    for (row_index, row) in rows.iter().enumerate() {
        if row_index > 0 {
            out.push(b'\n');
        }
        paint_bytes(&mut out, theme.comment, b"record ", theme.reset);
        paint_bytes(
            &mut out,
            theme.muted,
            (row_index + 1).to_string().as_bytes(),
            theme.reset,
        );
        out.push(b'\n');
        for (label, value) in header.iter().zip(row) {
            out.extend_from_slice(b"  ");
            paint_bytes(&mut out, theme.muted, label, theme.reset);
            out.extend(std::iter::repeat_n(
                b' ',
                label_width.saturating_sub(label.len()) + 2,
            ));
            if value.is_empty() {
                paint_bytes(&mut out, theme.comment, "—".as_bytes(), theme.reset);
                out.push(b'\n');
                continue;
            }
            let mut lines = value.split(|byte| *byte == b'\n');
            if let Some(first) = lines.next() {
                paint_delimited_value(&mut out, first, theme, false);
            }
            for continuation in lines {
                out.push(b'\n');
                out.extend(std::iter::repeat_n(b' ', label_width + 4));
                paint_delimited_value(&mut out, continuation, theme, false);
            }
            out.push(b'\n');
        }
    }
    out
}

fn paint_delimited_value(out: &mut Vec<u8>, cell: &[u8], theme: &Theme, header: bool) {
    let trimmed = trim_ascii(cell);
    let color = if header {
        Some(theme.muted)
    } else if trimmed.is_empty() {
        Some(theme.comment)
    } else if looks_numeric(trimmed) {
        Some(theme.number)
    } else if matches!(
        lower_ascii(trimmed).as_slice(),
        b"true" | b"false" | b"null" | b"nil" | b"na" | b"n/a"
    ) {
        Some(theme.keyword)
    } else if looks_like_delimited_link(trimmed) {
        Some(theme.string)
    } else {
        None
    };
    if let Some(color) = color {
        paint_bytes(out, color, cell, theme.reset);
    } else {
        out.extend_from_slice(cell);
    }
}

fn looks_like_delimited_link(cell: &[u8]) -> bool {
    cell.starts_with(b"http://")
        || cell.starts_with(b"https://")
        || (cell.contains(&b'.')
            && !cell.contains(&b' ')
            && cell.iter().all(|byte| {
                byte.is_ascii_alphanumeric() || b"-._~:/?#[]@!$&'()*+;=".contains(byte)
            }))
}

/// Color one delimiter-separated row without changing its layout.
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

pub(crate) fn split_unquoted(
    content: &[u8],
    delimiter: u8,
    csv_quotes: bool,
) -> Option<Vec<(usize, usize)>> {
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

pub(crate) fn lower_ascii(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(u8::to_ascii_lowercase).collect()
}

fn paint_sql(out: &mut Vec<u8>, color: &str, bytes: &[u8], reset: &str) {
    out.extend_from_slice(color.as_bytes());
    out.extend_from_slice(bytes);
    out.extend_from_slice(reset.as_bytes());
}

pub(crate) fn find_sql_block_comment_end(bytes: &[u8]) -> Option<usize> {
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
