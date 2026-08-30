//! `glimps off` / `glimps on` — pause and resume formatting mid-session.
//!
//! Runs *inside* the wrapped shell, so stdout ultimately flows through the
//! supervisor's PTY. The signal travels in-band as a private OSC (`7340`) that
//! the OSC-133 scanner captures; terminals ignore unknown OSCs, so the sequence
//! is invisible even when nothing is listening. Session-scoped by design — the
//! persistent switches are `GLIMPS=0` and `enabled = false` in `~/.glimpsrc`.

use anyhow::Result;
use std::io::{IsTerminal, Write};

/// Emit the pause/resume toggle. Returns the process exit code.
pub fn toggle(resume: bool) -> Result<i32> {
    if std::env::var_os("GLIMPS_ACTIVE").is_none() {
        let verb = if resume { "resume" } else { "pause" };
        eprintln!("glimps: not inside a GLIMPS session; nothing to {verb}.");
        eprintln!(
            "glimps: to keep formatting off persistently, use GLIMPS=0 or set \
             `enabled = false` in ~/.glimpsrc."
        );
        return Ok(1);
    }
    if std::env::var_os("GLIMPS").as_deref() == Some(std::ffi::OsStr::new("0")) {
        eprintln!("glimps: formatting is disabled by GLIMPS=0 in this environment; the toggle has no effect.");
        return Ok(1);
    }
    // The OSC must travel through the session's PTY to reach the supervisor.
    // Redirected/piped stdout would swallow it — and inject an escape sequence
    // into the user's file or pipe. Refuse instead of pretending it worked.
    if !std::io::stdout().is_terminal() {
        eprintln!("glimps: stdout is not a terminal; the toggle cannot reach the session.");
        return Ok(2);
    }
    let mut stdout = std::io::stdout();
    let body = if resume { "on" } else { "off" };
    // OSC first (the supervisor acts on it), then the human confirmation.
    write!(stdout, "\x1b]7340;{body}\x07")?;
    if resume {
        writeln!(stdout, "glimps: formatting resumed for this session.")?;
    } else {
        writeln!(
            stdout,
            "glimps: formatting paused for this session — run `glimps on` to resume."
        )?;
    }
    stdout.flush()?;
    Ok(0)
}
