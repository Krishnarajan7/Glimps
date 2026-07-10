//! Error-line pinning — find the one line worth repeating in the failure
//! footer (F3 of `docs/FAILURE_INTELLIGENCE_PLAN.md`).
//!
//! When a command fails deep in a wall of output, the question is always
//! "where's the actual error?". This module watches the output zone as it
//! streams and remembers the best candidate, so `emit_command_status` can
//! quote it under the `✗` line with a how-far-up hint.
//!
//! ## Why a shadow assembler
//!
//! The OSC-133 scanner routes every escape byte to `Seg::Pass`, so colored
//! output (rustc/cargo error lines are colored — they see a TTY because
//! GLIMPS owns the PTY) reaches the streaming path *fragmented*, never as
//! whole lines. `ErrorPin` therefore assembles its own private copy of each
//! line at the segment level: it is fed both Output bytes and in-zone Pass
//! bytes, strips escape sequences with a small state machine, and matches on
//! the clean text. It is strictly **read-only** — nothing it does can alter
//! a single emitted byte, so the worst possible bug is a missed or wrong
//! pin, never corrupted output.
//!
//! ## The confidence ladder
//!
//! A wrong pin is worse than no pin (invariant #2 applied to chrome), so
//! matching is tiered, high-precision first:
//!
//! 1. **Tool errors** (first wins): rustc `error[…]:` / `error:` (with the
//!    `--> file:line` of the *next* line attached), gcc/clang `: error: `,
//!    git `fatal: `, Rust panics. Compilers cascade — the first error is
//!    the cause.
//! 2. **Exception lines** (last wins): `ValueError: broken config` with the
//!    deepest preceding `File "…", line N` frame attached. Tracebacks put
//!    the point *last*.
//! 3. **ERROR-severity log lines** (first wins), reusing `linefmt`'s
//!    delimited-token matcher.
//!
//! Nothing confident → no pin, and the footer stays one line.
//!
//! Bounded by construction: one line buffer capped at [`MATCH_CAP`] (longer
//! lines are counted but never matched), three candidate slots, O(n) feed.

use super::linefmt;

/// Longest line we attempt to match or quote. Beyond this, a line is counted
/// for the distance hint but skipped for matching — a multi-KB line is not a
/// human-readable error message.
const MATCH_CAP: usize = 1024;

/// How far into a line we scan for infix markers (`: error: `). Compiler
/// messages put the path first; 256 bytes of path is already generous.
const INFIX_WINDOW: usize = 256;

/// One pinned candidate: the (escape-stripped) line text and which complete
/// line of the command's output it was, 1-based.
struct Candidate {
    text: Vec<u8>,
    line_no: usize,
}

/// The chosen pin, handed to the footer renderer.
pub(crate) struct PinnedError {
    /// Escape-free line text (still needs `sanitize_display` + truncation
    /// before entering GLIMPS chrome — raw control bytes may remain).
    pub text: Vec<u8>,
    /// How many complete lines ago it scrolled past (0 = the last line).
    pub lines_up: usize,
}

/// Escape-stripping state machine state.
#[derive(Clone, Copy, PartialEq)]
enum Esc {
    Plain,
    /// Saw ESC; deciding the sequence kind.
    Escape,
    /// Inside CSI (`ESC [ … final`), final byte is `0x40..=0x7E`.
    Csi,
    /// Inside a string sequence — OSC (`ESC ] … BEL`/`ESC ] … ESC \`) or
    /// DCS/SOS/PM/APC (`ESC P/X/^/_ … ESC \`) — whose body is not text.
    Osc,
    /// Saw ESC inside a string sequence; a `\` terminates it.
    OscEscape,
}

/// Streaming error-line detector for one command's output zone.
pub(crate) struct ErrorPin {
    esc: Esc,
    /// Saw a `\r`; the next byte decides whether it was a CRLF line ending
    /// (`\n` follows — PTY line discipline ends EVERY line this way) or a
    /// progress-bar overwrite (anything else follows). Chunk-split safe.
    pending_cr: bool,
    /// The current line, escape-stripped, capped at [`MATCH_CAP`].
    line: Vec<u8>,
    /// The current line outgrew [`MATCH_CAP`]; count it, don't match it.
    overflow: bool,
    /// Complete lines seen so far (1-based line numbers come from this).
    lines_seen: usize,
    /// Tier 1: first tool-formatted error.
    primary: Option<Candidate>,
    /// The line after a rustc-style `error…` header may be its `--> file:line`
    /// location; this is armed for exactly one following line.
    awaiting_location: bool,
    /// Tier 2: the latest exception line (tracebacks put the point last).
    exception: Option<Candidate>,
    /// Deepest `File "…", line N` frame seen so far, as `path:N`.
    last_frame: Option<Vec<u8>>,
    /// Tier 3: first ERROR-severity log line.
    log_error: Option<Candidate>,
}

