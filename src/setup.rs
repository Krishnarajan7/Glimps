//! `glimps setup` — guided, consent-based install of the shell integration.
//!
//! Automates exactly what the README documents by hand: one guarded line near
//! the top of the rc file. Deliberately conservative: interactive-only, shows
//! the exact edit before asking, takes a timestamped backup, and writes via a
//! same-directory temp file + rename so the rc is never left half-written.
//! `glimps init <shell>` plus a manual edit remains the documented alternative.

use crate::doctor::{has_active_integration, integration_path, read_small_text, shell_name};
use anyhow::{Context, Result};
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

/// Run the guided setup. Returns the process exit code.
pub fn run(shell_arg: Option<&str>) -> Result<i32> {
    let shell = match shell_arg {
        Some(s) => s.to_string(),
        None => std::env::var("SHELL").unwrap_or_default(),
    };
    let Some(name) = shell_name(Path::new(&shell)).map(str::to_string) else {
        eprintln!("glimps: cannot tell which shell to set up; run `glimps setup zsh` or `glimps setup bash`.");
        return Ok(2);
    };
    if !matches!(name.as_str(), "zsh" | "bash") {
        eprintln!(
            "glimps: {name} is not supported yet (zsh and bash are). fish is on the roadmap."
        );
        return Ok(2);
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        eprintln!(
            "glimps: setup is interactive. Non-interactively, add this line near the top of \
             your rc file yourself:"
        );
        eprintln!("  {}", integration_line(&name));
        return Ok(2);
    }

    let home = std::env::var_os("HOME").map(PathBuf::from);
    let Some(rc) = integration_path(Path::new(&shell), home.as_deref()) else {
        eprintln!("glimps: HOME is not set; cannot locate an rc file.");
        return Ok(2);
    };
    // Dotfile repos routinely symlink the rc (`~/.zshrc -> ~/dotfiles/zshrc`).
    // `rename` over the symlink would silently replace it with a regular file
    // and detach the repo — resolve to the real file and edit that instead.
    let rc = match std::fs::canonicalize(&rc) {
        Ok(real) => real,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => rc,
        Err(err) => {
            eprintln!("glimps: cannot resolve {}: {err}", rc.display());
            return Ok(2);
        }
    };

    let expected = format!("glimps init {name}");
    let existing = match read_small_text(&rc) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("glimps: cannot inspect {}: {err}", rc.display());
            return Ok(2);
        }
    };
    if let Some(text) = &existing {
        if has_active_integration(text, &expected) {
            println!(
                "glimps: shell integration is already enabled in {}. Nothing to do.",
                rc.display()
            );
            return Ok(0);
        }
    }

    let line = integration_line(&name);
    println!("glimps setup will make one change:");
    println!();
    println!("  file:  {}", rc.display());
    println!("  where: at the top (anything above the line runs twice per session)");
    println!("  add:   {line}");
    println!();
    if existing.is_some() {
        println!("  A timestamped backup of the file is taken first.");
        println!();
    }
    print!("Proceed? [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    if !is_affirmative(&answer) {
        println!("glimps: no changes made.");
        return Ok(0);
    }

    if let Some(text) = &existing {
        let backup =
            write_backup(&rc, text).with_context(|| format!("cannot back up {}", rc.display()))?;
        println!("glimps: backed up {} to {}", rc.display(), backup.display());
    }

    let updated = insert_integration(existing.as_deref().unwrap_or(""), &line);
    write_replacing(&rc, updated.as_bytes())
        .with_context(|| format!("cannot update {}", rc.display()))?;

    println!("glimps: added the integration to {}.", rc.display());
    if cfg!(target_os = "macos") && name == "bash" {
        println!(
            "glimps: note — macOS terminals start bash as a *login* shell, which reads \
             ~/.bash_profile, not ~/.bashrc. Make sure ~/.bash_profile contains \
             `[ -f ~/.bashrc ] && source ~/.bashrc`."
        );
    }
    println!(
        "glimps: restart your terminal (or run `exec {name}`), then `glimps doctor` to verify."
    );
    Ok(0)
}

/// The guarded one-liner the README documents, plus the comment that explains it.
fn integration_line(shell: &str) -> String {
    format!("command -v glimps >/dev/null 2>&1 && eval \"$(glimps init {shell})\"")
}

/// New rc content with the integration inserted at the very top, under a short
/// provenance comment, separated from the existing content by a blank line.
fn insert_integration(existing: &str, line: &str) -> String {
    let mut out = String::with_capacity(existing.len() + line.len() + 128);
    out.push_str("# GLIMPS shell integration — added by `glimps setup`. Keep near the top:\n");
    out.push_str("# anything above this line runs twice per session (see the GLIMPS README).\n");
    out.push_str(line);
    out.push('\n');
    if !existing.is_empty() {
        out.push('\n');
        out.push_str(existing);
    }
    out
}

