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
//! 2. The first non-whitespace byte of an output run *decides* how to handle it
//!    (see [`Formatter::decide`]), staying safe and responsive:
//!      - **binary** (a NUL/other control byte, or invalid UTF-8) streams
//!        verbatim with no header — never framed or reformatted;
//!      - a run a **buffered formatter** claims by its leading bytes (JSON `{`/`[`,
//!        HTML `<`, a unified diff) is accumulated until the command ends, then
//!        reformatted as a unit — bounded by a size cap, and emitted verbatim if it
//!        doesn't actually parse;
//!      - everything else is **streamed** line-by-line, coloring only the lines a
//!        streaming formatter recognizes (log severity / HTTP status / stack trace).
//! 3. Which formatters run is a small **registry** ([`enabled_buffered`] /
//!    [`enabled_streaming`]) built from config; the core dispatches through the
//!    [`BufferedFormatter`] / [`StreamingFormatter`] traits and hard-codes no
//!    content-type knowledge of its own.
//!
//! Net effect: the stream is byte-identical to the input except where a run is a
//! recognized document (reformatted with a badge) or a recognized line (wrapped in
//! color) — and with the plain theme even those are byte-identical, which is how
//! every formatter's byte-safety is tested. Formatting only ever engages when
//! OSC-133 markers are present (a shell with `glimps init`); without them the zone
//! stays Unknown and GLIMPS is a pure pass-through.

mod cmdline;
mod diff;
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

/// A **buffered** formatter: recognizes a fully-collected output run by its leading
/// bytes, then reformats the whole run as a unit. Implemented by JSON / HTML / diff
/// (each in its own module). The core state machine drives all of them uniformly
/// through this trait, so adding one is a new `impl` plus a single line in
/// [`enabled_buffered`] — `decide`/`finalize` never change.
trait BufferedFormatter {
    /// Cheap check on the run's leading bytes (from its first non-whitespace byte):
    /// could this run be ours, i.e. should the core *buffer* it as a candidate?
    /// Being over-eager is safe — an unconfirmed candidate is emitted verbatim.
    ///
    /// `head` is only the bytes of the *first chunk that commits to text*, not the
    /// whole run; a multi-line signature split exactly across a PTY read may be
    /// missed (the run then streams / passes through — never corrupted). Keep the
    /// check satisfiable from the first line or two.
    fn could_start(&self, head: &[u8]) -> bool;
    /// Confirm and reformat the whole buffered run, or decline with `None` (then
    /// the run is emitted verbatim).
    fn try_format(&self, bytes: &[u8], theme: &Theme) -> Option<Vec<u8>>;
    /// The content-type badge shown above the reformatted output (e.g. `JSON`).
    fn label(&self) -> &'static str;
    /// Whether the formatted bytes use bare `\n` and so must be CRLF-normalized for
    /// the raw terminal. Formatters that *regenerate* content (JSON/HTML) return
    /// `true`; ones that *preserve* the user's own line endings (diff) return
    /// `false` — running those through `push_crlf` would double their CRs.
    fn needs_crlf(&self) -> bool;
}

/// A **streaming** formatter: colors a single complete output line as it streams
/// (suits unbounded output like `tail -f`). Implemented by the log-severity /
/// HTTP-status / stack-trace colorizers. Returns the SGR color to wrap the line's
/// content in, or `None` to leave the line untouched.
trait StreamingFormatter {
    fn line_color(&self, content: &[u8], theme: &Theme) -> Option<&'static str>;
}

/// The buffered formatters enabled by `fmts`, in priority order (more specific
/// before looser). This single list is the whole buffered-dispatch registry.
fn enabled_buffered(fmts: &crate::config::Formatters) -> Vec<&'static dyn BufferedFormatter> {
    let mut v: Vec<&'static dyn BufferedFormatter> = Vec::new();
    if fmts.json {
        v.push(&json::Json);
    }
    if fmts.html {
        v.push(&html::Html);
    }
    if fmts.diff {
        v.push(&diff::Diff);
    }
    v
}

