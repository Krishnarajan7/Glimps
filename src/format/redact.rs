//! Screen-share redaction: mask secrets in output GLIMPS writes (`[redact]` in
//! `.glimpsrc`, or `glimps redact on` for the current session).
//!
//! **Best effort, by design.** GLIMPS can only mask text that passes through
//! its line formatter, so these are never masked:
//!
//! - what you type, and the shell's echo of it (the prompt zone is the
//!   shell's, not GLIMPS's);
//! - full-screen programs and interactive sessions (vim, less, ssh, …),
//!   which stream through untouched, and binary output;
//! - a secret with no recognizable shape printed by an ordinary command;
//! - the part of a token already written before a color change or a pause
//!   split it (the rest is masked: the current line is carried between
//!   pieces).
//!
//! Detection only uses shapes that are secrets with high confidence, so
//! ordinary output is not mangled:
//!
//! - tokens with a vendor prefix (`ghp_…`, `sk_live_…`, `AKIA…`, `xoxb-…`, …);
//! - JSON Web Tokens (`eyJ….eyJ….…`);
//! - the password in a URL (`postgres://user:password@host`);
//! - `Authorization: Bearer …` / `Basic …` / `Token …` values;
//! - values assigned to a secret-named field (`API_KEY=…`, `password: …`,
//!   `"clientSecret": "…"`, `?access_token=…`), except values that are
//!   plainly not secrets (numbers, types, placeholders, `token_type`);
//! - the body of a `-----BEGIN … PRIVATE KEY-----` block.
//!
//! A command that exists to print a secret (`pass show`) has every complete
//! line masked instead ([`Redactor::mask_all`]).
//!
//! A masked character becomes as many `*` as the columns it takes, so
//! columns and line widths do not move; a token keeps its vendor prefix
//! (`ghp_****…`) and a field its name, so you can still tell what was
//! hidden. Escape sequences and control bytes (`\r`, backspace, tab) are
//! never touched. Every scan is bounded, so a hostile line costs linear time.

/// What a masked character becomes. ASCII, so it is one column wide in every
/// terminal (a bullet is "ambiguous width" and two columns in CJK setups).
const MASK: u8 = b'*';

/// Longest secret considered. Longer runs are not secrets of these shapes.
const MAX_SECRET: usize = 4096;

/// Which bytes a token's body may contain.
#[derive(Clone, Copy)]
enum Body {
    /// `[A-Za-z0-9]`
    Alnum,
    /// `[A-Za-z0-9_]`
    AlnumUnderscore,
    /// `[A-Za-z0-9_-]`
    UrlSafe,
    /// `[A-Za-z0-9-]`
    AlnumDash,
    /// `[A-Z0-9]`
    UpperAlnum,
    /// `[a-f0-9]`
    LowerHex,
    /// `[A-Fa-f0-9]`
    Hex,
    /// `[A-Za-z0-9_.-]`
    UrlSafeDot,
}

impl Body {
    fn allows(self, b: u8) -> bool {
        match self {
            Body::Alnum => b.is_ascii_alphanumeric(),
            Body::AlnumUnderscore => b.is_ascii_alphanumeric() || b == b'_',
            Body::UrlSafe => b.is_ascii_alphanumeric() || b == b'_' || b == b'-',
            Body::AlnumDash => b.is_ascii_alphanumeric() || b == b'-',
            Body::UpperAlnum => b.is_ascii_uppercase() || b.is_ascii_digit(),
            Body::LowerHex => b.is_ascii_digit() || (b'a'..=b'f').contains(&b),
            Body::Hex => b.is_ascii_hexdigit(),
            Body::UrlSafeDot => b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.',
        }
    }
}

/// A vendor token: its literal prefix, what its body is made of, and how long
/// the body is (`min..=max`).
struct Token {
    prefix: &'static [u8],
    body: Body,
    min: usize,
    max: usize,
}

const fn token(prefix: &'static [u8], body: Body, min: usize, max: usize) -> Token {
    Token {
        prefix,
        body,
        min,
        max,
    }
}

/// Vendor-prefixed tokens. Order matters only where one prefix starts
/// another (`sk-ant-`, `sk-proj-` before `sk-`): the first match wins.
const TOKENS: &[Token] = &[
    // GitHub: classic, OAuth, user-to-server, server-to-server, refresh;
    // fine-grained.
    token(b"ghp_", Body::Alnum, 36, 255),
    token(b"gho_", Body::Alnum, 36, 255),
    token(b"ghu_", Body::Alnum, 36, 255),
    token(b"ghs_", Body::Alnum, 36, 255),
    token(b"ghr_", Body::Alnum, 36, 255),
    token(b"github_pat_", Body::AlnumUnderscore, 40, 255),
    // GitLab personal / deploy / runner tokens.
    token(b"glpat-", Body::UrlSafe, 20, 255),
    token(b"gldt-", Body::UrlSafe, 20, 255),
    token(b"glrt-", Body::UrlSafe, 20, 255),
    // Slack bot / user / app-level / refresh / config tokens.
    token(b"xoxb-", Body::AlnumDash, 10, 255),
    token(b"xoxp-", Body::AlnumDash, 10, 255),
    token(b"xoxa-", Body::AlnumDash, 10, 255),
    token(b"xoxr-", Body::AlnumDash, 10, 255),
    token(b"xoxs-", Body::AlnumDash, 10, 255),
    token(b"xoxe-", Body::AlnumDash, 10, 255),
    token(b"xapp-", Body::AlnumDash, 10, 255),
    // Stripe secret / restricted keys (live and test) and webhook secrets.
    token(b"sk_live_", Body::Alnum, 16, 255),
    token(b"sk_test_", Body::Alnum, 16, 255),
    token(b"rk_live_", Body::Alnum, 16, 255),
    token(b"rk_test_", Body::Alnum, 16, 255),
    token(b"whsec_", Body::Alnum, 24, 255),
    // Anthropic, OpenAI (project, service account, legacy).
    token(b"sk-ant-", Body::UrlSafe, 32, 255),
    token(b"sk-proj-", Body::UrlSafe, 32, 255),
    token(b"sk-svcacct-", Body::UrlSafe, 32, 255),
    token(b"sk-", Body::Alnum, 40, 255),
    // AWS access key IDs (long-term and temporary).
    token(b"AKIA", Body::UpperAlnum, 16, 16),
    token(b"ASIA", Body::UpperAlnum, 16, 16),
    // Google API keys.
    token(b"AIza", Body::UrlSafe, 35, 35),
    // npm, PyPI, Hugging Face.
    token(b"npm_", Body::Alnum, 36, 255),
    token(b"pypi-", Body::UrlSafe, 50, 255),
    token(b"hf_", Body::Alnum, 30, 255),
    // DigitalOcean personal / OAuth tokens.
    token(b"dop_v1_", Body::LowerHex, 64, 64),
    token(b"doo_v1_", Body::LowerHex, 64, 64),
    // Shopify admin / shared-secret / custom-app tokens.
    token(b"shpat_", Body::Hex, 32, 32),
    token(b"shpss_", Body::Hex, 32, 32),
    token(b"shpca_", Body::Hex, 32, 32),
    // SendGrid (`SG.<id>.<secret>`).
    token(b"SG.", Body::UrlSafeDot, 60, 120),
    // age secret keys.
    token(b"AGE-SECRET-KEY-1", Body::UpperAlnum, 50, 80),
];

