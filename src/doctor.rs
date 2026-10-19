//! Read-only installation and runtime diagnostics for `glimps doctor`.

use crate::config::{
    config_path, configured_shell, find_on_path, shell_name, Config, SUPPORTED_SHELLS,
};
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
    // Diagnose the shell the user actually has; on Windows the platform
    // default stands in for the never-set SHELL, and the report says so.
    let (shell, defaulted) = match configured_shell() {
        Some((shell, defaulted)) => (Some(PathBuf::from(shell)), defaulted),
        None => (None, false),
    };
    let home = crate::config::home_dir();
    let session_config = quiet_config(config_path().as_deref());
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
        check_shell(shell.as_deref(), defaulted),
        check_integration(shell.as_deref(), home.as_deref()),
        check_config(config_path().as_deref()),
        check_notify(&session_config, |key| env::var(key).ok()),
        check_links(&session_config, |key| env::var(key).ok()),
        check_redact(&session_config),
        check_path(&current_exe, env::var_os("PATH").as_deref()),
    ];
    let rc = rc_text(shell.as_deref(), home.as_deref());
    checks.extend(check_rc_hygiene(shell.as_deref(), rc.as_deref()));
    checks.push(check_coexistence(
        rc.as_deref(),
        env::var("TERM_PROGRAM").ok().as_deref(),
        env::var_os("GHOSTTY_RESOURCES_DIR").is_some(),
    ));

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

    checks.push(check_session(
        env::var_os("GLIMPS_ACTIVE").is_some(),
        env::var_os("GLIMPS_BIN").map(PathBuf::from).as_deref(),
        &current_exe,
    ));

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

fn check_shell(shell: Option<&Path>, defaulted: bool) -> Check {
    match shell.and_then(shell_name) {
        Some(name) if SUPPORTED_SHELLS.contains(&name) && defaulted => Check::pass(
            "shell",
            format!("{name} is supported (SHELL unset; platform default)"),
        ),
        Some(name) if SUPPORTED_SHELLS.contains(&name) => {
            Check::pass("shell", format!("{name} is supported"))
        }
        Some(name) => Check::fail(
            "shell",
            format!("{name} is unsupported; use zsh, bash, or PowerShell"),
        ),
        None => Check::fail("shell", "no shell found: set SHELL or pass --shell"),
    }
}