impl ErrorPin {
    pub(crate) fn new() -> Self {
        ErrorPin {
            esc: Esc::Plain,
            pending_cr: false,
            line: Vec::new(),
            overflow: false,
            lines_seen: 0,
            primary: None,
            awaiting_location: false,
            exception: None,
            last_frame: None,
            log_error: None,
        }
    }

    /// Forget everything; called at each command's OutputStart.
    pub(crate) fn reset(&mut self) {
        *self = ErrorPin::new();
    }

    /// Observe a slice of the output zone (Output bytes or in-zone Pass bytes,
    /// in stream order). Never emits anything.
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            match self.esc {
                Esc::Plain => {
                    // Resolve a held `\r` first: `\r\n` is just a CRLF line
                    // ending (the PTY ends every line that way); a lone `\r`
                    // rewrites the line on screen (progress bars) — mirror
                    // that so overwritten junk is not matched.
                    if self.pending_cr {
                        self.pending_cr = false;
                        if b != b'\n' {
                            self.line.clear();
                            self.overflow = false;
                        }
                    }
                    match b {
                        0x1b => self.esc = Esc::Escape,
                        b'\n' => self.complete_line(),
                        b'\r' => self.pending_cr = true,
                        _ => {
                            if self.line.len() < MATCH_CAP {
                                self.line.push(b);
                            } else {
                                self.overflow = true;
                            }
                        }
                    }
                }
                Esc::Escape => {
                    self.esc = match b {
                        b'[' => Esc::Csi,
                        // String sequences: OSC, and DCS/SOS/PM/APC — their
                        // bodies (sixel data, terminal replies) are not text
                        // and must never reach the matcher. All share the
                        // `ESC \` terminator; BEL also ends the state (xterm
                        // OSC style) — a BEL inside a DCS body ends stripping
                        // early, leaving the tail as "text", which at worst
                        // costs a wrong-but-sanitized pin, never bad output.
                        b']' | b'P' | b'X' | b'^' | b'_' => Esc::Osc,
                        // Two-byte escape (ESC c, ESC ( B, …): swallow the
                        // follow byte and return to text.
                        _ => Esc::Plain,
                    };
                }
                Esc::Csi => {
                    if (0x40..=0x7e).contains(&b) {
                        self.esc = Esc::Plain;
                    }
                }
                Esc::Osc => match b {
                    0x07 => self.esc = Esc::Plain,
                    0x1b => self.esc = Esc::OscEscape,
                    _ => {}
                },
                Esc::OscEscape => {
                    self.esc = if b == b'\\' { Esc::Plain } else { Esc::Osc };
                }
            }
        }
    }

    /// The best pin for the failure footer, or `None`. Ladder order: tool
    /// error, then exception line, then ERROR log line.
    pub(crate) fn take(&mut self) -> Option<PinnedError> {
        let total = self.lines_seen;
        let chosen = self
            .primary
            .take()
            .or_else(|| self.exception.take())
            .or_else(|| self.log_error.take());
        chosen.map(|c| PinnedError {
            text: c.text,
            lines_up: total.saturating_sub(c.line_no),
        })
    }

    fn complete_line(&mut self) {
        self.lines_seen += 1;
        let matchable = !self.overflow && !self.line.is_empty();
        if matchable {
            self.match_line();
        } else {
            self.awaiting_location = false;
        }
        self.line.clear();
        self.overflow = false;
    }

    fn match_line(&mut self) {
        let line = std::mem::take(&mut self.line);
        let trimmed = linefmt::ltrim(&line);

        // One-shot: the line after a rustc-style header may carry its
        // `--> src/pty.rs:214:18` location; attach it to the pinned text.
        if self.awaiting_location {
            self.awaiting_location = false;
            if let Some(rest) = trimmed.strip_prefix(b"--> ") {
                if !rest.is_empty() {
                    if let Some(primary) = self.primary.as_mut() {
                        primary.text.extend_from_slice(" \u{2192} ".as_bytes());
                        primary.text.extend_from_slice(rest);
                    }
                }
            }
        }

        // Tier 1 — tool-formatted errors, first wins.
        if self.primary.is_none() {
            // rustc / generic `error[E0308]:` / `error: …` at line start.
            let rustc_style = trimmed.starts_with(b"error[") || trimmed.starts_with(b"error:");
            // gcc/clang `path:10:5: error: …` (path first, marker infix).
            let infix = &line[..line.len().min(INFIX_WINDOW)];
            let cc_style =
                window_contains(infix, b": error: ") || window_contains(infix, b": fatal error: ");
            // git and friends.
            let fatal_style = trimmed.starts_with(b"fatal: ");
            // Rust panic (column 0; the message and location share the line).
            let panic_style =
                line.starts_with(b"thread '") && window_contains(&line, b"panicked at");
            if rustc_style || cc_style || fatal_style || panic_style {
                self.primary = Some(Candidate {
                    text: line.clone(),
                    line_no: self.lines_seen,
                });
                // Only the rustc shape puts the location on the next line.
                self.awaiting_location = rustc_style;
                self.line = line;
                return;
            }
        }

        // Tier 2 — traceback shape. Remember the deepest frame; on an
        // exception line, pin it (last wins) with that frame attached.
        if let Some(frame) = python_frame(trimmed) {
            self.last_frame = Some(frame);
        } else if linefmt::is_exception_line(&line) {
            let mut text = line.clone();
            if let Some(frame) = &self.last_frame {
                text.extend_from_slice(" \u{2192} ".as_bytes());
                text.extend_from_slice(frame);
            }
            self.exception = Some(Candidate {
                text,
                line_no: self.lines_seen,
            });
        }

        // Tier 3 — first ERROR-severity log line.
        if self.log_error.is_none() && linefmt::is_error_log_line(&line) {
            self.log_error = Some(Candidate {
                text: line.clone(),
                line_no: self.lines_seen,
            });
        }

        self.line = line;
    }
}