/// Name segments that make a field secret. Matched against whole segments of
/// the field name (`DB_PASSWORD`, `clientSecret`), never substrings, so
/// `TOKENIZERS_PARALLELISM` and `author` stay visible.
const SECRET_SEGMENTS: &[&str] = &[
    "PASSWORD",
    "PASSWD",
    "PASSPHRASE",
    "SECRET",
    "SECRETS",
    "TOKEN",
    "APIKEY",
    "CREDENTIAL",
    "CREDENTIALS",
];

/// A last name segment that describes a secret rather than holding it:
/// `token_type`, `credential.helper`, `password_file`, `secret_name`,
/// `PASSWORD_VALIDATORS`, `api_key_id`.
const DESCRIPTIVE_SEGMENTS: &[&str] = &[
    "TYPE",
    "HELPER",
    "FILE",
    "PATH",
    "DIR",
    "URL",
    "URI",
    "ENDPOINT",
    "NAME",
    "ID",
    "IDS",
    "LENGTH",
    "LEN",
    "COUNT",
    "EXPIRES",
    "EXPIRY",
    "EXPIRATION",
    "TTL",
    "VALIDATORS",
    "POLICY",
    "FORMAT",
    "KIND",
    "MIN",
    "MAX",
    "PROMPT",
    "LABEL",
    "FIELD",
    "ENV",
    "VAR",
    "HEADER",
    "PREFIX",
    "STORE",
    "MANAGER",
    "PROVIDER",
    "SOURCE",
    "HASH",
    "HASHER",
    "ALGORITHM",
    "MODE",
    "REQUIRED",
    "ENABLED",
    "CHANGED",
    "RESET",
    "AGE",
    "SIZE",
    "LIMIT",
    "SCOPE",
    "SCOPES",
    "USAGE",
];

/// Adjacent name segments that make a field secret (`API_KEY`, `accessKey`).
const SECRET_PAIRS: &[(&str, &str)] = &[
    ("API", "KEY"),
    ("ACCESS", "KEY"),
    ("SECRET", "KEY"),
    ("PRIVATE", "KEY"),
    ("SIGNING", "KEY"),
    ("ENCRYPTION", "KEY"),
    ("MASTER", "KEY"),
    ("SESSION", "KEY"),
    ("CLIENT", "SECRET"),
];

/// Masks secrets, one command's output at a time.
///
/// Output reaches it in pieces: whole lines, but also fragments split by the
/// program's own escapes (`jq` colors `"token"`, `:` and `"value"` apart) or by
/// a pause. The shown text of the current line so far is carried between
/// calls, so a field or a token that spans pieces is still found; only the
/// part not yet written can be masked.
#[derive(Debug, Default)]
pub(super) struct Redactor {
    /// Inside a `-----BEGIN … PRIVATE KEY-----` block.
    in_private_key: bool,
    /// Lines masked in the current block (bounded by [`KEY_BLOCK_CAP`]).
    key_lines: usize,
    /// Shown text of the current line already written by earlier calls.
    carry: Vec<u8>,
}

/// The most shown text of the current line carried between calls.
const CARRY_CAP: usize = 512;

/// A private-key block longer than this many lines is not a key (an RSA-4096
/// PEM is about 50); it is closed rather than masking the rest of the output.
const KEY_BLOCK_CAP: usize = 200;

impl Redactor {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Forget all state; called when a command starts.
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    /// `rendered` (text that may carry escape sequences) with every detected
    /// secret in its shown text masked, or `None` when nothing was masked.
    pub(super) fn redact(&mut self, rendered: &[u8]) -> Option<Vec<u8>> {
        self.process(rendered, false)
    }

    /// `rendered` with every shown character masked: for the complete lines
    /// of a command that exists to print secrets (`pass show`), whose bare
    /// secret has no shape to detect. `None` when there was nothing to mask.
    pub(super) fn mask_all(&mut self, rendered: &[u8]) -> Option<Vec<u8>> {
        self.process(rendered, true)
    }

