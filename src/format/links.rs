//! Clickable links: OSC 8 hyperlinks on URLs and `file:line` locations
//! (`[links]` in `.glimpsrc`).
//!
//! A link is an invisible escape sequence wrapped around text that is already
//! on the line; the visible bytes never change. Three rules keep it honest:
//!
//! - **The target is the text.** A URL links to exactly the characters shown;
//!   a path links to that path, resolved against the command's working
//!   directory. Output cannot make a link say one thing and open another.
//! - **Only text GLIMPS already owns.** Whole output lines that carried no
//!   escape of their own (a colored compiler line is never touched,
//!   invariant #3), and the pinned error line GLIMPS writes itself. Nothing
//!   for sensitive or bypassed commands.
//! - **Off where it is not understood.** `auto` links only in terminals known
//!   to support OSC 8, and never under tmux or screen, which drop or mangle
//!   it unless configured.
//!
//! Detection is a byte scan with no filesystem access: a path that does not
//! exist still gets a link (opening it fails harmlessly), but the formatter
//! never blocks on a slow or hung mount.

use std::borrow::Cow;

use crate::config::{Links, LinksMode};

/// Longest URL or path that gets a link. Longer runs are left as they are.
const MAX_TARGET: usize = 2048;

/// Extensions that make a slash-less `name.ext:12` a file location. With a
/// `/` in the path any extension counts; without one, `example.com:443` must
/// not become a file link.
const SOURCE_EXTENSIONS: &[&str] = &[
    "bash", "c", "cc", "cfg", "conf", "cpp", "cs", "css", "cxx", "dart", "erl", "ex", "exs", "go",
    "gradle", "h", "hpp", "hs", "html", "ini", "java", "js", "json", "jsx", "kt", "kts", "less",
    "lock", "log", "lua", "m", "md", "mjs", "cjs", "ml", "mm", "nim", "php", "pl", "proto", "py",
    "r", "rb", "rs", "sass", "scala", "scss", "sh", "sql", "svelte", "swift", "tf", "toml", "ts",
    "tsx", "txt", "vue", "xml", "yaml", "yml", "zig", "zsh",
];

/// Whether the terminal GLIMPS runs in is known to support OSC 8. `env` reads
/// one environment variable.
pub(crate) fn supported(env: impl Fn(&str) -> Option<String>) -> bool {
    let term = env("TERM").unwrap_or_default();
    if env("TMUX").is_some() || term.starts_with("tmux") || term.starts_with("screen") {
        return false;
    }
    if matches!(
        env("TERM_PROGRAM").as_deref(),
        Some("iTerm.app" | "WezTerm" | "ghostty" | "vscode")
    ) {
        return true;
    }
    if env("KITTY_WINDOW_ID").is_some() || term == "xterm-kitty" {
        return true;
    }
    if env("ALACRITTY_WINDOW_ID").is_some() || term == "alacritty" || term.starts_with("foot") {
        return true;
    }
    if env("WT_SESSION").is_some() {
        return true;
    }
    // GNOME Terminal, Tilix and other VTE terminals: OSC 8 since VTE 0.50.
    env("VTE_VERSION")
        .and_then(|version| version.parse::<u32>().ok())
        .is_some_and(|version| version >= 5000)
}

/// Whether `mode` turns links on in this terminal.
pub(crate) fn enabled(mode: LinksMode, env: impl Fn(&str) -> Option<String>) -> bool {
    match mode {
        LinksMode::On => true,
        LinksMode::Off => false,
        LinksMode::Auto => supported(env),
    }
}

/// What a file link opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileTarget {
    /// `file://host/path`. The line number has no standard place in it.
    FileUrl,
    /// An editor URL with `{path}`, `{line}` and `{column}` placeholders.
    Template(String),
}

/// Parse the `editor` setting. `Err` names why a custom template was refused.
pub(crate) fn file_target(editor: &str) -> Result<FileTarget, &'static str> {
    let template = match editor {
        "file" => return Ok(FileTarget::FileUrl),
        "vscode" => "vscode://file{path}:{line}:{column}",
        "cursor" => "cursor://file{path}:{line}:{column}",
        "zed" => "zed://file{path}:{line}:{column}",
        custom => custom,
    };
    if !template.contains("{path}") {
        return Err("the template has no {path}");
    }
    // The URL lands inside an escape sequence: printable ASCII only, so it
    // can neither end the sequence nor hide a space.
    if !template.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        return Err("the template must be printable ASCII without spaces");
    }
    let scheme_ok = template.split_once(':').is_some_and(|(scheme, _)| {
        scheme
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic())
            && scheme
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-'))
    });
    if !scheme_ok {
        return Err("the template does not start with a URL scheme");
    }
    Ok(FileTarget::Template(template.to_string()))
}

