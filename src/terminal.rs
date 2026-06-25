//! Controlling-terminal helpers: raw mode (restored on drop) and size query.

use anyhow::Result;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size};

/// Puts the real terminal into raw mode and restores it on drop.
///
/// Raw mode is required so that keystrokes are forwarded byte-for-byte to the
/// inner shell (no local line editing / echo by the outer terminal). The Drop
/// impl guarantees the terminal is restored even if we panic mid-session — a
/// hard requirement for a tool that sits in front of everything you type.
pub struct RawGuard;

impl RawGuard {
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        Ok(RawGuard)
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

/// Current terminal size as (cols, rows). Falls back to 80x24 if unknown.
pub fn term_size() -> (u16, u16) {
    size().unwrap_or((80, 24))
}