    fn process(&mut self, rendered: &[u8], all: bool) -> Option<Vec<u8>> {
        // The shown bytes, and where each sits in `rendered`.
        let mut shown = Vec::with_capacity(rendered.len());
        let mut at = Vec::with_capacity(rendered.len());
        let mut i = 0;
        while i < rendered.len() {
            if rendered[i] == 0x1b {
                // An unterminated escape: its tail is not shown text.
                i += super::links::escape_len(&rendered[i..]).unwrap_or(rendered.len() - i);
                continue;
            }
            shown.push(rendered[i]);
            at.push(i);
            i += 1;
        }
        let mut masked = vec![false; shown.len()];
        let mut any = false;
        let mut start = 0;
        while start < shown.len() {
            // A line ends at `\n` or `\r` (a redraw starts a new one).
            let end = shown[start..]
                .iter()
                .position(|&b| b == b'\n' || b == b'\r')
                .map_or(shown.len(), |len| start + len);
            let ended = end < shown.len();
            let segment = &shown[start..end];
            let carried = self.carry.len();
            let mut line = std::mem::take(&mut self.carry);
            line.extend_from_slice(segment);
            let spans = if all {
                vec![(0, line.len())]
            } else {
                self.spans(&line)
            };
            for (from, to) in spans {
                // Only what this call writes can still be masked.
                let from = from.max(carried) - carried;
                let to = to.saturating_sub(carried).min(segment.len());
                for index in start + from..start + to.max(from) {
                    if maskable(shown[index]) {
                        masked[index] = true;
                        any = true;
                    }
                }
            }
            if !ended {
                // Keep the tail, cut on a character boundary.
                let mut cut = line.len().saturating_sub(CARRY_CAP);
                while cut < line.len() && line[cut] & 0xc0 == 0x80 {
                    cut += 1;
                }
                line.drain(..cut);
                self.carry = line;
            }
            start = end + 1;
        }
        if !any {
            return None;
        }
        let mut hide = vec![false; rendered.len()];
        for (index, &position) in at.iter().enumerate() {
            hide[position] = masked[index];
        }
        // One pass: a masked character becomes as many `*` as the columns it
        // takes, its continuation bytes go, everything else is copied.
        let mut out = Vec::with_capacity(rendered.len());
        let mut i = 0;
        while i < rendered.len() {
            let byte = rendered[i];
            if !hide[i] {
                out.push(byte);
                i += 1;
                continue;
            }
            let len = utf8_len(byte).min(rendered.len() - i);
            let width = std::str::from_utf8(&rendered[i..i + len])
                .ok()
                .and_then(|text| text.chars().next())
                .map_or(1, columns);
            out.extend(std::iter::repeat_n(MASK, width));
            i += len.max(1);
        }
        Some(out)
    }

    /// Byte ranges of one line (no line break) to mask.
    fn spans(&mut self, line: &[u8]) -> Vec<(usize, usize)> {
        let trimmed = trim(line);
        if self.in_private_key {
            if is_key_marker(trimmed, b"-----END ") {
                self.in_private_key = false;
                return Vec::new();
            }
            self.key_lines += 1;
            if self.key_lines > KEY_BLOCK_CAP || !is_key_body(trimmed) {
                // Not a key after all (or one too long to be real): stop
                // masking rather than hide the rest of the output.
                self.in_private_key = false;
            } else {
                return nonblank_span(line).into_iter().collect();
            }
        }
        if is_key_marker(trimmed, b"-----BEGIN ") {
            self.in_private_key = true;
            self.key_lines = 0;
            return Vec::new();
        }
        let mut spans = Vec::new();
        find_tokens(line, &mut spans);
        find_jwts(line, &mut spans);
        find_url_passwords(line, &mut spans);
        find_authorization(line, &mut spans);
        find_secret_fields(line, &mut spans);
        spans.retain(|&(from, to)| from < to && to - from <= MAX_SECRET);
        spans
    }
}

/// Bytes a mask may replace: shown characters, never control bytes (a `\r`,
/// a backspace or a tab is the program's cursor movement, not text).
fn maskable(byte: u8) -> bool {
    byte >= 0x20 && byte != 0x7f
}

/// The length of the UTF-8 sequence a lead byte starts (1 for anything else).
fn utf8_len(lead: u8) -> usize {
    match lead {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 1,
    }
}

/// Terminal columns a character takes, close enough to keep a masked line
/// the width it was: combining marks and joiners take none, East Asian wide
/// characters and emoji take two.
fn columns(c: char) -> usize {
    match u32::from(c) {
        0x0300..=0x036f | 0x200b..=0x200f | 0xfe00..=0xfe0f | 0x20d0..=0x20ff => 0,
        0x1100..=0x115f
        | 0x2e80..=0x303e
        | 0x3041..=0x33ff
        | 0x3400..=0x4dbf
        | 0x4e00..=0x9fff
        | 0xa000..=0xa4cf
        | 0xac00..=0xd7a3
        | 0xf900..=0xfaff
        | 0xfe30..=0xfe4f
        | 0xff00..=0xff60
        | 0xffe0..=0xffe6
        | 0x1f300..=0x1f64f
        | 0x1f900..=0x1f9ff
        | 0x20000..=0x3fffd => 2,
        _ => 1,
    }
}

/// The line without surrounding ASCII whitespace.
fn trim(line: &[u8]) -> &[u8] {
    line.trim_ascii()
}

/// A whole line `-----BEGIN [WORDS ]PRIVATE KEY[ BLOCK]-----` (or `END`):
/// anchored, so a line that merely mentions one (`grep` output, a string in
/// source code) does not open a block.
fn is_key_marker(trimmed: &[u8], start: &[u8]) -> bool {
    let Some(rest) = trimmed.strip_prefix(start) else {
        return false;
    };
    let Some(rest) = rest.strip_suffix(b"-----") else {
        return false;
    };
    let words = rest
        .strip_suffix(b"PRIVATE KEY")
        .or_else(|| rest.strip_suffix(b"PRIVATE KEY BLOCK"));
    words.is_some_and(|words| {
        words
            .iter()
            .all(|&b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b' ')
    })
}