/// One link: a byte range of the source line and where it points.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Span {
    start: usize,
    end: usize,
    target: String,
}

/// Finds link targets in a line and wraps them in OSC 8.
#[derive(Debug)]
pub(super) struct Linker {
    urls: bool,
    files: bool,
    target: FileTarget,
    /// The host part of `file://` URLs, so a terminal can tell a link from a
    /// remote GLIMPS session apart from a local file. Empty if unknown.
    host: String,
    home: Option<Vec<u8>>,
}

impl Linker {
    /// `None` when links are off for this terminal or both kinds are off.
    pub(super) fn new(config: &Links, env: impl Fn(&str) -> Option<String>) -> Option<Self> {
        if !enabled(config.mode, &env) || !(config.urls || config.files) {
            return None;
        }
        Some(Linker {
            urls: config.urls,
            files: config.files,
            target: file_target(&config.editor).unwrap_or(FileTarget::FileUrl),
            host: hostname().unwrap_or_default(),
            home: env("HOME")
                .filter(|home| home.starts_with('/'))
                .map(String::into_bytes),
        })
    }

    /// `rendered` (`source` as it is about to be written, possibly with SGR
    /// added) with links around the targets found in `source`. `None` when
    /// there is nothing to link, or when `rendered` does not show exactly the
    /// bytes of `source` (a view that rewrote text): then nothing is linked.
    pub(super) fn link_line(
        &self,
        source: &[u8],
        rendered: &[u8],
        cwd: Option<&[u8]>,
    ) -> Option<Vec<u8>> {
        let spans = self.spans(source, cwd);
        if spans.is_empty() {
            return None;
        }
        apply(rendered, source, &spans)
    }

