//! PTY supervisor — the heart of GLIMPS.
//!
//! We open a pseudo-terminal, spawn the user's shell on the *slave* side, and
//! own the *master* side. Three jobs run concurrently:
//!   1. stdin  -> PTY master   (forward the user's keystrokes to the shell)
//!   2. PTY master -> Formatter -> stdout   (the shell's output, reformatted)
//!   3. SIGWINCH -> resize the PTY   (keep the inner shell's size in sync)
//!
//! In this Phase 0 spike, job #2's Formatter is a transparent pass-through,
//! so the session should feel identical to a normal terminal.

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use signal_hook::consts::SIGWINCH;
use signal_hook::iterator::Signals;
use std::io::{Read, Write};
use std::thread;
use std::time::Duration;

use crate::format::Formatter;
use crate::terminal::{term_size, RawGuard};

/// Wrap `shell` inside a PTY and run it transparently. Returns the shell's
/// exit code once it terminates.
pub fn run_shell(shell: &str) -> Result<i32> {
    let (cols, rows) = term_size();

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("failed to open pty")?;

    // Spawn the shell as a login shell on the slave side, marking the
    // environment so a nested GLIMPS won't re-wrap (see main.rs guard).
    let mut cmd = CommandBuilder::new(shell);
    cmd.arg("-l");
    cmd.env("GLIMPS_ACTIVE", "1");
    if let Ok(cwd) = std::env::current_dir() {
        cmd.cwd(cwd);
    }

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .context("failed to spawn shell")?;

    // The child holds its own handle to the slave now; drop ours so the master
    // sees EOF promptly when the shell exits.
    drop(pair.slave);

    // Put the real terminal in raw mode (restored when `_raw` drops).
    let _raw = RawGuard::new().context("failed to enable raw mode")?;

    // Job #2: PTY -> Formatter -> stdout
    let mut reader = pair.master.try_clone_reader()?;
    let reader_thread = thread::spawn(move || {
        let mut formatter = Formatter::new();
        let mut stdout = std::io::stdout();
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break, // shell exited, master closed
                Ok(n) => {
                    let out = formatter.process(&buf[..n]);
                    if stdout.write_all(&out).is_err() || stdout.flush().is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Job #1: stdin -> PTY
    let mut writer = pair.master.take_writer()?;
    thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 4096];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if writer.write_all(&buf[..n]).is_err() || writer.flush().is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Job #3 + lifecycle: on the main thread, watch for window resizes and for
    // the shell exiting. Polling keeps the spike simple and dependency-light.
    let mut signals = Signals::new([SIGWINCH])?;
    let exit_code = loop {
        if let Some(status) = child.try_wait()? {
            break status.exit_code() as i32;
        }
        for _ in signals.pending() {
            let (cols, rows) = term_size();
            let _ = pair.master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
        thread::sleep(Duration::from_millis(40));
    };

    // Let the reader drain the last bytes before we restore the terminal.
    let _ = reader_thread.join();
    Ok(exit_code)
}