/// A line that can be inside a PEM/OpenSSH/PGP key: base64, blank, or a
/// `Header: value` line (`Proc-Type:`, `Comment:`).
fn is_key_body(trimmed: &[u8]) -> bool {
    trimmed
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
        || trimmed
            .iter()
            .position(|&b| b == b':')
            .is_some_and(|colon| {
                colon > 0
                    && trimmed[..colon]
                        .iter()
                        .all(|&b| b.is_ascii_alphanumeric() || b == b'-')
            })
}

/// The span from the first to the last non-blank byte, if any.
fn nonblank_span(line: &[u8]) -> Option<(usize, usize)> {
    let from = line.iter().position(|b| !b.is_ascii_whitespace())?;
    let to = line.iter().rposition(|b| !b.is_ascii_whitespace())? + 1;
    Some((from, to))
}

/// Whether `b` could continue an identifier-like run, so a token found right
/// after it is really the middle of a longer word.
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Vendor tokens. Every body scan is bounded by the token's maximum length,
/// and a failed run is skipped whole, so a hostile line (`xoxb-xoxb-…`) costs
/// linear time.
fn find_tokens(line: &[u8], spans: &mut Vec<(usize, usize)>) {
    let mut i = 0;
    while i < line.len() {
        if i > 0 && is_word_byte(line[i - 1]) {
            i += 1;
            continue;
        }
        let rest = &line[i..];
        let mut advance = 1;
        for token in TOKENS {
            let Some(body) = rest.strip_prefix(token.prefix) else {
                continue;
            };
            let len = body
                .iter()
                .take(token.max + 1)
                .take_while(|&&b| token.body.allows(b))
                .count();
            if (token.min..=token.max).contains(&len) {
                spans.push((i + token.prefix.len(), i + token.prefix.len() + len));
                advance = token.prefix.len() + len;
                break;
            }
            advance = advance.max(token.prefix.len() + len);
        }
        i += advance;
    }
}

/// `eyJ<header>.eyJ<payload>.<signature>`: all but the leading `eyJ`.
fn find_jwts(line: &[u8], spans: &mut Vec<(usize, usize)>) {
    let segment = |bytes: &[u8]| {
        bytes
            .iter()
            .take(MAX_SECRET + 1)
            .take_while(|&&b| Body::UrlSafe.allows(b))
            .count()
    };
    let mut from = 0;
    while let Some(found) = find(&line[from..], b"eyJ") {
        let start = from + found;
        from = start + 3;
        if start > 0 && is_word_byte(line[start - 1]) {
            continue;
        }
        let header = segment(&line[start..]);
        // Nothing starting inside this run can do better: skip it whole.
        from = from.max(start + header);
        let mut end = start + header;
        if header < 10 || line.get(end) != Some(&b'.') {
            continue;
        }
        let payload = segment(&line[end + 1..]);
        if payload < 10 || !line[end + 1..].starts_with(b"eyJ") {
            continue;
        }
        end += 1 + payload;
        if line.get(end) != Some(&b'.') {
            continue;
        }
        let signature = segment(&line[end + 1..]);
        if signature < 8 {
            continue;
        }
        end += 1 + signature;
        spans.push((start + 3, end));
        from = end;
    }
}

/// The password in `scheme://user:password@host`.
fn find_url_passwords(line: &[u8], spans: &mut Vec<(usize, usize)>) {
    let mut from = 0;
    while let Some(found) = find(&line[from..], b"://") {
        let authority = from + found + 3;
        let end = line[authority..]
            .iter()
            .position(|&b| matches!(b, b'/' | b'?' | b'#') || b.is_ascii_whitespace())
            .map_or(line.len(), |len| authority + len);
        from = authority.max(end);
        let userinfo = &line[authority..end];
        // The last `@` ends the credentials.
        let Some(at) = userinfo.iter().rposition(|&b| b == b'@') else {
            continue;
        };
        let Some(colon) = userinfo[..at].iter().position(|&b| b == b':') else {
            continue;
        };
        if colon == 0 || colon + 1 >= at {
            continue;
        }
        spans.push((authority + colon + 1, authority + at));
    }
}

/// `Bearer <token>` anywhere; `Basic <creds>` and `Token <token>` only on a
/// line that names `Authorization`.
fn find_authorization(line: &[u8], spans: &mut Vec<(usize, usize)>) {
    let lower = line.to_ascii_lowercase();
    let authorization = contains(&lower, b"authorization");
    for (scheme, needs_header, min) in [
        (&b"bearer "[..], false, 16),
        (&b"basic "[..], true, 8),
        (&b"token "[..], true, 8),
    ] {
        if needs_header && !authorization {
            continue;
        }
        let mut from = 0;
        while let Some(found) = find(&lower[from..], scheme) {
            let start = from + found;
            from = start + scheme.len();
            if start > 0 && is_word_byte(lower[start - 1]) {
                continue;
            }
            let value = from;
            let len = line[value..]
                .iter()
                .take(MAX_SECRET + 1)
                .take_while(|&&b| {
                    b.is_ascii_alphanumeric()
                        || matches!(b, b'.' | b'_' | b'~' | b'+' | b'/' | b'-' | b'=')
                })
                .count();
            if (min..=MAX_SECRET).contains(&len) {
                spans.push((value, value + len));
            }
            from = value + len;
        }
    }
}

