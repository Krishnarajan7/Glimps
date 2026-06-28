//! The formatting seam.
//!
//! EVERYTHING GLIMPS does to output flows through `Formatter::process`. The
//! PTY supervisor (src/pty.rs) feeds it raw byte chunks from the shell, and it
//! returns the bytes to actually write to the screen.
//!
//! ## How a chunk is processed
//! 1. The OSC-133 scanner partitions the chunk into *output-content* runs (the
//!    only bytes we may touch) and *everything-else* runs (markers, prompt,
//!    typed input, other zones) which always pass through verbatim.
//! 2. Output-content is **accumulated** so a whole-command output can be
//!    detected and reformatted as a unit. To stay safe and responsive:
//!      - we only start buffering once the first non-whitespace output byte is
//!        seen, and we *give up immediately* (stream verbatim) unless it is
//!        `{` or `[` — so the overwhelming majority of output (ls, git, …) is
//!        never delayed;
//!      - any control byte ends the run and flushes the buffer, so output that
//!        already contains ANSI is never swallowed;
//!      - a size cap bounds memory and latency for large/streaming output.
//! 3. When a buffered output run ends (a marker or other non-output byte, i.e.
//!    the command finished), we attempt to format it; if it isn't exactly one
//!    JSON value the bytes are emitted unchanged.
//!
//! Net effect: the stream is byte-identical to the input unless a run is a clean
//! JSON document, in which case that run — and only that run — is reformatted.
//! Formatting only ever engages when OSC-133 markers are present (a shell with
//! `glimps init`); without them the zone stays Unknown and GLIMPS is a pure
//! pass-through.

mod html;
mod json;
mod linefmt;
mod osc133;
mod theme;

use std::borrow::Cow;
use std::io::IsTerminal;

use crate::config::Config;
use osc133::{Osc133Scanner, Seg};
// `Zone` is the seam's vocabulary; only tests consume it directly today.
#[cfg_attr(not(test), allow(unused_imports))]
pub use osc133::Zone;
use theme::Theme;

/// Where the separator's timestamp comes from. Kept injectable so the supervisor
/// can use real local time while tests stay deterministic.
#[derive(Debug, Clone, Copy)]
pub enum Clock {
    /// No timestamp (just a divider). Default for `Formatter::new`.
    Off,
    /// Real local wall-clock at this UTC offset (captured once, at startup).
    Local(time::UtcOffset),
    /// A fixed `HH:MM:SS` string — for deterministic tests.
    #[cfg_attr(not(test), allow(dead_code))]
    Fixed(&'static str),
}

/// Dim styling for the separator rule (independent of the JSON color theme).
const SEP_DIM: &[u8] = b"\x1b[2m";
const SEP_RESET: &[u8] = b"\x1b[0m";
/// The box-drawing rule character, U+2500 (`─`).
const SEP_DASH: &str = "─";

// Buffer/line/sniff size caps live in `Config::limits` (defaults there mirror the
// historical constants: 1 MiB / 64 KiB / 64 bytes).

/// State of the OUTPUT-zone accumulator.
enum Collect {
    /// Not collecting: outside the output zone, or we gave up for this command.
    Idle,
    /// Seen only whitespace so far this output run; still deciding.
    Sniff(Vec<u8>),
    /// Output looks like JSON (`{`/`[`) or HTML (`<`); accumulating until the run
    /// ends, then formatted as a unit.
    Buffer(Vec<u8>),
    /// Plain text output: streamed line-by-line with log/HTTP coloring. Holds the
    /// current partial (un-terminated) line across chunks.
    Stream(Vec<u8>),
    /// Decided not to touch this output run; stream the rest verbatim.
    Passthrough,
}

/// Stateful, streaming output processor.
pub struct Formatter {
    /// Master off-switch (safety invariant #6). Sampled once at construction
    /// from `GLIMPS` (a process can't have its env changed from outside anyway).
    /// When disabled, `process` is a pure, zero-copy pass-through that does no
    /// scanning. A future mid-session toggle (e.g. a hotkey) layers on top.
    enabled: bool,
    /// Tracks PROMPT / INPUT / OUTPUT zones from OSC-133 markers in the stream.
    scanner: Osc133Scanner,
    /// The OUTPUT-zone accumulator (see [`Collect`]).
    collect: Collect,
    /// Colors for formatted output (derived from `config.color`).
    theme: Theme,
    /// Source of the separator timestamp.
    clock: Clock,
    /// User configuration (`.glimpsrc`): per-type toggles, color, separator, caps.
    config: Config,
    /// A command's output has started but no output byte has been emitted yet, so
    /// the separator is owed and will be written just before that first byte.
    /// Lazy on purpose: commands with no output get no separator.
    pending_separator: bool,
    /// Whether the previous chunk left a full-screen TUI on the alternate screen.
    /// Tracked so the chunk that *exits* alt-screen is also passed through.
    was_alt_screen: bool,
}

impl Formatter {
    pub fn new() -> Self {
        Self::with_clock(Clock::Off)
    }

    /// Construct with an explicit timestamp source (tests pass a fixed clock).
    /// Formatting is gated only by the `GLIMPS` env here; config is the default.
    pub fn with_clock(clock: Clock) -> Self {
        Self::build(clock, true, Config::default())
    }

    /// Construct for the live supervisor. Formatting is on only when the config's
    /// master switch is set, `GLIMPS != 0`, AND stdout is a terminal — so anything
    /// capturing GLIMPS's output (piped/redirected) gets raw bytes, never injected
    /// ANSI/separators (safety invariant #3).
    pub fn for_supervisor(clock: Clock, config: Config) -> Self {
        Self::build(clock, std::io::stdout().is_terminal(), config)
    }

    fn build(clock: Clock, output_is_tty: bool, config: Config) -> Self {
        Formatter {
            enabled: config.enabled && glimps_enabled() && output_is_tty,
            scanner: Osc133Scanner::new(),
            collect: Collect::Idle,
            theme: if config.color {
                Theme::default()
            } else {
                Theme::plain()
            },
            clock,
            config,
            pending_separator: false,
            was_alt_screen: false,
        }
    }

    /// Process one chunk of bytes from the PTY, returning the bytes to write to
    /// the real terminal.
    ///
    /// Returns a `Cow` so the common pass-through case is zero-copy:
    /// `Borrowed(chunk)` when nothing is buffered and the chunk has no output
    /// content to consider; `Owned(..)` only when we actually segment, buffer,
    /// or reformat.
    pub fn process<'a>(&mut self, chunk: &'a [u8]) -> Cow<'a, [u8]> {
        if !self.enabled {
            return Cow::Borrowed(chunk);
        }