pub(crate) fn integration_path(shell: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let home = home?;
    match shell_name(shell)? {
        "zsh" => Some(
            env::var_os("ZDOTDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.to_path_buf())
                .join(".zshrc"),
        ),
        "bash" => Some(home.join(".bashrc")),
        // fish reads `conf.d` before `config.fish`, so a GLIMPS line there
        // re-execs before any other configuration runs (nothing runs twice).
        "fish" => Some(fish_config_dir(home).join("conf.d").join("00-glimps.fish")),
        // PowerShell's `$PROFILE` lives under Documents, which Windows may
        // redirect (OneDrive). Only the shell itself knows the real path, so
        // `glimps doctor` reports the conventional location and `glimps setup`
        // asks the user to edit `$PROFILE` by hand.
        "pwsh" => Some(
            home.join("Documents")
                .join("PowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
        ),
        "powershell" => Some(
            home.join("Documents")
                .join("WindowsPowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
        ),
        _ => None,
    }
}

/// `$XDG_CONFIG_HOME/fish`, else `~/.config/fish`.
pub(crate) fn fish_config_dir(home: &Path) -> PathBuf {
    fish_config_dir_in(home, env::var_os("XDG_CONFIG_HOME"))
}

/// [`fish_config_dir`] with the environment passed in. fish itself ignores a
/// relative `XDG_CONFIG_HOME`, and so does this.
fn fish_config_dir_in(home: &Path, xdg_config_home: Option<std::ffi::OsString>) -> PathBuf {
    xdg_config_home
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .unwrap_or_else(|| home.join(".config"))
        .join("fish")
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
    let name = shell_name(shell).unwrap_or_default();
    // A fish line in `config.fish` works too; it just runs after `conf.d`.
    if name == "fish" {
        if let Some(config) = home.map(|home| fish_config_dir(home).join("config.fish")) {
            if let Ok(Some(text)) = read_small_text(&config) {
                if integration_line_index(&text, name).is_some()
                    && !matches!(read_small_text(&path), Ok(Some(own)) if integration_line_index(&own, name).is_some())
                {
                    return Check::warning(
                        "integration",
                        format!(
                            "found in {}; {} runs before it, so conf.d snippets run twice per \
                             session — move the line to {}",
                            config.display(),
                            path.parent()
                                .map(|dir| dir.display().to_string())
                                .unwrap_or_default(),
                            path.display()
                        ),
                    );
                }
            }
        }
    }
    match read_small_text(&path) {
        Ok(Some(text)) if integration_line_index(&text, name).is_some() => {
            Check::pass("integration", format!("found in {}", path.display()))
        }
        Ok(Some(_)) => Check::fail(
            "integration",
            format!("\"glimps init {name}\" is missing from {}", path.display()),
        ),
        Ok(None) => Check::fail("integration", format!("{} does not exist", path.display())),
        Err(err) => Check::fail(
            "integration",
            format!("cannot inspect {}: {err}", path.display()),
        ),
    }
}

pub(crate) fn has_active_integration(text: &str, expected: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_start();
        !line.starts_with('#') && line.contains(expected)
    })
}

/// The first non-comment line index that runs the GLIMPS integration for
/// `shell`. The binary may be named directly (`glimps init zsh`), by path
/// (`/opt/homebrew/bin/glimps init zsh`) or through a variable
/// (`"$GLIMPS_DOGFOOD_BIN" init zsh`, as the dogfood rc does), so any mention
/// of glimps before `init <shell>` counts — and `starship init zsh` does not.
pub(crate) fn integration_line_index(text: &str, shell: &str) -> Option<usize> {
    text.lines()
        .position(|line| line_runs_integration(line, shell))
}

fn line_runs_integration(line: &str, shell: &str) -> bool {
    let line = line.trim_start();
    if line.starts_with('#') || shell.is_empty() {
        return false;
    }
    let lower = line.to_ascii_lowercase();
    let needle = format!("init {}", shell.to_ascii_lowercase());
    let mut from = 0;
    while let Some(found) = lower[from..].find(&needle) {
        let at = from + found;
        let end = at + needle.len();
        let word_ends = lower[end..]
            .chars()
            .next()
            .is_none_or(|next| !next.is_ascii_alphanumeric() && next != '_');
        if word_ends && lower[..at].contains("glimps") {
            return true;
        }
        from = end;
    }
    false
}

/// The rc file's text, when a shell/home pair selects one that exists and is
/// readable. Read once in `run()` and shared by the hygiene and coexistence
/// checks; never mutated.
fn rc_text(shell: Option<&Path>, home: Option<&Path>) -> Option<String> {
    let path = integration_path(shell?, home)?;
    read_small_text(&path).ok().flatten()
}

/// Frameworks whose rc lines mark "everything above the GLIMPS line runs twice
/// per session" territory. The GLIMPS integration re-execs the shell, and the
/// re-exec'd shell re-sources the same rc — so a plugin manager or prompt
/// framework *above* the GLIMPS line is initialized twice (slow startup, and
/// occasionally double-registered hooks). Needles are invocation-shaped —
/// `antigen apply`, not the bare word `antigen`, which would also match
/// `export PATH="$HOME/.antigen/bin:$PATH"` — and matched against non-comment
/// lines only.
const REEXEC_HAZARDS: &[(&str, &str)] = &[
    ("oh-my-zsh", "oh-my-zsh.sh"),
    ("zinit", "zinit.zsh"),
    ("zinit", "zinit light"),
    ("zinit", "zinit snippet"),
    ("antigen", "antigen.zsh"),
    ("antigen", "antigen apply"),
    ("zplug", "zplug/init.zsh"),
    ("zplug", "zplug load"),
    ("sheldon", "sheldon source"),
    ("starship", "starship init"),
    ("powerlevel10k instant prompt", "p10k-instant-prompt"),
    ("oh-my-posh", "oh-my-posh init"),
];

/// The first non-comment line index containing `needle`, if any.
fn active_line_index(text: &str, needle: &str) -> Option<usize> {
    text.lines().position(|line| {
        let line = line.trim_start();
        !line.starts_with('#') && line.contains(needle)
    })
}

/// If the GLIMPS integration sits *below* a known plugin manager / prompt
/// framework, name the offender that appears earliest *in the file* (not in
/// the hazard table). `None` means placement is fine (or the integration/rc is
/// absent — the integration check already reports that).
fn placement_hazard(text: &str, shell: &str) -> Option<&'static str> {
    let glimps_at = integration_line_index(text, shell)?;
    REEXEC_HAZARDS
        .iter()
        .filter_map(|(name, needle)| Some((active_line_index(text, needle)?, *name)))
        .filter(|&(at, _)| at < glimps_at)
        .min_by_key(|&(at, _)| at)
        .map(|(_, name)| name)
}

