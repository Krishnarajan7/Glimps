//! End-to-end PTY supervisor tests — the regression net for the scariest code.
//!
//! These drive the *real* `glimps` binary inside a pseudo-terminal (via
//! `portable-pty`, the same crate the supervisor uses) and assert the behavior
//! that ad-hoc scripts used to check by hand:
//!   * `exit`, Ctrl-D, and `kill` (SIGTERM) all terminate GLIMPS — the
//!     "can't exit the app" bug class, which has no other automated guard;
//!   * OSC-133 markers frame output and a JSON body is reformatted end-to-end;
//!   * binary output is passed through without a header injected.
//!
//! Lifecycle tests use `/bin/sh` (always present), so they run everywhere.
//! Formatting tests need `zsh` + the shell integration and skip gracefully where
//! `zsh` isn't installed (e.g. a bare Linux CI image).
#![cfg(unix)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

const GLIMPS: &str = env!("CARGO_BIN_EXE_glimps");

/// A running `glimps` session we can write to and observe.
struct Session {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    output: Arc<Mutex<Vec<u8>>>,
    // The master must outlive the reader/writer; keep it owned here.
    _master: Box<dyn portable_pty::MasterPty + Send>,
}

impl Session {
    fn write(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("write to pty");
        self.writer.flush().expect("flush pty");
    }

    fn snapshot(&self) -> Vec<u8> {
        self.output.lock().expect("output lock").clone()
    }

    /// Poll until the captured output contains `needle`, or the timeout elapses.
    fn wait_for(&self, needle: &[u8], timeout: Duration) -> bool {
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            if contains(&self.snapshot(), needle) {
                return true;
            }
            thread::sleep(Duration::from_millis(25));
        }
        contains(&self.snapshot(), needle)
    }

    /// Poll until the child process has exited, or the timeout elapses.
    fn wait_exit(&mut self, timeout: Duration) -> bool {
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            match self.child.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) => thread::sleep(Duration::from_millis(25)),
                Err(_) => return true, // already reaped / gone
            }
        }
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Don't let a failed test leak a process.
        let _ = self.child.kill();
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Spawn `glimps` wrapping `shell` inside a fresh PTY. If `home` is given it is
/// exported as `ZDOTDIR`/`HOME` so the inner shell sources our integration —
/// zsh reads `$ZDOTDIR/.zshrc`, bash reads `$HOME/.bashrc`.
fn spawn(shell: &str, zdot: Option<&Path>) -> Session {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");

    let mut cmd = CommandBuilder::new(GLIMPS);
    // CommandBuilder inherits the full parent environment, so scrub anything that
    // would change glimps's behavior: GLIMPS_ACTIVE (would trip the no-nesting
    // guard if the test runner is itself inside a glimps session) and GLIMPSRC
    // (a stray config would make formatting non-deterministic).
    cmd.env_remove("GLIMPS_ACTIVE");
    cmd.env_remove("GLIMPSRC");
    cmd.env("SHELL", shell);
    cmd.env("GLIMPS", "1");
    cmd.env("TERM", "xterm-256color");
    if let Some(path) = std::env::var_os("PATH") {
        cmd.env("PATH", path);
    }
    match zdot {
        Some(dir) => {
            cmd.env("ZDOTDIR", dir);
            cmd.env("HOME", dir);
        }
        None => {
            if let Some(home) = std::env::var_os("HOME") {
                cmd.env("HOME", home);
            }
        }
    }

    let child = pair.slave.spawn_command(cmd).expect("spawn glimps");
    drop(pair.slave); // the child holds its own handle now

    let mut reader = pair.master.try_clone_reader().expect("clone reader");
    let writer = pair.master.take_writer().expect("take writer");
    let output = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&output);
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => sink.lock().expect("sink lock").extend_from_slice(&buf[..n]),
            }
        }
    });

    Session {
        child,
        writer,
        output,
        _master: pair.master,
    }
}

