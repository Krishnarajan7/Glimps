//! HTML detector + structural re-indenter — GLIMPS's second formatter.
//!
//! Goal (GLIMPS-PLAN §4 / the long-HTML pain point): turn a single wall of HTML
//! into an indented, readable tree. This is NOT a DOM/validator; it is a
//! tolerant tokenizer + indenter, because real-world HTML (a `curl`'d page) is
//! messy — unclosed `<p>`/`<li>`, void elements without a slash, etc.
//!
//! Safety:
//! * [`detect`] is high-precision: first non-ws `<`, last non-ws `>`, and a clear
//!   HTML signal (`</`, `/>`, or `<!`).
//! * [`try_format`] returns `None` (→ pass-through) on invalid UTF-8 or any
//!   unterminated construct (a `<` with no `>`, an open comment, an unclosed
//!   raw-text element). It never drops text or tag bytes — it only changes the
//!   surrounding whitespace — and never panics.
//! * Content inside raw-text elements (`script`, `style`, `pre`, `textarea`,
//!   `title`) is preserved verbatim, so we don't mangle preformatted or
//!   code/style content.

use super::theme::Theme;

/// Elements that never have children and so never increase depth.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Elements whose content must be preserved verbatim (not tokenized/reflowed).
const RAW_ELEMENTS: &[&str] = &["script", "style", "pre", "textarea", "title"];

/// High-precision check that `bytes` looks like an HTML document/fragment.
pub fn detect(bytes: &[u8]) -> bool {
    let t = trim_ascii_ws(bytes);
    if t.first() != Some(&b'<') || t.last() != Some(&b'>') {
        return false;
    }
    // The opening `<` must begin a real tag/markup construct, not prose like
    // "<3 you </3>": the next byte must be a tag-name letter, `!`, `/`, or `?`.
    match t.get(1) {
        Some(c) if c.is_ascii_alphabetic() || matches!(c, b'!' | b'/' | b'?') => {}
        _ => return false,
    }
    t.starts_with(b"<!") || contains_sub(t, b"</") || contains_sub(t, b"/>")
}

/// Re-indent `bytes` as HTML, returning `Some` only when it both [`detect`]s and
/// tokenizes cleanly. `None` means "pass the bytes through unchanged".
pub fn try_format(bytes: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if !detect(bytes) {
        return None;
    }
    // Tag/text splitting works on bytes, but raw-text capture and rendering read
    // text as UTF-8; reject non-UTF-8 outright (HTML output is text).
    if std::str::from_utf8(bytes).is_err() {
        return None;
    }
    let tokens = tokenize(bytes)?;
    let mut out = String::with_capacity(bytes.len() + bytes.len() / 4);
    render(&tokens, bytes, theme, &mut out);
    if out.ends_with('\n') {
        out.pop();
    }
    Some(out.into_bytes())
}

/// Registry entry for the HTML formatter. See [`super::BufferedFormatter`].
pub struct Html;

impl super::BufferedFormatter for Html {
    fn could_start(&self, head: &[u8]) -> bool {
        head.first() == Some(&b'<')
    }
    fn try_format(&self, bytes: &[u8], theme: &Theme) -> Option<Vec<u8>> {
        try_format(bytes, theme)
    }
    fn label(&self) -> &'static str {
        "HTML"
    }
    fn needs_crlf(&self) -> bool {
        true // re-indents content with bare `\n`
    }
}

// ---- tokenizer ------------------------------------------------------------

#[derive(Debug)]
enum Token {
    Text(usize, usize),
    Comment(usize, usize),
    Decl(usize, usize),
    Open {
        start: usize,
        end: usize,
        name: String,
        self_close: bool,
    },
    Close {
        start: usize,
        end: usize,
        name: String,
    },
    /// A raw-text element: its open tag, verbatim inner content, and close tag.
    Raw {
        open: (usize, usize),
        inner: (usize, usize),
        close: (usize, usize),
    },
}

