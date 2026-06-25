//! GLIMPS — zero-config smart terminal output formatter.
//!
//! PHASE 0 SPIKE (this file):
//! The single make-or-break question for the whole project is:
//! "Can we run the user's shell inside a PTY we own, transparently, so it
//!  feels exactly like a normal terminal?" If that feels native, everything
//!  else (JSON/HTML/log formatting) plugs into one seam: `format::Formatter`.
//!
//! This spike does ONLY the transparent pass-through. No formatting yet.
//! Run it, use your shell normally (try vim, ssh, ls, curl), and confirm it
//! feels indistinguishable from a normal terminal. That's the gate.

use anyhow::Result;
use glimps::pty;

fn main() -> Result<()> {
    // Guard against re-exec loops: if we're already inside a GLIMPS PTY,
    // launching another would nest forever. (Matters once `glimps init`
    // auto-wraps the shell from .zshrc.)
    if std::env::var("GLIMPS_ACTIVE").is_ok() {
        eprintln!("glimps: already running inside a GLIMPS session; not nesting.");
        return Ok(());
    }

    // The shell to wrap. Defaults to the user's $SHELL, falling back to zsh.
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    eprintln!("\x1b[2m── glimps spike: wrapping {shell} (type `exit` to leave) ──\x1b[0m");

    let status = pty::run_shell(&shell)?;
    std::process::exit(status);
}