/// The absolute path to `zsh` (resolved via `command -v`, so it works whether zsh
/// is at `/bin/zsh` or `/usr/bin/zsh`), or `None` if it isn't installed. Used both
/// to gate the formatting tests AND as the `SHELL` they spawn — a hardcoded
/// `/bin/zsh` would fail spuriously on Linux where zsh lives in `/usr/bin`.
fn zsh_path() -> Option<String> {
    let out = std::process::Command::new("sh")
        .args(["-c", "command -v zsh"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!path.is_empty()).then_some(path)
}

/// The absolute path to `bash`, or `None` if it isn't installed. Mirrors
/// [`zsh_path`] so the bash formatting test gates and spawns the same way.
fn bash_path() -> Option<String> {
    let out = std::process::Command::new("sh")
        .args(["-c", "command -v bash"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!path.is_empty()).then_some(path)
}

/// A throwaway `$HOME` containing a `.bashrc` with GLIMPS's bash integration, so
/// the wrapped bash emits the same OSC-133 markers. Removed on drop (incl. panic).
struct BashHome {
    path: PathBuf,
}

impl BashHome {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("glimps-bit-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create bash home");
        let init = std::process::Command::new(GLIMPS)
            .args(["init", "bash"])
            .output()
            .expect("glimps init bash");
        assert!(init.status.success(), "glimps init bash failed");
        std::fs::write(path.join(".bashrc"), init.stdout).expect("write .bashrc");
        BashHome { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for BashHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A throwaway ZDOTDIR containing GLIMPS's zsh integration (so the wrapped shell
/// emits the OSC-133 markers the formatter needs). The directory is unique per
/// instance and removed on drop — including on a test panic, so failures don't
/// leave temp dirs behind.
struct ZdotDir {
    path: PathBuf,
}

impl ZdotDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("glimps-it-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path); // clear any stale dir from a crashed run
        std::fs::create_dir_all(&path).expect("create zdotdir");
        std::fs::write(path.join(".zshenv"), "unsetopt GLOBAL_RCS\n").expect("write .zshenv");
        let init = std::process::Command::new(GLIMPS)
            .args(["init", "zsh"])
            .output()
            .expect("glimps init zsh");
        assert!(init.status.success(), "glimps init zsh failed");
        std::fs::write(path.join(".zshrc"), init.stdout).expect("write .zshrc");
        ZdotDir { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ZdotDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

const STARTUP: Duration = Duration::from_millis(800);
const EXIT_BUDGET: Duration = Duration::from_secs(10);
const FORMAT_BUDGET: Duration = Duration::from_secs(6);

#[test]
fn exit_command_terminates() {
    let mut s = spawn("/bin/sh", None);
    thread::sleep(STARTUP);
    s.write(b"exit\n");
    assert!(
        s.wait_exit(EXIT_BUDGET),
        "glimps did not terminate after `exit` (the original hang bug)"
    );
}

#[test]
fn ctrl_d_terminates() {
    let mut s = spawn("/bin/sh", None);
    thread::sleep(STARTUP);
    s.write(&[0x04]); // EOT / Ctrl-D at an empty prompt
    assert!(
        s.wait_exit(EXIT_BUDGET),
        "glimps did not terminate after Ctrl-D"
    );
}

#[test]
fn sigterm_terminates_and_reaps() {
    let mut s = spawn("/bin/sh", None);
    thread::sleep(STARTUP);
    let pid =
        libc::pid_t::try_from(s.child.process_id().expect("child pid")).expect("pid fits pid_t");
    // SAFETY: `kill(2)` with a valid pid and a standard signal; touches no Rust state.
    let rc = unsafe { libc::kill(pid, libc::SIGTERM) };
    assert_eq!(rc, 0, "kill(SIGTERM) failed to deliver");
    assert!(
        s.wait_exit(EXIT_BUDGET),
        "glimps did not terminate after SIGTERM (`kill`)"
    );
}

/// OSC-133 prompt-start marker, emitted by the integration's `precmd` before each
/// prompt (including the first). Seeing it means the shell has sourced `.zshrc`
/// and the hooks are installed — a deterministic readiness signal.
const PROMPT_READY: &[u8] = b"\x1b]133;A\x07";
const READY_BUDGET: Duration = Duration::from_secs(3);

#[test]
fn json_output_is_formatted_end_to_end() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let mut s = spawn(&zsh, Some(zdot.path()));
    // Wait for the hooks to be installed before issuing the command (removes the
    // startup timing race; input is buffered anyway, so this is belt-and-braces).
    s.wait_for(PROMPT_READY, READY_BUDGET);
    s.write(b"echo '{\"alpha\":1}'\n");
    // The JSON badge proves: markers framed the output AND the buffered JSON
    // formatter ran end-to-end through the real supervisor.
    assert!(
        s.wait_for(b"JSON", FORMAT_BUDGET),
        "JSON output was not formatted (no badge). Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn bash_json_output_is_formatted_end_to_end() {
    // Same end-to-end contract as zsh, but through the bash DEBUG-trap /
    // PROMPT_COMMAND integration: markers must frame the output and the buffered
    // JSON formatter must run. Skips gracefully where bash isn't installed.
    let Some(bash) = bash_path() else {
        eprintln!("skipping: bash not available");
        return;
    };
    let home = BashHome::new();
    let mut s = spawn(&bash, Some(home.path()));
    s.wait_for(PROMPT_READY, READY_BUDGET);
    s.write(b"echo '{\"alpha\":1}'\n");
    assert!(
        s.wait_for(b"JSON", FORMAT_BUDGET),
        "JSON output was not formatted under bash (no badge). Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn binary_output_is_not_framed() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let mut s = spawn(&zsh, Some(zdot.path()));
    s.wait_for(PROMPT_READY, READY_BUDGET);
    // Emit raw binary; assert the verbatim bytes appear and NO command header bar
    // (▌, U+258C = E2 96 8C) was injected around them.
    let baseline = s.snapshot().len();
    s.write(b"printf 'A\\x01\\x02\\x03B'\n");
    assert!(
        s.wait_for(b"A\x01\x02\x03B", FORMAT_BUDGET),
        "binary payload not passed through verbatim"
    );
    // Give any (erroneous) header a chance to show, then check none was added in
    // the binary command's output region.
    thread::sleep(Duration::from_millis(200));
    let after = s.snapshot();
    assert!(
        !contains(&after[baseline..], "▌".as_bytes()),
        "a command header was injected around binary output"
    );
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
    // `zdot` is removed by its Drop.
}