fn tokenize(bytes: &[u8]) -> Option<Vec<Token>> {
    let n = bytes.len();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < n {
        if bytes[i] != b'<' {
            let start = i;
            while i < n && bytes[i] != b'<' {
                i += 1;
            }
            toks.push(Token::Text(start, i));
            continue;
        }

        if bytes[i..].starts_with(b"<!--") {
            // Comment up to and including `-->`.
            let close = find_sub(bytes, i + 4, b"-->")?;
            let end = close + 3;
            toks.push(Token::Comment(i, end));
            i = end;
        } else if bytes[i..].starts_with(b"<!") || bytes[i..].starts_with(b"<?") {
            let end = find_byte(bytes, i, b'>')? + 1;
            toks.push(Token::Decl(i, end));
            i = end;
        } else if bytes.get(i + 1) == Some(&b'/') {
            let end = scan_tag_end(bytes, i)?;
            let name = tag_name(&bytes[i + 1..end - 1]);
            toks.push(Token::Close {
                start: i,
                end,
                name,
            });
            i = end;
        } else {
            let end = scan_tag_end(bytes, i)?;
            let self_close = end >= 2 && bytes[end - 2] == b'/';
            let name = tag_name(&bytes[i + 1..end - 1]);
            if !self_close && RAW_ELEMENTS.contains(&name.as_str()) {
                let (inner_start, close_start, close_end) = find_raw_close(bytes, end, &name)?;
                toks.push(Token::Raw {
                    open: (i, end),
                    inner: (inner_start, close_start),
                    close: (close_start, close_end),
                });
                i = close_end;
            } else {
                toks.push(Token::Open {
                    start: i,
                    end,
                    name,
                    self_close,
                });
                i = end;
            }
        }
    }
    Some(toks)
}

/// Scan from a `<` at `start` to the matching unquoted `>`; returns the index
/// just past it. Honors `'`/`"` so a `>` inside an attribute value is skipped.
fn scan_tag_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut quote = 0u8;
    let mut j = start + 1;
    while j < bytes.len() {
        let c = bytes[j];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'>' {
            return Some(j + 1);
        }
        j += 1;
    }
    None
}

/// Lowercased element name from a tag's inner bytes (between `<`/`</` and `>`).
fn tag_name(inner: &[u8]) -> String {
    let mut name = Vec::new();
    for &c in inner {
        if c == b'/' {
            if name.is_empty() {
                continue; // leading slash of a close tag
            }
            break;
        }
        if c.is_ascii_whitespace() || c == b'>' {
            break;
        }
        name.push(c.to_ascii_lowercase());
    }
    String::from_utf8_lossy(&name).into_owned()
}

/// Find `</name ...>` (case-insensitive) at/after `from`. Returns
/// `(inner_start, close_start, close_end)`.
fn find_raw_close(bytes: &[u8], from: usize, name: &str) -> Option<(usize, usize, usize)> {
    let needle = format!("</{name}");
    let nb = needle.as_bytes();
    let mut j = from;
    while j + nb.len() <= bytes.len() {
        if bytes[j..j + nb.len()].eq_ignore_ascii_case(nb) {
            // The char after the name must end the name (`>`, `/`, or space) so
            // `</script>` matches but `</scripted>` does not.
            let after = bytes.get(j + nb.len());
            let ends = matches!(after, Some(b'>') | Some(b'/'))
                || after.is_some_and(|c| c.is_ascii_whitespace());
            if ends {
                let close_end = scan_tag_end(bytes, j)?;
                return Some((from, j, close_end));
            }
        }
        j += 1;
    }
    None
}

// ---- renderer -------------------------------------------------------------

