//! Delimited tables, database result tables, and SQL syntax views.

use super::super::theme::Theme;
use super::{paint_bytes, paint_whole, split_line, trim_ascii, trim_ascii_end, trim_ascii_start};

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
            theme.table_header
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

/// Per-command memory for [`colorize_sql_result_line`].
///
/// An interactive database shell (`mysql`, `psql`, `sqlite3`) is ONE command
/// from GLIMPS's point of view: every table the session prints lands inside a
/// single output run, so "is this the header row?" cannot be answered from a
/// line's position in the command's output. It is answered by structure — a
/// header is the first row after a box table's top rule, or after prose for
/// the borderless `psql`/`sqlite3` shapes — and by remembering that a box row
/// left its last cell open across lines (`SHOW CREATE TABLE`). The state is
/// a couple of bytes, reset at every command boundary, and only ever steers
/// *which colour* a line gets; it never withholds or reorders bytes.
#[derive(Debug, Default)]
pub struct SqlResultState {
    prev: SqlLineKind,
    /// Lines consumed inside an open multi-line cell; bounds the hold so a
    /// row that never closes cannot lex the rest of the session as SQL.
    open_lines: usize,
}

impl SqlResultState {
    /// Forget everything. Called when the command whose tables were being
    /// tracked ends, and when a line went out that this view never saw (a
    /// prompt released verbatim by a stall flush): an unclassified line
    /// breaks table continuity, so whatever follows starts from prose.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn enter_open_cell(&mut self, lexed: bool) {
        self.prev = SqlLineKind::OpenCell(lexed);
        self.open_lines = 0;
    }

    fn leave_open_cell(&mut self, next: SqlLineKind) {
        self.prev = next;
        self.open_lines = 0;
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum SqlLineKind {
    /// Prose, a status line, a blank line, or nothing yet.
    #[default]
    Other,
    /// A `+---+` rule not preceded by a row: the top edge of a box table.
    TopRule,
    /// A rule preceded by a row: the header/data divider or the bottom edge.
    InnerRule,
    Row,
    /// Inside a box-drawn row whose last cell continues on following lines.
    /// `true` when that cell opened with a SQL statement and is being lexed;
    /// `false` when it is some other multi-line value left uncoloured.
    OpenCell(bool),
}

/// Longest multi-line cell we follow before giving up on the row. A
/// `SHOW CREATE TABLE` body is dozens of lines; a stored routine may be a few
/// hundred. Past this, the row is assumed never to close.
const OPEN_CELL_CAP: usize = 1024;

/// How far into a line the `:` of `ERROR 1064 (42000): …` may sit.
const SQL_ERROR_HEAD_CAP: usize = 64;

/// Color common database CLI result tables (`psql`, `sqlite3`, `mysql`/MariaDB)
/// without reflowing columns. Borders and row-count/status lines are dimmed;
/// header cells are keyed; data cells reuse the CSV/TSV value palette; the
/// body of a multi-line SQL cell (`SHOW CREATE TABLE`) gets the query lexer;
/// `ERROR 1064 (42000):` prefixes are painted as errors.
///
/// `state` carries what the previous lines of the same command established
/// (see [`SqlResultState`]). Every line this view sees — coloured or
/// declined — updates it; a line the stream released verbatim without
/// consulting the view resets it (see `push_stream`).
pub fn colorize_sql_result_line(
    line: &[u8],
    theme: &Theme,
    state: &mut SqlResultState,
) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    let trimmed = trim_ascii(content);
    if let SqlLineKind::OpenCell(lexed) = state.prev {
        return continue_open_cell(content, ending, theme, state, lexed);
    }
    if trimmed.is_empty() {
        // A blank line inside a result is what a value containing a newline
        // produces; it neither confirms nor breaks the table, so the previous
        // classification stands and the next row is not re-armed as a header.
        return None;
    }
    if starts_with_repl_prompt(trimmed) {
        // A prompt or its continuation (`mysql> …`, `    -> …`, `sqlite> …`)
        // is the echo of what you TYPED, never result output — GLIMPS must not
        // recolour typed input. Without this, a typed line whose indentation
        // opens a two-space gap (`-> <spaces> col AS alias`) reads as a
        // whitespace-aligned table and gets cell colouring. Declining it also
        // breaks table continuity so the next real result starts fresh.
        state.prev = SqlLineKind::Other;
        return None;
    }
    if is_sql_table_rule(trimmed) {
        state.prev = if state.prev == SqlLineKind::Row {
            SqlLineKind::InnerRule
        } else if is_box_top_rule(trimmed) {
            SqlLineKind::TopRule
        } else {
            // A bare `-----` under prose is a separator, not a table edge;
            // it must not hand the next row the relaxed header check.
            SqlLineKind::Other
        };
        return Some(paint_whole(content, ending, theme.debug, theme.reset));
    }
    if is_sql_result_meta(trimmed) {
        state.prev = SqlLineKind::Other;
        return Some(paint_whole(content, ending, theme.debug, theme.reset));
    }
    if let Some(painted) = colorize_sql_error_line(content, ending, theme) {
        state.prev = SqlLineKind::Other;
        return Some(painted);
    }
    if let Some(painted) = colorize_status_report_line(content, ending, theme) {
        state.prev = SqlLineKind::Other;
        return Some(painted);
    }
    // The first row after a top rule (mysql) or after prose (psql/sqlite) is
    // the header; anything after the divider or another row is data.
    let header_hint = matches!(state.prev, SqlLineKind::Other | SqlLineKind::TopRule);
    // A row directly under a box table's top rule IS the header by
    // construction, so the cell-shape test relaxes: `desc` has a `Null`
    // column and `show tables` a single one, and neither may read as data.
    let under_top_rule = state.prev == SqlLineKind::TopRule;
    if content.contains(&b'|') {
        // A box row that does not close on this line (`| brands | CREATE TABLE
        // `brands` (`) continues its last cell below. Only box rows qualify,
        // recognised by their padded border — `|` in column 0 followed by
        // whitespace. A borderless psql row is indented, and a sqlite list
        // row with an empty first field (`|alice|1`) is unpadded; neither
        // ends with a pipe, and neither may open a phantom cell.
        let opens = matches!(content, [b'|', pad, ..] if pad.is_ascii_whitespace())
            && !trimmed.ends_with(b"|");
        let lex_last = opens && opens_sql_statement(last_pipe_cell(trimmed));
        let painted = colorize_pipe_table_line(
            content,
            ending,
            theme,
            header_hint && !opens,
            under_top_rule,
            lex_last,
        );
        match (&painted, opens) {
            (None, _) => state.prev = SqlLineKind::Other,
            (Some(_), true) => state.enter_open_cell(lex_last),
            (Some(_), false) => state.prev = SqlLineKind::Row,
        }
        return painted;
    }
    if content.contains(&b'\t') {
        let painted = colorize_delimited_line(
            line,
            theme,
            b'\t',
            header_hint && looks_sql_header_row(content, b'\t'),
        );
        state.prev = if painted.is_some() {
            SqlLineKind::Row
        } else {
            SqlLineKind::Other
        };
        return painted;
    }
    let Some(spans) = whitespace_table_spans(content).filter(|spans| spans.len() >= 2) else {
        state.prev = SqlLineKind::Other;
        return None;
    };
    let is_header = header_hint && looks_header_spans(content, &spans, false);
    state.prev = SqlLineKind::Row;
    Some(colorize_spanned_cells(
        content, ending, &spans, theme, is_header, false,
    ))
}