    /// Link GLIMPS's own plain chrome text (the pinned error line). A
    /// location cut short by the display cap (`…` right after it) would point
    /// at the wrong line, so it is left alone.
    pub(super) fn link_chrome<'a>(&self, text: &'a [u8], cwd: Option<&[u8]>) -> Cow<'a, [u8]> {
        let ellipsis = "\u{2026}".as_bytes();
        let spans: Vec<Span> = self
            .spans(text, cwd)
            .into_iter()
            .filter(|span| !text[span.end..].starts_with(ellipsis))
            .collect();
        if spans.is_empty() {
            return Cow::Borrowed(text);
        }
        apply(text, text, &spans).map_or(Cow::Borrowed(text), Cow::Owned)
    }

    fn spans(&self, line: &[u8], cwd: Option<&[u8]>) -> Vec<Span> {
        // Every target needs a `:` (`://`, `:12`) except a Python frame.
        if !line.contains(&b':') && !contains(line, b"File \"") {
            return Vec::new();
        }
        let mut spans = Vec::new();
        if self.urls {
            find_urls(line, &mut spans);
        }
        if self.files {
            let urls = spans.len();
            let mut files = Vec::new();
            self.find_locations(line, cwd, &mut files);
            self.find_python_frames(line, cwd, &mut files);
            // A URL already covers its own `host:port/path`.
            files.retain(|file| {
                !spans[..urls]
                    .iter()
                    .any(|url| file.start < url.end && url.start < file.end)
            });
            spans.extend(files);
        }
        spans.sort_by_key(|span| span.start);
        // Overlaps cannot nest in OSC 8: keep the first of any two.
        let mut kept: Vec<Span> = Vec::with_capacity(spans.len());
        for span in spans {
            if kept.last().is_none_or(|last| last.end <= span.start) {
                kept.push(span);
            }
        }
        kept
    }

    /// `path:line` and `path:line:column`.
    fn find_locations(&self, line: &[u8], cwd: Option<&[u8]>, spans: &mut Vec<Span>) {
        let mut i = 0;
        while i < line.len() {
            if line[i] != b':' || !line.get(i + 1).is_some_and(|b| (b'1'..=b'9').contains(b)) {
                i += 1;
                continue;
            }
            let start = line[..i]
                .iter()
                .rposition(|&b| !is_path_byte(b))
                .map_or(0, |at| at + 1);
            let path = &line[start..i];
            let Some(line_end) = digits_end(line, i + 1) else {
                i += 1;
                continue;
            };
            let line_no = &line[i + 1..line_end];
            let (end, column) = match line.get(line_end) {
                Some(b':') if line.get(line_end + 1).is_some_and(u8::is_ascii_digit) => {
                    match digits_end(line, line_end + 1) {
                        Some(col_end) => (col_end, Some(&line[line_end + 1..col_end])),
                        None => (line_end, None),
                    }
                }
                _ => (line_end, None),
            };
            let bounded = !line
                .get(end)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
            if bounded && looks_like_file(path) {
                if let Some(target) = self.file_link(path, line_no, column, cwd) {
                    spans.push(Span { start, end, target });
                }
            }
            i = end.max(i + 1);
        }
    }

    /// Python's `File "app.py", line 7`: the quoted path links to line 7.
    fn find_python_frames(&self, line: &[u8], cwd: Option<&[u8]>, spans: &mut Vec<Span>) {
        let needle = b"File \"";
        let mut from = 0;
        while let Some(at) = find(&line[from..], needle) {
            let start = from + at + needle.len();
            from = start;
            let Some(len) = line[start..].iter().position(|&b| b == b'"') else {
                break;
            };
            let path = &line[start..start + len];
            let after = &line[start + len..];
            let Some(rest) = after.strip_prefix(b"\", line ") else {
                continue;
            };
            let Some(digits) = digits_end(rest, 0) else {
                continue;
            };
            // `<stdin>`, `<string>`, `<frozen …>` are not files.
            if path.is_empty() || path.starts_with(b"<") || path.iter().any(|&b| b < 0x20) {
                continue;
            }
            if let Some(target) = self.file_link(path, &rest[..digits], None, cwd) {
                spans.push(Span {
                    start,
                    end: start + len,
                    target,
                });
            }
        }
    }

    fn file_link(
        &self,
        path: &[u8],
        line: &[u8],
        column: Option<&[u8]>,
        cwd: Option<&[u8]>,
    ) -> Option<String> {
        let absolute = resolve(path, cwd, self.home.as_deref())?;
        if absolute.len() > MAX_TARGET {
            return None;
        }
        let encoded = percent_encode(&absolute);
        let line = std::str::from_utf8(line).ok()?;
        let column = column
            .and_then(|column| std::str::from_utf8(column).ok())
            .unwrap_or("1");
        Some(match &self.target {
            FileTarget::FileUrl => format!("file://{}{encoded}", self.host),
            FileTarget::Template(template) => template
                .replace("{path}", &encoded)
                .replace("{line}", line)
                .replace("{column}", column),
        })
    }
}

/// `http://` and `https://` URLs, without the trailing punctuation of the
/// sentence around them.
fn find_urls(line: &[u8], spans: &mut Vec<Span>) {
    let mut from = 0;
    while let Some(at) = find(&line[from..], b"://") {
        let colon = from + at;
        from = colon + 3;
        let start = if line[..colon].ends_with(b"https") {
            colon - 5
        } else if line[..colon].ends_with(b"http") {
            colon - 4
        } else {
            continue;
        };
        if start > 0 && line[start - 1].is_ascii_alphanumeric() {
            continue;
        }
        let mut end = line[from..]
            .iter()
            .position(|&b| !is_url_byte(b))
            .map_or(line.len(), |len| from + len);
        if end - start > MAX_TARGET {
            from = end;
            continue;
        }
        // Trailing punctuation belongs to the sentence, and a closing bracket
        // only to the URL when the URL opened it. Counted once, then kept
        // current while trimming, so this stays linear.
        let count = |byte: u8| line[start..end].iter().filter(|&&b| b == byte).count();
        let (opens, mut closes) = (count(b'('), count(b')'));
        let (square_opens, mut square_closes) = (count(b'['), count(b']'));
        while end > from {
            match line[end - 1] {
                b'.' | b',' | b';' | b':' | b'!' | b'?' | b'\'' | b'"' | b'*' => {}
                b')' if closes > opens => closes -= 1,
                b']' if square_closes > square_opens => square_closes -= 1,
                _ => break,
            }
            end -= 1;
        }
        // A host must follow the scheme.
        if end <= from || !line[from].is_ascii_alphanumeric() {
            continue;
        }
        let target = String::from_utf8_lossy(&line[start..end]).into_owned();
        spans.push(Span { start, end, target });
        from = end;
    }
}