/// The streaming formatters enabled by `fmts`, in priority order.
fn enabled_streaming(fmts: &crate::config::Formatters) -> Vec<&'static dyn StreamingFormatter> {
    let mut v: Vec<&'static dyn StreamingFormatter> = Vec::new();
    if fmts.http {
        v.push(&linefmt::Http);
    }
    if fmts.logs {
        v.push(&linefmt::Logs);
    }
    if fmts.stacktrace {
        v.push(&linefmt::StackTrace);
    }
    v
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
    /// the header is owed and will be written just before that first byte.
    /// Lazy on purpose: commands with no output get no header.
    pending_separator: bool,
    /// The command for the current output zone (captured at output start), shown
    /// in the header. `None` falls the header back to a plain divider.
    pending_command: Option<Vec<u8>>,
    /// Whether the previous chunk left a full-screen TUI on the alternate screen.
    /// Tracked so the chunk that *exits* alt-screen is also passed through.
    was_alt_screen: bool,
    /// Buffered formatters (JSON/HTML/diff) enabled by config, in priority order.
    buffered: Vec<&'static dyn BufferedFormatter>,
    /// Streaming line colorizers (HTTP/logs/stack-trace) enabled by config.
    streaming: Vec<&'static dyn StreamingFormatter>,
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
        let buffered = enabled_buffered(&config.formatters);
        let streaming = enabled_streaming(&config.formatters);
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
            pending_command: None,
            was_alt_screen: false,
            buffered,
            streaming,
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
            // header here is intentional — including the common case where output
            // just started with no text yet: a TUI gets no header. Also clear the
            // captured command so it can't leak into the next command's header.
            let mut out = Vec::with_capacity(chunk.len());
            self.finalize(&mut out);
            self.pending_separator = false;
            self.pending_command = None;
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
                // A command's output has begun: capture the command (for the
                // header) and owe the header, emitted lazily before the first
                // output byte (so empty output -> no header). If the command is on
                // the bypass list, its output streams through untouched.
                Seg::OutputStart => {
                    self.pending_separator = true;
                    self.pending_command = self.scanner.take_command();
                    let bypass = self
                        .pending_command
                        .as_deref()
                        .and_then(cmdline::first_word)
                        .is_some_and(|name| self.config.bypass.iter().any(|b| b == &name));
                    if bypass {
                        self.collect = Collect::Passthrough;
                    }
                }
                // The output ended: flush, and drop an unfulfilled header/command.
                Seg::OutputEnd => {
                    self.finalize(&mut out);
                    self.pending_separator = false;
                    self.pending_command = None;
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
                // Bypassed commands (vim/ssh/…) still get a header, then stream
                // verbatim. On the binary path the owed header was already cleared
                // in `decide` (so this is a no-op there); bypass is forced at
                // OutputStart and skips `decide`, so its header is emitted here.
                self.emit_header(out);
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
    /// binary content ([`looks_binary`]: a NUL or other non-text control byte, or
    /// invalid UTF-8) detected before any text commits is streamed verbatim with no
    /// separator (invariant #3: never frame or reformat binary).
    fn decide(&mut self, mut acc: Vec<u8>, seg: &[u8], out: &mut Vec<u8>) -> Collect {
        // `acc` only ever holds ASCII whitespace (see the `Sniff` arm below), so
        // scanning it is belt-and-suspenders — but it also means a multibyte char
        // can never straddle the acc/seg seam, so checking the two separately is
        // sound (no split character is misread as invalid UTF-8).
        if looks_binary(&acc) || looks_binary(seg) {
            self.pending_separator = false; // suppress: this is binary, never frame it
            out.extend_from_slice(&acc);
            out.extend_from_slice(seg);
            return Collect::Passthrough;
        }
        match seg.iter().position(|b| !b.is_ascii_whitespace()) {
            Some(pos) => {
                // First real byte: commit to text -> the separator is now due.
                // Does any enabled buffered formatter want to claim this run by its
                // leading bytes? Confirmation happens later in `recognize`; an
                // unconfirmed candidate is emitted verbatim, so eagerness is safe.
                let buffer_candidate = self.buffered.iter().any(|f| f.could_start(&seg[pos..]));
                acc.extend_from_slice(seg);
                self.emit_header(out);
                if buffer_candidate && acc.len() <= self.config.limits.buffer_cap {
                    Collect::Buffer(acc) // a formatting candidate; hold it
                } else if !buffer_candidate && !self.streaming.is_empty() {
                    // Plain text: stream line-by-line through the streaming colorizers.
                    self.push_stream(Vec::new(), &acc, out)
                } else {
                    // No formatter wants it (or the blob is too big): verbatim.
                    out.extend_from_slice(&acc);
                    Collect::Passthrough
                }
            }
            None => {
                // Still only whitespace; keep waiting (no separator yet), but don't
                // hoard unboundedly.
                acc.extend_from_slice(seg);
                if acc.len() > self.config.limits.sniff_cap {
                    self.emit_header(out);
                    out.extend_from_slice(&acc);
                    Collect::Passthrough
                } else {
                    Collect::Sniff(acc)
                }
            }
        }
    }

    /// Emit the owed command header (if any) to `out`, clearing the debt. When the
    /// header is disabled in config, the debt is cleared without emitting.
    fn emit_header(&mut self, out: &mut Vec<u8>) {
        if self.pending_separator {
            self.pending_separator = false;
            if self.config.separator {
                let header = self.render_header();
                out.extend_from_slice(&header);
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
                emit_line(out, &line, &self.theme, &self.streaming);
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
                match recognize(&self.buffered, &buf, &self.theme) {
                    // Reformatted: tag it with a content-type badge, then emit the
                    // formatted bytes — CRLF-normalized only for formatters that say
                    // so (JSON/HTML regenerate `\n`; diff preserves the user's own).
                    Some((label, formatted, needs_crlf)) => {
                        out.extend_from_slice(&render_badge(label, self.config.color));
                        if needs_crlf {
                            push_crlf(out, &formatted);
                        } else {
                            out.extend_from_slice(&formatted);
                        }
                    }
                    // Not a type we format: the user's bytes, emitted exactly as-is.
                    None => out.extend_from_slice(&buf),
                }
            }
            Collect::Sniff(acc) => {
                // Whitespace-only output that never committed: it still counts as
                // output, so emit the owed separator, then the whitespace verbatim.
                self.emit_header(out);
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

    /// Render the command/output header: the syntax-colored command (captured at
    /// output start) with a dim bar prefix and timestamp, so you can instantly
    /// find your input. Falls back to a dim rule when no command was captured
    /// (e.g. a shell without `glimps init`'s command marker). CRLF-framed.
    fn render_header(&self) -> Vec<u8> {
        let dim: &[u8] = if self.config.color { SEP_DIM } else { b"" };
        let reset: &[u8] = if self.config.color { SEP_RESET } else { b"" };
        let mut h = Vec::new();
        match self.pending_command.as_deref() {
            Some(cmd) if !cmd.is_empty() => {
                // ▌ <colored command>   <dim timestamp>
                h.extend_from_slice(dim);
                h.extend_from_slice("▌ ".as_bytes());
                h.extend_from_slice(reset);
                // The command is GLIMPS-rendered display: convert any newline to
                // CRLF for the raw terminal.
                push_crlf(&mut h, &cmdline::render(cmd, &self.theme));
                if let Some(ts) = self.timestamp() {
                    h.extend_from_slice(dim);
                    h.extend_from_slice(b"  ");
                    h.extend_from_slice(ts.as_bytes());
                    h.extend_from_slice(reset);
                }
            }
            // No command captured: fall back to a plain dim rule (+ timestamp).
            _ => {
                h.extend_from_slice(dim);
                match self.timestamp() {
                    Some(ts) => {
                        h.extend_from_slice(SEP_DASH.repeat(8).as_bytes());
                        h.push(b' ');
                        h.extend_from_slice(ts.as_bytes());
                        h.push(b' ');
                        h.extend_from_slice(SEP_DASH.repeat(8).as_bytes());
                    }
                    None => h.extend_from_slice(SEP_DASH.repeat(18).as_bytes()),
                }
                h.extend_from_slice(reset);
            }
        }
        h.extend_from_slice(b"\r\n");
        h
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

/// Try each formatter in the registry in order, returning the content-type label,
/// reformatted bytes, and CRLF policy of the first that matches — or `None` to pass
/// the buffer through verbatim. The whole buffered dispatch is this one `find_map`.
fn recognize(
    buffered: &[&dyn BufferedFormatter],
    bytes: &[u8],
    theme: &Theme,
) -> Option<(&'static str, Vec<u8>, bool)> {
    buffered.iter().find_map(|f| {
        f.try_format(bytes, theme)
            .map(|out| (f.label(), out, f.needs_crlf()))
    })
}

/// Test-facing convenience: build the registry for `fmts` and run [`recognize`].
#[cfg(test)]
fn format_recognized(
    bytes: &[u8],
    theme: &Theme,
    fmts: &crate::config::Formatters,
) -> Option<(&'static str, Vec<u8>, bool)> {
    recognize(&enabled_buffered(fmts), bytes, theme)
}

/// Whether a run of OUTPUT bytes looks like **binary** — content GLIMPS must never
/// frame, color, or reformat (invariant #3). Deliberately conservative: a false
/// positive only means we pass real text through untouched (safe), whereas a false
/// negative means we mangle binary — the failure this prevents (invariant #2).
///
/// Two signals, both vanishingly rare in terminal *text*:
///   * a [binary control byte](is_binary_byte) (NUL and the other non-printing C0
///     controls a binary file is full of, or DEL); or
///   * an invalid UTF-8 sequence — but NOT a multibyte character merely *split* at
///     the end of the chunk, which is a buffering boundary, not corruption
///     (invariant #4). `Utf8Error::error_len() == None` marks that incomplete-tail
///     case and is treated as "not (yet) binary".
fn looks_binary(bytes: &[u8]) -> bool {
    if bytes.iter().copied().any(is_binary_byte) {
        return true;
    }
    match std::str::from_utf8(bytes) {
        Ok(_) => false,
        Err(e) => e.error_len().is_some(),
    }
}

/// A byte that essentially never occurs in plain terminal text: a C0 control other
/// than the ordinary whitespace/formatting ones (TAB, LF, VT, FF, CR, BEL, BS) or
/// `ESC` (which introduces ANSI), or `DEL`.
fn is_binary_byte(b: u8) -> bool {
    matches!(b, 0x00..=0x06 | 0x0e..=0x1a | 0x1c..=0x1f | 0x7f)
}

/// Emit one complete line, colored by log severity / HTTP status (per the enabled
/// categories) if it matches, else verbatim. The user's bytes (content + line
/// ending) are always preserved; only color codes are added.
fn emit_line(out: &mut Vec<u8>, line: &[u8], theme: &Theme, streaming: &[&dyn StreamingFormatter]) {
    if let Some(colored) = linefmt::colorize_line(line, theme, streaming) {
        // Only inject color into genuine text. A line that slipped binary control
        // bytes into a text stream (binary appearing mid-run, after a text commit)
        // is emitted verbatim — never wrap binary in SGR (invariant #3). The scan
        // runs only on a *matched* line, so it's off the common hot path. (ESC
        // can't appear here: the scanner routes every escape byte to `Pass`.)
        if !line.iter().copied().any(is_binary_byte) {
            out.extend_from_slice(&colored);
            return;
        }
    }
    out.extend_from_slice(line);
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
mod tests;