/// `status` in the mysql monitor (and sqlite's `.show`) prints a report, not
/// a table: a version banner, `Label:<tabs>value` lines, then one line of
/// `Label: n` pairs. Tab-separated, those lines would otherwise be keyed as
/// TSV data — label and value in the same colour — and the banner as a
/// header row. Here the label gets its own colour, the colon recedes, and the
/// value is painted by kind (number, path, text).
fn colorize_status_report_line(content: &[u8], ending: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if content.contains(&b'|') {
        return None;
    }
    if let Some(painted) = colorize_client_banner(content, ending, theme) {
        return Some(painted);
    }
    let cells = report_cells(content);
    let (first_start, first_end) = *cells.first()?;
    let first = &content[first_start..first_end];
    let mut out = Vec::with_capacity(content.len() + ending.len() + cells.len() * 24);
    if let Some(label) = first
        .strip_suffix(b":")
        .filter(|label| is_report_label(label))
    {
        // `Connection id:\t\t16` — one label, the value in the cells after it.
        out.extend_from_slice(&content[..first_start]);
        paint_bytes(&mut out, theme.label, label, theme.reset);
        paint_bytes(&mut out, theme.html_delim, b":", theme.reset);
        let mut cursor = first_end;
        for &(start, end) in &cells[1..] {
            out.extend_from_slice(&content[cursor..start]);
            paint_report_value(&content[start..end], theme, &mut out);
            cursor = end;
        }
        out.extend_from_slice(&content[cursor..]);
        out.extend_from_slice(ending);
        return Some(out);
    }
    // `Threads: 3  Questions: 68  Slow queries: 0`: every cell is a pair. One
    // cell alone is a sentence (`Note: …`), not a report.
    if cells.len() < 2 {
        return None;
    }
    let pair = |cell: &[u8]| -> Option<usize> {
        let colon = cell.windows(2).position(|pair| pair == b": ")?;
        let value = trim_ascii_start(&cell[colon + 2..]);
        (is_report_label(&cell[..colon]) && !value.is_empty()).then_some(colon)
    };
    if !cells
        .iter()
        .all(|&(start, end)| pair(&content[start..end]).is_some())
    {
        return None;
    }
    let mut cursor = 0;
    for &(start, end) in &cells {
        let cell = &content[start..end];
        let colon = pair(cell)?;
        let gap = cell[colon + 1..].len() - trim_ascii_start(&cell[colon + 1..]).len();
        out.extend_from_slice(&content[cursor..start]);
        paint_bytes(&mut out, theme.label, &cell[..colon], theme.reset);
        paint_bytes(&mut out, theme.html_delim, b":", theme.reset);
        out.extend_from_slice(&cell[colon + 1..colon + 1 + gap]);
        paint_report_value(&cell[colon + 1 + gap..], theme, &mut out);
        cursor = end;
    }
    out.extend_from_slice(&content[cursor..]);
    out.extend_from_slice(ending);
    Some(out)
}