/// Values of secret-named fields: `NAME=value`, `NAME: value`,
/// `"name": "value"`, `name = "value"`, `?name=value`.
fn find_secret_fields(line: &[u8], spans: &mut Vec<(usize, usize)>) {
    let first = line
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(line.len());
    let mut i = 0;
    while i < line.len() {
        let separator = line[i];
        if separator != b'=' && separator != b':' {
            i += 1;
            continue;
        }
        // `==`, `:=`, `=>`, `::` and `://` are operators or schemes.
        let next = line.get(i + 1).copied();
        if matches!(next, Some(b'=' | b':' | b'>' | b'/')) || (i > 0 && line[i - 1] == b'=') {
            i += 2;
            continue;
        }
        let Some((name_start, name)) = field_name(&line[..i]) else {
            i += 1;
            continue;
        };
        let Some(strength) = secret_name(name) else {
            i += 1;
            continue;
        };
        // An assignment that starts the line (dotenv, `env`, `export`, YAML,
        // headers) owns the rest of the line; one mid-line (query strings,
        // flags, log text) ends at the next separator.
        let starts_line = name_start == first
            || line[first..name_start]
                .strip_prefix(b"export ")
                .is_some_and(|rest| rest.iter().all(u8::is_ascii_whitespace))
            || line[first..name_start]
                .iter()
                .all(|&b| matches!(b, b'-' | b' ' | b'"' | b'\''));
        let mut value = i + 1;
        while line.get(value).is_some_and(|&b| b == b' ' || b == b'\t') {
            value += 1;
        }
        if separator == b':'
            && value == i + 1
            && line.get(value).is_some_and(|&b| b != b'"' && b != b'\'')
        {
            // `name:value` without a space is a URL, a time or a key path.
            i += 1;
            continue;
        }
        let Some((from, to)) = field_value(line, value, starts_line) else {
            i += 1;
            continue;
        };
        if !is_placeholder(&line[from..to]) && !is_unlikely_secret(&line[from..to], strength) {
            spans.push((from, to));
        }
        i = to.max(i + 1);
    }
}

/// The field name ending right before a separator at `before.len()`, with an
/// optional closing quote and spaces in between (`"name" :`), and where it
/// starts.
fn field_name(before: &[u8]) -> Option<(usize, &[u8])> {
    let mut end = before.len();
    while end > 0 && before[end - 1] == b' ' {
        end -= 1;
    }
    let quoted = end > 0 && matches!(before[end - 1], b'"' | b'\'');
    if quoted {
        end -= 1;
    }
    // Names are short; never scan back further than this.
    let floor = end.saturating_sub(128);
    let start = before[floor..end]
        .iter()
        .rposition(|&b| !(b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')))
        .map_or(floor, |at| floor + at + 1);
    if start == end {
        return None;
    }
    let name_start = if quoted && start > 0 && matches!(before[start - 1], b'"' | b'\'') {
        start - 1
    } else {
        start
    };
    Some((name_start, &before[start..end]))
}

/// How sure a field name is about holding a secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strength {
    /// A password: even a short or numeric value (`PIN`-like) is masked.
    Password,
    /// A token, key or secret: values must look like one.
    Secret,
}

/// Whether a field name holds a secret (`DB_PASSWORD`, `clientSecret`,
/// `x-api-key`), and how sure that is. A name that only describes a secret
/// (`token_type`, `credential.helper`) does not.
fn secret_name(name: &[u8]) -> Option<Strength> {
    let name = std::str::from_utf8(name).ok()?;
    let mut segments: Vec<String> = Vec::new();
    for part in name.split(|c: char| !c.is_ascii_alphanumeric()) {
        let mut current = String::new();
        let mut previous_lower = false;
        for c in part.chars() {
            if c.is_ascii_uppercase() && previous_lower && !current.is_empty() {
                segments.push(std::mem::take(&mut current));
            }
            previous_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
            current.push(c.to_ascii_uppercase());
        }
        if !current.is_empty() {
            segments.push(current);
        }
    }
    if segments
        .last()
        .is_some_and(|last| DESCRIPTIVE_SEGMENTS.contains(&last.as_str()))
    {
        return None;
    }
    if segments
        .iter()
        .any(|segment| matches!(segment.as_str(), "PASSWORD" | "PASSWD" | "PASSPHRASE"))
    {
        return Some(Strength::Password);
    }
    let secret = segments
        .iter()
        .any(|segment| SECRET_SEGMENTS.contains(&segment.as_str()))
        || segments.windows(2).any(|pair| {
            SECRET_PAIRS
                .iter()
                .any(|(a, b)| pair[0] == *a && pair[1] == *b)
        });
    secret.then_some(Strength::Secret)
}

/// Values a secret-named field holds in code and logs rather than a secret:
/// shorter than 4 characters (`'}'`, `[`), a number for anything but a
/// password (`token_count: 12`), a type (`password: String`,
/// `token: Option<&str>`), or something that opens a block (`= [`).
fn is_unlikely_secret(value: &[u8], strength: Strength) -> bool {
    let text = String::from_utf8_lossy(value);
    let trimmed =
        text.trim_matches(|c: char| c == '&' || c == ',' || c == ';' || c.is_whitespace());
    if trimmed.chars().count() < 4 && strength == Strength::Secret {
        return true;
    }
    if trimmed.chars().count() < 2 {
        return true;
    }
    if strength == Strength::Secret && trimmed.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return true;
    }
    if trimmed.ends_with(['[', '(', '{']) {
        return true;
    }
    const TYPES: &[&str] = &[
        "String",
        "str",
        "string",
        "bool",
        "boolean",
        "number",
        "int",
        "i32",
        "i64",
        "u32",
        "u64",
        "usize",
        "Option",
        "Vec",
        "Secret",
        "SecretString",
        "SecretBox",
        "Box",
        "Any",
        "object",
        "Object",
        "bytes",
        "Bytes",
        "Buffer",
        "char",
        "text",
        "varchar",
        "VARCHAR",
        "TEXT",
    ];
    TYPES.iter().any(|ty| {
        trimmed
            .strip_prefix(ty)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['<', '(', '?', '[']))
    })
}

/// The value starting at `from`: inside its quotes when quoted; else to the
/// end of the line when `whole_line`, or to the next space, `&`, `,`, `;` or
/// quote otherwise. Trailing commas and spaces are not part of it.
fn field_value(line: &[u8], from: usize, whole_line: bool) -> Option<(usize, usize)> {
    let first = *line.get(from)?;
    if first == b'"' || first == b'\'' {
        let close = line[from + 1..].iter().position(|&b| b == first)?;
        return Some((from + 1, from + 1 + close));
    }
    let end = if whole_line {
        line.len()
    } else {
        line[from..]
            .iter()
            .position(|&b| {
                b.is_ascii_whitespace() || matches!(b, b'&' | b',' | b';' | b'"' | b'\'')
            })
            .map_or(line.len(), |len| from + len)
    };
    let mut to = end;
    while to > from && (line[to - 1].is_ascii_whitespace() || line[to - 1] == b',') {
        to -= 1;
    }
    (to > from).then_some((from, to))
}

