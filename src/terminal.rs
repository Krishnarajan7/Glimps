//! Controlling-terminal helpers: raw mode (restored on drop *and* on panic) and
//! size query.

use std::sync::Once;

use anyhow::Result;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size};

/// Puts the real terminal into raw mode and restores it on every exit path.
///
/// Raw mode is required so keystrokes are forwarded byte-for-byte to the inner
/// shell (no local line editing / echo by the outer terminal). Restoring it is
/// safety invariant #1 — leaving the terminal broken loses a user forever.
///
/// Two mechanisms cover the two ways we can leave:
///   * **Normal / error return:** the `Drop` impl runs and calls
///     `disable_raw_mode()`.
///   * **Panic:** the release profile builds with `panic = "abort"`, so unwinding
///     does *not* run `Drop`. A panic hook (installed by `new`) therefore
///     restores the terminal before the process aborts. This is the standard
///     belt-and-suspenders pattern for full-screen/raw terminal apps and is what
///     makes invariant #1 hold even though `Drop` alone would not under `abort`.
pub struct RawGuard;

impl RawGuard {
    pub fn new() -> Result<Self> {
        install_panic_restore_hook();
        enable_raw_mode()?;
        Ok(RawGuard)
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

/// Install (once) a panic hook that restores the terminal before delegating to
/// the previous hook (which prints the panic message / aborts). Idempotent and
/// safe to call from `RawGuard::new` on every session; `Once` guarantees a
/// single registration. `disable_raw_mode()` is harmless if raw mode is already
/// off, so it does not matter that the hook and `Drop` may both run.
fn install_panic_restore_hook() {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            previous(info);
        }));
    });
}

/// Current terminal size as (cols, rows). Falls back to 80x24 if unknown.
pub fn term_size() -> (u16, u16) {
    size().unwrap_or((80, 24))
}
