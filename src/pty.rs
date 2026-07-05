//! PTY supervisor — the heart of GLIMPS.
//!
//! We open a pseudo-terminal, spawn the user's shell on the *slave* side, and
//! own the *master* side. Three jobs run concurrently:
//!   1. stdin  -> PTY master   (forward the user's keystrokes to the shell)
//!   2. PTY master -> Formatter -> stdout   (the shell's output, reformatted)
//!   3. SIGWINCH -> resize the PTY   (keep the inner shell's size in sync)
//!
//! Job #2 still defaults to pass-through unless the formatter confidently claims
//! an output run, so the session should feel identical to a normal terminal for
//! commands GLIMPS does not understand.

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM, SIGWINCH};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::config::Config;
use crate::format::{Clock, Formatter};
use crate::terminal::{term_size, RawGuard};

/// Wrap `shell` inside a PTY and run it transparently. Returns the shell's
/// exit code once it terminates. `clock` sources the separator timestamp
/// (captured by the caller while still single-threaded); `config` is the loaded
/// `.glimpsrc`.
pub fn run_shell(shell: &str, clock: Clock, config: Config) -> Result<i32> {
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

    // Spawn the shell as an *interactive* (not login) shell on the slave side,
    // marking the environment so a nested GLIMPS won't re-wrap (see main.rs
    // guard). `-i`, not `-l`, on purpose:
    //   * The outer shell (the one whose rc `exec`s us) already sourced the login
    //     files (`.zprofile`/`.bash_profile`/`.zlogin`) and we inherit its whole
    //     environment. A login inner shell would re-run those files — doubling
    //     PATH edits, banners, and startup work every session. An interactive
    //     shell re-runs only the interactive rc (`.zshrc`/`.bashrc`), once.
    //   * bash reads `.bashrc` for an interactive shell but `.bash_profile` for a
    //     login one — and the integration line lives in `.bashrc`. `-i` is what
    //     makes the inner bash actually load the hooks.
    // The shell is interactive anyway (its stdio is this PTY); `-i` just makes it
    // explicit and picks the right rc file.
    let mut cmd = CommandBuilder::new(shell);
    cmd.arg("-i");
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

    // Job #2: PTY -> Formatter -> stdout. Signals completion on `done_tx` so the
    // main thread can wait for it with a timeout rather than block forever (the
    // master read does not always return EOF promptly on shell exit).
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let mut reader = pair.master.try_clone_reader()?;
    thread::spawn(move || {
        let mut formatter = Formatter::for_supervisor(clock, config);
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
        // The master closed while the formatter may still be holding a buffered
        // command output that never got an OSC-133 `D` marker (shell crash, SSH
        // drop, …). Flush it so those bytes are never silently truncated.
        let tail = formatter.flush();
        if !tail.is_empty() {
            let _ = stdout.write_all(&tail);
            let _ = stdout.flush();
        }
        let _ = done_tx.send(());
    });

    // Job #1: stdin -> PTY. Detached on purpose: this thread blocks in
    // `stdin.read()` and is reclaimed by `process::exit` when the shell exits.
    // (The reader thread, by contrast, is joined below so it can drain.)
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

    // Job #3 + lifecycle: on the main thread, watch for window resizes, the shell
    // exiting, and termination signals. Catching SIGTERM/SIGINT/SIGHUP is what
    // lets `kill` actually work: we exit through the normal path so the RawGuard
    // restores the terminal (a default-handled signal would leave it in raw mode).
    // Atomic flags set by the signal handlers are the simplest reliable mechanism.
    let terminate = Arc::new(AtomicBool::new(false));
    for sig in [SIGINT, SIGTERM, SIGHUP] {
        signal_hook::flag::register(sig, Arc::clone(&terminate))
            .context("failed to register termination signal handler")?;
    }
    let resized = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(SIGWINCH, Arc::clone(&resized))
        .context("failed to register SIGWINCH handler")?;

    let mut signaled = false;
    let exit_code = loop {
        if let Some(status) = child.try_wait()? {
            // Mask to the low byte — the only part `_exit`/the kernel keep — so a
            // wide `u32` (e.g. a signal-encoded status) can't wrap to a negative
            // `i32` on the way to `libc::_exit`.
            break (status.exit_code() & 0xff) as i32;
        }
        if terminate.load(Ordering::Acquire) {
            // Asked to terminate (e.g. `kill`): exit cleanly so the terminal is
            // restored. We deliberately do NOT kill/reap the shell here — when we
            // exit, the kernel revokes our controlling terminal and SIGHUPs the
            // shell's process group, tearing it down. Reaping it first can stall
            // the session-leader exit path. (Same teardown as an outside SIGKILL.)
            // Trade-off: because the shell keeps running, the reader stays blocked
            // and any output still buffered inside the formatter is discarded by
            // the `_exit` below. That is acceptable for a forced kill (and the
            // buffer is empty at an idle prompt, the usual moment to `kill`); a
            // clean `exit`/Ctrl-D always flushes via the EOF path in the reader.
            signaled = true;
            break 130;
        }
        if resized.swap(false, Ordering::Acquire) {
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

    // On a clean shell exit, reap it (no-op if `try_wait` already did) so its slave
    // fd is fully closed — which lets the reader see EOF and finish. On the signal
    // path the shell is still running; we leave it to the kernel and exit promptly.
    if !signaled {
        let _ = child.wait();
    }

    // Drop our master handle (just cleanup — the reader holds its own cloned fd,
    // so this alone does not wake it) and let the reader drain the final output,
    // then return: dropping `_raw` restores the terminal and `main` exits the
    // process, reaping the detached threads. On a clean exit the reader's EOF comes
    // from the slave closing once `child.wait()` reaps the shell; we then wait
    // generously so a large trailing burst is never truncated. The wait is always
    // BOUNDED (never an unbounded `join()`, which hung when the read never EOF'd):
    // on the signal path the shell is still alive so the reader never EOFs and we
    // just burn the short budget before exiting.
    drop(pair.master);
    let drain = if signaled {
        Duration::from_millis(200)
    } else {
        Duration::from_secs(2)
    };
    let _ = done_rx.recv_timeout(drain);
    Ok(exit_code)
}
