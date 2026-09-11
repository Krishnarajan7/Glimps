//! PTY byte-fidelity probe — the Windows/ConPTY spike tool. Not part of the
//! `glimps` binary and excluded from the published crate.
//!
//! GLIMPS assumes the PTY master hands it the child's bytes verbatim (true of
//! Unix PTYs). ConPTY instead keeps a virtual screen and re-synthesizes VT
//! output, which may inject cursor/attribute sequences, hard-wrap long lines at
//! the console width, or drop the OSC-133 markers the shell integration emits.
//! This probe runs a command inside the same `portable-pty` layer GLIMPS uses
//! and prints every byte read back, so a human can answer those questions by
//! inspection. Nothing is formatted, logged, or kept.
//!
//! ```text
//! cargo run --example pty_probe -- --cols 40 pwsh -NoLogo -Command "'{\"a\":1}'"
//! cargo run --example pty_probe -- --send "echo hi\r" --send "exit\r" pwsh -NoLogo
//! cargo run --example pty_probe -- --send "printf '%s\n' '{\"a\":1}'\r" --send "exit\r" zsh -f
//! ```
//!
//! Options (before the command): `--cols N` / `--rows N` set the PTY size;
//! `--send TEXT` (repeatable) types TEXT into the PTY after a short delay, with
//! `\r`, `\n`, `\t`, `\e` and `\xNN` escapes expanded.

use anyhow::{bail, Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Stop reading once the PTY has been silent this long (or after EOF).
const IDLE_TIMEOUT: Duration = Duration::from_millis(1500);
/// Overall cap so a stuck interactive child never hangs the probe.
const HARD_TIMEOUT: Duration = Duration::from_secs(20);
/// Gap between `--send` payloads so each one lands after the previous output.
const SEND_GAP: Duration = Duration::from_millis(600);

struct Options {
    cols: u16,
    rows: u16,
    sends: Vec<Vec<u8>>,
    command: Vec<String>,
}

fn parse_args() -> Result<Options> {
    let mut options = Options {
        cols: 80,
        rows: 24,
        sends: Vec::new(),
        command: Vec::new(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--cols" => {
                options.cols = args
                    .next()
                    .context("--cols needs a number")?
                    .parse()
                    .context("--cols must be a number")?;
            }
            "--rows" => {
                options.rows = args
                    .next()
                    .context("--rows needs a number")?
                    .parse()
                    .context("--rows must be a number")?;
            }
            "--send" => {
                let text = args.next().context("--send needs text")?;
                options.sends.push(unescape(&text));
            }
            _ => {
                options.command.push(arg);
                options.command.extend(args.by_ref());
            }
        }
    }
    if options.command.is_empty() {
        bail!("usage: pty_probe [--cols N] [--rows N] [--send TEXT]... <command> [args...]");
    }
    Ok(options)
}

/// Expand `\r`, `\n`, `\t`, `\e`, `\\` and `\xNN` in a `--send` payload.
fn unescape(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            let mut buf = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        match chars.next() {
            Some('r') => out.push(b'\r'),
            Some('n') => out.push(b'\n'),
            Some('t') => out.push(b'\t'),
            Some('e') => out.push(0x1b),
            Some('\\') => out.push(b'\\'),
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                match u8::from_str_radix(&hex, 16) {
                    Ok(byte) => out.push(byte),
                    Err(_) => {
                        out.extend_from_slice(b"\\x");
                        out.extend_from_slice(hex.as_bytes());
                    }
                }
            }
            Some(other) => {
                out.push(b'\\');
                let mut buf = [0u8; 4];
                out.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
            }
            None => out.push(b'\\'),
        }
    }
    out
}