        let segments = self.scanner.feed_segments(chunk);

        // Interactive bypass: while a full-screen program owns the alternate
        // screen (vim, less, htop, fzf, …) — or on the chunk that exits it — pass
        // every byte through untouched. Injecting a separator or buffering into a
        // TUI's redraw stream would corrupt its display (and feel un-native).
        //
        // Alt-screen state is sampled at end-of-chunk, so this latches the bypass
        // across `process` calls (which is how real TUIs arrive: enter, then many
        // draw reads, then exit). A degenerate chunk that both enters AND exits
        // alt-screen is intentionally not bypassed; that doesn't happen for real
        // interactive programs, whose lifetimes span many reads.
        let alt = self.scanner.in_alt_screen();
        if alt || self.was_alt_screen {
            self.was_alt_screen = alt;
            if matches!(self.collect, Collect::Idle) && !self.pending_separator {
                return Cow::Borrowed(chunk);
            }
            // Flush any withheld bytes (a TUI shouldn't begin mid-buffer, but we
            // never drop user bytes), then stream the chunk. Dropping an owed
            // separator here is intentional — including the common case where
            // output just started with no text yet: a TUI gets no separator.
            let mut out = Vec::with_capacity(chunk.len());
            self.finalize(&mut out);
            self.pending_separator = false;
            out.extend_from_slice(chunk);
            return Cow::Owned(out);
        }

        // Zero-copy fast paths, where the chunk's bytes are emitted verbatim:
        //   * Idle with only pass-through bytes -> nothing to buffer or frame;
        //   * Passthrough with only output bytes -> we already decided not to
        //     format this run, so every byte streams as-is (the common steady
        //     state of a large non-JSON command output).
        // Both require no owed separator (which would need to be inserted) and no
        // edge markers (which change state). Then the emitted bytes equal the
        // input and we can borrow it.
        let only_pass = segments.iter().all(|s| matches!(s, Seg::Pass(..)));
        let only_output = segments.iter().all(|s| matches!(s, Seg::Output(..)));
        let zero_copy = !self.pending_separator
            && match self.collect {
                Collect::Idle => only_pass,
                Collect::Passthrough => only_output,
                // Stream may color lines, Sniff/Buffer hold bytes: never borrow.
                Collect::Sniff(_) | Collect::Buffer(_) | Collect::Stream(_) => false,
            };
        if zero_copy {
            return Cow::Borrowed(chunk);
        }