fn render(tokens: &[Token], bytes: &[u8], theme: &Theme, out: &mut String) {
    // The stack of currently-open element names; its length is the indent depth.
    let mut stack: Vec<&str> = Vec::new();
    for tok in tokens {
        match tok {
            Token::Text(a, b) => {
                let collapsed = collapse_ws(&bytes[*a..*b]);
                if !collapsed.is_empty() {
                    line(out, stack.len(), "", &collapsed, "");
                }
            }
            Token::Comment(a, b) | Token::Decl(a, b) => {
                line(
                    out,
                    stack.len(),
                    theme.comment,
                    &slice(bytes, *a, *b),
                    theme.reset,
                );
            }
            Token::Open {
                start,
                end,
                name,
                self_close,
            } => {
                line(
                    out,
                    stack.len(),
                    theme.tag,
                    &slice(bytes, *start, *end),
                    theme.reset,
                );
                if !self_close && !VOID_ELEMENTS.contains(&name.as_str()) {
                    stack.push(name);
                }
            }
            Token::Close { start, end, name } => {
                // Tolerant close: if the name is open somewhere, drop down to it
                // (auto-closing any unclosed children); otherwise leave depth.
                if let Some(pos) = stack.iter().rposition(|n| *n == name.as_str()) {
                    stack.truncate(pos);
                }
                line(
                    out,
                    stack.len(),
                    theme.tag,
                    &slice(bytes, *start, *end),
                    theme.reset,
                );
            }
            Token::Raw { open, inner, close } => {
                let depth = stack.len();
                line(
                    out,
                    depth,
                    theme.tag,
                    &slice(bytes, open.0, open.1),
                    theme.reset,
                );
                let inner_str = slice(bytes, inner.0, inner.1);
                if inner_str.contains('\n') {
                    // Multi-line (pre/script/style): preserve exactly.
                    out.push_str(&inner_str);
                    if !inner_str.ends_with('\n') {
                        out.push('\n');
                    }
                } else {
                    let t = inner_str.trim();
                    if !t.is_empty() {
                        line(out, depth + 1, "", t, "");
                    }
                }
                line(
                    out,
                    depth,
                    theme.tag,
                    &slice(bytes, close.0, close.1),
                    theme.reset,
                );
            }
        }
    }
}

/// Emit one indented, optionally colored line.
fn line(out: &mut String, depth: usize, color: &str, text: &str, reset: &str) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    out.push_str(color);
    out.push_str(text);
    out.push_str(reset);
    out.push('\n');
}

fn slice(bytes: &[u8], a: usize, b: usize) -> String {
    String::from_utf8_lossy(&bytes[a..b]).into_owned()
}

