//! GLIMPS — zero-config smart terminal output formatter.
//!
//! `glimps` with no arguments wraps your shell inside the PTY supervisor (the
//! heart of the tool). `glimps init zsh` prints the shell integration you add to
//! `~/.zshrc`. All output transformation lives behind `format::Formatter`.

use anyhow::Result;
use glimps::config::Config;
use glimps::format::Clock;
use glimps::{init, pty};
use std::io::{IsTerminal, Write};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("init") => {
            // `glimps init <shell>` prints integration to stdout for `eval`.
            return init::print_init(args.get(1).map(String::as_str));
        }
        Some("-h") | Some("--help") => {
            print_help();
            return Ok(());
        }
        Some("-V") | Some("--version") => {
            println!("glimps {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some(other) => {
            eprintln!("glimps: unknown argument '{other}'. Try `glimps --help`.");
            std::process::exit(2);
        }
        None => {}
    }

    // Default action: wrap the shell. Guard against re-exec loops — if we're
    // already inside a GLIMPS PTY, launching another would nest forever.
    if std::env::var_os("GLIMPS_ACTIVE").is_some() {
        eprintln!("glimps: already running inside a GLIMPS session; not nesting.");
        return Ok(());
    }

    // Load ~/.glimpsrc (or $GLIMPSRC); missing/broken -> defaults.
    let config = Config::load();

    // Capture the local UTC offset for separator timestamps NOW, while we are
    // still single-threaded — reading it after `run_shell` spawns threads would
    // be unsound (and `time` would refuse). Disabled if the config turns
    // timestamps off, or on offset-read failure.
    let clock = if config.timestamp {
        match time::UtcOffset::current_local_offset() {
            Ok(offset) => Clock::Local(offset),
            Err(_) => Clock::Off,
        }
    } else {
        Clock::Off
    };

    // The shell to wrap. Defaults to the user's $SHELL, falling back to zsh.
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    let status = pty::run_shell(&shell, clock, config)?;
    print_farewell(status);

    // Exit via `_exit`, not `std::process::exit`: the latter runs libc atexit /
    // teardown, which can race the detached stdin/stdout I/O threads as they wind
    // down and deadlock (the process gets stuck "exiting" — e.g. after `kill`).
    // The terminal was already restored when `run_shell` returned (RawGuard drop)
    // and all output is flushed, so an immediate exit is safe.
    // SAFETY: `_exit` simply terminates the process; it touches no Rust state.
    unsafe { libc::_exit(status) }
}

fn print_farewell(status: i32) {
    if status != 0 || !std::io::stderr().is_terminal() {
        return;
    }
    let mut stderr = std::io::stderr();
    let _ = writeln!(
        stderr,
        "glimps: okay, leaving. Try not to miss the readable stuff."
    );
    let _ = stderr.flush();
}

fn print_help() {
    println!(
        "glimps {} — zero-config smart terminal output formatter\n\
         \n\
         USAGE:\n\
         \x20   glimps              Wrap your shell inside GLIMPS (formats output).\n\
         \x20   glimps init zsh     Print shell integration for ~/.zshrc.\n\
         \x20   glimps --help       Show this help.\n\
         \x20   glimps --version    Show the version.\n\
         \n\
         ENVIRONMENT:\n\
         \x20   GLIMPS=0            Disable all formatting (pure pass-through).\n\
         \n\
         Enable in zsh:  echo 'command -v glimps >/dev/null 2>&1 && eval \"$(glimps init zsh)\"' >> ~/.zshrc",
        env!("CARGO_PKG_VERSION")
    );
}