/// Parse a Python traceback frame line (already ltrimmed): `File "app.py",
/// line 7, in <module>` → `app.py:7`. `None` if the shape doesn't match.
fn python_frame(trimmed: &[u8]) -> Option<Vec<u8>> {
    let rest = trimmed.strip_prefix(b"File \"")?;
    let quote = rest.iter().position(|&b| b == b'"')?;
    let path = &rest[..quote];
    let after = rest[quote..].strip_prefix(b"\", line ")?;
    let digits: Vec<u8> = after
        .iter()
        .copied()
        .take_while(u8::is_ascii_digit)
        .collect();
    if path.is_empty() || digits.is_empty() {
        return None;
    }
    let mut frame = path.to_vec();
    frame.push(b':');
    frame.extend_from_slice(&digits);
    Some(frame)
}

fn window_contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin_of(chunks: &[&[u8]]) -> Option<PinnedError> {
        let mut p = ErrorPin::new();
        for c in chunks {
            p.feed(c);
        }
        p.take()
    }

    fn text(pin: &PinnedError) -> String {
        String::from_utf8_lossy(&pin.text).into_owned()
    }

    #[test]
    fn rustc_error_pins_first_and_attaches_location() {
        let pin = pin_of(&[
            b"   Compiling glimps v0.0.1\n",
            b"error[E0308]: mismatched types\n",
            b"  --> src/pty.rs:214:18\n",
            b"   |\n",
            b"error[E0999]: a second error\n",
            b"error: could not compile `glimps`\n",
        ])
        .unwrap();
        assert_eq!(
            text(&pin),
            "error[E0308]: mismatched types \u{2192} src/pty.rs:214:18"
        );
        assert_eq!(pin.lines_up, 4); // pinned line 2 of 6
    }

    #[test]
    fn colored_rustc_error_is_stripped_then_matched() {
        // What cargo actually emits on a TTY: SGR around the error tokens.
        let pin = pin_of(&[
            b"\x1b[1m\x1b[31merror[E0308]\x1b[0m\x1b[1m: mismatched types\x1b[0m\n",
            b"\x1b[1m\x1b[34m  --> \x1b[0msrc/pty.rs:214:18\n",
        ])
        .unwrap();
        assert_eq!(
            text(&pin),
            "error[E0308]: mismatched types \u{2192} src/pty.rs:214:18"
        );
    }

    #[test]
    fn escape_split_across_feed_chunks_still_strips() {
        let pin = pin_of(&[b"\x1b[3", b"1merror: boom\x1b", b"[0m\n"]).unwrap();
        assert_eq!(text(&pin), "error: boom");
    }

    #[test]
    fn gcc_style_infix_error_pins_whole_line() {
        let pin = pin_of(&[b"main.c:10:5: error: expected ';'\n"]).unwrap();
        assert_eq!(text(&pin), "main.c:10:5: error: expected ';'");
    }

    #[test]
    fn git_fatal_pins() {
        let pin = pin_of(&[b"fatal: not a git repository\n"]).unwrap();
        assert_eq!(text(&pin), "fatal: not a git repository");
    }

    #[test]
    fn rust_panic_pins() {
        let pin = pin_of(&[b"thread 'main' panicked at src/main.rs:7:5:\n", b"boom\n"]).unwrap();
        assert!(text(&pin).starts_with("thread 'main' panicked at"));
    }

    #[test]
    fn python_traceback_pins_last_exception_with_deepest_frame() {
        let pin = pin_of(&[
            b"Traceback (most recent call last):\n",
            b"  File \"app.py\", line 3, in <module>\n",
            b"    run()\n",
            b"  File \"app.py\", line 7, in run\n",
            b"ValueError: broken config\n",
        ])
        .unwrap();
        assert_eq!(text(&pin), "ValueError: broken config \u{2192} app.py:7");
        assert_eq!(pin.lines_up, 0); // it was the last line
    }

    #[test]
    fn tool_error_outranks_exception_and_log() {
        let pin = pin_of(&[
            b"ERROR something logged\n",
            b"error: the compiler speaks\n",
            b"ValueError: later exception\n",
        ])
        .unwrap();
        assert_eq!(text(&pin), "error: the compiler speaks");
    }

    #[test]
    fn error_log_line_is_the_fallback_tier() {
        let pin = pin_of(&[
            b"INFO  boot ok\n",
            b"ERROR connection reset by peer\n",
            b"ERROR second error is not pinned\n",
        ])
        .unwrap();
        assert_eq!(text(&pin), "ERROR connection reset by peer");
        assert_eq!(pin.lines_up, 1);
    }

    #[test]
    fn nothing_confident_pins_nothing() {
        assert!(pin_of(&[b"all good\n", b"200 OK\n", b"Terror: a film\n"]).is_none());
        assert!(pin_of(&[]).is_none());
        // A partial trailing line without newline is never matched.
        assert!(pin_of(&[b"error: no newline"]).is_none());
    }

    #[test]
    fn crlf_line_endings_are_normal_line_endings() {
        // PTY line discipline ends EVERY line with \r\n. The \r must not be
        // mistaken for a progress-bar overwrite (regression: this exact bug
        // shipped past the unit tests and was caught end-to-end).
        let pin = pin_of(&[b"ERROR boom\r\n"]).unwrap();
        assert_eq!(text(&pin), "ERROR boom");
        // Same with the \r\n split across feed chunks.
        let pin = pin_of(&[b"error: split ending\r", b"\n"]).unwrap();
        assert_eq!(text(&pin), "error: split ending");
    }

    #[test]
    fn carriage_return_overwrite_discards_progress_junk() {
        // A progress bar rewriting itself, then a real error. The overwritten
        // junk must not be matched or counted as lines.
        let pin = pin_of(&[
            b"Downloading 10%\rDownloading 50%\rDownloading 100%\r\n",
            b"error: checksum mismatch\n",
        ])
        .unwrap();
        assert_eq!(text(&pin), "error: checksum mismatch");
        assert_eq!(pin.lines_up, 0);
    }

    #[test]
    fn overlong_lines_are_counted_but_never_matched() {
        let long = [b"error: ".to_vec(), vec![b'x'; MATCH_CAP * 2]].concat();
        let mut p = ErrorPin::new();
        p.feed(&long);
        p.feed(b"\n");
        p.feed(b"ERROR real one\n");
        let pin = p.take().unwrap();
        assert_eq!(text(&pin), "ERROR real one");
        assert_eq!(pin.lines_up, 0); // the overlong line still counted
    }

    #[test]
    fn dcs_and_apc_bodies_are_invisible_to_matching() {
        // Sixel/DCS and APC payloads are not text; an `error:` inside one
        // must not be pinned. The real error afterwards must be.
        let pin = pin_of(&[
            b"\x1bPq#0;error: sixel junk\x1b\\\n",
            b"\x1b_error: apc junk\x1b\\ERROR real one\n",
        ])
        .unwrap();
        assert_eq!(text(&pin), "ERROR real one");
    }

    #[test]
    fn osc_sequences_are_invisible_to_matching() {
        // OSC-133 markers and hyperlinks travel through the zone as Pass
        // bytes; they must not glue onto or break line text.
        let pin = pin_of(&[
            b"\x1b]133;C\x07error: after a marker\n",
            b"\x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\\n",
        ])
        .unwrap();
        assert_eq!(text(&pin), "error: after a marker");
    }

    #[test]
    fn reset_forgets_prior_command() {
        let mut p = ErrorPin::new();
        p.feed(b"error: old command\n");
        p.reset();
        p.feed(b"all fine\n");
        assert!(p.take().is_none());
    }

    proptest::proptest! {
        /// Arbitrary bytes fed in arbitrary chunkings: never panics, and any
        /// pinned text is escape-free (the strip machine ate every ESC).
        #[test]
        fn prop_feed_never_panics_and_pins_are_escape_free(
            chunks in proptest::collection::vec(
                proptest::collection::vec(0u8..=255, 0..128), 0..16)
        ) {
            let mut p = ErrorPin::new();
            for c in &chunks {
                p.feed(c);
            }
            if let Some(pin) = p.take() {
                proptest::prop_assert!(!pin.text.contains(&0x1b));
            }
        }
    }
}