        let mut out = Vec::with_capacity(chunk.len());
        for seg in &segments {
            match *seg {
                Seg::Output(start, end) => self.push_output(&chunk[start..end], &mut out),
                Seg::Pass(start, end) => {
                    // Non-output bytes (markers, prompt, input, mid-output ANSI):
                    // finalize anything pending, then pass through untouched.
                    self.finalize(&mut out);
                    out.extend_from_slice(&chunk[start..end]);
                }
                // A command's output has begun: owe a separator, emitted lazily
                // right before the first output byte (so empty output -> none).
                Seg::OutputStart => self.pending_separator = true,
                // The output ended: flush, and drop an unfulfilled separator.
                Seg::OutputEnd => {
                    self.finalize(&mut out);
                    self.pending_separator = false;
                }
            }
        }
        Cow::Owned(out)
    }

    /// Feed one run of OUTPUT-zone content into the accumulator. The owed
    /// separator is emitted lazily, only once the run *commits to text* — so
    /// binary output (detected before commit) is never framed.
    fn push_output(&mut self, seg: &[u8], out: &mut Vec<u8>) {
        self.collect = match std::mem::replace(&mut self.collect, Collect::Idle) {
            Collect::Passthrough => {
                out.extend_from_slice(seg);
                Collect::Passthrough
            }
            Collect::Buffer(mut buf) => {
                // Separator was already emitted when this run committed to text.
                if buf.len().saturating_add(seg.len()) > self.config.limits.buffer_cap {
                    // Too big to hold/format: flush what we have and stream the rest.
                    out.extend_from_slice(&buf);
                    out.extend_from_slice(seg);
                    Collect::Passthrough
                } else {
                    buf.extend_from_slice(seg);
                    Collect::Buffer(buf)
                }
            }
            // Plain-text run: keep streaming lines (separator already emitted).
            Collect::Stream(line) => self.push_stream(line, seg, out),
            // Idle (fresh run) and Sniff (only whitespace so far) are both still
            // undecided: classify, and emit the separator only on a text commit.
            Collect::Idle => self.decide(Vec::new(), seg, out),
            Collect::Sniff(acc) => self.decide(acc, seg, out),
        };
    }

    /// Decide what an undecided run is, given `acc` (whitespace seen so far) and
    /// the new `seg`. Emits the owed separator only when committing to *text*;
    /// binary (a NUL before any text commits) is streamed verbatim with no
    /// separator (invariant #3: never frame or reformat binary).
    fn decide(&mut self, mut acc: Vec<u8>, seg: &[u8], out: &mut Vec<u8>) -> Collect {
        if acc.contains(&0) || seg.contains(&0) {
            self.pending_separator = false; // suppress: this is binary
            out.extend_from_slice(&acc);
            out.extend_from_slice(seg);
            return Collect::Passthrough;
        }
        match seg.iter().position(|b| !b.is_ascii_whitespace()) {
            Some(pos) => {
                // First real byte: commit to text -> the separator is now due.
                acc.extend_from_slice(seg);
                let is_structured = matches!(seg[pos], b'{' | b'[' | b'<');
                self.emit_separator(out);
                let json_or_html = self.config.formatters.json || self.config.formatters.html;
                let want_lines = self.config.formatters.logs || self.config.formatters.http;
                if is_structured && json_or_html && acc.len() <= self.config.limits.buffer_cap {
                    Collect::Buffer(acc) // a formatting candidate; hold it
                } else if !is_structured && want_lines {
                    // Plain text: stream line-by-line with log/HTTP coloring.
                    self.push_stream(Vec::new(), &acc, out)
                } else {
                    // Formatting for this kind is off (or blob too big): verbatim.
                    out.extend_from_slice(&acc);
                    Collect::Passthrough
                }
            }
            None => {
                // Still only whitespace; keep waiting (no separator yet), but don't
                // hoard unboundedly.
                acc.extend_from_slice(seg);
                if acc.len() > self.config.limits.sniff_cap {
                    self.emit_separator(out);
                    out.extend_from_slice(&acc);
                    Collect::Passthrough
                } else {
                    Collect::Sniff(acc)
                }
            }
        }
    }

    /// Emit the owed separator (if any) to `out`, clearing the debt. When the
    /// separator is disabled in config, the debt is cleared without emitting.
    fn emit_separator(&mut self, out: &mut Vec<u8>) {
        if self.pending_separator {
            self.pending_separator = false;
            if self.config.separator {
                let sep = self.render_separator();
                out.extend_from_slice(&sep);
            }
        }
    }

    /// Stream plain-text output line-by-line, coloring each *complete* line (log
    /// severity / HTTP status) and carrying the trailing partial line across
    /// chunks. Returns the next state: `Stream` with the carried partial, or
    /// `Passthrough` if the partial line grew past `LINE_CAP` (not a log line).
    fn push_stream(&mut self, mut line: Vec<u8>, seg: &[u8], out: &mut Vec<u8>) -> Collect {
        let mut start = 0;
        for (i, &b) in seg.iter().enumerate() {
            if b == b'\n' {
                line.extend_from_slice(&seg[start..=i]);
                emit_line(out, &line, &self.theme, &self.config.formatters);
                line.clear();
                start = i + 1;
            }
        }
        line.extend_from_slice(&seg[start..]);
        if line.len() > self.config.limits.line_cap {
            out.extend_from_slice(&line);
            Collect::Passthrough
        } else {
            Collect::Stream(line)
        }
    }

    /// Emit any output still held in the accumulator. **Must** be called once
    /// when the stream ends (PTY EOF): if a command's output was being buffered
    /// but never closed by an OSC-133 `D` marker — the shell exits, crashes, or
    /// the connection drops mid-output — those bytes would otherwise be silently
    /// truncated, violating byte-safety (invariant #4). Idempotent: leaves the
    /// accumulator Idle, so a second call returns empty.
    pub fn flush(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        self.finalize(&mut out);
        out
    }

    /// End of an output run: emit whatever was being collected. A JSON buffer is
    /// pretty-printed (with its GLIMPS-generated newlines converted to CRLF for
    /// the raw terminal); if it doesn't parse it is emitted verbatim. A
    /// whitespace-only sniff buffer is emitted verbatim.
    fn finalize(&mut self, out: &mut Vec<u8>) {
        match std::mem::replace(&mut self.collect, Collect::Idle) {
            Collect::Buffer(buf) => {
                match format_recognized(&buf, &self.theme, &self.config.formatters) {
                    // Reformatted: tag it with a content-type badge, then emit the
                    // formatted bytes (ours, so their `\n` become `\r\n`).
                    Some((label, pretty)) => {
                        out.extend_from_slice(&render_badge(label, self.config.color));
                        push_crlf(out, &pretty);
                    }
                    // Not a type we format: the user's bytes, emitted exactly as-is.
                    None => out.extend_from_slice(&buf),
                }
            }
            Collect::Sniff(acc) => {
                // Whitespace-only output that never committed: it still counts as
                // output, so emit the owed separator, then the whitespace verbatim.
                self.emit_separator(out);
                out.extend_from_slice(&acc);
            }
            Collect::Stream(line) => {
                // The trailing partial line has no newline; emit it verbatim (we
                // only color complete lines). The separator was emitted at commit.
                out.extend_from_slice(&line);
            }
            Collect::Passthrough | Collect::Idle => {}
        }
    }

    /// Render the command/output separator: a dim rule, optionally centered on a
    /// timestamp, framed by CRLF so it sits on its own line in the raw terminal.
    fn render_separator(&self) -> Vec<u8> {
        let dim: &[u8] = if self.config.color { SEP_DIM } else { b"" };
        let reset: &[u8] = if self.config.color { SEP_RESET } else { b"" };
        let mut sep = Vec::new();
        sep.extend_from_slice(dim);
        match self.timestamp() {
            Some(ts) => {
                sep.extend_from_slice(SEP_DASH.repeat(8).as_bytes());
                sep.push(b' ');
                sep.extend_from_slice(ts.as_bytes());
                sep.push(b' ');
                sep.extend_from_slice(SEP_DASH.repeat(8).as_bytes());
            }
            None => sep.extend_from_slice(SEP_DASH.repeat(18).as_bytes()),
        }
        sep.extend_from_slice(reset);
        sep.extend_from_slice(b"\r\n");
        sep
    }

    /// The current `HH:MM:SS` for the separator, per the configured [`Clock`].
    fn timestamp(&self) -> Option<String> {
        match self.clock {
            Clock::Off => None,
            Clock::Fixed(s) => Some(s.to_string()),
            Clock::Local(offset) => {
                // `checked_to_offset` instead of `to_offset`: the latter panics
                // (internally `.expect`) at the ±9999-year boundary. Unreachable
                // for the real wall clock, but the formatter path must not be able
                // to panic — fall back to no timestamp instead.
                let now = time::OffsetDateTime::now_utc().checked_to_offset(offset)?;
                Some(format!(
                    "{:02}:{:02}:{:02}",
                    now.hour(),
                    now.minute(),
                    now.second()
                ))
            }
        }
    }

    /// The zone the stream is currently in. Exposed for tests.
    #[cfg(test)]
    pub fn zone(&self) -> Zone {
        self.scanner.zone()
    }

    /// Whether formatting is active (i.e. `GLIMPS` is not set to `0`).
    #[cfg(test)]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Try each enabled formatter in precedence order (most precise first), returning
/// the content-type label and reformatted bytes of the first that matches, or
/// `None` to pass the buffer through verbatim.
fn format_recognized(
    bytes: &[u8],
    theme: &Theme,
    fmts: &crate::config::Formatters,
) -> Option<(&'static str, Vec<u8>)> {
    if fmts.json {
        if let Some(out) = json::try_format(bytes, theme) {
            return Some(("JSON", out));
        }
    }
    if fmts.html {
        if let Some(out) = html::try_format(bytes, theme) {
            return Some(("HTML", out));
        }
    }
    None
}

