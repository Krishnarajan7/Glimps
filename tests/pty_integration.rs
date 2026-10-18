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

    /// Like `wait_for`, but matched against the output with every CSI escape
    /// removed, for a needle GLIMPS may legitimately paint (a `KEY=value`
    /// line gets its key colored by the report pass).
    fn wait_for_plain(&self, needle: &[u8], timeout: Duration) -> bool {
        let end = Instant::now() + timeout;
        loop {
            if contains(&strip_csi(&self.snapshot()), needle) {
                return true;
            }
            if Instant::now() >= end {
                return false;
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn wait_for_any(&self, needles: &[&[u8]], timeout: Duration) -> bool {
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            let snapshot = self.snapshot();
            if needles.iter().any(|needle| contains(&snapshot, needle)) {
                return true;
            }
            thread::sleep(Duration::from_millis(25));
        }
        let snapshot = self.snapshot();
        needles.iter().any(|needle| contains(&snapshot, needle))
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

/// Remove CSI sequences (`ESC [ … final`), leaving every other byte in place.
fn strip_csi(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && bytes.get(i + 1) == Some(&b'[') {
            let mut j = i + 2;
            while j < bytes.len() && !(0x40..=0x7e).contains(&bytes[j]) {
                j += 1;
            }
            i = (j + 1).min(bytes.len());
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Spawn `glimps` wrapping `shell` inside a fresh PTY. If `home` is given it is
/// exported as `ZDOTDIR`/`HOME` so the inner shell sources our integration —
/// zsh reads `$ZDOTDIR/.zshrc`, bash reads `$HOME/.bashrc`.
fn spawn(shell: &str, zdot: Option<&Path>) -> Session {
    spawn_with_config(shell, zdot, None)
}

fn spawn_with_config(shell: &str, zdot: Option<&Path>, config: Option<&Path>) -> Session {
    spawn_with_env(shell, zdot, config, &[])
}

fn spawn_with_env(
    shell: &str,
    zdot: Option<&Path>,
    config: Option<&Path>,
    extra_env: &[(&str, &std::ffi::OsStr)],
) -> Session {
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
    match config {
        Some(path) => cmd.env("GLIMPSRC", path),
        None => cmd.env_remove("GLIMPSRC"),
    }
    cmd.env("SHELL", shell);
    cmd.env("GLIMPS", "1");
    cmd.env("TERM", "xterm-256color");
    // Terminal identity drives `auto` detection ([links], [notify]); the
    // terminal running the tests must not change what GLIMPS emits.
    for var in [
        "TERM_PROGRAM",
        "KITTY_WINDOW_ID",
        "ALACRITTY_WINDOW_ID",
        "WT_SESSION",
        "VTE_VERSION",
        "TMUX",
    ] {
        cmd.env_remove(var);
    }
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

    for (key, value) in extra_env {
        cmd.env(key, value);
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

/// The absolute path to `fish`, or `None` if it isn't installed.
fn fish_path() -> Option<String> {
    let out = std::process::Command::new("sh")
        .args(["-c", "command -v fish"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!path.is_empty()).then_some(path)
}

/// Resolve `more` for the pager-liveness regression, or skip where unavailable.
fn more_path() -> Option<String> {
    let out = std::process::Command::new("sh")
        .args(["-c", "command -v more"])
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
        // Keep the test shell hermetic. Hosted runners may install global zsh
        // startup files or completion directories with permissions that make
        // `compinit` stop for an interactive security prompt before our hooks
        // load. `GLOBAL_RCS` skips those runner-owned files while still loading
        // this ZDOTDIR's `.zshrc`.
        std::fs::write(path.join(".zshenv"), b"unsetopt GLOBAL_RCS\n").expect("write .zshenv");
        let mut zshrc = String::from(
            "setopt auto_menu complete_in_word\n\
             zstyle ':completion:*' menu select\n",
        );
        zshrc.push_str(std::str::from_utf8(&init.stdout).expect("zsh init is utf8"));
        std::fs::write(path.join(".zshrc"), zshrc).expect("write .zshrc");
        std::fs::write(
            path.join("Cargo.toml"),
            b"[package]\nname = \"glimps-tab-fixture\"\n",
        )
        .expect("write completion fixture");
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
    assert!(
        s.wait_for(b"glimps:", Duration::from_secs(1)),
        "normal `exit` did not print the farewell"
    );
    assert!(
        contains(&s.snapshot(), b"GLIMPS warning: shell '/bin/sh'"),
        "unsupported shell fallback was silent"
    );
}

#[test]
fn farewell_config_switch_silences_clean_exit_message() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);

    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let config_path = std::env::temp_dir().join(format!(
        "glimps-no-farewell-{}-{n}.toml",
        std::process::id()
    ));
    std::fs::write(&config_path, b"farewell = false\n").expect("write test config");
    let mut s = spawn_with_config("/bin/sh", None, Some(&config_path));
    thread::sleep(STARTUP);
    s.write(b"exit\n");
    assert!(s.wait_exit(EXIT_BUDGET), "glimps did not exit");
    thread::sleep(Duration::from_millis(100));
    assert!(
        !contains(&s.snapshot(), "✨ glimps:".as_bytes()),
        "farewell=false still printed the farewell"
    );
    let _ = std::fs::remove_file(config_path);
}

#[test]
fn exit_after_failed_command_still_prints_farewell() {
    let mut s = spawn("/bin/sh", None);
    thread::sleep(STARTUP);
    s.write(b"false\nexit\n");
    assert!(
        s.wait_exit(EXIT_BUDGET),
        "glimps did not terminate after a failed command followed by `exit`"
    );
    assert!(
        s.wait_for(b"glimps:", Duration::from_secs(1)),
        "a previous nonzero command incorrectly suppressed the farewell"
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
    assert!(
        s.wait_for(b"glimps:", Duration::from_secs(1)),
        "Ctrl-D did not print the farewell"
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
    assert!(
        !contains(&s.snapshot(), b"glimps:"),
        "forced termination should not print a conversational farewell"
    );
}

#[test]
fn sigterm_kills_a_hup_ignoring_foreground_process() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);

    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let pid_path =
        std::env::temp_dir().join(format!("glimps-foreground-pid-{}-{n}", std::process::id()));
    let _ = std::fs::remove_file(&pid_path);

    let mut s = spawn("/bin/sh", None);
    thread::sleep(STARTUP);
    let command = format!(
        "sh -c 'trap \"\" HUP TERM; echo $$ > {}; exec sleep 30'\n",
        pid_path.display()
    );
    s.write(command.as_bytes());

    let pid_deadline = Instant::now() + Duration::from_secs(3);
    let foreground_pid = loop {
        if let Ok(text) = std::fs::read_to_string(&pid_path) {
            if let Ok(pid) = text.trim().parse::<libc::pid_t>() {
                break pid;
            }
        }
        assert!(
            Instant::now() < pid_deadline,
            "foreground command did not publish its pid; captured: {:?}",
            String::from_utf8_lossy(&s.snapshot())
        );
        thread::sleep(Duration::from_millis(25));
    };

    let glimps_pid =
        libc::pid_t::try_from(s.child.process_id().expect("child pid")).expect("pid fits pid_t");
    // SAFETY: valid process id owned by this test and a standard signal.
    assert_eq!(unsafe { libc::kill(glimps_pid, libc::SIGTERM) }, 0);
    assert!(
        s.wait_exit(EXIT_BUDGET),
        "glimps did not exit after SIGTERM"
    );

    let gone_deadline = Instant::now() + Duration::from_secs(3);
    while process_exists(foreground_pid) && Instant::now() < gone_deadline {
        thread::sleep(Duration::from_millis(25));
    }
    if process_exists(foreground_pid) {
        // SAFETY: best-effort cleanup of the test-owned process before failing.
        let _ = unsafe { libc::kill(foreground_pid, libc::SIGKILL) };
        panic!("foreground process {foreground_pid} survived GLIMPS shutdown");
    }
    let _ = std::fs::remove_file(pid_path);
}

fn process_exists(pid: libc::pid_t) -> bool {
    // SAFETY: signal 0 performs existence/permission checking only.
    let rc = unsafe { libc::kill(pid, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

/// OSC-133 prompt-start marker, emitted by the integration's `precmd` before each
/// prompt (including the first). Seeing it means the shell has sourced `.zshrc`
/// and the hooks are installed — a deterministic readiness signal.
const PROMPT_READY: &[u8] = b"\x1b]133;A\x07";
const READY_BUDGET: Duration = Duration::from_secs(3);

fn assert_prompt_ready(session: &Session) {
    assert!(
        session.wait_for(PROMPT_READY, READY_BUDGET),
        "shell integration did not reach its first prompt. Captured: {:?}",
        String::from_utf8_lossy(&session.snapshot())
    );
}

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
    assert_prompt_ready(&s);
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
fn slow_html_chunks_are_formatted_end_to_end() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);

    // The 150 ms pause is well beyond the supervisor's 40 ms interactive
    // liveness window and models a remote server pausing between HTML chunks.
    s.write(
        b"printf '%s' '<!DOCTYPE html><html><head>'; sleep 0.15; printf '%s\\n' '<title>Slow page</title></head><body>Ready</body></html>'\n",
    );
    assert!(
        s.wait_for(b"HTML", FORMAT_BUDGET),
        "slow HTML response fell back to raw pass-through: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    assert!(
        s.wait_for(b"Slow page", FORMAT_BUDGET),
        "formatted HTML content was lost"
    );

    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn dotenv_file_is_colored_end_to_end() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let env_path = zdot.path().join(".env.local");
    std::fs::write(&env_path, b"API_TOKEN=fixture-only\nPORT=3000\n")
        .expect("write dotenv fixture");
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);
    s.write(format!("cat -- {}\n", env_path.display()).as_bytes());
    assert!(
        s.wait_for(b"\x1b[36mAPI_TOKEN\x1b[0m", FORMAT_BUDGET),
        "dotenv key was not colored through the real supervisor: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    assert!(
        s.wait_for(b"\x1b[38;5;220m3000\x1b[0m", FORMAT_BUDGET),
        "dotenv numeric value was not colored through the real supervisor"
    );
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn gitignore_file_is_colored_end_to_end() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let ignore_path = zdot.path().join(".gitignore");
    std::fs::write(
        &ignore_path,
        b"# build output\n/target/\n*.log\n!important.log\n",
    )
    .expect("write gitignore fixture");
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);
    s.write(format!("cat -- {}\n", ignore_path.display()).as_bytes());
    assert!(
        s.wait_for(b"\x1b[2m# build output\x1b[0m", FORMAT_BUDGET),
        "gitignore comment was not colored through the real supervisor: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    assert!(
        s.wait_for(b"\x1b[35m!\x1b[0m", FORMAT_BUDGET),
        "gitignore negation marker was not colored through the real supervisor"
    );
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn htaccess_file_is_colored_end_to_end() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let htaccess_path = zdot.path().join(".htaccess");
    std::fs::write(
        &htaccess_path,
        b"# Force HTTPS\nRewriteEngine On\nRewriteCond %{HTTPS} off\nRewriteRule ^ https://%{HTTP_HOST}%{REQUEST_URI} [L,R=301]\n",
    )
    .expect("write htaccess fixture");
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);
    s.write(format!("cat -- {}\n", htaccess_path.display()).as_bytes());
    assert!(
        s.wait_for(b"\x1b[38;2;224;82;125mRewriteEngine\x1b[0m", FORMAT_BUDGET),
        "Apache directive was not colored through the real supervisor: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    assert!(
        s.wait_for(b"\x1b[36m%{HTTPS}\x1b[0m", FORMAT_BUDGET),
        "Apache variable was not colored through the real supervisor"
    );
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn custom_prompt_grep_tail_auth_logs_are_colored_end_to_end() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let log_path = zdot.path().join("auth.log");
    std::fs::write(
        &log_path,
        b"2026-08-07T14:59:12.573694+00:00 server sshd[493736]: Failed password for invalid user centos from 172.182.10.105 port 61497 ssh2\n",
    )
    .expect("write authentication log fixture");
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);

    s.write(b"PS1='$ '\n");
    assert!(
        s.wait_for(b"$ ", FORMAT_BUDGET),
        "custom prompt did not appear"
    );
    s.write(
        format!(
            "L={}; grep \"Failed password\" $L | tail -40\n",
            log_path.display()
        )
        .as_bytes(),
    );
    assert!(
        s.wait_for(b"\x1b[31mFailed password\x1b[0m", FORMAT_BUDGET),
        "authentication event stayed unformatted through the real pipeline: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    assert!(
        s.wait_for(b"\x1b[38;5;220m493736\x1b[0m", FORMAT_BUDGET),
        "sshd PID was not semantically colored through the real pipeline"
    );
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn private_metadata_path_is_not_inherited_by_commands() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);
    s.write(b"printf 'META_ENV=%s\\n' \"${GLIMPS_META_PATH-unset}\"\n");
    assert!(
        s.wait_for_plain(b"META_ENV=unset", FORMAT_BUDGET),
        "metadata capability leaked to child environment: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn failed_command_footer_decodes_exit_127_end_to_end() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);
    // A command that cannot exist: the shell reports exit 127, and the footer
    // must decode it. `on PATH` is GLIMPS's wording, distinct from the shell's
    // own `command not found:` message — seeing it proves the D;127 marker
    // reached the dictionary through the real supervisor.
    s.write(b"glimps-integration-no-such-command\n");
    assert!(
        s.wait_for(b"failed exit 127", FORMAT_BUDGET),
        "exit-127 footer missing. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    assert!(
        s.wait_for(b"command not found on PATH", FORMAT_BUDGET),
        "exit-127 decode missing. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn zsh_tab_completion_still_reaches_the_inner_shell() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);

    // `Cargo.to<Tab>` should complete to the repo's `Cargo.toml`. If GLIMPS
    // swallows Tab or the disposable zsh lacks completion setup, zsh runs
    // `cat Cargo.to` instead and this package header never appears.
    s.write(b"cat Cargo.to\t\n");
    assert!(
        s.wait_for(b"[package]", FORMAT_BUDGET),
        "zsh Tab completion did not complete Cargo.to -> Cargo.toml. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn no_newline_interactive_prompt_is_visible_before_reply() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);

    // Keep the expected prompt text non-contiguous in the typed command so the
    // assertion cannot accidentally match terminal echo or GLIMPS's header.
    s.write(b"printf 'INTERACTIVE_%s' 'QUESTION_VISIBLE'; read reply\n");
    assert!(
        s.wait_for(b"INTERACTIVE_QUESTION_VISIBLE", FORMAT_BUDGET),
        "no-newline prompt stayed hidden while the command waited for input: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    s.write(b"n\n");
    assert!(
        s.wait_for(b"done exit 0", FORMAT_BUDGET),
        "shell did not resume after answering the visible prompt"
    );
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn cat_redirect_hint_is_visible_before_input_and_never_enters_the_file() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);

    s.write(b"cat > \"$HOME/glimps-stdin-note.txt\"\n");
    assert!(
        s.wait_for(b"writing to ", FORMAT_BUDGET)
            && s.wait_for(b"$HOME/glimps-stdin-note.txt", FORMAT_BUDGET),
        "cat's stdin-writing state was not explained before input: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    assert!(
        s.wait_for(b"Enter starts a new line; Ctrl-D finishes", FORMAT_BUDGET),
        "cat's completion instruction was missing"
    );

    s.write(b"GLIMPS_CAT_STDIN_PAYLOAD\n\x04");
    assert!(
        s.wait_for(b"done exit 0", FORMAT_BUDGET),
        "cat did not finish after Ctrl-D: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    s.write(b"cat \"$HOME/glimps-stdin-note.txt\"\n");
    assert!(
        s.wait_for(b"GLIMPS_CAT_STDIN_PAYLOAD", FORMAT_BUDGET),
        "cat input was not written to the destination file"
    );
    let saved = std::fs::read_to_string(zdot.path().join("glimps-stdin-note.txt"))
        .expect("read cat destination");
    assert_eq!(saved, "GLIMPS_CAT_STDIN_PAYLOAD\n");
    assert!(!saved.contains("writing to"));
    assert!(!saved.contains("Ctrl-D"));

    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn more_formats_file_lines_without_hiding_the_live_pager_prompt() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let Some(more) = more_path() else {
        eprintln!("skipping: more not available");
        return;
    };
    let zdot = ZdotDir::new();
    let fixture = (0..80)
        .map(|line| format!("# GLIMPS_MORE_LINE_{line:03}\n"))
        .collect::<String>();
    std::fs::write(zdot.path().join("more-fixture.md"), fixture).expect("write more fixture");
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);

    s.write(format!("{more} \"$HOME/more-fixture.md\"\n").as_bytes());
    assert!(
        s.wait_for(b"# GLIMPS_MORE_LINE_000\x1b[0m", FORMAT_BUDGET),
        "more's completed Markdown lines were not formatted: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    assert!(
        s.wait_for_any(&[b"--More--", b"\x1b[7m"], FORMAT_BUDGET),
        "more's no-newline pager prompt stayed hidden: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );

    s.write(b"q");
    thread::sleep(Duration::from_millis(100));
    s.write(b"printf 'MORE_%s\\n' 'SHELL_RESUMED'\n");
    assert!(
        s.wait_for(b"MORE_SHELL_RESUMED", FORMAT_BUDGET),
        "shell did not resume after quitting more: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn zsh_bulk_delete_confirmation_is_visible_before_reply() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    for index in 0..6 {
        std::fs::write(zdot.path().join(format!("rm-fixture-{index}")), b"safe\n")
            .expect("write rm confirmation fixture");
    }
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);
    s.write(format!("cd -- {}\n", zdot.path().display()).as_bytes());
    assert!(
        s.wait_for(b"moved to", FORMAT_BUDGET),
        "fixture cd did not complete"
    );

    s.write(b"unsetopt RM_STAR_SILENT RM_STAR_WAIT; rm -rf ./*\n");
    assert!(
        s.wait_for(b"sure you want to delete all", FORMAT_BUDGET),
        "zsh's bulk-delete question stayed hidden before any reply: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    s.write(b"n\n");
    // Queue a harmless verification immediately; the inner shell will execute
    // it only after the confirmation has accepted the reply and returned.
    s.write(b"test -f rm-fixture-0 && printf 'RM_%s\\n' 'DECLINED_FILES_SAFE'\n");
    assert!(
        s.wait_for(b"RM_DECLINED_FILES_SAFE", FORMAT_BUDGET),
        "bulk-delete fixture was unexpectedly removed or the shell did not resume: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn failed_command_footer_pins_the_error_line_end_to_end() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);
    // An ERROR-severity line buried by later context: the footer must quote the
    // line under the status (the `↳ ` prefix is GLIMPS's) so it is easy to find.
    s.write(
        b"printf 'ERROR connection reset by peer\\ncontext one\\ncontext two\\ncontext three\\n'; false\n",
    );
    // Colored mode interleaves ANSI between the `↳ ` marker and the quoted
    // text, so assert the two pieces separately: the pin marker, and the
    // quote in the muted pin color (the original output line above it is
    // painted log-error red instead, so this needle is unique to the pin).
    assert!(
        s.wait_for("\u{21b3} ".as_bytes(), FORMAT_BUDGET),
        "pin marker missing. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    assert!(
        s.wait_for(
            b"\x1b[38;5;153mERROR connection reset by peer",
            FORMAT_BUDGET
        ),
        "pinned quote missing/miscolored. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn successful_negated_command_is_not_reported_as_failed_end_to_end() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);
    let baseline = s.snapshot().len();

    s.write(b"! printf 'key installed\\n'\n");
    assert!(
        s.wait_for(
            b"underlying command succeeded; ! inverted its status",
            FORMAT_BUDGET
        ),
        "negated-success notice missing. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    let command_output = &s.snapshot()[baseline..];
    assert!(
        !command_output
            .windows(b"command failed:".len())
            .any(|window| window == b"command failed:"),
        "negated success was recapped as a failure: {:?}",
        String::from_utf8_lossy(command_output)
    );

    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

#[test]
fn pipeline_stage_failure_warns_even_when_shell_exit_is_zero() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);

    // Without pipefail, zsh reports this pipeline's final exit as 0 because
    // `cat` succeeds. GLIMPS should still observe `$pipestatus` and call out the
    // failed first stage without changing shell semantics.
    s.write(b"sh -c 'printf \"ERROR pipe failed\\n\"; exit 7' | cat\n");
    assert!(
        s.wait_for(b"pipeline stage failed", FORMAT_BUDGET),
        "pipeline warning missing. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    assert!(
        s.wait_for(b"stage 1 exit 7; final exit 0", FORMAT_BUDGET),
        "pipeline stage detail missing. Captured: {:?}",
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
    assert_prompt_ready(&s);
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
fn bash_pipeline_stage_failure_warns_even_when_shell_exit_is_zero() {
    let Some(bash) = bash_path() else {
        eprintln!("skipping: bash not available");
        return;
    };
    let home = BashHome::new();
    let mut s = spawn(&bash, Some(home.path()));
    assert_prompt_ready(&s);

    s.write(b"sh -c 'printf \"ERROR bash pipe failed\\n\"; exit 7' | cat\n");
    assert!(
        s.wait_for(b"pipeline stage failed", FORMAT_BUDGET),
        "bash pipeline warning missing. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    assert!(
        s.wait_for(b"stage 1 exit 7; final exit 0", FORMAT_BUDGET),
        "bash pipeline stage detail missing. Captured: {:?}",
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
    assert_prompt_ready(&s);
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

#[test]
fn off_and_on_toggle_formatting_end_to_end() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);

    // Baseline: formatting is on.
    // Pretty-printed keys render as `\x1b[36m"key"\x1b[0m: ` — the colored key
    // followed by the colon+space GLIMPS regenerates. The compact typed echo
    // (`{"alpha":1}`) can never contain that byte sequence.
    s.write(b"echo '{\"alpha\":1}'\n");
    assert!(
        s.wait_for(b"\"alpha\"\x1b[0m: ", FORMAT_BUDGET),
        "baseline JSON was not pretty-printed. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );

    // Pause via the real subcommand (GLIMPS_ACTIVE is set inside the session).
    s.write(format!("{GLIMPS} off\n").as_bytes());
    assert!(
        s.wait_for(b"formatting paused", FORMAT_BUDGET),
        "`glimps off` did not confirm. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );

    // While paused, JSON must pass through verbatim. The typed command echo
    // contains the compact form either way, so the proof of pass-through is the
    // *absence* of the colored pretty-printed key.
    s.write(b"echo '{\"beta\":2}'\n");
    s.write(b"echo done-beta\n");
    assert!(
        s.wait_for(b"done-beta", FORMAT_BUDGET),
        "sentinel after paused command did not appear. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    assert!(
        !contains(&s.snapshot(), b"\"beta\"\x1b[0m: "),
        "output was formatted while paused. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );

    // Resume and prove formatting is back.
    s.write(format!("{GLIMPS} on\n").as_bytes());
    assert!(
        s.wait_for(b"formatting resumed", FORMAT_BUDGET),
        "`glimps on` did not confirm. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    s.write(b"echo '{\"gamma\":3}'\n");
    assert!(
        s.wait_for(b"\"gamma\"\x1b[0m: ", FORMAT_BUDGET),
        "JSON was not formatted after resume. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );

    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

/// A command line typed with literal control bytes (Ctrl-V quoted insert, or a
/// paste) must never re-emerge as live escape sequences on the user's real
/// terminal. The line editor itself only ever *displays* such bytes as `^G` /
/// `^[`; the integration's preexec re-sends the command inside private OSC
/// 7337, so a raw BEL there would end the OSC early and the ESC sequence that
/// follows (here an OSC 52 clipboard write) would reach the terminal verbatim.
fn assert_command_control_bytes_are_neutralized(shell: &str, home: &Path) {
    let mut s = spawn(shell, Some(home));
    assert_prompt_ready(&s);
    // `:` is a no-op and the payload is one quoted word, so only the command
    // marker can leak it. Each control byte is preceded by Ctrl-V (0x16) so
    // both zle and readline insert it literally instead of acting on it.
    s.write(b": 'ctl-probe \x16\x07\x16\x1b]52;c;cHduZWQ=\x16\x07 tail'\n");
    // The arithmetic makes the sentinel appear only as real output, never in
    // the echoed command line.
    s.write(b"echo after-ctl-$((6*7))\n");
    assert!(
        s.wait_for(b"after-ctl-42", FORMAT_BUDGET),
        "sentinel after the control-byte command did not appear. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    let out = s.snapshot();
    assert!(
        !contains(&out, b"\x1b]52;"),
        "an escape sequence from the typed command reached the terminal. Captured: {:?}",
        String::from_utf8_lossy(&out)
    );
    // The marker still carries the command, with each control byte replaced
    // by `?`, so the header and bypass-by-name keep working.
    assert!(
        contains(&out, b"\x1b]7337;: 'ctl-probe ??]52;c;cHduZWQ=? tail'\x07"),
        "the command marker was not sent with its controls neutralized. Captured: {:?}",
        String::from_utf8_lossy(&out)
    );
    s.write(b"exit\n");
    assert!(
        s.wait_exit(EXIT_BUDGET),
        "shell did not exit after the probe. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
}

#[test]
fn zsh_command_control_bytes_never_reach_the_terminal() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    assert_command_control_bytes_are_neutralized(&zsh, zdot.path());
}

#[test]
fn bash_command_control_bytes_never_reach_the_terminal() {
    let Some(bash) = bash_path() else {
        eprintln!("skipping: bash not available");
        return;
    };
    let home = BashHome::new();
    assert_command_control_bytes_are_neutralized(&bash, home.path());
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

/// Wait until the shell has reported `n` command ends in total.
fn wait_for_command_ends(s: &Session, n: usize) -> bool {
    let end = Instant::now() + FORMAT_BUDGET;
    while Instant::now() < end {
        if count(&s.snapshot(), b"\x1b]133;D") >= n {
            return true;
        }
        thread::sleep(Duration::from_millis(25));
    }
    false
}

#[test]
fn long_command_sends_a_completion_notice_end_to_end() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);

    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let config_path =
        std::env::temp_dir().join(format!("glimps-notify-{}-{n}.toml", std::process::id()));
    std::fs::write(
        &config_path,
        b"[notify]\nenabled = true\nafter_seconds = 1\nmethod = \"osc9\"\n",
    )
    .expect("write test config");
    let zdot = ZdotDir::new();
    let mut s = spawn_with_config(&zsh, Some(zdot.path()), Some(&config_path));
    assert_prompt_ready(&s);
    let notice = "\x1b]9;\u{2713} sleep finished in 1.".as_bytes();
    let any_notice = b"\x1b]9;";

    // Past the threshold, untouched while it ran: one notice naming the
    // program only, terminated, after the footer.
    s.write(b"sleep 1.4 && print slow-$((6*7))\n");
    assert!(
        s.wait_for(notice, FORMAT_BUDGET),
        "notice missing. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    let snapshot = s.snapshot();
    let at = snapshot
        .windows(notice.len())
        .position(|w| w == notice)
        .expect("notice present");
    let tail = &snapshot[at..];
    let body_end = tail
        .windows(2)
        .position(|w| w == b"\x1b\\")
        .expect("notice is terminated with ST");
    assert!(
        !contains(&tail[..body_end], b"slow") && !contains(&tail[..body_end], b"1.4 "),
        "notice repeated arguments: {:?}",
        String::from_utf8_lossy(&tail[..body_end])
    );
    assert!(contains(&snapshot[..at], b"slow-42"));
    assert!(contains(&snapshot[..at], b"done exit 0 in 1."));
    let ends = count(&snapshot, b"\x1b]133;D");

    // Below the threshold: nothing.
    s.write(b"sleep 0.1; print quick-$((6*7))\n");
    assert!(s.wait_for(b"quick-42", FORMAT_BUDGET));
    assert!(wait_for_command_ends(&s, ends + 1));
    assert_eq!(count(&s.snapshot(), any_notice), 1);

    // Past the threshold, but the user answered a prompt at the end: they
    // were there to see it finish (the stdin forwarder's activity record).
    s.write(b"sleep 1.3; read -r answer; print got-$answer\n");
    thread::sleep(Duration::from_millis(1600));
    s.write(b"yes\n");
    assert!(s.wait_for(b"got-yes", FORMAT_BUDGET));
    assert!(wait_for_command_ends(&s, ends + 2));
    thread::sleep(Duration::from_millis(150));
    assert_eq!(
        count(&s.snapshot(), any_notice),
        1,
        "notified for a command the user typed into: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );

    // A command line led by a bypassed program never notifies, however long
    // it runs (`less -V` prints its version and exits, so nothing waits).
    s.write(b"less -V >/dev/null 2>&1; sleep 1.2; true\n");
    assert!(wait_for_command_ends(&s, ends + 3));
    thread::sleep(Duration::from_millis(150));
    assert_eq!(count(&s.snapshot(), any_notice), 1);

    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
    let _ = std::fs::remove_file(config_path);
}

#[test]
fn links_are_added_end_to_end_only_when_enabled() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);

    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let line = b"print -r -- 'at src/lib.rs:19:5 see https://example.com/x.'\n";
    let link_open = b"\x1b]8;;";

    // The harness scrubs the terminal identity, so `auto` (the default) sees
    // an unknown terminal: the output is exactly what it was before links.
    let zdot = ZdotDir::new();
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);
    s.write(line);
    assert!(s.wait_for(b"https://example.com/x.\r\n", FORMAT_BUDGET));
    assert!(!contains(&s.snapshot(), link_open));
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);

    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let config_path =
        std::env::temp_dir().join(format!("glimps-links-{}-{n}.toml", std::process::id()));
    std::fs::write(
        &config_path,
        b"[links]\nmode = \"on\"\neditor = \"vscode\"\n",
    )
    .expect("write test config");
    let zdot = ZdotDir::new();
    let mut s = spawn_with_config(&zsh, Some(zdot.path()), Some(&config_path));
    assert_prompt_ready(&s);
    s.write(line);
    let url = b"\x1b]8;;https://example.com/x\x1b\\https://example.com/x\x1b]8;;\x1b\\.";
    assert!(
        s.wait_for(url, FORMAT_BUDGET),
        "URL link missing. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    // The relative path resolves against the directory the shell runs in
    // (reported over the private metadata channel; the test shell starts in
    // its ZDOTDIR home), with line and column for the editor.
    let cwd = std::fs::canonicalize(zdot.path()).expect("canonical cwd");
    let file = format!(
        "\x1b]8;;vscode://file{}/src/lib.rs:19:5\x1b\\src/lib.rs:19:5\x1b]8;;\x1b\\",
        cwd.display()
    );
    let snapshot = s.snapshot();
    assert!(
        contains(&snapshot, file.as_bytes()),
        "file link missing: want {file:?} in {:?}",
        String::from_utf8_lossy(&snapshot)
    );
    // The echo of what was typed is never linked: only output is.
    let typed = snapshot
        .windows(b"print -r --".len())
        .position(|w| w == b"print -r --")
        .expect("echo present");
    let output_starts = snapshot
        .windows(b"\x1b]133;C".len())
        .position(|w| w == b"\x1b]133;C")
        .expect("output marker");
    assert!(!contains(&snapshot[typed..output_starts], link_open));
    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
    let _ = std::fs::remove_file(config_path);
}

/// A fish home with the integration installed where `glimps setup` puts it:
/// `$XDG_CONFIG_HOME/fish/conf.d/00-glimps.fish`. Removed on drop.
struct FishHome {
    path: PathBuf,
}

impl FishHome {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("glimps-fish-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let conf_d = path.join("config").join("fish").join("conf.d");
        std::fs::create_dir_all(&conf_d).expect("create conf.d");
        std::fs::write(
            conf_d.join("00-glimps.fish"),
            format!("'{GLIMPS}' init fish | source\n"),
        )
        .expect("write conf.d snippet");
        FishHome { path }
    }

    fn spawn(&self, fish: &str) -> Session {
        let config = self.path.join("config");
        let data = self.path.join("data");
        spawn_with_env(
            fish,
            Some(&self.path),
            None,
            &[
                ("XDG_CONFIG_HOME", config.as_os_str()),
                // fish keeps history here; keep it out of the real one.
                ("XDG_DATA_HOME", data.as_os_str()),
            ],
        )
    }
}

impl Drop for FishHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn fish_session_is_formatted_end_to_end() {
    let Some(fish) = fish_path() else {
        eprintln!("skipping: fish not available");
        return;
    };
    let home = FishHome::new();
    let mut s = home.spawn(&fish);
    assert_prompt_ready(&s);

    // Output is framed by GLIMPS's markers (fish 4 adds its own; they must
    // not frame anything) and formatted.
    s.write(b"echo '{\"alpha\":1}'\n");
    assert!(
        s.wait_for(b"\"alpha\"\x1b[0m: ", FORMAT_BUDGET),
        "JSON was not formatted under fish. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );

    // A failing pipeline stage and the exit status come from the result
    // record written by fish_postexec.
    s.write(b"false | true\n");
    assert!(
        s.wait_for(
            b"pipeline stage failed: stage 1 exit 1; final exit 0",
            FORMAT_BUDGET
        ),
        "pipeline warning missing under fish. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    s.write(b"sh -c 'exit 3'\n");
    assert!(
        s.wait_for(b"failed exit 3", FORMAT_BUDGET),
        "failure footer missing under fish. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );

    // A silent command draws nothing: fish's title `\r` is not output.
    let before = s.snapshot().len();
    s.write(b"true\n");
    s.write(b"echo after-$(math 6 \\* 7)\n");
    assert!(s.wait_for(b"after-42", FORMAT_BUDGET));
    let tail = &s.snapshot()[before..];
    let headers = tail
        .windows("\u{258c}".len())
        .filter(|w| *w == "\u{258c}".as_bytes())
        .count();
    assert_eq!(
        headers,
        1,
        "only the echo gets a header: {:?}",
        String::from_utf8_lossy(tail)
    );

    // `cd` leaves its breadcrumb (the cwd marker from fish_postexec).
    s.write(b"cd /\n");
    assert!(
        s.wait_for_plain(b"moved to /", FORMAT_BUDGET),
        "cd breadcrumb missing under fish. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );

    // `glimps off`/`on` keep their provenance under fish.
    s.write(format!("{GLIMPS} off\n").as_bytes());
    assert!(s.wait_for(b"formatting paused", FORMAT_BUDGET));
    s.write(b"echo '{\"beta\":2}'; echo done-beta\n");
    assert!(s.wait_for(b"done-beta\r\n", FORMAT_BUDGET));
    assert!(!contains(&s.snapshot(), b"\"beta\"\x1b[0m: "));
    s.write(format!("{GLIMPS} on\n").as_bytes());
    assert!(s.wait_for(b"formatting resumed", FORMAT_BUDGET));

    // The metadata capability is not inherited by commands.
    s.write(b"env | string match -q 'GLIMPS_META_PATH=*'; and echo leaked; or echo clean-$(math 6 \\* 7)\n");
    assert!(
        s.wait_for(b"clean-42", FORMAT_BUDGET),
        "GLIMPS_META_PATH leaked to fish's children. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );

    s.write(b"exit\n");
    assert!(
        s.wait_exit(EXIT_BUDGET),
        "glimps did not exit after fish exited"
    );
}

/// Run `program args` (not glimps) in a PTY with a scrubbed environment.
fn spawn_program(program: &str, args: &[&str], env: &[(&str, &std::ffi::OsStr)]) -> Session {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    let mut cmd = CommandBuilder::new(program);
    cmd.args(args);
    cmd.env_clear();
    cmd.env("TERM", "xterm-256color");
    for (key, value) in env {
        cmd.env(key, value);
    }
    let child = pair.slave.spawn_command(cmd).expect("spawn program");
    drop(pair.slave);
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

/// The outer shell's rc hands the shell itself to `glimps --shell <this
/// shell>`, not to whatever `$SHELL` names: a fish or zsh started from a
/// different login shell must stay that shell inside GLIMPS. A stand-in
/// `glimps` on PATH records its arguments; the real one prints the snippet.
#[test]
fn outer_shells_hand_glimps_their_own_path() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("glimps-outer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).expect("bin dir");
    let fake = bin.join("glimps");
    std::fs::write(&fake, "#!/bin/sh\nprintf 'FAKE-GLIMPS[%s]\\n' \"$*\"\n").expect("fake glimps");
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let snippet = |shell: &str| {
        let out = std::process::Command::new(GLIMPS)
            .args(["init", shell])
            .output()
            .expect("glimps init");
        String::from_utf8(out.stdout).expect("utf8 snippet")
    };

    let mut checked = 0;
    // zsh, with $SHELL naming another shell.
    if let Some(zsh) = zsh_path() {
        let home = dir.join("zsh");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(".zshenv"), "unsetopt GLOBAL_RCS\n").unwrap();
        std::fs::write(home.join(".zshrc"), snippet("zsh")).unwrap();
        let s = spawn_program(
            &zsh,
            &["-i"],
            &[
                ("HOME", home.as_os_str()),
                ("ZDOTDIR", home.as_os_str()),
                ("PATH", path.as_ref()),
                ("SHELL", "/bin/sh".as_ref()),
            ],
        );
        assert!(
            s.wait_for(b"FAKE-GLIMPS[--shell ", FORMAT_BUDGET),
            "zsh did not hand itself over. Captured: {:?}",
            String::from_utf8_lossy(&s.snapshot())
        );
        let text = String::from_utf8_lossy(&s.snapshot()).into_owned();
        assert!(
            text.contains("FAKE-GLIMPS[--shell zsh]") || text.contains(&format!("--shell {zsh}]")),
            "{text:?}"
        );
        checked += 1;
    }
    // bash: its own binary, whatever $SHELL says.
    if let Some(bash) = bash_path() {
        let home = dir.join("bash");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(".bashrc"), snippet("bash")).unwrap();
        let s = spawn_program(
            &bash,
            &["--rcfile", home.join(".bashrc").to_str().unwrap(), "-i"],
            &[
                ("HOME", home.as_os_str()),
                ("PATH", path.as_ref()),
                ("SHELL", "/bin/zsh".as_ref()),
            ],
        );
        let want = format!("FAKE-GLIMPS[--shell {bash}]");
        assert!(
            s.wait_for(want.as_bytes(), FORMAT_BUDGET),
            "bash did not hand itself over. Captured: {:?}",
            String::from_utf8_lossy(&s.snapshot())
        );
        checked += 1;
    }
    // fish: `status fish-path`, from a zsh login.
    if let Some(fish) = fish_path() {
        let home = dir.join("fish");
        let conf_d = home.join("config").join("fish").join("conf.d");
        std::fs::create_dir_all(&conf_d).unwrap();
        std::fs::write(conf_d.join("00-glimps.fish"), snippet("fish")).unwrap();
        let s = spawn_program(
            &fish,
            &["-i"],
            &[
                ("HOME", home.as_os_str()),
                ("XDG_CONFIG_HOME", home.join("config").as_os_str()),
                ("PATH", path.as_ref()),
                ("SHELL", "/bin/zsh".as_ref()),
                ("fish_history", "".as_ref()),
            ],
        );
        let real = std::fs::canonicalize(&fish).unwrap();
        let want = format!("FAKE-GLIMPS[--shell {}]", real.display());
        assert!(
            s.wait_for(want.as_bytes(), FORMAT_BUDGET),
            "fish did not hand itself over (want {want:?}). Captured: {:?}",
            String::from_utf8_lossy(&s.snapshot())
        );
        checked += 1;
    }
    let _ = std::fs::remove_dir_all(&dir);
    if checked == 0 {
        eprintln!("skipping: no zsh, bash or fish available");
    }
}

#[test]
fn redact_toggle_masks_secrets_end_to_end() {
    let Some(zsh) = zsh_path() else {
        eprintln!("skipping: zsh not available");
        return;
    };
    let zdot = ZdotDir::new();
    let mut s = spawn(&zsh, Some(zdot.path()));
    assert_prompt_ready(&s);
    // Built from parts so the typed command line never holds the token whole.
    let print = b"print -r -- \"token ghp_${(l:36::a:)}\"\n";
    let token = format!("ghp_{}", "a".repeat(36));
    let masked = format!("token ghp_{}", "*".repeat(36));

    s.write(print);
    assert!(s.wait_for(format!("token {token}\r\n").as_bytes(), FORMAT_BUDGET));

    s.write(format!("{GLIMPS} redact on\n").as_bytes());
    // GLIMPS confirms it itself, after the command's own "asked" line.
    assert!(s.wait_for(
        b"glimps: asked this session to mask secrets.",
        FORMAT_BUDGET
    ));
    assert!(s.wait_for(b"masking secrets in this session's output", FORMAT_BUDGET));
    let before = s.snapshot().len();
    s.write(print);
    assert!(
        s.wait_for(masked.as_bytes(), FORMAT_BUDGET),
        "token was not masked. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot()[before..])
    );
    assert!(!contains(&s.snapshot()[before..], token.as_bytes()));

    s.write(format!("{GLIMPS} redact off\n").as_bytes());
    assert!(s.wait_for(b"no longer masking secrets", FORMAT_BUDGET));
    let before = s.snapshot().len();
    s.write(print);
    assert!(
        s.wait_for(format!("token {token}\r\n").as_bytes(), FORMAT_BUDGET),
        "masking did not stop. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot()[before..])
    );

    s.write(b"exit\n");
    let _ = s.wait_exit(EXIT_BUDGET);
}

/// A shell that cannot start inside GLIMPS must not take the terminal with
/// it: an rc file `exec`s glimps, so exiting would close the tab. GLIMPS hands
/// over to a plain shell (`GLIMPS=0`) instead.
#[test]
fn an_unstartable_shell_falls_back_to_a_plain_one() {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    let mut cmd = CommandBuilder::new(GLIMPS);
    cmd.args(["--shell", "/nonexistent/glimps-test-zsh"]);
    cmd.env_remove("GLIMPS_ACTIVE");
    cmd.env_remove("GLIMPSRC");
    cmd.env("GLIMPS", "1");
    cmd.env("TERM", "xterm-256color");
    cmd.env("SHELL", "/bin/sh");
    let child = pair.slave.spawn_command(cmd).expect("spawn glimps");
    drop(pair.slave);
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
    let mut s = Session {
        child,
        writer,
        output,
        _master: pair.master,
    };
    assert!(
        s.wait_for(b"starting /bin/sh without GLIMPS", FORMAT_BUDGET),
        "no fallback. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    s.write(b"echo fallback-$((6*7)) GLIMPS=$GLIMPS\n");
    assert!(
        s.wait_for(b"fallback-42 GLIMPS=0", FORMAT_BUDGET),
        "the plain shell did not run. Captured: {:?}",
        String::from_utf8_lossy(&s.snapshot())
    );
    s.write(b"exit\n");
    assert!(s.wait_exit(EXIT_BUDGET));
}