/// RFC 3986 characters, minus nothing: a URL is linked exactly as printed.
fn is_url_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b':'
                | b'/'
                | b'?'
                | b'#'
                | b'['
                | b']'
                | b'@'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b'%'
        )
}

/// Characters a path in a `path:line` location is made of. Bytes from 0x80
/// up are whole UTF-8 characters (lead and continuation bytes alike), so a
/// scan over them never stops inside one.
fn is_path_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(b, b'.' | b'_' | b'-' | b'/' | b'~' | b'+' | b'@')
        || b >= 0x80
}

/// The end of a run of 1–9 ASCII digits starting at `from`.
fn digits_end(bytes: &[u8], from: usize) -> Option<usize> {
    let len = bytes
        .get(from..)?
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    (1..=9).contains(&len).then_some(from + len)
}

/// A file name with an extension that has a letter in it, and either a `/`
/// or a source-code extension: `src/main.rs`, `app.py`, `./build/out.o`, but
/// not `example.com`, `10.0.0.1` or `v1.2`.
fn looks_like_file(path: &[u8]) -> bool {
    let Ok(path) = std::str::from_utf8(path) else {
        return false;
    };
    let name = path.rsplit('/').next().unwrap_or(path);
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    if stem.is_empty() && !path.contains('/') {
        return false;
    }
    if extension.is_empty()
        || extension.len() > 12
        || !extension.bytes().all(|b| b.is_ascii_alphanumeric())
        || !extension.bytes().any(|b| b.is_ascii_alphabetic())
    {
        return false;
    }
    path.contains('/') || SOURCE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
}

