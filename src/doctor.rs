//! Read-only installation and runtime diagnostics for `glimps doctor`.

use crate::config::{config_path, Config};
use crate::metadata::MetadataChannel;
use anyhow::Result;
use std::env;
use std::fs;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

const MAX_DIAGNOSTIC_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Level {
    Pass,
    Warning,
    Fail,
}

#[derive(Debug, PartialEq, Eq)]
struct Check {
    level: Level,
    name: &'static str,
    detail: String,
}

impl Check {
    fn pass(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            level: Level::Pass,
            name,
            detail: detail.into(),
        }
    }

    fn warning(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            level: Level::Warning,
            name,
            detail: detail.into(),
        }
    }

    fn fail(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            level: Level::Fail,
            name,
            detail: detail.into(),
        }
    }
}

/// Run all diagnostics, print a compact report, and return a process exit code.
/// No check changes shell files, configuration, or machine state.
pub fn run() -> Result<i32> {
    let current_exe = env::current_exe()?;
    let shell = env::var_os("SHELL").map(PathBuf::from);
    let home = env::var_os("HOME").map(PathBuf::from);
    let mut checks = vec![
        Check::pass(
            "binary",
            format!(
                "glimps {} at {}",
                env!("CARGO_PKG_VERSION"),
                current_exe.display()
            ),
        ),
        Check::pass(
            "platform",
            format!("{} / {}", env::consts::OS, env::consts::ARCH),
        ),
        check_shell(shell.as_deref()),
        check_integration(shell.as_deref(), home.as_deref()),
        check_config(config_path().as_deref()),
        check_path(&current_exe, env::var_os("PATH").as_deref()),
    ];

    checks.push(
        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            Check::pass("terminal", "stdin and stdout are TTYs")
        } else {
            Check::warning("terminal", "not attached to an interactive TTY")
        },
    );

    checks.push(match env::var("TERM") {
        Ok(term) if term != "dumb" && !term.is_empty() => Check::pass("TERM", term),
        Ok(term) => Check::warning("TERM", format!("{term:?} limits terminal capabilities")),
        Err(_) => Check::warning("TERM", "not set"),
    });

    checks.push(if env::var_os("GLIMPS_ACTIVE").is_some() {
        Check::pass("session", "currently inside a GLIMPS-managed shell")
    } else {
        Check::warning("session", "this shell is not currently managed by GLIMPS")
    });

    checks.push(
        if env::var_os("GLIMPS").as_deref() == Some(std::ffi::OsStr::new("0")) {
            Check::warning("formatting", "disabled by GLIMPS=0")
        } else {
            Check::pass(
                "formatting",
                "not disabled by the GLIMPS environment variable",
            )
        },
    );

    checks.push(match MetadataChannel::create() {
        Ok(_) => Check::pass("metadata", "private command metadata channel is available"),
        Err(err) => Check::fail(
            "metadata",
            format!("cannot create private channel: {err:#}"),
        ),
    });

    println!("GLIMPS doctor {}", env!("CARGO_PKG_VERSION"));
    println!();
    for check in &checks {
        let marker = match check.level {
            Level::Pass => "[ok]",
            Level::Warning => "[warn]",
            Level::Fail => "[fail]",
        };
        println!("{marker:<6} {:<12} {}", check.name, check.detail);
    }

    let failures = checks
        .iter()
        .filter(|check| check.level == Level::Fail)
        .count();
    let warnings = checks
        .iter()
        .filter(|check| check.level == Level::Warning)
        .count();
    println!();
    if failures == 0 {
        println!("Ready with {warnings} warning(s). No changes were made.");
        Ok(0)
    } else {
        println!("Found {failures} problem(s) and {warnings} warning(s). No changes were made.");
        Ok(1)
    }
}

fn shell_name(shell: &Path) -> Option<&str> {
    shell.file_name()?.to_str()
}

fn check_shell(shell: Option<&Path>) -> Check {
    match shell.and_then(shell_name) {
        Some(name @ ("zsh" | "bash")) => Check::pass("shell", format!("{name} is supported")),
        Some(name) => Check::fail("shell", format!("{name} is unsupported; use zsh or bash")),
        None => Check::fail("shell", "SHELL is missing or invalid"),
    }
}

