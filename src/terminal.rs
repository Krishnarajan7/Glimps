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
        #[cfg(windows)]
        windows_vt::remember_console_modes();
        enable_raw_mode()?;
        // The guard exists from this point on, so every later failure — and a
        // panic — restores the terminal. VT enablement is best-effort: an old
        // console that rejects the flags degrades to "no VT", it must never
        // abort the session and leave raw mode on with no guard in scope.
        let guard = RawGuard;
        #[cfg(windows)]
        let _ = windows_vt::enable();
        Ok(guard)
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

/// Everything needed to hand the terminal back: crossterm's raw mode off, and
/// on Windows the console modes put back exactly as they were found. Harmless
/// to run more than once (the panic hook and `Drop` may both call it).
fn restore_terminal() {
    let _ = disable_raw_mode();
    #[cfg(windows)]
    windows_vt::restore();
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
            restore_terminal();
            previous(info);
        }));
    });
}

/// Current terminal size as (cols, rows). Falls back to 80x24 if unknown.
pub fn term_size() -> (u16, u16) {
    try_term_size().unwrap_or((80, 24))
}

/// Current terminal size as (cols, rows), or `None` if the query failed. For
/// callers that poll: a transient failure must be skipped, never applied as
/// the 80x24 fallback (which would reflow the user's screen).
pub fn try_term_size() -> Option<(u16, u16)> {
    size().ok()
}

/// Windows console VT flags that crossterm's raw mode does not touch.
///
/// GLIMPS treats the console as a byte pipe: it reads keystrokes from stdin as
/// bytes and writes the ConPTY's ANSI output to stdout as bytes. Two console
/// modes make that true on Windows:
///   * `ENABLE_VIRTUAL_TERMINAL_INPUT` on stdin, so arrow/function keys arrive
///     as VT escape sequences (forwardable to the inner shell) instead of only
///     as `INPUT_RECORD`s we never read.
///   * `ENABLE_VIRTUAL_TERMINAL_PROCESSING` on stdout, so colors and cursor
///     sequences render under legacy conhost (Windows Terminal sets it already).
///
/// The pre-session modes are remembered once and restored exactly, because
/// crossterm's `disable_raw_mode` only ORs its own flags back in and would
/// otherwise leave VT input enabled in the user's outer shell.
#[cfg(windows)]
mod windows_vt {
    use crossterm_winapi::{ConsoleMode, Handle};
    use std::sync::atomic::{AtomicU64, Ordering};

    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
    const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x0200;
    /// "Not captured" sentinel; a real mode is a `u32` and never reaches this.
    const UNSET: u64 = u64::MAX;
    static SAVED_INPUT: AtomicU64 = AtomicU64::new(UNSET);
    static SAVED_OUTPUT: AtomicU64 = AtomicU64::new(UNSET);

    /// Capture the console modes before anything changes them (first call wins).
    pub(super) fn remember_console_modes() {
        if let Ok(mode) = Handle::current_in_handle().and_then(|h| ConsoleMode::from(h).mode()) {
            let _ = SAVED_INPUT.compare_exchange(
                UNSET,
                u64::from(mode),
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        if let Ok(mode) = Handle::current_out_handle().and_then(|h| ConsoleMode::from(h).mode()) {
            let _ = SAVED_OUTPUT.compare_exchange(
                UNSET,
                u64::from(mode),
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
    }

    /// Turn on VT input and VT output processing on top of raw mode.
    pub(super) fn enable() -> std::io::Result<()> {
        let input = ConsoleMode::from(Handle::current_in_handle()?);
        input.set_mode(input.mode()? | ENABLE_VIRTUAL_TERMINAL_INPUT)?;
        let output = ConsoleMode::from(Handle::current_out_handle()?);
        output.set_mode(output.mode()? | ENABLE_VIRTUAL_TERMINAL_PROCESSING)?;
        Ok(())
    }

    /// Put both console modes back exactly as `remember_console_modes` found them.
    pub(super) fn restore() {
        // `UNSET` is the only value that fails the conversion (a real mode is a
        // `u32`), so "not captured" and "restore" fall out of one check.
        if let Ok(saved_input) = u32::try_from(SAVED_INPUT.load(Ordering::Acquire)) {
            if let Ok(handle) = Handle::current_in_handle() {
                let _ = ConsoleMode::from(handle).set_mode(saved_input);
            }
        }
        if let Ok(saved_output) = u32::try_from(SAVED_OUTPUT.load(Ordering::Acquire)) {
            if let Ok(handle) = Handle::current_out_handle() {
                let _ = ConsoleMode::from(handle).set_mode(saved_output);
            }
        }
    }
}