fn is_affirmative(answer: &str) -> bool {
    matches!(answer.trim(), "y" | "Y" | "yes" | "Yes" | "YES")
}

/// A sibling path built from the rc's file name plus `suffix`. Errors instead
/// of degenerating when the path has no final component.
fn sibling(rc: &Path, suffix: &str) -> std::io::Result<PathBuf> {
    let Some(name) = rc.file_name() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "rc path has no file name",
        ));
    };
    let mut name = name.to_os_string();
    name.push(suffix);
    Ok(rc.with_file_name(name))
}

/// Write `text` to a fresh backup next to the rc and return its path.
/// `create_new` refuses to clobber an existing backup (two runs in the same
/// second, or a retry) — on collision the suffix is bumped instead. The backup
/// is created with the rc's own mode, so an `0600` rc (people keep API keys in
/// rc files) never gets a world-readable copy.
fn write_backup(rc: &Path, text: &str) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mode = std::fs::metadata(rc)
        .map(|meta| meta.permissions().mode())
        .unwrap_or(0o600);
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    for attempt in 0..100u32 {
        let suffix = if attempt == 0 {
            format!(".glimps-backup-{secs}")
        } else {
            format!(".glimps-backup-{secs}-{attempt}")
        };
        let path = sibling(rc, &suffix)?;
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(text.as_bytes())?;
                file.sync_all()?;
                return Ok(path);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not find a free backup name",
    ))
}

/// Write `bytes` to `path` via a same-directory temp file, fsync, and rename
/// (atomic on the same filesystem), preserving the original file's exact
/// permissions. The temp file is created with the original's mode from the
/// start so a `0600` rc's content is never exposed through a looser temp file,
/// and it is removed if any step fails.
fn write_replacing(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let tmp = sibling(path, &format!(".glimps-tmp-{}", std::process::id()))?;
    let mode = std::fs::metadata(path)
        .map(|meta| meta.permissions().mode())
        .unwrap_or(0o600);
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&tmp)?;
        // The open mode is masked by umask; set the exact original mode.
        file.set_permissions(std::fs::Permissions::from_mode(mode))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integration_is_inserted_at_the_top_and_preserves_content() {
        let existing = "export PATH=~/bin:$PATH\nsource ~/oh-my-zsh.sh\n";
        let updated = insert_integration(existing, &integration_line("zsh"));
        assert!(updated.starts_with("# GLIMPS shell integration"));
        assert!(updated.contains("eval \"$(glimps init zsh)\""));
        assert!(updated.ends_with(existing));
        // The integration line appears before the existing content.
        let glimps_at = updated.find("glimps init zsh").unwrap();
        let omz_at = updated.find("oh-my-zsh.sh").unwrap();
        assert!(glimps_at < omz_at);
    }

    #[test]
    fn empty_rc_gets_no_trailing_blank_line() {
        let updated = insert_integration("", &integration_line("bash"));
        assert!(updated.ends_with("glimps init bash)\"\n"));
    }

    #[test]
    fn affirmative_answers_are_narrow() {
        assert!(is_affirmative("y\n"));
        assert!(is_affirmative("Yes\n"));
        assert!(!is_affirmative("\n"));
        assert!(!is_affirmative("n\n"));
        assert!(!is_affirmative("maybe\n"));
    }

    #[test]
    fn sibling_paths_stay_in_the_rc_directory_and_need_a_file_name() {
        let backup = sibling(Path::new("/home/u/.zshrc"), ".glimps-backup-1").unwrap();
        assert!(backup.starts_with("/home/u"));
        assert_eq!(
            backup.file_name().unwrap().to_str().unwrap(),
            ".zshrc.glimps-backup-1"
        );
        assert!(sibling(Path::new("/"), ".glimps-tmp").is_err());
    }

    #[test]
    fn backups_never_clobber_an_existing_backup() {
        let dir = std::env::temp_dir().join(format!("glimps-setup-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rc = dir.join(".zshrc");
        std::fs::write(&rc, "one\n").unwrap();
        let first = write_backup(&rc, "one\n").unwrap();
        let second = write_backup(&rc, "two\n").unwrap();
        assert_ne!(first, second, "same-second backups must not collide");
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "one\n");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "two\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_replacing_replaces_content_and_cleans_up_temp_files() {
        let dir = std::env::temp_dir().join(format!("glimps-setup-wr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rc = dir.join(".zshrc");
        std::fs::write(&rc, "old\n").unwrap();
        write_replacing(&rc, b"new\n").unwrap();
        assert_eq!(std::fs::read_to_string(&rc).unwrap(), "new\n");
        // No stray temp files remain.
        let stray = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("glimps-tmp"))
            .count();
        assert_eq!(stray, 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
