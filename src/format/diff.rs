//! Unified-diff colorizer (a buffered formatter, like JSON/HTML).
//!
//! Detection is anchored on a real hunk header — `@@ -<n>[,<n>] +<n>[,<n>] @@` —
//! so a stray `+`/`-` line (a Markdown list, an `ls -l` long listing, a CLI
//! `--flag`) can never be mistaken for a diff (charter invariant #2: a false
//! positive that mangles normal output is the worst failure). With no hunk header
//! we decline (`None`) and the bytes pass through untouched.
//!
//! Coloring is per line by role: additions green, deletions red, hunk headers
//! cyan, file/meta headers dim, context lines unchanged. Byte-safe: only color
//! escapes are inserted, so with the plain theme the output is byte-identical to
//! the input (the property test pins this).

use super::theme::Theme;

/// Whether `bytes` (already positioned at the run's first non-whitespace byte)
/// begins like a unified diff, used to decide whether to *buffer* the run as a
/// diff candidate. Confirmation still requires a hunk header at format time.
///
/// `diff --git ` is unambiguous. A bare `--- ` is only treated as a diff start if
/// a `+++ ` old/new-file pair follows, so ordinary `--- banner ---` text (or a log
/// that opens with `--- `) is NOT needlessly buffered out of the streaming path.
pub fn looks_like_start(bytes: &[u8]) -> bool {
    bytes.starts_with(b"diff --git ") || (bytes.starts_with(b"--- ") && contains(bytes, b"\n+++ "))
}

/// Whether `needle` occurs anywhere in `haystack`.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Registry entry for the diff formatter. See [`super::BufferedFormatter`].
pub struct Diff;

impl super::BufferedFormatter for Diff {
    fn could_start(&self, head: &[u8]) -> bool {
        looks_like_start(head)
    }
    fn try_format(&self, bytes: &[u8], theme: &Theme) -> Option<Vec<u8>> {
        try_format(bytes, theme)
    }
    fn label(&self) -> &'static str {
        "DIFF"
    }
    fn needs_crlf(&self) -> bool {
        false // preserves the user's own line endings — never re-CRLF it
    }
}

/// Try to format `bytes` as a unified diff. Returns the colored bytes if the
/// input contains at least one valid hunk header, else `None` (decline).
pub fn try_format(bytes: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if !bytes.split(|&b| b == b'\n').any(is_hunk_header) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() + 64);
    for line in lines_with_endings(bytes) {
        let (content, ending) = split_ending(line);
        match line_color(content, theme) {
            Some(color) => {
                out.extend_from_slice(color.as_bytes());
                out.extend_from_slice(content);
                out.extend_from_slice(theme.reset.as_bytes());
                out.extend_from_slice(ending);
            }
            // Context line (or anything unstyled): verbatim.
            None => out.extend_from_slice(line),
        }
    }
    Some(out)
}

/// The color for one diff line's `content` (the line without its `\n`/`\r\n`), or
/// `None` to leave it unstyled. Order matters: the 3-char file markers `+++`/`---`
/// are checked before the 1-char add/remove markers.
fn line_color(content: &[u8], theme: &Theme) -> Option<&'static str> {
    if content.is_empty() {
        return None;
    }
    if content.starts_with(b"@@ ") {
        return Some(theme.key); // hunk header — cyan
    }
    if content.starts_with(b"+++ ") || content.starts_with(b"--- ") {
        return Some(theme.comment); // file header — dim
    }
    if content.starts_with(b"diff ")
        || content.starts_with(b"index ")
        || content.starts_with(b"new file")
        || content.starts_with(b"deleted file")
        || content.starts_with(b"old mode")
        || content.starts_with(b"new mode")
        || content.starts_with(b"similarity ")
        || content.starts_with(b"rename ")
        || content.starts_with(b"copy ")
    {
        return Some(theme.comment); // meta header — dim
    }
    match content[0] {
        b'+' => Some(theme.info),  // addition — green
        b'-' => Some(theme.error), // deletion — red
        _ => None,                 // context line — unchanged
    }
}

/// Whether `line` (a single line, no trailing `\n`) is a unified-diff hunk header:
/// `@@ -<digits>[,<digits>] +<digits>[,<digits>] @@` (optionally followed by a
/// section heading). Tolerant of a trailing `\r`.
fn is_hunk_header(line: &[u8]) -> bool {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let Some(rest) = line.strip_prefix(b"@@ -") else {
        return false;
    };
    // -<n>[,<n>] then " +"
    let Some(rest) = consume_range(rest) else {
        return false;
    };
    let Some(rest) = rest.strip_prefix(b" +") else {
        return false;
    };
    let Some(rest) = consume_range(rest) else {
        return false;
    };
    rest.starts_with(b" @@")
}

/// Consume `<digits>` or `<digits>,<digits>` from the front, returning the rest.
/// `None` if there isn't at least one leading digit.
fn consume_range(bytes: &[u8]) -> Option<&[u8]> {
    let rest = consume_digits(bytes)?;
    match rest.strip_prefix(b",") {
        Some(after_comma) => consume_digits(after_comma),
        None => Some(rest),
    }
}

/// Consume one or more ASCII digits, returning the rest; `None` if none present.
fn consume_digits(bytes: &[u8]) -> Option<&[u8]> {
    let n = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if n == 0 {
        None
    } else {
        Some(&bytes[n..])
    }
}