fn main() -> Result<()> {
    let options = parse_args()?;
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: options.rows,
            cols: options.cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("failed to open pty")?;

    let mut cmd = CommandBuilder::new(&options.command[0]);
    cmd.args(&options.command[1..]);
    if let Ok(cwd) = std::env::current_dir() {
        cmd.cwd(cwd);
    }
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .context("failed to spawn command")?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut writer = pair.master.take_writer()?;
    let sends = options.sends;
    thread::spawn(move || {
        for payload in sends {
            thread::sleep(SEND_GAP);
            if writer.write_all(&payload).is_err() || writer.flush().is_err() {
                break;
            }
        }
    });

    let start = Instant::now();
    let mut chunks: Vec<Vec<u8>> = Vec::new();
    loop {
        match rx.recv_timeout(IDLE_TIMEOUT) {
            Ok(chunk) => chunks.push(chunk),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if child.try_wait()?.is_some() {
                    break;
                }
            }
        }
        if start.elapsed() > HARD_TIMEOUT {
            let _ = child.kill();
            break;
        }
    }
    let _ = child.kill();
    drop(pair.master);

    report(&chunks, options.cols);
    Ok(())
}

fn report(chunks: &[Vec<u8>], cols: u16) {
    let all: Vec<u8> = chunks.concat();
    println!(
        "=== {} chunk(s), {} byte(s) total ===",
        chunks.len(),
        all.len()
    );
    for (index, chunk) in chunks.iter().enumerate() {
        println!("--- chunk {} ({} bytes) ---", index + 1, chunk.len());
        println!("{}", escaped(chunk));
    }
    println!("=== summary ===");
    let esc_count = all.iter().filter(|b| **b == 0x1b).count();
    println!("ESC bytes:              {esc_count}");
    println!(
        "OSC 133 markers:        {}",
        count_occurrences(&all, b"\x1b]133;")
    );
    println!(
        "OSC 7337/7338/7339:     {}",
        count_occurrences(&all, b"\x1b]7337;")
            + count_occurrences(&all, b"\x1b]7338;")
            + count_occurrences(&all, b"\x1b]7339;")
    );
    println!(
        "cursor moves (CSI H/C/D): {}",
        count_csi_final(&all, b"HCDAB")
    );
    let longest = longest_visible_line(&all);
    println!("longest visible line:   {longest} cells (pty is {cols} cols)");
    println!(
        "verdict hints: a clean pass-through shows 0 cursor moves, markers intact, and a\n\
         line longer than the PTY width when the child printed one."
    );
}

fn count_occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || haystack.len() < needle.len() {
        return 0;
    }
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}

/// Count CSI sequences whose final byte is one of `finals` (e.g. cursor moves).
fn count_csi_final(bytes: &[u8], finals: &[u8]) -> usize {
    let mut count = 0;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == 0x1b && bytes[i + 1] == b'[' {
            let mut j = i + 2;
            while j < bytes.len() && (0x30..=0x3f).contains(&bytes[j]) {
                j += 1;
            }
            if j < bytes.len() && finals.contains(&bytes[j]) {
                count += 1;
            }
            i = j;
        }
        i += 1;
    }
    count
}

/// Longest run of printable bytes between line breaks, ignoring escape
/// sequences. A rough cell count (multibyte characters count per byte).
fn longest_visible_line(bytes: &[u8]) -> usize {
    let mut longest = 0;
    let mut current = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            0x1b => {
                i = skip_escape(bytes, i);
                continue;
            }
            b'\n' | b'\r' => {
                longest = longest.max(current);
                current = 0;
            }
            b if b >= 0x20 => current += 1,
            _ => {}
        }
        i += 1;
    }
    longest.max(current)
}

/// Index just past the escape sequence starting at `start` (CSI, OSC, or a
/// single-byte escape).
fn skip_escape(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    match bytes.get(i) {
        Some(b'[') => {
            i += 1;
            while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                i += 1;
            }
            i + 1
        }
        Some(b']') => {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == 0x07 {
                    return i + 1;
                }
                if bytes[i] == 0x1b && bytes.get(i + 1) == Some(&b'\\') {
                    return i + 2;
                }
                i += 1;
            }
            i
        }
        Some(_) => i + 1,
        None => i,
    }
}

/// Human-readable rendering: printable ASCII as-is, `ESC` as `\e`, CR/LF as
/// `\r`/`\n` (with a real newline after `\n` for readability), other bytes as
/// `\xNN`.
fn escaped(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        match b {
            0x1b => out.push_str("\\e"),
            b'\r' => out.push_str("\\r"),
            b'\n' => out.push_str("\\n\n"),
            0x07 => out.push_str("\\a"),
            0x20..=0x7e => out.push(b as char),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out
}