fn integration_path(shell: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let home = home?;
    match shell_name(shell)? {
        "zsh" => Some(
            env::var_os("ZDOTDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.to_path_buf())
                .join(".zshrc"),
        ),
        "bash" => Some(home.join(".bashrc")),
        _ => None,
    }
}

fn check_integration(shell: Option<&Path>, home: Option<&Path>) -> Check {
    let Some(shell) = shell else {
        return Check::fail("integration", "cannot locate an rc file without SHELL");
    };
    let Some(path) = integration_path(shell, home) else {
        return Check::fail(
            "integration",
            "no supported shell rc file could be selected",
        );
    };
    let expected = format!("glimps init {}", shell_name(shell).unwrap_or_default());
    match read_small_text(&path) {
        Ok(Some(text)) if has_active_integration(&text, &expected) => {
            Check::pass("integration", format!("found in {}", path.display()))
        }
        Ok(Some(_)) => Check::fail(
            "integration",
            format!("{expected:?} is missing from {}", path.display()),
        ),
        Ok(None) => Check::fail("integration", format!("{} does not exist", path.display())),
        Err(err) => Check::fail(
            "integration",
            format!("cannot inspect {}: {err}", path.display()),
        ),
    }
}

fn has_active_integration(text: &str, expected: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_start();
        !line.starts_with('#') && line.contains(expected)
    })
}

fn check_config(path: Option<&Path>) -> Check {
    let Some(path) = path else {
        return Check::pass("config", "HOME and GLIMPSRC are unset; using defaults");
    };
    match read_small_text(path) {
        Ok(None) => Check::pass(
            "config",
            format!("{} is absent; using defaults", path.display()),
        ),
        Ok(Some(text)) => match Config::parse(&text) {
            Ok(config) if config.enabled => {
                Check::pass("config", format!("{} is valid", path.display()))
            }
            Ok(_) => Check::warning(
                "config",
                format!("{} is valid but enabled=false", path.display()),
            ),
            Err(err) => Check::fail("config", format!("{} is invalid: {err}", path.display())),
        },
        Err(err) => Check::fail(
            "config",
            format!("cannot inspect {}: {err}", path.display()),
        ),
    }
}

fn read_small_text(path: &Path) -> std::io::Result<Option<String>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    if !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    if metadata.len() > MAX_DIAGNOSTIC_FILE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file exceeds the 1 MiB diagnostic limit",
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)?
        .take(MAX_DIAGNOSTIC_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_DIAGNOSTIC_FILE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file grew beyond the 1 MiB diagnostic limit",
        ));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "file is not UTF-8"))
}

fn check_path(current_exe: &Path, path: Option<&std::ffi::OsStr>) -> Check {
    let Some(path) = path else {
        return Check::warning("PATH", "PATH is not set");
    };
    let matches = env::split_paths(path)
        .map(|dir| dir.join("glimps"))
        .filter(|candidate| candidate.is_file())
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return Check::warning("PATH", "no glimps executable found on PATH");
    }
    let current = current_exe.canonicalize().ok();
    let current_is_visible = matches
        .iter()
        .any(|candidate| candidate.canonicalize().ok() == current);
    if !current_is_visible {
        Check::warning(
            "PATH",
            format!(
                "PATH resolves another installation at {}",
                matches[0].display()
            ),
        )
    } else if matches.len() > 1 {
        Check::warning(
            "PATH",
            format!(
                "{} glimps executables found; the current one is visible",
                matches.len()
            ),
        )
    } else {
        Check::pass("PATH", format!("{}", matches[0].display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file(name: &str, contents: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "glimps-doctor-{}-{nonce}-{name}",
            std::process::id()
        ));
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn recognizes_supported_shells_by_basename() {
        assert_eq!(check_shell(Some(Path::new("/bin/zsh"))).level, Level::Pass);
        assert_eq!(check_shell(Some(Path::new("bash"))).level, Level::Pass);
        assert_eq!(check_shell(Some(Path::new("/bin/fish"))).level, Level::Fail);
    }

    #[test]
    fn validates_config_without_falling_back_silently() {
        let valid = temp_file("valid", "enabled = true\n");
        let invalid = temp_file("invalid", "unknown = true\n");
        assert_eq!(check_config(Some(&valid)).level, Level::Pass);
        assert_eq!(check_config(Some(&invalid)).level, Level::Fail);
        fs::remove_file(valid).unwrap();
        fs::remove_file(invalid).unwrap();
    }

    #[test]
    fn commented_integration_does_not_count_as_enabled() {
        assert!(has_active_integration(
            "command -v glimps && eval \"$(glimps init zsh)\"\n",
            "glimps init zsh"
        ));
        assert!(!has_active_integration(
            "  # command -v glimps && eval \"$(glimps init zsh)\"\n",
            "glimps init zsh"
        ));
    }

    #[test]
    fn diagnostic_reads_are_bounded_and_require_regular_files() {
        assert!(read_small_text(Path::new("/definitely/missing/glimpsrc"))
            .unwrap()
            .is_none());
        assert!(read_small_text(env::temp_dir().as_path()).is_err());
    }
}