/// An absolute, lexically normalized path: `/abs` as is, `~/x` under the home
/// directory, anything else under `cwd`. `None` when that needs a directory
/// GLIMPS does not know.
fn resolve(path: &[u8], cwd: Option<&[u8]>, home: Option<&[u8]>) -> Option<Vec<u8>> {
    let joined = if path.starts_with(b"/") {
        path.to_vec()
    } else if let Some(rest) = path.strip_prefix(b"~/") {
        [home?, b"/", rest].concat()
    } else if path.starts_with(b"~") {
        return None;
    } else {
        let cwd = cwd.filter(|cwd| cwd.starts_with(b"/"))?;
        [cwd, b"/", path].concat()
    };
    let mut parts: Vec<&[u8]> = Vec::new();
    for part in joined.split(|&b| b == b'/') {
        match part {
            b"" | b"." => {}
            b".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    let mut out = Vec::with_capacity(joined.len());
    for part in parts {
        out.push(b'/');
        out.extend_from_slice(part);
    }
    if out.is_empty() {
        out.push(b'/');
    }
    Some(out)
}

/// Percent-encode everything but RFC 3986 unreserved characters and `/`.
fn percent_encode(path: &[u8]) -> String {
    let mut out = String::with_capacity(path.len());
    for &b in path {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Wrap each span of `source` in OSC 8 while copying `rendered`, which must
/// show exactly `source`'s bytes between its escape sequences. A link opens
/// just before its first shown byte and closes just after its last, so SGR
/// changes on either side stay outside it.
fn apply(rendered: &[u8], source: &[u8], spans: &[Span]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(rendered.len() + spans.len() * 96);
    let mut spans = spans.iter().peekable();
    let mut cursor = Cursor { i: 0, shown: 0 };
    while cursor.i < rendered.len() {
        if rendered[cursor.i] == 0x1b {
            cursor.copy_escape(&mut out, rendered)?;
            continue;
        }
        match spans.next_if(|span| span.start == cursor.shown) {
            Some(span) => {
                out.extend_from_slice(b"\x1b]8;;");
                out.extend_from_slice(span.target.as_bytes());
                out.extend_from_slice(b"\x1b\\");
                cursor.copy_shown(&mut out, rendered, source, span.end)?;
                out.extend_from_slice(b"\x1b]8;;\x1b\\");
            }
            None => cursor.copy_shown(&mut out, rendered, source, cursor.shown + 1)?,
        }
    }
    (cursor.shown == source.len() && spans.next().is_none()).then_some(out)
}

/// Position in `rendered` (`i`) and in `source` (`shown`, the shown bytes).
struct Cursor {
    i: usize,
    shown: usize,
}

impl Cursor {
    fn copy_escape(&mut self, out: &mut Vec<u8>, rendered: &[u8]) -> Option<()> {
        let len = escape_len(rendered.get(self.i..)?)?;
        out.extend_from_slice(rendered.get(self.i..self.i + len)?);
        self.i += len;
        Some(())
    }

    /// Copy until `end` source bytes have been shown, escapes in between
    /// included; `None` if a shown byte differs from the source.
    fn copy_shown(
        &mut self,
        out: &mut Vec<u8>,
        rendered: &[u8],
        source: &[u8],
        end: usize,
    ) -> Option<()> {
        while self.shown < end {
            let byte = *rendered.get(self.i)?;
            if byte == 0x1b {
                self.copy_escape(out, rendered)?;
                continue;
            }
            if source.get(self.shown) != Some(&byte) {
                return None;
            }
            out.push(byte);
            self.shown += 1;
            self.i += 1;
        }
        Some(())
    }
}

/// Length of the escape sequence at the start of `bytes` (which begins with
/// ESC), measured the way the OSC-133 scanner reads it: CSI to its final
/// byte, OSC to BEL or ST, DCS/SOS/PM/APC to ST, intermediates (`ESC ( B`)
/// to their final byte, else two bytes. An ESC inside a sequence abandons it
/// (the sequence ends right before that ESC), CAN/SUB cancel it, and other C0
/// controls met inside are executed and skipped over. `None` if unterminated.
pub(super) fn escape_len(bytes: &[u8]) -> Option<usize> {
    const ESC: u8 = 0x1b;
    let cancels = |b: u8| matches!(b, 0x18 | 0x1a);
    let second = *bytes.get(1)?;
    match second {
        // `ESC ESC`: the first ESC is abandoned on its own.
        ESC => Some(1),
        b'[' => {
            for (i, &b) in bytes.iter().enumerate().skip(2) {
                if b == ESC {
                    return Some(i);
                }
                if (0x40..=0x7e).contains(&b) || cancels(b) {
                    return Some(i + 1);
                }
            }
            None
        }
        b']' | b'P' | b'X' | b'^' | b'_' => {
            let osc = second == b']';
            let mut i = 2;
            while i < bytes.len() {
                match bytes[i] {
                    0x07 if osc => return Some(i + 1),
                    ESC if bytes.get(i + 1) == Some(&b'\\') => return Some(i + 2),
                    ESC => return Some(i),
                    _ => i += 1,
                }
            }
            None
        }
        0x18 | 0x1a => Some(2),
        0x00..=0x2f => {
            for (i, &b) in bytes.iter().enumerate().skip(2) {
                if b == ESC {
                    return Some(i);
                }
                if cancels(b) || !(0x00..=0x2f).contains(&b) {
                    return Some(i + 1);
                }
            }
            None
        }
        _ => Some(2),
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    find(haystack, needle).is_some()
}

/// This machine's host name, for `file://host/…`.
#[cfg(unix)]
fn hostname() -> Option<String> {
    let mut buf = [0u8; 256];
    // SAFETY: `buf` is valid for writes of `buf.len()` bytes and outlives the
    // call; gethostname writes at most that many bytes. Only the bytes before
    // the first NUL inside `buf` are read back.
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    if rc != 0 {
        return None;
    }
    let len = buf.iter().position(|&b| b == 0)?;
    let name = std::str::from_utf8(&buf[..len]).ok()?;
    (!name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.')))
    .then(|| name.to_string())
}

#[cfg(not(unix))]
fn hostname() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn linker(target: FileTarget) -> Linker {
        Linker {
            urls: true,
            files: true,
            target,
            host: "host".to_string(),
            home: Some(b"/Users/me".to_vec()),
        }
    }

    /// `(linked text, target)` for each span found in `line`.
    fn found(line: &str, cwd: Option<&str>) -> Vec<(String, String)> {
        linker(FileTarget::FileUrl)
            .spans(line.as_bytes(), cwd.map(str::as_bytes))
            .into_iter()
            .map(|span| (line[span.start..span.end].to_string(), span.target))
            .collect()
    }

    fn pair(text: &str, target: &str) -> (String, String) {
        (text.to_string(), target.to_string())
    }

    /// Drop every OSC 8 sequence, leaving what the terminal shows.
    fn without_links(bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i..].starts_with(b"\x1b]8;") {
                let len = escape_len(&bytes[i..]).unwrap();
                i += len;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        out
    }

    #[test]
    fn auto_links_only_in_terminals_known_to_support_osc8() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |key: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| v.to_string())
            }
        };
        for program in ["iTerm.app", "WezTerm", "ghostty", "vscode"] {
            let pairs: &'static [(&str, &str)] = Box::leak(Box::new([("TERM_PROGRAM", program)]));
            assert!(supported(env(pairs)), "{program}");
        }
        assert!(supported(env(&[("TERM", "xterm-kitty")])));
        assert!(supported(env(&[("KITTY_WINDOW_ID", "3")])));
        assert!(supported(env(&[("TERM", "alacritty")])));
        assert!(supported(env(&[("TERM", "foot")])));
        assert!(supported(env(&[("WT_SESSION", "x")])));
        assert!(supported(env(&[("VTE_VERSION", "7600")])));
        assert!(!supported(env(&[("VTE_VERSION", "4802")])));
        assert!(!supported(env(&[("TERM_PROGRAM", "Apple_Terminal")])));
        assert!(!supported(env(&[("TERM", "xterm-256color")])));
        assert!(!supported(env(&[])));
        assert!(!supported(env(&[
            ("TERM_PROGRAM", "ghostty"),
            ("TMUX", "/t")
        ])));
        assert!(!supported(env(&[
            ("TERM_PROGRAM", "iTerm.app"),
            ("TERM", "screen")
        ])));
        assert!(enabled(LinksMode::On, env(&[])));
        assert!(!enabled(
            LinksMode::Off,
            env(&[("TERM_PROGRAM", "ghostty")])
        ));
    }

    #[test]
    fn editor_presets_and_template_validation() {
        assert_eq!(file_target("file"), Ok(FileTarget::FileUrl));
        assert_eq!(
            file_target("vscode"),
            Ok(FileTarget::Template(
                "vscode://file{path}:{line}:{column}".to_string()
            ))
        );
        assert!(matches!(file_target("zed"), Ok(FileTarget::Template(_))));
        assert!(matches!(file_target("cursor"), Ok(FileTarget::Template(_))));
        assert!(matches!(
            file_target("idea://open?file={path}&line={line}"),
            Ok(FileTarget::Template(_))
        ));
        assert!(file_target("vscode://file").is_err());
        assert!(file_target("my editor://{path}").is_err());
        assert!(file_target("x://{path}\x07").is_err());
        assert!(file_target("{path}").is_err());
        assert!(file_target("1x://{path}").is_err());
        assert!(file_target("").is_err());
    }

    #[test]
    fn urls_drop_sentence_punctuation_and_keep_balanced_parens() {
        assert_eq!(
            found("see https://example.com/docs.", None),
            [pair("https://example.com/docs", "https://example.com/docs")]
        );
        assert_eq!(
            found("(see http://x.test/a?b=1&c=2#top), then", None),
            [pair(
                "http://x.test/a?b=1&c=2#top",
                "http://x.test/a?b=1&c=2#top"
            )]
        );
        assert_eq!(
            found("https://en.wikipedia.org/wiki/Rust_(language)", None),
            [pair(
                "https://en.wikipedia.org/wiki/Rust_(language)",
                "https://en.wikipedia.org/wiki/Rust_(language)"
            )]
        );
        assert_eq!(
            found("url=\"https://a.test:8443/x.rs:12\"", None),
            [pair(
                "https://a.test:8443/x.rs:12",
                "https://a.test:8443/x.rs:12"
            )]
        );
        assert_eq!(found("a https://b.test c http://d.test", None).len(), 2);
        assert!(found("xhttps://a.test", None).is_empty());
        assert!(found("https:// nothing", None).is_empty());
        assert!(found("ftp://a.test", None).is_empty());
        assert!(found("https://", None).is_empty());
    }

    #[test]
    fn locations_resolve_against_the_command_directory() {
        let cwd = Some("/work/repo");
        assert_eq!(
            found("  --> src/pty.rs:214:18", cwd),
            [pair(
                "src/pty.rs:214:18",
                "file://host/work/repo/src/pty.rs"
            )]
        );
        assert_eq!(
            found("src/main.rs:42:fn main() {", cwd),
            [pair("src/main.rs:42", "file://host/work/repo/src/main.rs")]
        );
        assert_eq!(
            found("./main.go:12:3: undefined: x", cwd),
            [pair("./main.go:12:3", "file://host/work/repo/main.go")]
        );
        assert_eq!(
            found("../lib/x.rs:9", cwd),
            [pair("../lib/x.rs:9", "file://host/work/lib/x.rs")]
        );
        assert_eq!(
            found("at /abs/app.js:10:5)", None),
            [pair("/abs/app.js:10:5", "file://host/abs/app.js")]
        );
        assert_eq!(
            found("~/notes/todo.md:3", None),
            [pair(
                "~/notes/todo.md:3",
                "file://host/Users/me/notes/todo.md"
            )]
        );
        assert_eq!(
            found("app.py:7", cwd),
            [pair("app.py:7", "file://host/work/repo/app.py")]
        );
        // A relative path needs a known directory.
        assert!(found("src/main.rs:42", None).is_empty());
        assert!(found("src/main.rs:42", Some("relative")).is_empty());
    }

    #[test]
    fn non_locations_are_left_alone() {
        let cwd = Some("/w");
        for line in [
            "example.com:443",
            "api.github.com:443 open",
            "10.0.0.1:8080",
            "12:30:45 up 3 days",
            "ratio 1.5:3",
            "v1.2.3:4",
            "Makefile:12",
            "a.rs:0",
            "a.rs:12abc",
            "a.rs:1234567890",
            "~user/a.rs:3",
            "a.rs: 12",
            ".rs:12",
        ] {
            assert!(found(line, cwd).is_empty(), "{line}");
        }
    }

    #[test]
    fn python_frames_link_the_quoted_path() {
        assert_eq!(
            found(r#"  File "/srv/My App/app.py", line 7, in <module>"#, None),
            [pair(
                "/srv/My App/app.py",
                "file://host/srv/My%20App/app.py"
            )]
        );
        assert_eq!(
            found(r#"  File "pkg/mod.py", line 12, in run"#, Some("/w")),
            [pair("pkg/mod.py", "file://host/w/pkg/mod.py")]
        );
        assert!(found(r#"  File "<stdin>", line 1, in <module>"#, Some("/w")).is_empty());
        assert!(found(r#"  File "x.py", in run"#, Some("/w")).is_empty());
    }

    #[test]
    fn non_ascii_paths_are_percent_encoded() {
        assert_eq!(
            found("/tmp/caf\u{e9}/\u{1f600}.rs:1", None),
            [pair(
                "/tmp/caf\u{e9}/\u{1f600}.rs:1",
                "file://host/tmp/caf%C3%A9/%F0%9F%98%80.rs"
            )]
        );
    }

    #[test]
    fn editor_templates_carry_line_and_column() {
        let vscode = linker(file_target("vscode").unwrap());
        let spans = vscode.spans(b"src/a.rs:12:5 and b.rs:3", Some(b"/w"));
        let targets: Vec<&str> = spans.iter().map(|span| span.target.as_str()).collect();
        assert_eq!(
            targets,
            ["vscode://file/w/src/a.rs:12:5", "vscode://file/w/b.rs:3:1"]
        );
    }

    #[test]
    fn links_wrap_text_inside_existing_color() {
        let source = b"error at src/a.rs:3 now\r\n";
        let rendered = b"\x1b[31merror\x1b[0m at \x1b[36msrc/a.rs:3\x1b[0m now\r\n";
        let linked = linker(FileTarget::FileUrl)
            .link_line(source, rendered, Some(b"/w"))
            .unwrap();
        assert_eq!(
            linked,
            b"\x1b[31merror\x1b[0m at \x1b[36m\x1b]8;;file://host/w/src/a.rs\x1b\\src/a.rs:3\x1b]8;;\x1b\\\x1b[0m now\r\n"
        );
        assert_eq!(without_links(&linked), rendered);
    }

    #[test]
    fn charset_escapes_are_skipped_whole() {
        let source = b"see https://a.test\r\n";
        let rendered = b"\x1b(Bsee https://a.test\x1b(0\r\n";
        let linked = linker(FileTarget::FileUrl)
            .link_line(source, rendered, None)
            .unwrap();
        assert_eq!(without_links(&linked), rendered);
    }

    #[test]
    fn rewritten_text_is_never_linked() {
        let source = b"see https://a.test\n";
        let rewritten = b"see: https://a.test\n";
        assert!(linker(FileTarget::FileUrl)
            .link_line(source, rewritten, None)
            .is_none());
        let truncated = b"see https://a.te";
        assert!(linker(FileTarget::FileUrl)
            .link_line(source, truncated, None)
            .is_none());
        // An unterminated escape: give up rather than guess.
        assert!(linker(FileTarget::FileUrl)
            .link_line(source, b"see \x1b]0;x https://a.test\n", None)
            .is_none());
    }

    #[test]
    fn chrome_links_skip_a_location_cut_by_the_display_cap() {
        let linker = linker(FileTarget::FileUrl);
        let cut = "error[E0308]: mismatched \u{2192} src/pty.rs:21\u{2026}";
        assert_eq!(
            linker.link_chrome(cut.as_bytes(), Some(b"/w")),
            cut.as_bytes()
        );
        let whole = "error[E0308]: mismatched \u{2192} src/pty.rs:214:18";
        let linked = linker.link_chrome(whole.as_bytes(), Some(b"/w"));
        assert!(contains(
            &linked,
            b"\x1b]8;;file://host/w/src/pty.rs\x1b\\src/pty.rs:214:18"
        ));
    }

    #[test]
    fn switches_limit_what_is_linked() {
        let mut only_urls = linker(FileTarget::FileUrl);
        only_urls.files = false;
        let spans = only_urls.spans(b"a.rs:1 https://x.test", Some(b"/w"));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].target, "https://x.test");
        let mut only_files = linker(FileTarget::FileUrl);
        only_files.urls = false;
        let spans = only_files.spans(b"a.rs:1 https://x.test", Some(b"/w"));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].target, "file://host/w/a.rs");
    }

    #[test]
    fn pathological_lines_stay_linear() {
        // 64 KiB lines (the default line cap) shaped to hit every scan's worst
        // case. Quadratic work here would take seconds; linear takes
        // milliseconds even unoptimized.
        let linker = linker(FileTarget::FileUrl);
        let cases: Vec<Vec<u8>> = vec![
            [b"https://a".as_slice(), &[b')'; 65_536]].concat(),
            [
                b"https://a/".as_slice(),
                &b"(".repeat(30_000),
                &[b')'; 30_000],
            ]
            .concat(),
            b"a/".repeat(32_768).into_iter().chain(*b":1").collect(),
            b"x.rs:1:".repeat(9_000),
            b"https://".repeat(8_000),
            b"File \"".repeat(10_000),
            b":1".repeat(32_768),
        ];
        let started = std::time::Instant::now();
        for line in &cases {
            let _ = linker.link_line(line, line, Some(b"/w"));
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn host_name_is_safe_for_a_url() {
        if let Some(host) = hostname() {
            assert!(!host.is_empty());
            assert!(host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.'));
        }
    }

    proptest! {
        /// Whatever the line, linking never panics, never changes a shown
        /// byte, and every target is printable ASCII (it cannot end the
        /// sequence it travels in).
        #[test]
        fn prop_links_are_invisible_and_well_formed(
            line in proptest::collection::vec(
                proptest::prop_oneof![
                    0x20u8..0x7f,
                    Just(b':'),
                    Just(b'/'),
                    Just(b'.'),
                    0x80u8..=0xff,
                ],
                0..120,
            ),
            prefix in proptest::sample::select(vec![
                "", "https://", "src/", "File \"", "~/", "../",
            ]),
            suffix in proptest::sample::select(vec!["", ".rs:12", ":3:4", "\", line 9"]),
        ) {
            let mut source = prefix.as_bytes().to_vec();
            source.extend_from_slice(&line);
            source.extend_from_slice(suffix.as_bytes());
            let linker = linker(file_target("vscode").unwrap());
            for span in linker.spans(&source, Some(b"/w")) {
                prop_assert!(span.start < span.end && span.end <= source.len());
                prop_assert!(span.target.bytes().all(|b| (0x21..=0x7e).contains(&b)));
            }
            if let Some(linked) = linker.link_line(&source, &source, Some(b"/w")) {
                prop_assert_eq!(without_links(&linked), source.clone());
            }
            let chrome = linker.link_chrome(&source, Some(b"/w"));
            prop_assert_eq!(without_links(&chrome), source);
        }
    }
}