/// Emit one complete line, colored by log severity / HTTP status (per the enabled
/// categories) if it matches, else verbatim. The user's bytes (content + line
/// ending) are always preserved; only color codes are added.
fn emit_line(out: &mut Vec<u8>, line: &[u8], theme: &Theme, fmts: &crate::config::Formatters) {
    match linefmt::colorize_line(line, theme, fmts.logs, fmts.http) {
        Some(colored) => out.extend_from_slice(&colored),
        None => out.extend_from_slice(line),
    }
}

/// Render a content-type badge shown just above reformatted output: inverse-video
/// ` JSON ` when color is on, plain `[JSON]` otherwise. CRLF-terminated.
fn render_badge(label: &str, color: bool) -> Vec<u8> {
    let mut badge = Vec::with_capacity(label.len() + 8);
    if color {
        badge.extend_from_slice(b"\x1b[7m ");
        badge.extend_from_slice(label.as_bytes());
        badge.extend_from_slice(b" \x1b[0m\r\n");
    } else {
        badge.push(b'[');
        badge.extend_from_slice(label.as_bytes());
        badge.extend_from_slice(b"]\r\n");
    }
    badge
}

/// Append `bytes` to `out`, converting each `\n` to `\r\n`. Used only for
/// GLIMPS-generated content (formatted JSON, the separator): the outer terminal
/// is in raw mode, so a bare `\n` would not return the cursor to column 0. The
/// user's own pass-through bytes already carry CRLF from the inner PTY and are
/// never run through this. `bytes` here never contains a `\r` (the JSON printer
/// only emits `\n`), so no double-CR can result.
fn push_crlf(out: &mut Vec<u8>, bytes: &[u8]) {
    for &b in bytes {
        if b == b'\n' {
            out.push(b'\r');
        }
        out.push(b);
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new()
    }
}