/// Values that are clearly not secrets: empty-ish, booleans, `null`, an
/// already-masked value, or a reference to one (`$SECRET`, `${TOKEN}`,
/// `<your-token>`).
fn is_placeholder(value: &[u8]) -> bool {
    let lower = value.to_ascii_lowercase();
    matches!(
        lower.as_slice(),
        b"" | b"null" | b"none" | b"nil" | b"true" | b"false" | b"undefined" | b"{}" | b"[]"
    ) || value
        .iter()
        .all(|&b| matches!(b, b'*' | b'x' | b'X' | b'.' | b'-'))
        || value.starts_with(b"$")
        || (value.starts_with(b"<") && value.ends_with(b">"))
        || (value.starts_with(b"{{") && value.ends_with(b"}}"))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    find(haystack, needle).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// The visible result of redacting one line.
    fn masked(line: &str) -> String {
        let mut redactor = Redactor::new();
        match redactor.redact(line.as_bytes()) {
            Some(out) => String::from_utf8(out).unwrap(),
            None => line.to_string(),
        }
    }

    fn stars(n: usize) -> String {
        "*".repeat(n)
    }

    #[test]
    fn vendor_tokens_keep_their_prefix() {
        let ghp = format!("ghp_{}", "a1B2".repeat(9));
        assert_eq!(
            masked(&format!("token {ghp} ok")),
            format!("token ghp_{} ok", stars(36))
        );
        let cases = [
            format!("github_pat_{}", "A1_b".repeat(20)),
            format!("glpat-{}", "x".repeat(20)),
            "xoxb-1234567890-abcdefghij".to_string(),
            format!("sk_live_{}", "Z9".repeat(12)),
            format!("sk-ant-api03-{}", "a".repeat(40)),
            format!("sk-proj-{}", "b".repeat(40)),
            format!("sk-{}", "c1".repeat(24)),
            "AKIAIOSFODNN7EXAMPLE".to_string(),
            format!("AIza{}", "d".repeat(35)),
            format!("npm_{}", "e".repeat(36)),
            format!("hf_{}", "f".repeat(34)),
            format!("dop_v1_{}", "a0".repeat(32)),
            format!("shpat_{}", "Ab".repeat(16)),
            format!("SG.{}.{}", "g".repeat(22), "h".repeat(43)),
            format!("AGE-SECRET-KEY-1{}", "Q".repeat(58)),
        ];
        for token in cases {
            let out = masked(&format!("x {token} y"));
            assert_ne!(out, format!("x {token} y"), "{token}");
            assert_eq!(out.len(), token.len() + 4, "{token}");
            assert!(out.ends_with(" y") && out.starts_with("x "), "{out}");
        }
    }

    #[test]
    fn near_misses_are_left_alone() {
        for line in [
            "ghp_short",
            "the sk-learn package",
            "AKIA is a prefix",
            "XAKIAIOSFODNN7EXAMPLE",
            "AKIAIOSFODNN7EXAMPLE1",
            "my_ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "TOKENIZERS_PARALLELISM=false",
            "Author: Krish <k@example.com>",
            "PWD=/home/me",
            "SSH_AUTH_SOCK=/tmp/agent.sock",
            "https://example.com:8443/path",
            "time 12:30:45",
            "if a == b && c := d",
            "password: ",
            "API_KEY=",
            "password: null",
            "SECRET=${SECRET}",
            "token = \"<your-token>\"",
            "api_key: \"********\"",
            "eyJhbGciOi",
        ] {
            assert_eq!(masked(line), line, "{line}");
        }
    }

    #[test]
    fn secret_fields_mask_their_values() {
        assert_eq!(masked("API_KEY=abc123"), format!("API_KEY={}", stars(6)));
        assert_eq!(
            masked("export DB_PASSWORD='hunter 2'"),
            format!("export DB_PASSWORD='{}'", stars(8))
        );
        assert_eq!(
            masked("  \"clientSecret\": \"s3cr3t\","),
            format!("  \"clientSecret\": \"{}\",", stars(6))
        );
        assert_eq!(
            masked("{\"user\":\"me\",\"access_token\":\"abcdef\"}"),
            format!("{{\"user\":\"me\",\"access_token\":\"{}\"}}", stars(6))
        );
        assert_eq!(
            masked("password: hunter2"),
            format!("password: {}", stars(7))
        );
        assert_eq!(
            masked("- passphrase: a b c"),
            format!("- passphrase: {}", stars(5))
        );
        assert_eq!(
            masked("X-Api-Key: abc.def"),
            format!("X-Api-Key: {}", stars(7))
        );
        assert_eq!(
            masked("GET /cb?code=1&access_token=xyz789&state=2"),
            format!("GET /cb?code=1&access_token={}&state=2", stars(6))
        );
        assert_eq!(
            masked("run --token=abcd --verbose"),
            format!("run --token={} --verbose", stars(4))
        );
        assert_eq!(
            masked("SECRET_KEY_BASE = \"a1b2c3d4e5\""),
            format!("SECRET_KEY_BASE = \"{}\"", stars(10))
        );
        // A dotenv line owns the rest of the line, `&` and all.
        assert_eq!(
            masked("PASSWORD=ab&cd ef"),
            format!("PASSWORD={}", stars(8))
        );
        // The line ending stays.
        assert_eq!(masked("TOKEN=abcd\r"), format!("TOKEN={}\r", stars(4)));
    }

    #[test]
    fn url_passwords_jwts_and_authorization() {
        assert_eq!(
            masked("DATABASE_URL is postgres://app:pa55@db:5432/main"),
            format!("DATABASE_URL is postgres://app:{}@db:5432/main", stars(4))
        );
        assert_eq!(masked("https://user@host/x"), "https://user@host/x");
        let jwt = format!(
            "eyJ{}.eyJ{}.{}",
            "a".repeat(20),
            "b".repeat(30),
            "c".repeat(43)
        );
        assert_eq!(
            masked(&format!("id_token {jwt}")),
            format!("id_token eyJ{}", stars(jwt.len() - 3))
        );
        let bearer = "a".repeat(32);
        assert_eq!(
            masked(&format!("curl -H 'Authorization: Bearer {bearer}'")),
            format!("curl -H 'Authorization: Bearer {}'", stars(32))
        );
        assert_eq!(
            masked("Authorization: Basic dXNlcjpwYXNz"),
            format!("Authorization: Basic {}", stars(12))
        );
        // `Basic` without an Authorization header is just a word.
        assert_eq!(masked("Basic usage example"), "Basic usage example");
    }

    #[test]
    fn private_key_blocks_mask_their_body() {
        let mut redactor = Redactor::new();
        let block = "-----BEGIN OPENSSH PRIVATE KEY-----\r\nb3BlbnNzaC1rZXk\r\n  AAAAB3Nza  \r\n-----END OPENSSH PRIVATE KEY-----\r\nafter\r\n";
        let out = String::from_utf8(redactor.redact(block.as_bytes()).unwrap()).unwrap();
        assert_eq!(
            out,
            format!(
                "-----BEGIN OPENSSH PRIVATE KEY-----\r\n{}\r\n  {}  \r\n-----END OPENSSH PRIVATE KEY-----\r\nafter\r\n",
                stars(15),
                stars(9)
            )
        );
        // The block also spans separate calls (one line at a time).
        let mut redactor = Redactor::new();
        assert!(redactor
            .redact(b"-----BEGIN RSA PRIVATE KEY-----\r\n")
            .is_none());
        assert_eq!(redactor.redact(b"MIIEow\r\n").unwrap(), b"******\r\n");
        assert!(redactor
            .redact(b"-----END RSA PRIVATE KEY-----\r\n")
            .is_none());
        assert!(redactor.redact(b"plain\r\n").is_none());
        // A public key is not secret.
        let mut redactor = Redactor::new();
        assert!(redactor
            .redact(b"-----BEGIN PUBLIC KEY-----\r\nMIIB\r\n")
            .is_none());
    }

    #[test]
    fn escapes_and_multibyte_text_survive() {
        let mut redactor = Redactor::new();
        let rendered = b"\x1b[36mAPI_KEY\x1b[0m\x1b[2m=\x1b[0m\x1b[33mab\x1b(Bc\xc3\xa9\x1b[0m\r\n";
        let out = redactor.redact(rendered).unwrap();
        assert_eq!(
            out,
            b"\x1b[36mAPI_KEY\x1b[0m\x1b[2m=\x1b[0m\x1b[33m**\x1b(B**\x1b[0m\r\n"
        );
        assert!(std::str::from_utf8(&out).is_ok());
    }

    #[test]
    fn redaction_is_idempotent_and_never_moves_line_breaks() {
        let text = "API_KEY=abc\npassword: x y\nghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n";
        let once = Redactor::new().redact(text.as_bytes()).unwrap();
        assert!(Redactor::new().redact(&once).is_none());
        assert_eq!(
            once.iter().filter(|&&b| b == b'\n').count(),
            text.matches('\n').count()
        );
    }

    #[test]
    fn hostile_lines_cost_linear_time() {
        // The audit's inputs (61 s per MiB before the bounded scans, so about
        // 4 s for these 256 KiB even optimized), plus the other scanners'
        // worst cases, in one budget an unoptimized build meets with room.
        let mib = 1 << 18;
        let cases: Vec<Vec<u8>> = vec![
            b"-eyJ".repeat(mib / 4),
            b"xoxb-".repeat(mib / 5),
            b"ghp_".repeat(mib / 4),
            b"sk-".repeat(mib / 3),
            b"AKIA".repeat(mib / 4),
            b"bearer ".repeat(mib / 7),
            b"://a:b@".repeat(mib / 7),
            b"token=".repeat(mib / 6),
            b"a".repeat(mib - 1).into_iter().chain(*b":").collect(),
            b"x.y.z-"
                .repeat(mib / 6)
                .into_iter()
                .chain(*b"_TOKEN: v")
                .collect(),
        ];
        let started = std::time::Instant::now();
        for case in &cases {
            let _ = Redactor::new().redact(case);
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn control_bytes_are_never_masked() {
        // A redraw (`\r`) starts a new line; backspace-erase and tabs stay.
        assert_eq!(
            masked("TOKEN=abcd\rTOKEN=wxyz"),
            format!("TOKEN={}\rTOKEN={}", stars(4), stars(4))
        );
        assert_eq!(
            masked("password: ab\x08 \x08cd\tef"),
            "password: **\x08*\x08**\t**"
        );
    }

    #[test]
    fn fields_split_by_escapes_are_still_found() {
        // jq colors the key, the colon and the value separately.
        let mut redactor = Redactor::new();
        assert!(redactor.redact(b"  \"access_token\"").is_none());
        assert!(redactor.redact(b":").is_none());
        assert_eq!(
            redactor.redact(b"\"abc123secretvalue\",\r\n").unwrap(),
            format!("\"{}\",\r\n", stars(17)).as_bytes()
        );
        // The line ended: nothing carries into the next one.
        assert!(redactor.redact(b"plain words\r\n").is_none());
        // A token split by a color change: the part not yet shown is masked.
        let mut redactor = Redactor::new();
        assert!(redactor.redact(b"ghp_").is_none());
        let rest = "a".repeat(36);
        assert_eq!(
            redactor
                .redact(format!("{rest} done\n").as_bytes())
                .unwrap(),
            format!("{} done\n", stars(36)).as_bytes()
        );
    }

    #[test]
    fn ordinary_output_from_the_audit_is_left_alone() {
        for line in [
            "  \"prompt_tokens\": 12,",
            "max_tokens: 4096",
            "Tokens: 2.4k sent, 120 received",
            "    password: String,",
            "    token: Option<&str>,",
            "PASSWORD_VALIDATORS = [",
            "unexpected token: '}'",
            "credential.helper=osxkeychain",
            "  \"token_type\": \"Bearer\",",
            "api_key_id = 1234",
            "secret_name: prod-db",
            "src/keys.rs:3: const PEM: &str = \"-----BEGIN RSA PRIVATE KEY-----\";",
        ] {
            assert_eq!(masked(line), line, "{line}");
        }
        // A mention of a key block does not open one.
        let mut redactor = Redactor::new();
        assert!(redactor
            .redact(b"grep: \"-----BEGIN RSA PRIVATE KEY-----\"\nnext line\nmore\n")
            .is_none());
        // A block whose body is not key material closes instead of hiding
        // the rest of the output.
        let mut redactor = Redactor::new();
        let out = redactor
            .redact(b"-----BEGIN RSA PRIVATE KEY-----\nMIIEow\nthis is prose, not base64\nlater\n")
            .unwrap();
        assert_eq!(
            out,
            b"-----BEGIN RSA PRIVATE KEY-----\n******\nthis is prose, not base64\nlater\n"
        );
    }

    #[test]
    fn masks_keep_the_width_of_what_they_hide() {
        // Wide characters take two columns, combining marks none.
        assert_eq!(masked("password: \u{79d8}\u{5bc6}"), "password: ****");
        assert_eq!(masked("password: e\u{301}tre"), "password: ****");
    }

    #[test]
    fn escape_lengths_match_the_scanner() {
        use super::super::links::escape_len;
        assert_eq!(escape_len(b"\x1b\x1b[31m"), Some(1));
        assert_eq!(escape_len(b"\x1b[3\x1b[31m"), Some(3));
        assert_eq!(escape_len(b"\x1b]0;t\x1b[m"), Some(5));
        assert_eq!(escape_len(b"\x1b(\x1b[m"), Some(2));
        assert_eq!(escape_len(b"\x1b(\x18B"), Some(3));
        assert_eq!(escape_len(b"\x1bP\x07x\x1b\\"), Some(6));
        // `API_KEY=` then a stray ESC: the mask never creates an escape.
        let out = Redactor::new()
            .redact(b"API_KEY=\x1b\x1b[31mvalue1\x1b[0m")
            .unwrap();
        assert_eq!(out, b"API_KEY=\x1b\x1b[31m******\x1b[0m");
    }

    /// Split rendered bytes into (escape sequences in order, shown text).
    fn split(bytes: &[u8]) -> (Vec<Vec<u8>>, Vec<u8>) {
        let mut escapes = Vec::new();
        let mut shown = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == 0x1b {
                let len = super::super::links::escape_len(&bytes[i..]).unwrap_or(bytes.len() - i);
                escapes.push(bytes[i..i + len].to_vec());
                i += len;
            } else {
                shown.push(bytes[i]);
                i += 1;
            }
        }
        (escapes, shown)
    }

    proptest! {
        /// Masking only ever turns whole shown characters into `*`: every
        /// escape sequence survives in order, line breaks stay put, the shown
        /// character count is unchanged, and valid UTF-8 stays valid.
        #[test]
        fn prop_masking_only_replaces_shown_characters(
            parts in proptest::collection::vec(
                proptest::prop_oneof![
                    Just("API_KEY=".to_string()),
                    Just("password: ".to_string()),
                    Just("ghp_".to_string()),
                    Just("AKIA".to_string()),
                    Just("eyJ".to_string()),
                    Just("://u:p@h".to_string()),
                    Just("Bearer ".to_string()),
                    Just("-----BEGIN RSA PRIVATE KEY-----".to_string()),
                    Just("-----END RSA PRIVATE KEY-----".to_string()),
                    Just("\x1b[31m".to_string()),
                    Just("\x1b]8;;https://x\x1b\\".to_string()),
                    Just("\x1b(B".to_string()),
                    Just("\r\n".to_string()),
                    Just("\n".to_string()),
                    "[a-zA-Z0-9_.=:\"' -]{0,40}",
                    "\\PC{0,8}",
                ],
                0..30,
            ),
        ) {
            let input = parts.concat();
            let mut redactor = Redactor::new();
            let Some(out) = redactor.redact(input.as_bytes()) else {
                return Ok(());
            };
            let out = String::from_utf8(out);
            prop_assert!(out.is_ok(), "masking produced invalid UTF-8");
            let out = out.unwrap_or_default();
            let (escapes_in, shown_in) = split(input.as_bytes());
            let (escapes_out, shown_out) = split(out.as_bytes());
            prop_assert_eq!(escapes_in, escapes_out);
            // Each shown character is either kept or becomes one `*` per
            // column it takes; control characters are always kept.
            let shown_in = String::from_utf8_lossy(&shown_in).into_owned();
            let shown_out = String::from_utf8_lossy(&shown_out).into_owned();
            let mut rest = shown_out.chars().peekable();
            for c in shown_in.chars() {
                if rest.peek() == Some(&c) {
                    rest.next();
                    continue;
                }
                prop_assert!(!c.is_control(), "control {:?} was masked", c);
                for _ in 0..columns(c) {
                    prop_assert_eq!(rest.next(), Some('*'), "{:?} not masked to its width", c);
                }
            }
            prop_assert_eq!(rest.next(), None);
        }

        /// Arbitrary bytes never panic.
        #[test]
        fn prop_arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
            let _ = Redactor::new().redact(&bytes);
        }
    }
}