/// Split a line into its content and trailing line ending (`""`, `"\n"`, or
/// `"\r\n"`), so coloring wraps only the content.
fn split_ending(line: &[u8]) -> (&[u8], &[u8]) {
    match line.strip_suffix(b"\n") {
        Some(rest) => {
            let content_len = rest.strip_suffix(b"\r").map_or(rest.len(), |r| r.len());
            line.split_at(content_len)
        }
        None => (line, &line[line.len()..]),
    }
}

/// Iterate over the lines of `bytes`, each slice INCLUDING its trailing `\n`
/// (the final line may have none). Concatenating the slices reconstructs `bytes`.
fn lines_with_endings(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut start = 0;
    std::iter::from_fn(move || {
        if start >= bytes.len() {
            return None;
        }
        let end = match bytes[start..].iter().position(|&b| b == b'\n') {
            Some(i) => start + i + 1,
            None => bytes.len(),
        };
        let line = &bytes[start..end];
        start = end;
        Some(line)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // NB: explicit `\n` (not `\`-continuations, which would strip the leading
    // space on the context line).
    const SAMPLE: &[u8] = b"diff --git a/x.rs b/x.rs\nindex e69de29..4b825dc 100644\n--- a/x.rs\n+++ b/x.rs\n@@ -1,3 +1,4 @@\n context line\n-removed line\n+added line\n+another add\n";

    #[test]
    fn detects_and_colors_a_unified_diff() {
        let out = try_format(SAMPLE, &Theme::default_colored()).expect("should detect diff");
        let s = String::from_utf8(out).unwrap();
        assert!(
            s.contains("\x1b[32m+added line\x1b[0m\n"),
            "addition not green"
        );
        assert!(
            s.contains("\x1b[31m-removed line\x1b[0m\n"),
            "deletion not red"
        );
        assert!(
            s.contains("\x1b[36m@@ -1,3 +1,4 @@\x1b[0m\n"),
            "hunk not cyan"
        );
        assert!(s.contains(" context line\n"), "context line altered");
    }

    #[test]
    fn declines_without_a_hunk_header() {
        // A `-`/`+` heavy block that is NOT a diff (no `@@` hunk) must be declined,
        // so it is never recolored. Precision guard (invariant #2).
        let not_diff = b"- buy milk\n- walk dog\n+ added sugar to list\n--- not a header\n";
        assert!(try_format(not_diff, &Theme::default_colored()).is_none());
    }

    #[test]
    fn looks_like_start_is_tight() {
        assert!(looks_like_start(b"diff --git a/x b/x\nindex ..."));
        assert!(looks_like_start(b"--- a/x\n+++ b/x\n@@ -1 +1 @@\n"));
        // A `--- ` banner / log opener with no `+++ ` header must NOT be buffered
        // as a diff candidate (it would lose streaming coloring).
        assert!(!looks_like_start(b"--- starting up ---\nERROR boom\n"));
        assert!(!looks_like_start(b"--- just a heading\n"));
        assert!(!looks_like_start(b"ordinary output line\n"));
    }

    #[test]
    fn declines_plain_prose_and_flags() {
        assert!(try_format(b"usage: tool --foo --bar\n", &Theme::default_colored()).is_none());
        assert!(try_format(
            b"-rw-r--r-- 1 user staff 0 file\n",
            &Theme::default_colored()
        )
        .is_none());
        assert!(try_format(b"", &Theme::default_colored()).is_none());
    }

    #[test]
    fn hunk_header_variants() {
        assert!(is_hunk_header(b"@@ -1,3 +1,4 @@"));
        assert!(is_hunk_header(b"@@ -0,0 +1 @@")); // single-count form
        assert!(is_hunk_header(b"@@ -1 +1 @@"));
        assert!(is_hunk_header(b"@@ -12,7 +13,9 @@ fn main() {")); // with section heading
        assert!(is_hunk_header(b"@@ -1,3 +1,4 @@\r")); // CRLF
        assert!(!is_hunk_header(b"@@ no numbers @@"));
        assert!(!is_hunk_header(b"@@ -1,3 1,4 @@")); // missing +
        assert!(!is_hunk_header(b"not a hunk"));
    }

    #[test]
    fn plain_theme_is_byte_identical() {
        // The byte-safety guarantee: with no colors, a detected diff reproduces the
        // input exactly.
        let out = try_format(SAMPLE, &Theme::plain()).unwrap();
        assert_eq!(out, SAMPLE);
    }

    #[test]
    fn file_headers_are_not_treated_as_add_remove() {
        let out = try_format(SAMPLE, &Theme::default_colored()).unwrap();
        let s = String::from_utf8(out).unwrap();
        // `--- a/x.rs` / `+++ b/x.rs` are dim file headers, NOT red/green content.
        assert!(s.contains("\x1b[2m--- a/x.rs\x1b[0m\n"));
        assert!(s.contains("\x1b[2m+++ b/x.rs\x1b[0m\n"));
        assert!(!s.contains("\x1b[31m--- a/x.rs"));
        assert!(!s.contains("\x1b[32m+++ b/x.rs"));
    }

    proptest::proptest! {
        /// Never panics; with the plain theme, a detected diff is byte-identical to
        /// its input (so the colorizer can never corrupt user bytes).
        #[test]
        fn prop_plain_is_identity_and_never_panics(bytes: Vec<u8>) {
            if let Some(out) = try_format(&bytes, &Theme::plain()) {
                proptest::prop_assert_eq!(out, bytes);
            }
        }
    }
}