/// Warn when the GLIMPS rc line sits below a plugin manager or prompt
/// framework (the double-sourcing footgun the README documents). Emitted only
/// when the rc and an active integration line actually exist.
fn check_rc_hygiene(shell: Option<&Path>, rc: Option<&str>) -> Option<Check> {
    let name = shell_name(shell?)?;
    let text = rc?;
    integration_line_index(text, name)?;
    Some(match placement_hazard(text, name) {
        Some(name) => Check::warning(
            "rc order",
            format!(
                "the GLIMPS line sits below {name}; everything above it runs twice \
                 per session — move it near the top of the rc file"
            ),
        ),
        None => Check::pass("rc order", "the GLIMPS line is above known frameworks"),
    })
}

/// Detect other shell integrations that also emit OSC-133-style marks. GLIMPS
/// finds the command/output boundary with those marks; a second emitter in the
/// same session is untested territory and can misplace headers. Informational:
/// a warning here does not mean anything is broken.
fn check_coexistence(
    rc: Option<&str>,
    term_program: Option<&str>,
    ghostty_resources: bool,
) -> Check {
    if let Some(text) = rc {
        if has_active_integration(text, "iterm2_shell_integration") {
            return Check::warning(
                "coexistence",
                "iTerm2 shell integration is also installed; both emit shell-integration \
                 marks — if command headers misplace, load it only outside GLIMPS sessions",
            );
        }
    }
    if ghostty_resources || term_program == Some("ghostty") {
        return Check::warning(
            "coexistence",
            "Ghostty injects its own shell integration (OSC-133); if command headers \
             misplace, disable Ghostty's shell-integration feature for GLIMPS sessions",
        );
    }
    if term_program == Some("WarpTerminal") {
        return Check::warning(
            "coexistence",
            "Warp rewrites terminal blocks with its own integration; GLIMPS inside \
             Warp is untested",
        );
    }
    Check::pass("coexistence", "no competing shell integration detected")
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

/// The configuration a session would run with, without the stderr warning
/// `Config::load` prints for a broken file (`check_config` reports that).
fn quiet_config(path: Option<&Path>) -> Config {
    let mut config = path
        .and_then(|path| read_small_text(path).ok().flatten())
        .and_then(|text| Config::parse(&text).ok())
        .unwrap_or_default();
    config.clamp_limits();
    config
}

/// Whether completion notices are on and how they will be sent. `auto` is
/// resolved against the same environment the session sees, so the report
/// names the sequence actually used.
fn check_notify(config: &Config, env: impl Fn(&str) -> Option<String>) -> Check {
    let notify = &config.notify;
    if !notify.enabled {
        return Check::pass("notify", "off (opt in with [notify] enabled = true)");
    }
    let backend = crate::format::resolve_notify_backend(notify.method, env);
    let how = if notify.method == crate::config::NotifyMethod::Auto {
        "chosen for this terminal"
    } else {
        "set in config"
    };
    Check::pass(
        "notify",
        format!(
            "commands over {}s notify via {} ({how})",
            notify.after_seconds,
            backend.describe()
        ),
    )
}

/// Whether secret masking starts on in every session.
fn check_redact(config: &Config) -> Check {
    Check::pass(
        "redact",
        if config.redact.enabled {
            "on in every session (best effort; `glimps redact off` stops it for one)"
        } else {
            "off (`glimps redact on` masks secrets for one session)"
        },
    )
}

/// Whether output gets clickable links here, and what a file link opens. A
/// refused `editor` template is a warning: links still work, as `file://`.
fn check_links(config: &Config, env: impl Fn(&str) -> Option<String>) -> Check {
    use crate::config::LinksMode;
    let links = &config.links;
    if !(links.urls || links.files) {
        return Check::pass("links", "off: urls and files are both false");
    }
    if !crate::format::links_enabled(links.mode, env) {
        return Check::pass(
            "links",
            match links.mode {
                LinksMode::Off => "off in config",
                _ => "off: this terminal is not known to support OSC 8 (mode = \"on\" forces it)",
            },
        );
    }
    let kinds = match (links.urls, links.files) {
        (true, true) => "URLs and file:line",
        (true, false) => "URLs",
        _ => "file:line",
    };
    match crate::format::links_file_target(&links.editor) {
        Ok(_) if !links.files => Check::pass("links", format!("{kinds} are clickable")),
        Ok(_) => Check::pass(
            "links",
            format!("{kinds} are clickable; files open with {:?}", links.editor),
        ),
        Err(why) => Check::warning(
            "links",
            format!(
                "{kinds} are clickable, but editor {:?} was refused ({why}); files open as file://",
                links.editor
            ),
        ),
    }
}

pub(crate) fn read_small_text(path: &Path) -> std::io::Result<Option<String>> {
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

/// Inside a managed shell, say so — and say which binary is doing the
/// managing when it is not this one. A dogfood session runs the repo build
/// while `glimps` on PATH is the installed release, and a doctor report from
/// the wrong binary otherwise reads as a diagnosis of the session.
fn check_session(active: bool, session_bin: Option<&Path>, current_exe: &Path) -> Check {
    if !active {
        return Check::warning("session", "this shell is not currently managed by GLIMPS");
    }
    let same = match session_bin {
        Some(bin) => match (bin.canonicalize(), current_exe.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => bin == current_exe,
        },
        None => true,
    };
    match session_bin {
        Some(bin) if !same => Check::warning(
            "session",
            format!(
                "inside a GLIMPS-managed shell run by {}, not by this binary",
                bin.display()
            ),
        ),
        _ => Check::pass("session", "currently inside a GLIMPS-managed shell"),
    }
}

fn check_path(current_exe: &Path, path: Option<&std::ffi::OsStr>) -> Check {
    let Some(path) = path else {
        return Check::warning("PATH", "PATH is not set");
    };
    let matches = find_on_path("glimps", path).collect::<Vec<_>>();
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

    fn temp_dir(name: &str) -> PathBuf {
        let path = temp_file(name, "");
        fs::remove_file(&path).unwrap();
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn recognizes_supported_shells_by_basename() {
        assert_eq!(
            check_shell(Some(Path::new("/bin/zsh")), false).level,
            Level::Pass
        );
        assert_eq!(
            check_shell(Some(Path::new("bash")), false).level,
            Level::Pass
        );
        assert_eq!(
            check_shell(Some(Path::new("pwsh.exe")), true).level,
            Level::Pass
        );
        assert!(check_shell(Some(Path::new("pwsh.exe")), true)
            .detail
            .contains("SHELL unset"));
        assert_eq!(
            check_shell(Some(Path::new("/opt/homebrew/bin/fish")), false).level,
            Level::Pass
        );
        assert_eq!(
            check_shell(Some(Path::new("/bin/tcsh")), false).level,
            Level::Fail
        );
        assert_eq!(
            check_shell(Some(Path::new("cmd.exe")), false).level,
            Level::Fail
        );
        assert_eq!(check_shell(None, true).level, Level::Fail);
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
    fn reports_notify_state_and_the_resolved_sequence() {
        let ghostty = |key: &str| (key == "TERM_PROGRAM").then(|| "ghostty".to_string());
        let off = check_notify(&Config::default(), ghostty);
        assert_eq!(off.level, Level::Pass);
        assert!(off.detail.starts_with("off"), "{}", off.detail);

        let mut config = Config::default();
        config.notify.enabled = true;
        let on = check_notify(&config, ghostty);
        assert_eq!(
            on.detail,
            "commands over 30s notify via OSC 9 (chosen for this terminal)"
        );
        let plain = check_notify(&config, |_| None);
        assert!(
            plain.detail.contains("the terminal bell"),
            "{}",
            plain.detail
        );

        config.notify.method = crate::config::NotifyMethod::Osc777;
        config.notify.after_seconds = 90;
        assert_eq!(
            check_notify(&config, ghostty).detail,
            "commands over 90s notify via OSC 777 (set in config)"
        );

        let file = temp_file("notify", "[notify]\nenabled = true\nafter_seconds = 0\n");
        let loaded = quiet_config(Some(&file));
        assert!(loaded.notify.enabled);
        assert_eq!(loaded.notify.after_seconds, 1);
        fs::remove_file(file).unwrap();
        let broken = temp_file("notify-broken", "[notify]\nenable = true\n");
        assert!(!quiet_config(Some(&broken)).notify.enabled);
        fs::remove_file(broken).unwrap();
    }

    #[test]
    fn reports_redact_state() {
        let mut config = Config::default();
        assert!(check_redact(&config).detail.starts_with("off"));
        config.redact.enabled = true;
        assert!(check_redact(&config)
            .detail
            .starts_with("on in every session"));
    }

    #[test]
    fn reports_links_state_and_refused_templates() {
        let ghostty = |key: &str| (key == "TERM_PROGRAM").then(|| "ghostty".to_string());
        let apple = |key: &str| (key == "TERM_PROGRAM").then(|| "Apple_Terminal".to_string());
        let mut config = Config::default();
        let on = check_links(&config, ghostty);
        assert_eq!(on.level, Level::Pass);
        assert_eq!(
            on.detail,
            "URLs and file:line are clickable; files open with \"file\""
        );
        let off = check_links(&config, apple);
        assert!(
            off.detail.contains("not known to support OSC 8"),
            "{}",
            off.detail
        );

        config.links.mode = crate::config::LinksMode::On;
        assert_eq!(check_links(&config, apple).level, Level::Pass);
        config.links.editor = "my editor://{path}".to_string();
        let refused = check_links(&config, apple);
        assert_eq!(refused.level, Level::Warning);
        assert!(
            refused.detail.contains("printable ASCII"),
            "{}",
            refused.detail
        );
        config.links.mode = crate::config::LinksMode::Off;
        assert_eq!(check_links(&config, ghostty).detail, "off in config");
    }

    #[test]
    fn fish_integration_lives_in_conf_d() {
        let home = Path::new("/home/u");
        assert_eq!(
            fish_config_dir_in(home, None),
            Path::new("/home/u/.config/fish")
        );
        // Absolute on every platform (`/xdg` has no drive letter on Windows).
        let xdg = env::temp_dir().join("xdg");
        assert_eq!(
            fish_config_dir_in(home, Some(xdg.clone().into_os_string())),
            xdg.join("fish")
        );
        assert_eq!(
            fish_config_dir_in(home, Some("relative".into())),
            Path::new("/home/u/.config/fish")
        );
        assert!(integration_line_index(
            "command -q glimps; and glimps init fish | source\n",
            "fish"
        )
        .is_some());
        assert!(integration_line_index("starship init fish | source\n", "fish").is_none());
        if env::var_os("XDG_CONFIG_HOME").is_some() {
            return;
        }
        let home = temp_dir("fish-home");
        let fish = home.join(".config").join("fish");
        fs::create_dir_all(fish.join("conf.d")).unwrap();
        let shell = Path::new("/usr/local/bin/fish");
        assert_eq!(
            integration_path(shell, Some(&home)).unwrap(),
            fish.join("conf.d").join("00-glimps.fish")
        );
        assert_eq!(
            check_integration(Some(shell), Some(&home)).level,
            Level::Fail
        );
        // In config.fish it works, but conf.d snippets run twice: a warning.
        fs::write(
            fish.join("config.fish"),
            "command -q glimps; and glimps init fish | source\n",
        )
        .unwrap();
        let check = check_integration(Some(shell), Some(&home));
        assert_eq!(check.level, Level::Warning, "{}", check.detail);
        assert!(check.detail.contains("00-glimps.fish"), "{}", check.detail);
        fs::write(
            fish.join("conf.d").join("00-glimps.fish"),
            "command -q glimps; and glimps init fish | source\n",
        )
        .unwrap();
        assert_eq!(
            check_integration(Some(shell), Some(&home)).level,
            Level::Pass
        );
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn commented_integration_does_not_count_as_enabled() {
        assert_eq!(
            integration_line_index("command -v glimps && eval \"$(glimps init zsh)\"\n", "zsh"),
            Some(0)
        );
        assert_eq!(
            integration_line_index(
                "  # command -v glimps && eval \"$(glimps init zsh)\"\n",
                "zsh"
            ),
            None
        );
    }

    #[test]
    fn integration_is_recognised_by_path_and_variable_not_by_other_tools() {
        for rc in [
            "eval \"$(/opt/homebrew/bin/glimps init zsh)\"\n",
            "eval \"$(\"$GLIMPS_DOGFOOD_BIN\" init zsh)\"\n",
            "GLIMPS_BIN=~/bin/glimps; eval \"$($GLIMPS_BIN init zsh)\"\n",
            "eval \"$(Glimps.exe init zsh)\"\n",
        ] {
            assert_eq!(integration_line_index(rc, "zsh"), Some(0), "{rc:?}");
        }
        for rc in [
            "eval \"$(starship init zsh)\"\n",
            "eval \"$(glimps init bash)\"\n",
            "eval \"$(glimps init zshrc)\"\n",
            "# eval \"$(\"$GLIMPS_DOGFOOD_BIN\" init zsh)\"\n",
            "echo glimps\neval \"$(starship init zsh)\"\n",
        ] {
            assert_eq!(integration_line_index(rc, "zsh"), None, "{rc:?}");
        }
        assert_eq!(integration_line_index("x\n", ""), None);
    }

    #[test]
    fn session_check_names_a_foreign_supervisor_binary() {
        let me = Path::new("/opt/homebrew/bin/glimps");
        assert_eq!(check_session(false, None, me).level, Level::Warning);
        assert_eq!(check_session(true, None, me).level, Level::Pass);
        assert_eq!(check_session(true, Some(me), me).level, Level::Pass);
        let other = check_session(true, Some(Path::new("/repo/target/debug/glimps")), me);
        assert_eq!(other.level, Level::Warning);
        assert!(other.detail.contains("/repo/target/debug/glimps"));
    }

    #[test]
    fn placement_hazard_flags_frameworks_above_the_glimps_line() {
        let below = "source $ZSH/oh-my-zsh.sh\n\
                     command -v glimps >/dev/null 2>&1 && eval \"$(glimps init zsh)\"\n";
        assert_eq!(placement_hazard(below, "zsh"), Some("oh-my-zsh"));

        let above = "command -v glimps >/dev/null 2>&1 && eval \"$(glimps init zsh)\"\n\
                     source $ZSH/oh-my-zsh.sh\n\
                     eval \"$(starship init zsh)\"\n";
        assert_eq!(placement_hazard(above, "zsh"), None);

        // Commented-out frameworks don't count.
        let commented = "# source $ZSH/oh-my-zsh.sh\n\
                         eval \"$(glimps init zsh)\"\n";
        assert_eq!(placement_hazard(commented, "zsh"), None);

        // No integration line at all -> no placement verdict.
        assert_eq!(placement_hazard("source x\n", "zsh"), None);
    }

    #[test]
    fn coexistence_flags_other_integrations_and_terminals() {
        let rc =
            "test -e ~/.iterm2_shell_integration.zsh && source ~/.iterm2_shell_integration.zsh\n";
        assert_eq!(
            check_coexistence(Some(rc), None, false).level,
            Level::Warning
        );
        assert_eq!(
            check_coexistence(None, Some("WarpTerminal"), false).level,
            Level::Warning
        );
        assert_eq!(check_coexistence(None, None, true).level, Level::Warning);
        assert_eq!(
            check_coexistence(
                Some("eval \"$(glimps init zsh)\"\n"),
                Some("iTerm.app"),
                false
            )
            .level,
            Level::Pass
        );
    }

    #[test]
    fn diagnostic_reads_are_bounded_and_require_regular_files() {
        assert!(read_small_text(Path::new("/definitely/missing/glimpsrc"))
            .unwrap()
            .is_none());
        assert!(read_small_text(env::temp_dir().as_path()).is_err());
    }
}