/// GLIMPS is on unless the user sets `GLIMPS=0`. Any other value (or unset)
/// leaves it enabled — the off-switch must be unambiguous and hard to trip by
/// accident, but trivial to reach on purpose.
fn glimps_enabled() -> bool {
    !matches!(std::env::var("GLIMPS").as_deref(), Ok("0"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const C: &[u8] = b"\x1b]133;C\x07"; // command output start
    const D: &[u8] = b"\x1b]133;D\x07"; // command output end

    /// The separator a fresh Formatter injects, for the given clock. GLIMPS frames
    /// command output with this; tests reconstruct expected output around it.
    fn sep_with(clock: Clock) -> Vec<u8> {
        Formatter::with_clock(clock).render_separator()
    }

    /// The default (timestamp-less) separator.
    fn sep() -> Vec<u8> {
        sep_with(Clock::Off)
    }

    /// Convert GLIMPS-generated `\n` to `\r\n`, mirroring what the Formatter does
    /// to formatted JSON before it hits the raw terminal.
    fn crlf(bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        push_crlf(&mut out, bytes);
        out
    }

    /// The content-type badge bytes for a label.
    fn badge(label: &str) -> Vec<u8> {
        render_badge(label, true)
    }

    /// Drive a sequence of chunks through one (timestamp-less, plain-theme)
    /// Formatter and return all emitted bytes concatenated. Plain theme means
    /// line coloring adds no bytes, so verbatim assertions stay exact.
    fn run(chunks: &[&[u8]]) -> Vec<u8> {
        let mut f = Formatter::new();
        f.theme = Theme::plain();
        let mut out = Vec::new();
        for c in chunks {
            out.extend_from_slice(&f.process(c));
        }
        out
    }

    /// Drive chunks through one plain-theme Formatter, then flush (PTY EOF).
    fn run_flush(chunks: &[&[u8]]) -> Vec<u8> {
        let mut f = Formatter::new();
        f.theme = Theme::plain();
        let mut out = Vec::new();
        for c in chunks {
            out.extend_from_slice(&f.process(c));
        }
        out.extend_from_slice(&f.flush());
        out
    }

    fn cat(parts: &[&[u8]]) -> Vec<u8> {
        parts.concat()
    }

    #[test]
    fn empty_chunk_is_safe() {
        let mut f = Formatter::new();
        assert!(f.process(b"").is_empty());
    }

    #[test]
    fn zone_advances_through_process() {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        f.process(C);
        assert_eq!(f.zone(), Zone::Output);
        f.process(b"some command output\n");
        assert_eq!(f.zone(), Zone::Output);
        f.process(D);
        assert_eq!(f.zone(), Zone::Unknown);
    }

    #[test]
    fn json_output_is_pretty_printed_with_separator_and_crlf() {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        f.theme = Theme::plain();
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(br#"{"a":1,"b":[2,3]}"#));
        out.extend_from_slice(&f.process(D)); // command end -> flush + format
        let pretty = crlf(b"{\n  \"a\": 1,\n  \"b\": [\n    2,\n    3\n  ]\n}");
        let expected = cat(&[C, &sep(), &badge("JSON"), &pretty, D]);
        assert_eq!(out, expected);
    }

    #[test]
    fn json_split_across_chunks_is_still_formatted() {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        f.theme = Theme::plain();
        let mut out = Vec::new();
        for part in [C, br#"{"a":"#, br#"1}"#, D] {
            out.extend_from_slice(&f.process(part));
        }
        let pretty = crlf(b"{\n  \"a\": 1\n}");
        assert_eq!(out, cat(&[C, &sep(), &badge("JSON"), &pretty, D]));
    }

    #[test]
    fn non_json_output_is_framed_but_unchanged() {
        let body = b"total 12\ndrwxr-xr-x  3 user staff\n";
        let input = cat(&[C, body, D]);
        // The user's bytes are untouched; only a separator is inserted at output start.
        let expected = cat(&[C, &sep(), body, D]);
        assert_eq!(run(&[&input]), expected);
    }

    #[test]
    fn output_that_looks_like_json_but_isnt_passes_through() {
        let body = b"{this is not json}";
        let input = cat(&[C, body, D]);
        assert_eq!(run(&[&input]), cat(&[C, &sep(), body, D]));
    }

    #[test]
    fn angle_bracket_output_that_isnt_html_passes_through() {
        // A `<`-leading run is buffered (the loose sniff trigger), but if it isn't
        // HTML it must be emitted verbatim (only framed by the separator).
        let body = b"<stdin>: not actually html\n";
        let input = cat(&[C, body, D]);
        assert_eq!(run(&[&input]), cat(&[C, &sep(), body, D]));
    }

    #[test]
    fn output_outside_any_command_is_untouched() {
        // No C marker -> zone stays Unknown -> pure pass-through, no separator.
        let stream = br#"{"a":1}"#;
        assert_eq!(run(&[stream]), stream);
    }

    #[test]
    fn prompt_and_input_are_never_formatted() {
        // The prompt/input zones (incl. a `{`-leading typed command) pass through
        // untouched; only the command's OUTPUT ("plain\n") is framed.
        let a = b"\x1b]133;A\x07";
        let b = b"\x1b]133;B\x07";
        let input = cat(&[a, b"{prompt} $ ", b, br#"echo {"x":1}"#, C, b"plain\n", D]);
        let expected = cat(&[
            a,
            b"{prompt} $ ",
            b,
            br#"echo {"x":1}"#,
            C,
            &sep(),
            b"plain\n",
            D,
        ]);
        assert_eq!(run(&[&input]), expected);
    }

    #[test]
    fn no_separator_for_empty_command_output() {
        // A command that produces no output gets no separator at all.
        let input = cat(&[C, D]);
        assert_eq!(run(&[&input]), input);
    }

    #[test]
    fn whitespace_only_output_is_still_framed() {
        // Whitespace counts as output: a command that prints only a blank line is
        // framed (its bytes are preserved verbatim behind the separator). Pins the
        // documented behavior.
        let body = b"\n";
        let input = cat(&[C, body, D]);
        assert_eq!(run(&[&input]), cat(&[C, &sep(), body, D]));
    }

    #[test]
    fn log_and_http_lines_are_colored_streaming() {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        // Default (colored) theme; lines arrive in separate chunks (tail -f style).
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(b"INFO starting up\n"));
        out.extend_from_slice(&f.process(b"ERROR boom\n"));
        out.extend_from_slice(&f.process(b"HTTP/1.1 404 Not Found\n"));
        out.extend_from_slice(&f.process(b"just a plain line\n"));
        out.extend_from_slice(&f.process(D));
        let s = String::from_utf8(out).unwrap();
        // Separator once, then each recognized line wrapped in its color; the
        // plain line is untouched.
        assert!(s.contains("\x1b[32mINFO starting up\x1b[0m\n")); // green
        assert!(s.contains("\x1b[31mERROR boom\x1b[0m\n")); // red
        assert!(s.contains("\x1b[33mHTTP/1.1 404 Not Found\x1b[0m\n")); // yellow
        assert!(s.contains("just a plain line\n"));
        assert!(!s.contains("\x1b[31mjust a plain line")); // plain line not colored
    }

    #[test]
    fn log_line_split_across_chunks_is_colored_once_whole() {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(b"ERR")); // partial line, no newline yet
        out.extend_from_slice(&f.process(b"OR boom\n")); // completes the line
        out.extend_from_slice(&f.process(D));
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\x1b[31mERROR boom\x1b[0m\n"));
    }

    #[test]
    fn http_status_split_across_chunks_is_colored_whole() {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(b"HTTP/1.1 4")); // code split mid-number
        out.extend_from_slice(&f.process(b"04 Not Found\n"));
        out.extend_from_slice(&f.process(D));
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\x1b[33mHTTP/1.1 404 Not Found\x1b[0m\n"));
    }

    #[test]
    fn crlf_log_line_is_colored_with_ending_preserved() {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(b"ERROR x\r\n")); // CRLF, as from a real PTY
        out.extend_from_slice(&f.process(D));
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\x1b[31mERROR x\x1b[0m\r\n")); // reset before \r\n
    }

    #[test]
    fn unterminated_final_line_flushes_verbatim_uncolored() {
        // A colorable-looking line with no trailing newline at EOF is flushed
        // verbatim (we only color complete lines) — and no bytes are lost.
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(b"ERROR boom")); // no newline
        out.extend_from_slice(&f.flush()); // EOF
        let s = String::from_utf8(out).unwrap();
        assert!(s.ends_with("ERROR boom"));
        assert!(!s.contains("\x1b[31m")); // not colored (partial line)
    }

    #[test]
    fn very_long_line_streams_verbatim_without_coloring() {
        // A line longer than LINE_CAP with no newline overflows the line buffer:
        // it must be streamed verbatim (no coloring, no byte loss).
        let mut body = b"ERROR ".to_vec(); // would match if it were a complete line
        body.extend(std::iter::repeat_n(
            b'x',
            Config::default().limits.line_cap + 16,
        ));
        let input = cat(&[C, &body]); // no D; overflow forces verbatim, then EOF
        assert_eq!(run_flush(&[&input]), cat(&[C, &sep(), &body]));
    }

    #[test]
    fn binary_output_is_passed_through_without_a_separator() {
        // Output beginning with a NUL byte is binary: no separator, no buffering,
        // streamed exactly (invariant #3: never reformat binary).
        let body = b"\x7fELF\x00\x01\x02\x00\x00rest";
        let input = cat(&[C, body, D]);
        assert_eq!(run(&[&input]), input);
    }

    #[test]
    fn whitespace_then_binary_gets_no_separator() {
        // Output that begins with whitespace and THEN reveals a NUL is still
        // binary: because the separator is deferred until a text commit, it is
        // correctly suppressed (not injected ahead of the binary).
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(b"  ")); // whitespace (undecided)
        out.extend_from_slice(&f.process(b"\x00\x01bin")); // NUL -> binary
        out.extend_from_slice(&f.process(D));
        assert_eq!(out, cat(&[C, b"  ", b"\x00\x01bin", D]));
    }

    #[test]
    fn nul_after_text_committed_still_streams_verbatim() {
        // Once a run has committed to text (Passthrough), a later NUL just streams
        // through. The separator was already shown for the text — that's correct.
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(b"hello ")); // commits to text -> separator
        out.extend_from_slice(&f.process(b"\x00\x00")); // NUL later -> verbatim
        out.extend_from_slice(&f.process(D));
        assert_eq!(out, cat(&[C, &sep(), b"hello ", b"\x00\x00", D]));
    }

    /// Build a Formatter with a specific config (plain timestamp, treated as a TTY).
    fn fmt_with(config: Config) -> Formatter {
        Formatter::build(Clock::Off, true, config)
    }

    #[test]
    fn config_master_disable_is_pure_passthrough() {
        let cfg = Config {
            enabled: false,
            ..Config::default()
        };
        let mut f = fmt_with(cfg);
        let stream = cat(&[C, br#"{"a":1}"#, D]);
        assert_eq!(&*f.process(&stream), stream.as_slice());
    }

    #[test]
    fn config_json_off_passes_json_through_verbatim() {
        let cfg = Config {
            formatters: crate::config::Formatters {
                json: false,
                ..crate::config::Formatters::default()
            },
            ..Config::default()
        };
        let mut f = fmt_with(cfg);
        let body = br#"{"a":1}"#;
        let stream = cat(&[C, body, D]);
        // JSON disabled -> not reformatted; still framed by the separator.
        assert_eq!(f.process(&stream).into_owned(), cat(&[C, &sep(), body, D]));
    }

    #[test]
    fn config_logs_off_does_not_color_log_lines() {
        let cfg = Config {
            formatters: crate::config::Formatters {
                logs: false,
                ..crate::config::Formatters::default()
            },
            ..Config::default()
        };
        let mut f = fmt_with(cfg);
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(b"ERROR boom\n"));
        out.extend_from_slice(&f.process(D));
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("ERROR boom\n"));
        assert!(!s.contains("\x1b[31m")); // not colored red
    }

    #[test]
    fn config_color_off_emits_no_ansi_but_still_structures() {
        let cfg = Config {
            color: false,
            ..Config::default()
        };
        let mut f = fmt_with(cfg);
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(br#"{"a":1}"#));
        out.extend_from_slice(&f.process(D));
        let s = String::from_utf8(out).unwrap();
        // No SGR color codes (`\x1b[`). The OSC-133 markers use `\x1b]` and are
        // passed through, so we don't assert against bare ESC.
        assert!(!s.contains("\x1b[")); // no color codes (separator/badge/json)
        assert!(s.contains("[JSON]")); // plain badge instead of inverse
        assert!(s.contains("{\r\n  \"a\": 1\r\n}")); // still indented (CRLF)
    }

    #[test]
    fn config_separator_off_hides_the_divider() {
        let cfg = Config {
            separator: false,
            ..Config::default()
        };
        let mut f = fmt_with(cfg);
        let body = b"plain output\n";
        let stream = cat(&[C, body, D]);
        // No separator, and (plain log line) no coloring change -> verbatim.
        assert_eq!(f.process(&stream).into_owned(), stream);
    }

    #[test]
    fn non_tty_supervisor_output_disables_formatting() {
        // Under `cargo test`, stdout is not a terminal, so the supervisor
        // constructor must disable formatting (raw pass-through).
        if std::io::stdout().is_terminal() {
            return; // can't assert the gate when run attached to a real tty
        }
        let mut f = Formatter::for_supervisor(Clock::Off, Config::default());
        assert!(!f.is_enabled());
        // And it truly passes through, markers/JSON included.
        let stream = cat(&[C, br#"{"a":1}"#, D]);
        assert_eq!(&*f.process(&stream), stream.as_slice());
    }

    #[test]
    fn alt_screen_app_is_passed_through_untouched() {
        // A full-screen app (vim): enter alt screen, draw content that even looks
        // like JSON, exit. No separator, no badge, no formatting — pure verbatim.
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        f.theme = Theme::plain();
        let alt_on = b"\x1b[?1049h";
        let alt_off = b"\x1b[?1049l";
        let parts: [&[u8]; 5] = [C, alt_on, br#"{"a":1}"#, alt_off, D];
        let mut out = Vec::new();
        for p in parts {
            out.extend_from_slice(&f.process(p));
        }
        assert_eq!(out, parts.concat());
    }

    #[test]
    fn alt_screen_entering_mid_buffer_flushes_without_byte_loss() {
        // Defensive path: a buffered JSON-candidate run is pending when alt-screen
        // is entered. The withheld bytes must be flushed verbatim (incomplete
        // JSON doesn't format), then the alt-screen chunk streamed — nothing lost.
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        f.theme = Theme::plain();
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(C)); // OutputStart, separator owed
        out.extend_from_slice(&f.process(br#"{"a":1"#)); // buffered (incomplete)
        out.extend_from_slice(&f.process(b"\x1b[?1049h")); // alt-screen enters
                                                           // Separator was emitted lazily before the buffered bytes; on alt-enter the
                                                           // buffer is flushed verbatim and the chunk streamed.
        assert_eq!(out, cat(&[C, &sep(), br#"{"a":1"#, b"\x1b[?1049h"]));
    }

    #[test]
    fn formatting_resumes_after_alt_screen_exits() {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        f.theme = Theme::plain();
        // A TUI session, discarded.
        for p in [C, b"\x1b[?1049h".as_slice(), b"\x1b[?1049l", D] {
            let _ = f.process(p);
        }
        // The next command's JSON output is formatted normally again.
        let mut out = Vec::new();
        for p in [C, br#"{"a":1}"#.as_slice(), D] {
            out.extend_from_slice(&f.process(p));
        }
        assert_eq!(
            out,
            cat(&[C, &sep(), &badge("JSON"), &crlf(b"{\n  \"a\": 1\n}"), D])
        );
    }

    #[test]
    fn separator_owed_across_a_chunk_boundary() {
        // OutputStart arrives at the end of one chunk (C marker), the first output
        // byte in the next. The owed separator must still be emitted once, before
        // that byte — exercising the `pending_separator` Cow-fast-path guard.
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(C)); // OutputStart, separator owed
        out.extend_from_slice(&f.process(b"hello\n")); // first output byte next chunk
        out.extend_from_slice(&f.process(D));
        assert_eq!(out, cat(&[C, &sep(), b"hello\n", D]));
    }

    #[test]
    fn separator_carries_timestamp_with_a_clock() {
        let mut f = Formatter::with_clock(Clock::Fixed("12:34:56"));
        f.theme = Theme::plain();
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(b"hi\n"));
        out.extend_from_slice(&f.process(D));
        let expected = cat(&[C, &sep_with(Clock::Fixed("12:34:56")), b"hi\n", D]);
        assert_eq!(out, expected);
        // The timestamp text is present in the emitted separator.
        assert!(out.windows(8).any(|w| w == b"12:34:56"));
    }

    #[test]
    fn eof_flush_emits_withheld_non_json_unchanged() {
        // Output that starts like JSON but never closes (no `D`), then EOF. The
        // withheld bytes survive verbatim, behind the separator.
        let body = br#"{"a":1"#; // incomplete: no closing brace
        let input = cat(&[C, body]);
        assert_eq!(run_flush(&[&input]), cat(&[C, &sep(), body]));
    }

    #[test]
    fn eof_flush_formats_withheld_complete_json() {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        f.theme = Theme::plain();
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(br#"{"a":1}"#)); // buffered, no D yet
        out.extend_from_slice(&f.flush()); // EOF -> format the complete value
        assert_eq!(
            out,
            cat(&[C, &sep(), &badge("JSON"), &crlf(b"{\n  \"a\": 1\n}")])
        );
    }

    #[test]
    fn two_consecutive_json_outputs_each_framed_and_formatted() {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        f.theme = Theme::plain();
        let mut out = Vec::new();
        for part in [C, br#"{"a":1}"#, D, C, b"[1,2]", D] {
            out.extend_from_slice(&f.process(part));
        }
        let one = crlf(b"{\n  \"a\": 1\n}");
        let two = crlf(b"[\n  1,\n  2\n]");
        let expected = cat(&[
            C,
            &sep(),
            &badge("JSON"),
            &one,
            D,
            C,
            &sep(),
            &badge("JSON"),
            &two,
            D,
        ]);
        assert_eq!(out, expected);
    }

    #[test]
    fn html_output_is_indented_with_badge() {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        f.theme = Theme::plain();
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(b"<p>hi</p>"));
        out.extend_from_slice(&f.process(D));
        let indented = crlf(b"<p>\n  hi\n</p>");
        assert_eq!(out, cat(&[C, &sep(), &badge("HTML"), &indented, D]));
    }

    #[test]
    fn buffer_cap_overflow_streams_verbatim() {
        // A `{`-leading run larger than BUFFER_CAP gives up and streams the bytes
        // unchanged (behind the separator) rather than holding them.
        let mut body = vec![b'{'];
        body.extend(std::iter::repeat_n(
            b'x',
            Config::default().limits.buffer_cap + 1,
        ));
        let input = cat(&[C, &body]); // no D; overflow forces verbatim
        assert_eq!(run_flush(&[&input]), cat(&[C, &sep(), &body]));
    }

    #[test]
    fn ansi_escape_mid_json_keeps_bytes_intact() {
        // Output containing an ANSI escape can't be one clean JSON value; the
        // user's bytes pass through unchanged (invariant #3) behind the separator.
        let body = cat(&[br#"{"a":1"#, b"\x1b[31m", b"}"]);
        let input = cat(&[C, &body, D]);
        assert_eq!(run(&[&input]), cat(&[C, &sep(), &body, D]));
    }

    /// Expected framing of a single command's non-JSON output: a separator is
    /// inserted only if the command actually produced output bytes.
    fn framed(body: &[u8], trailing_d: bool) -> Vec<u8> {
        let mut v = C.to_vec();
        if !body.is_empty() {
            v.extend_from_slice(&sep());
            v.extend_from_slice(body);
        }
        if trailing_d {
            v.extend_from_slice(D);
        }
        v
    }

    /// A token used to build fuzz bodies that interleave plain text, ANSI SGR
    /// sequences, and newlines — mimicking real colored command output.
    #[derive(Debug, Clone)]
    enum Tok {
        Text(Vec<u8>),
        Sgr(u8),
        Newline,
    }

    /// Remove the first occurrence of `needle` from `haystack`.
    fn strip_first(haystack: &[u8], needle: &[u8]) -> Vec<u8> {
        match haystack.windows(needle.len()).position(|w| w == needle) {
            Some(pos) => {
                let mut v = haystack[..pos].to_vec();
                v.extend_from_slice(&haystack[pos + needle.len()..]);
                v
            }
            None => haystack.to_vec(),
        }
    }

    #[test]
    fn corpus_common_commands_preserve_every_byte() {
        // Zero interference across a corpus of real-world command output —
        // including ANSI-colored, Unicode, man-overstrike, control-only, tables,
        // and empty/whitespace cases. GLIMPS may only INSERT one separator; every
        // user byte must survive. Plain theme so line coloring adds nothing; we
        // then strip the single injected separator and require the original back.
        // (Stripping handles ANSI-leading output, where the separator lands after
        // the leading escape rather than right after the C marker.)
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/corpus/commands");
        let sep = sep();
        let mut count = 0;
        for entry in std::fs::read_dir(dir).expect("corpus dir") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("txt") {
                continue;
            }
            let sample = std::fs::read(&path).expect("read fixture");
            let input = cat(&[C, &sample, D]);
            let out = run(&[&input]);
            let recovered = strip_first(&out, &sep);
            assert_eq!(
                recovered,
                cat(&[C, &sample, D]),
                "interference on corpus fixture {:?}",
                path.file_name()
            );
            count += 1;
        }
        assert!(count >= 30, "expected a sizable corpus, found only {count}");
    }

    #[test]
    fn password_prompt_output_is_never_touched() {
        // A no-echo password prompt is just command output (the program prints
        // "Password:"); GLIMPS must pass it through unchanged. The typed password
        // is no-echo, so it never appears in the stream at all — GLIMPS can't see
        // it. This pins the "password prompts never touched" promise.
        let prompt = b"Password:";
        let stream = cat(&[C, prompt, D]);
        // No trailing newline, prompt waits inline -> Stream holds it, flushed on D.
        assert_eq!(run(&[&stream]), cat(&[C, &sep(), prompt, D]));
    }

    #[test]
    fn latency_budget_no_pathological_blowup() {
        use std::time::Instant;
        // ~4 MiB of line-oriented output fed in PTY-sized chunks through the
        // streaming (per-line) path — the realistic hot path. A generous wall
        // budget catches O(n^2)/pathological regressions (criterion measures the
        // real micro-latency separately). Debug builds are slow, hence 10s.
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        f.theme = Theme::plain();
        let line = b"the quick brown fox jumps over the lazy dog 0123456789\n";
        let mut body = Vec::with_capacity(4 * 1024 * 1024);
        while body.len() < 4 * 1024 * 1024 {
            body.extend_from_slice(line);
        }
        let start = Instant::now();
        let _ = f.process(C);
        for chunk in body.chunks(8192) {
            let _ = f.process(chunk);
        }
        let _ = f.process(D);
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_secs() < 10,
            "processed {} bytes in {elapsed:?} (budget 10s — suspect a complexity regression)",
            body.len()
        );
    }

    proptest::proptest! {
        /// Arbitrary config (random toggles + small/zero caps) over arbitrary
        /// command output never panics, and when the master switch is off the
        /// output is byte-identical to the input (pure pass-through).
        #[test]
        #[allow(clippy::too_many_arguments)]
        fn prop_arbitrary_config_is_safe(
            enabled: bool,
            color: bool,
            separator: bool,
            json: bool,
            html: bool,
            logs: bool,
            http: bool,
            buffer_cap in 0usize..2048,
            line_cap in 0usize..2048,
            sniff_cap in 0usize..128,
            body in proptest::collection::vec(0u8..=255, 0..256),
        ) {
            let cfg = Config {
                enabled,
                color,
                separator,
                timestamp: false,
                formatters: crate::config::Formatters { json, html, logs, http },
                limits: crate::config::Limits { buffer_cap, line_cap, sniff_cap },
            };
            let mut f = Formatter::build(Clock::Off, true, cfg);
            let stream = [C, &body, D].concat();
            let mut out = f.process(&stream).into_owned();
            out.extend_from_slice(&f.flush()); // also exercises EOF flush; must not panic
            if !enabled {
                proptest::prop_assert_eq!(out, stream); // off => verbatim
            }
        }

        /// Fuzz: realistic colored output — arbitrary interleavings of plain
        /// text, ANSI SGR sequences, and newlines — is preserved byte-for-byte
        /// (plain theme; the one injected separator stripped back out), and never
        /// panics. Exercises the ESC-splitting + line-streaming paths on
        /// adversarial-but-realistic input.
        #[test]
        fn prop_text_and_ansi_preserve_every_byte(
            ops in proptest::collection::vec(
                proptest::prop_oneof![
                    proptest::collection::vec(
                        proptest::prop_oneof![Just(b' '), 0x61u8..0x7b, 0x30u8..0x3a],
                        1..12,
                    ).prop_map(Tok::Text),
                    (0u8..8).prop_map(Tok::Sgr),
                    Just(Tok::Newline),
                ],
                0..40,
            )
        ) {
            // Lead with a plain byte so the run is classified as text (not
            // JSON/HTML/binary); tokens contain no NUL, no `ESC ]`, no `─`/`\r`,
            // so the body can neither form a marker nor collide with a separator.
            let mut body = vec![b'x'];
            for op in &ops {
                match op {
                    Tok::Text(bytes) => body.extend_from_slice(bytes),
                    Tok::Sgr(n) => {
                        body.extend_from_slice(b"\x1b[");
                        body.extend_from_slice(n.to_string().as_bytes());
                        body.push(b'm');
                    }
                    Tok::Newline => body.push(b'\n'),
                }
            }
            let mut f = Formatter::new();
            if !f.is_enabled() {
                return Ok(());
            }
            f.theme = Theme::plain();
            let stream = [C, &body, D].concat();
            let out = f.process(&stream).into_owned();
            let recovered = strip_first(&out, &sep());
            proptest::prop_assert_eq!(recovered, stream);
        }

        /// Byte-safety invariant #4 for the pass-through path: with no escape
        /// sequences in the stream (so the zone never leaves Unknown, and no
        /// separator is ever inserted), the concatenation of outputs equals the
        /// concatenation of inputs exactly.
        #[test]
        fn prop_passthrough_is_byte_identical(
            chunks in proptest::collection::vec(proptest::collection::vec(0u8..=255, 0..64), 0..16)
        ) {
            let mut f = Formatter::new();
            let mut out = Vec::new();
            let mut expected = Vec::new();
            for chunk in &chunks {
                // Strip ESC so no escape sequence (and thus no zone change) can form.
                let clean: Vec<u8> = chunk.iter().copied().filter(|&b| b != 0x1b).collect();
                expected.extend_from_slice(&clean);
                out.extend_from_slice(&f.process(&clean));
            }
            proptest::prop_assert_eq!(out, expected);
        }

        /// Non-JSON command output (arbitrary ESC-free bytes wrapped in C/D
        /// markers) is preserved byte-for-byte, with only the GLIMPS separator
        /// inserted at output start. Proves the buffering path withholds-then-
        /// flushes without altering the user's bytes.
        #[test]
        fn prop_non_json_output_preserves_user_bytes(
            body in proptest::collection::vec(0u8..=255, 0..256)
        ) {
            let mut f = Formatter::new();
            if !f.is_enabled() {
                return Ok(());
            }
            f.theme = Theme::plain(); // line coloring adds no bytes -> exact assertions
            // Drop ESC (no escape sequences/zone changes) and NUL (binary framing
            // is covered separately); this run exercises text framing.
            let clean: Vec<u8> = body.iter().copied().filter(|&b| b != 0x1b && b != 0).collect();
            // Skip inputs that ARE a formattable JSON value — those are meant to change.
            // Exclude anything a formatter would reformat — those are meant to change.
            proptest::prop_assume!(format_recognized(&clean, &Theme::plain(), &crate::config::Formatters::default()).is_none());

            let input = [C, &clean, D].concat();
            let mut out = Vec::new();
            out.extend_from_slice(&f.process(&input));
            proptest::prop_assert_eq!(out, framed(&clean, true));
        }

        /// The EOF-flush path preserves non-JSON user bytes even when the stream
        /// is split at arbitrary boundaries and never closed by a `D` marker (the
        /// shell-crash scenario). Directly guards the truncation bug the audit
        /// caught.
        #[test]
        fn prop_eof_flush_preserves_user_bytes(
            body in proptest::collection::vec(0u8..=255, 0..256),
            splits in proptest::collection::vec(1usize..40, 1..10),
        ) {
            let mut f = Formatter::new();
            if !f.is_enabled() {
                return Ok(());
            }
            f.theme = Theme::plain(); // line coloring adds no bytes -> exact assertions
            // Drop ESC (no escape sequences/zone changes) and NUL (binary framing
            // is covered separately); this run exercises text framing.
            let clean: Vec<u8> = body.iter().copied().filter(|&b| b != 0x1b && b != 0).collect();
            // Exclude anything a formatter would reformat — those are meant to change.
            proptest::prop_assume!(format_recognized(&clean, &Theme::plain(), &crate::config::Formatters::default()).is_none());

            // C + body, but NO closing D — then flush, simulating PTY EOF.
            let input = [C, &clean].concat();
            let mut out = Vec::new();
            let (mut i, mut si) = (0usize, 0usize);
            while i < input.len() {
                let step = splits[si % splits.len()].min(input.len() - i).max(1);
                out.extend_from_slice(&f.process(&input[i..i + step]));
                i += step;
                si += 1;
            }
            out.extend_from_slice(&f.flush());
            proptest::prop_assert_eq!(out, framed(&clean, false));
        }
    }
}
