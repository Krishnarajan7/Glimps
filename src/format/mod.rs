//! The formatting seam.
//!
//! EVERYTHING GLIMPS does to output flows through `Formatter::process`. The
//! PTY supervisor (src/pty.rs) feeds it raw byte chunks from the shell, and it
//! returns the bytes to actually write to the screen.
//!
//! Phase 0 (now): transparent pass-through. The session must feel native.
//! Phase 1+: this grows into:
//!   - an OSC-133 state machine that knows whether we're in the PROMPT zone
//!     (never touch) or the command-OUTPUT zone (safe to reformat),
//!   - safety gates: pass through if output already has ANSI color, is binary,
//!     or isn't going to a TTY,
//!   - detectors + formatters: JSON pretty-print, HTML indent, log severity,
//!     HTTP status — see ROADMAP.md.
//!
//! Keeping all of that behind this one type means the spike and the full tool
//! share the exact same wiring; only this module gets smarter.

/// Stateful, streaming output processor. Holds whatever cross-chunk state the
/// detectors need (current zone, partial-line buffer, etc.). For the spike it
/// holds nothing.
pub struct Formatter {
    // Phase 1: zone: Zone, line_buf: Vec<u8>, ...
}

impl Formatter {
    pub fn new() -> Self {
        Formatter {}
    }

    /// Process one chunk of bytes coming from the PTY, returning the bytes to
    /// write to the real terminal.
    ///
    /// SPIKE: identity. Do not change a single byte — that's how we prove the
    /// PTY layer itself is transparent before any formatting is added.
    pub fn process(&mut self, chunk: &[u8]) -> Vec<u8> {
        chunk.to_vec()
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new()
    }
}