/// Collapse runs of **ASCII** whitespace to single spaces and trim the ends.
/// Deliberately not `split_whitespace` (which splits on Unicode whitespace):
/// non-ASCII spaces like `U+00A0` (NBSP) are content-significant in HTML and must
/// be preserved verbatim, not rewritten to an ASCII space.
fn collapse_ws(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .split(|c: char| c.is_ascii_whitespace())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

// ---- small byte helpers ---------------------------------------------------

fn trim_ascii_ws(mut bytes: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = bytes {
        if first.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    while let [rest @ .., last] = bytes {
        if last.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    bytes
}

fn contains_sub(haystack: &[u8], needle: &[u8]) -> bool {
    find_sub(haystack, 0, needle).is_some()
}

fn find_sub(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || from > haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn find_byte(haystack: &[u8], from: usize, byte: u8) -> Option<usize> {
    haystack[from..]
        .iter()
        .position(|&b| b == byte)
        .map(|p| p + from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(bytes: &[u8]) -> Vec<u8> {
        try_format(bytes, &Theme::plain()).unwrap_or_else(|| bytes.to_vec())
    }

    #[test]
    fn golden_nested_document() {
        let input = include_bytes!("../../tests/corpus/html/page.html");
        let expected = include_bytes!("../../tests/corpus/html/page.expected");
        assert_eq!(plain(input), expected);
    }

    #[test]
    fn detects_real_html_but_not_lookalikes() {
        assert!(detect(b"<p>hi</p>"));
        assert!(detect(b"  <br/>\n"));
        assert!(detect(b"<!DOCTYPE html>"));
        // Not HTML:
        assert!(!detect(b"<stdin>")); // no </, />, or <!
        assert!(!detect(b"a < b > c")); // doesn't start with <
        assert!(!detect("<3 you </3>".as_bytes())); // prose: `<` not followed by a tag char
        assert!(!detect(b"<<<conflict>>>")); // not a tag opener
        assert!(!detect(b"{\"a\":1}"));
        assert!(!detect(b"plain text"));
        assert!(!detect(b""));
    }

    #[test]
    fn non_ascii_whitespace_is_preserved() {
        // NBSP (U+00A0) inside text is content; it must not be collapsed to a
        // plain space. ASCII runs around it still collapse.
        let input = "<p>a\u{00a0}b   c</p>".as_bytes();
        let out = String::from_utf8(plain(input)).unwrap();
        assert_eq!(out, "<p>\n  a\u{00a0}b c\n</p>");
    }

    #[test]
    fn doctype_and_declaration_render() {
        let out = String::from_utf8(plain(b"<!DOCTYPE html><html></html>")).unwrap();
        assert_eq!(out, "<!DOCTYPE html>\n<html>\n</html>");
    }

    #[test]
    fn unterminated_doctype_passes_through() {
        assert!(try_format(b"<!DOCTYPE html", &Theme::plain()).is_none());
    }

    #[test]
    fn close_lookalike_inside_script_does_not_end_it_early() {
        // A `</scripted>`-style lookalike inside a script must not close it; only
        // a real `</script>` does.
        let out = String::from_utf8(plain(b"<script>var s = '</scripted>'</script>")).unwrap();
        assert_eq!(out, "<script>\n  var s = '</scripted>'\n</script>");
    }

    #[test]
    fn void_elements_do_not_indent_following_content() {
        let out = plain(b"<div><br><span>x</span></div>");
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "<div>\n  <br>\n  <span>\n    x\n  </span>\n</div>"
        );
    }

    #[test]
    fn unclosed_tags_are_auto_closed_leniently() {
        // <p> never closed before </div>: the </div> drops back out cleanly.
        let out = plain(b"<div><p>hi</div>");
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "<div>\n  <p>\n    hi\n</div>"
        );
    }

    #[test]
    fn raw_text_elements_are_preserved_verbatim() {
        let out = plain(b"<style>.a {  color : red ; }</style>");
        // The CSS inner is not reflowed/collapsed (single line -> trimmed at +1).
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "<style>\n  .a {  color : red ; }\n</style>"
        );
    }

    #[test]
    fn script_with_angle_brackets_is_not_mistokenized() {
        let out = plain(b"<script>if (a<b && c>d) {}</script>");
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "<script>\n  if (a<b && c>d) {}\n</script>"
        );
    }

    #[test]
    fn attribute_with_gt_is_handled() {
        let out = plain(br#"<a title="x>y">z</a>"#);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "<a title=\"x>y\">\n  z\n</a>"
        );
    }

    #[test]
    fn colored_output_wraps_tags_and_is_valid_utf8() {
        let out = try_format(b"<p>hi</p>", &Theme::default_colored()).unwrap();
        let s = std::str::from_utf8(&out).unwrap();
        assert!(s.contains("\x1b[34m")); // tag color
        assert!(s.contains("\x1b[0m"));
    }

    #[test]
    fn unterminated_constructs_pass_through() {
        // try_format returns None -> caller keeps the bytes.
        assert!(try_format(b"<div>oops", &Theme::plain()).is_none()); // unterminated tag
        assert!(try_format(b"<!-- open", &Theme::plain()).is_none()); // unterminated comment
        assert!(try_format(b"<script>x", &Theme::plain()).is_none()); // unclosed raw element
    }

    proptest::proptest! {
        /// `try_format` never panics, always yields valid UTF-8 when it returns
        /// `Some`, and `detect == false` implies pass-through (no output at all).
        #[test]
        fn prop_never_panics_and_utf8(bytes: Vec<u8>) {
            if let Some(out) = try_format(&bytes, &Theme::plain()) {
                proptest::prop_assert!(std::str::from_utf8(&out).is_ok());
            }
            if !detect(&bytes) {
                proptest::prop_assert!(try_format(&bytes, &Theme::plain()).is_none());
            }
        }

        /// Re-indenting is a fixed point: formatting already-formatted HTML twice
        /// yields the same bytes (no drift), exercised over generated markup.
        #[test]
        fn prop_idempotent(
            tags in proptest::collection::vec(
                proptest::sample::select(vec!["div", "span", "p", "ul", "li", "section"]),
                0..6,
            )
        ) {
            // Build a balanced nested document from the generated tag names.
            let mut html = String::new();
            for t in &tags {
                html.push_str(&format!("<{t}>"));
            }
            html.push_str("text");
            for t in tags.iter().rev() {
                html.push_str(&format!("</{t}>"));
            }
            let once = plain(html.as_bytes());
            let twice = plain(&once);
            proptest::prop_assert_eq!(once, twice);
        }
    }
}