/// `mysql  Ver 9.5.0 for macos26.1 on arm64 (Homebrew)`: the client's own
/// banner. The name takes the label colour and the rest reads like
/// `mysql --version` — version number gold, vendor note muted, connective
/// words plain.
fn colorize_client_banner(content: &[u8], ending: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let lead = content.len() - trim_ascii_start(content).len();
    let name_end = content[lead..]
        .iter()
        .position(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
        .map(|len| lead + len)?;
    if name_end == lead {
        return None;
    }
    let rest = trim_ascii_start(&content[name_end..]);
    if rest.len() == content[name_end..].len() || !rest.starts_with(b"Ver ") {
        return None;
    }
    let tail = [&content[name_end..], ending].concat();
    let painted_tail = super::colorize_version_line(&tail, theme, false)?;
    let mut out = Vec::with_capacity(content.len() + ending.len() + 32);
    out.extend_from_slice(&content[..lead]);
    paint_bytes(&mut out, theme.label, &content[lead..name_end], theme.reset);
    out.extend_from_slice(&painted_tail);
    Some(out)
}

/// Cells of a report line: runs of text separated by a tab or by two or
/// more spaces. A single space stays inside a cell (`Not in use`,
/// `Slow queries: 0`).
fn report_cells(content: &[u8]) -> Vec<(usize, usize)> {
    let mut cells = Vec::new();
    let mut start = None;
    let mut i = 0;
    while i < content.len() {
        if content[i].is_ascii_whitespace() {
            let gap_start = i;
            let mut has_tab = false;
            while i < content.len() && content[i].is_ascii_whitespace() {
                has_tab |= content[i] == b'\t';
                i += 1;
            }
            if has_tab || i - gap_start >= 2 {
                if let Some(s) = start.take() {
                    cells.push((s, gap_start));
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
        cells.push((s, content.len()));
    }
    cells
}

/// `Connection id`, `Conn.  characterset`, `Queries per second avg`: a short
/// run of words starting with a letter. A value or SQL never fits.
fn is_report_label(label: &[u8]) -> bool {
    let label = trim_ascii(label);
    !label.is_empty()
        && label.len() <= 40
        && label[0].is_ascii_alphabetic()
        && label.iter().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b' ' | b'.' | b'/' | b'(' | b')' | b'-' | b'_')
        })
}

fn paint_report_value(value: &[u8], theme: &Theme, out: &mut Vec<u8>) {
    let color = if looks_numeric(value) {
        theme.number
    } else if value.starts_with(b"/") || value.starts_with(b"~/") {
        theme.path
    } else {
        theme.string
    };
    paint_bytes(out, color, value, theme.reset);
}

/// `+-----+-----+`: the top or bottom edge of a box-drawn table. A rule
/// without the corner `+` (`-----`) is psql's header divider or plain prose.
/// Whether a line begins with an interactive database-shell prompt or its
/// continuation, marking it as echoed input rather than result output. Covers
/// mysql/MariaDB (`mysql>`, `mariadb>`, `->`), sqlite (`sqlite>`, `...>`), and
/// psql (`db=>`, `db->`, `db(>`, `db*>`). Result rows never begin this way.
fn starts_with_repl_prompt(trimmed: &[u8]) -> bool {
    let end = trimmed
        .iter()
        .position(u8::is_ascii_whitespace)
        .unwrap_or(trimmed.len());
    let Some(head) = trimmed[..end].strip_suffix(b">") else {
        return false;
    };
    // mysql/sqlite continuation arrows.
    if head == b"-" || head == b"..." {
        return true;
    }
    // A prompt name, optionally followed by one of psql's mode markers.
    let name = match head.last() {
        Some(b'=' | b'-' | b'(' | b'*') => &head[..head.len() - 1],
        _ => head,
    };
    !name.is_empty()
        && name.len() <= 20
        && name[0].is_ascii_alphabetic()
        && name
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn is_box_top_rule(trimmed: &[u8]) -> bool {
    trimmed.starts_with(b"+") && trimmed.ends_with(b"+")
}

/// The bytes after the last `|` of a box row: the cell that stays open.
fn last_pipe_cell(trimmed: &[u8]) -> &[u8] {
    match trimmed.iter().rposition(|&b| b == b'|') {
        Some(pipe) => trim_ascii(&trimmed[pipe + 1..]),
        None => trimmed,
    }
}

/// Whether a cell opens with a SQL statement worth lexing across lines. Kept
/// to statement heads: a multi-line JSON or text value must stay uncoloured
/// rather than have its `in`/`on`/`as` painted as keywords.
fn opens_sql_statement(cell: &[u8]) -> bool {
    const HEADS: &[&[u8]] = &[
        b"CREATE ", b"SELECT ", b"INSERT ", b"UPDATE ", b"DELETE ", b"ALTER ", b"WITH ", b"BEGIN",
    ];
    let upper = upper_ascii(cell);
    HEADS.iter().any(|head| upper.starts_with(head))
}

/// One line inside a box row's open multi-line cell.
///
/// A SQL body is lexed like a `.sql` file; the closing ` |` (when the cell
/// ends on this line) is dimmed as a border. The border is mysql's padded
/// form — whitespace then `|` at the end of the line — so a `|` operator
/// inside the SQL (`(1|2)`) does not close the cell early. A rule line closes
/// any open cell on sight — a real cell never contains one — so a row we
/// misjudged as open cannot swallow the rest of the table. The line cap gives
/// up on a row that never closes, and the line that trips it is left plain.
fn continue_open_cell(
    content: &[u8],
    ending: &[u8],
    theme: &Theme,
    state: &mut SqlResultState,
    lexed: bool,
) -> Option<Vec<u8>> {
    let trimmed = trim_ascii(content);
    if is_sql_table_rule(trimmed) {
        state.leave_open_cell(SqlLineKind::InnerRule);
        return Some(paint_whole(content, ending, theme.debug, theme.reset));
    }
    let body = trim_ascii_end(content);
    let closes = matches!(body, [.., gap, b'|'] if gap.is_ascii_whitespace());
    state.open_lines = state.open_lines.saturating_add(1);
    if closes {
        state.leave_open_cell(SqlLineKind::Row);
    } else if state.open_lines > OPEN_CELL_CAP {
        state.leave_open_cell(SqlLineKind::Other);
        return None;
    }
    if !lexed || trimmed.is_empty() {
        return None;
    }
    // `closes` guarantees `body` ends with the border byte, so the body is
    // everything before it; otherwise the whole line is SQL.
    let body_end = if closes {
        body.len().saturating_sub(1)
    } else {
        content.len()
    };
    let mut out = Vec::with_capacity(content.len() + ending.len() + 64);
    lex_sql(&content[..body_end], theme, &mut out);
    if body_end < content.len() {
        paint_bytes(
            &mut out,
            theme.html_delim,
            &content[body_end..],
            theme.reset,
        );
    }
    out.extend_from_slice(ending);
    Some(out)
}

/// `ERROR 1064 (42000): You have an error…` / `ERROR 1146 (42S02) at line 3: …`
/// — paint the code, SQLSTATE and position red, leave the message readable.
fn colorize_sql_error_line(content: &[u8], ending: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    let lead = content.len() - trim_ascii_start(content).len();
    let trimmed = &content[lead..];
    let after_error = trimmed.strip_prefix(b"ERROR ")?;
    if !after_error.first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let colon = trimmed
        .iter()
        .take(SQL_ERROR_HEAD_CAP)
        .position(|&b| b == b':')?;
    let head = &trimmed[..colon];
    if !head
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || matches!(b, b' ' | b'(' | b')'))
    {
        return None;
    }
    let mut out = Vec::with_capacity(content.len() + ending.len() + 16);
    out.extend_from_slice(&content[..lead]);
    paint_bytes(&mut out, theme.error, &trimmed[..=colon], theme.reset);
    out.extend_from_slice(&trimmed[colon + 1..]);
    out.extend_from_slice(ending);
    Some(out)
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
    if !lex_sql(content, theme, &mut out) {
        return None;
    }
    out.extend_from_slice(ending);
    Some(out)
}

/// Append `content` to `out` with SQL syntax colour. Returns whether anything
/// was painted; either way every byte of `content` is appended in order.
fn lex_sql(content: &[u8], theme: &Theme, out: &mut Vec<u8>) -> bool {
    let mut i = 0;
    let mut colored_any = false;
    while i < content.len() {
        let b = content[i];
        if i + 1 < content.len() && content[i] == b'-' && content[i + 1] == b'-' {
            paint_sql(out, theme.comment, &content[i..], theme.reset);
            colored_any = true;
            i = content.len();
        } else if i + 1 < content.len() && content[i] == b'/' && content[i + 1] == b'*' {
            let end = find_sql_block_comment_end(&content[i + 2..])
                .map(|rel| i + 4 + rel)
                .unwrap_or(content.len());
            paint_sql(out, theme.comment, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if b == b'\'' || b == b'"' {
            let end = sql_quoted_end(content, i, b);
            paint_sql(out, theme.string, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if b == b'`' {
            // MySQL identifier quoting: `id`, `my table`. Painted as a key so
            // column names stand apart from the types and keywords around them.
            let end = sql_quoted_end(content, i, b);
            paint_sql(out, theme.key, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if b.is_ascii_digit()
            || (matches!(b, b'-' | b'+') && content.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            let end = sql_number_end(content, i);
            paint_sql(out, theme.number, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if is_sql_ident_start(b) {
            let end = sql_ident_end(content, i);
            let word = &content[i..end];
            if is_sql_keyword(word) {
                paint_sql(out, theme.keyword, word, theme.reset);
                colored_any = true;
            } else {
                out.extend_from_slice(word);
            }
            i = end;
        } else if is_sql_punctuation(b) {
            paint_sql(out, theme.html_delim, &content[i..i + 1], theme.reset);
            colored_any = true;
            i += 1;
        } else {
            out.push(b);
            i += 1;
        }
    }
    colored_any
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

/// `relaxed_header`: structural evidence (a top rule right above) already
/// says this is the header, so only the cell-shape sanity check remains.
/// `lex_last`: the final cell opens a multi-line SQL statement and gets the
/// query lexer instead of the value palette.
fn colorize_pipe_table_line(
    content: &[u8],
    ending: &[u8],
    theme: &Theme,
    header_hint: bool,
    relaxed_header: bool,
    lex_last: bool,
) -> Option<Vec<u8>> {
    let spans = split_unquoted(content, b'|', false)?;
    if spans.len() < 2 {
        return None;
    }
    let is_header = header_hint && looks_header_spans(content, &spans, relaxed_header);
    Some(colorize_spanned_cells(
        content, ending, &spans, theme, is_header, lex_last,
    ))
}

fn colorize_spanned_cells(
    content: &[u8],
    ending: &[u8],
    spans: &[(usize, usize)],
    theme: &Theme,
    is_header: bool,
    lex_last: bool,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(content.len() + ending.len() + spans.len() * 12);
    let mut cursor = 0;
    for (index, &(start, end)) in spans.iter().enumerate() {
        if cursor < start {
            out.extend_from_slice(theme.html_delim.as_bytes());
            out.extend_from_slice(&content[cursor..start]);
            out.extend_from_slice(theme.reset.as_bytes());
        }
        let cell = &content[start..end];
        if cell.is_empty() {
            out.extend_from_slice(cell);
        } else if lex_last && index + 1 == spans.len() {
            lex_sql(cell, theme, &mut out);
        } else {
            let color = if is_header {
                theme.table_header
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
    delimited_spans(content, delimiter)
        .is_some_and(|spans| looks_header_spans(content, &spans, false))
}

/// Whether a row's cells all read as column names. Without structural
/// evidence (`relaxed` false) this is the only thing standing between a data
/// row and the header colour, so it wants two named columns and no
/// value-shaped words. With a box table's top rule directly above (`relaxed`
/// true) the position already decided: one column is enough (`show tables`),
/// and the mixed-case `Null` that `desc` names its column is accepted while
/// the value words (`NULL`, `true`, `t`, …) still mark a headerless
/// `mysql -N` first row as data. Known residual: a `mysql -N` first row of
/// plain words (`| alice | admin |`) is indistinguishable from a header and
/// is keyed; the client's own output gives nothing to tell them apart.
fn looks_header_spans(content: &[u8], spans: &[(usize, usize)], relaxed: bool) -> bool {
    let mut meaningful = 0;
    for &(start, end) in spans {
        let cell = trim_ascii(&content[start..end]);
        if cell.is_empty() {
            continue;
        }
        meaningful += 1;
        if !looks_like_header_cell(cell, relaxed) {
            return false;
        }
    }
    meaningful >= if relaxed { 1 } else { 2 }
}

fn looks_like_header_cell(cell: &[u8], relaxed: bool) -> bool {
    if cell.is_empty() || looks_numeric(cell) || !cell.iter().any(u8::is_ascii_alphabetic) {
        return false;
    }
    // A bare value word is data, not a header — even directly under a top rule
    // (a headerless `mysql -N` first row). `desc` names a column `Null`, so
    // that one mixed-case spelling is allowed once position says header.
    let value_word = matches!(
        lower_ascii(cell).as_slice(),
        b"true" | b"false" | b"t" | b"f" | b"null" | b"nil"
    );
    if value_word && !(relaxed && cell == b"Null") {
        return false;
    }
    // Under a box table's top rule the position already proves this row is the
    // header, so a column EXPRESSION header (`coalesce(a,'x')`, `count(*)`,
    // `emp_name AS 'Full, Name'`) with commas, quotes or stars is accepted. A
    // borderless table has no such proof, so there the cell must still look
    // like a plain column identifier.
    relaxed
        || cell.iter().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(b, b'_' | b'-' | b'.' | b' ' | b'/' | b'(' | b')' | b'%')
        })
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
    if is_mysql_status_line(trimmed) {
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

/// The mysql/MariaDB client's own chatter after a statement: `13 rows in set
/// (0.002 sec)`, `1 row in set`, `Empty set (0.00 sec)`, `Database changed`,
/// `Records: 3  Duplicates: 0  Warnings: 0`, `Rows matched: 1  Changed: 1
/// Warnings: 0`, and the `Bye` on exit. Dimmed like psql's `(1 row)`.
fn is_mysql_status_line(trimmed: &[u8]) -> bool {
    let digits = trimmed.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits > 0 {
        let rest = &trimmed[digits..];
        if rest.starts_with(b" row in set") || rest.starts_with(b" rows in set") {
            return true;
        }
    }
    trimmed.starts_with(b"Empty set")
        || trimmed == b"Database changed"
        || trimmed == b"Bye"
        || trimmed.starts_with(b"Records: ")
        || trimmed.starts_with(b"Rows matched: ")
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
