//! Private shell-to-supervisor metadata transport.
//!
//! OSC markers remain in the PTY stream as compatibility boundary hints, but
//! trusted command/cwd/status values travel through this file. The shell captures
//! its path during startup and immediately unsets the exported environment value,
//! so ordinary child commands cannot discover it from their environment.

use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_RECORD_BYTES: usize = 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MetadataEvent {
    Command(Vec<u8>),
    Result {
        pipeline_statuses: Vec<i32>,
        cwd: Vec<u8>,
        exit_code: i32,
    },
}

pub(crate) struct MetadataChannel {
    dir: PathBuf,
    path: PathBuf,
    reader: Option<MetadataReader>,
}

impl MetadataChannel {
    pub(crate) fn create() -> Result<Self> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let nonce = random_nonce().unwrap_or_else(|| {
            // No `/dev/urandom` (Windows, or a Unix sandbox without one): mix the
            // OS-seeded hasher keys the standard library draws at first use with
            // the clock and a counter.
            // The name is one layer; the directory location (`channel_root`)
            // is the one that has to be private.
            use std::hash::{BuildHasher, Hasher};
            let seeded = std::collections::hash_map::RandomState::new()
                .build_hasher()
                .finish();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default();
            format!("{seeded:016x}{nanos:032x}{seq:016x}")
        });
        let dir = channel_root().join(format!("glimps-meta-{nonce}"));
        create_private_dir(&dir).context("failed to create metadata directory")?;
        let path = dir.join("events");
        let file = open_private_file(&path).context("failed to create metadata channel")?;
        Ok(Self {
            dir,
            path,
            reader: Some(MetadataReader { file }),
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn take_reader(&mut self) -> Option<MetadataReader> {
        self.reader.take()
    }
}

impl Drop for MetadataChannel {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        // The reader thread may still hold the file open for a few more
        // milliseconds. Unix unlinks regardless; Windows only marks the file
        // delete-pending, so the directory is briefly non-empty. Retry the
        // directory removal for a bounded moment rather than leak one
        // `glimps-meta-*` directory per session.
        for _ in 0..10 {
            if std::fs::remove_dir(&self.dir).is_ok() || !self.dir.exists() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

pub(crate) struct MetadataReader {
    file: File,
}

impl MetadataReader {
    /// Read and parse all complete records currently available. Shell hooks finish
    /// each append before emitting the corresponding OSC boundary, so this call is
    /// synchronous and non-blocking on a normal file. Keep the file append-only:
    /// truncating after a read can erase a fast command's concurrently appended
    /// result. The reader's file cursor advances incrementally, and the temporary
    /// channel is removed when the supervised session ends.
    pub(crate) fn drain(&mut self) -> Vec<MetadataEvent> {
        let mut bytes = Vec::new();
        if self.file.read_to_end(&mut bytes).is_err() {
            return Vec::new();
        }
        if bytes.len() > MAX_RECORD_BYTES {
            return Vec::new();
        }
        parse_records(&bytes)
    }
}

fn parse_records(bytes: &[u8]) -> Vec<MetadataEvent> {
    let fields = bytes.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut events = Vec::new();
    let mut idx = 0;
    while idx < fields.len() {
        match fields[idx] {
            b"C" if idx + 1 < fields.len() => {
                events.push(MetadataEvent::Command(fields[idx + 1].to_vec()));
                idx += 2;
            }
            b"R" if idx + 3 < fields.len() => {
                let statuses = parse_statuses(fields[idx + 1]);
                let exit = parse_i32(fields[idx + 3]);
                if let (Some(pipeline_statuses), Some(exit_code)) = (statuses, exit) {
                    events.push(MetadataEvent::Result {
                        pipeline_statuses,
                        cwd: fields[idx + 2].to_vec(),
                        exit_code,
                    });
                }
                idx += 4;
            }
            b"" => idx += 1,
            _ => idx += 1,
        }
    }
    events
}

fn parse_statuses(bytes: &[u8]) -> Option<Vec<i32>> {
    let text = std::str::from_utf8(bytes).ok()?;
    let statuses = text
        .split_ascii_whitespace()
        .map(str::parse::<i32>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .ok()?;
    (!statuses.is_empty()).then_some(statuses)
}

fn parse_i32(bytes: &[u8]) -> Option<i32> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

fn random_nonce() -> Option<String> {
    let mut bytes = [0u8; 24];
    File::open("/dev/urandom")
        .ok()?
        .read_exact(&mut bytes)
        .ok()?;
    Some(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Where the channel directory lives. The channel carries every command line
/// the user runs, so the parent must be private to the user.
///   * Unix: the temp dir, with the channel itself created `0o700`.
///   * Windows: there are no mode bits here (see `create_private_dir`), so the
///     channel inherits its parent's ACL. `%LOCALAPPDATA%\Temp` sits inside the
///     user profile, whose default ACL is owner-only; `%TEMP%` usually points
///     there too but can be redirected to a shared path (CI images, managed
///     desktops), so it is only the fallback.
fn channel_root() -> PathBuf {
    if cfg!(windows) {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let temp = PathBuf::from(local).join("Temp");
            if temp.is_dir() {
                return temp;
            }
        }
    }
    std::env::temp_dir()
}

#[cfg(unix)]
fn create_private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = std::fs::DirBuilder::new();
    builder.mode(0o700).create(path)
}

/// Windows has no mode bits; the directory inherits its parent's ACL, which is
/// why `channel_root` anchors it inside the user profile.
#[cfg(not(unix))]
fn create_private_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir(path)
}

#[cfg(unix)]
fn open_private_file(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_private_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_binary_safe_command_and_result_records() {
        let records = b"C\0printf 'a\\nb'\0R\0\x31 0\0/tmp/a\nfolder\0\x37\0";
        assert_eq!(
            parse_records(records),
            vec![
                MetadataEvent::Command(b"printf 'a\\nb'".to_vec()),
                MetadataEvent::Result {
                    pipeline_statuses: vec![1, 0],
                    cwd: b"/tmp/a\nfolder".to_vec(),
                    exit_code: 7,
                },
            ]
        );
    }

    #[test]
    fn channel_drains_incrementally_between_commands() {
        let mut channel = MetadataChannel::create().unwrap();
        let mut writer = OpenOptions::new()
            .append(true)
            .open(channel.path())
            .unwrap();
        writer.write_all(b"C\0echo one\0").unwrap();
        writer.flush().unwrap();
        let mut reader = channel.take_reader().unwrap();
        assert_eq!(
            reader.drain(),
            vec![MetadataEvent::Command(b"echo one".to_vec())]
        );
        writer.write_all(b"C\0echo two\0").unwrap();
        writer.flush().unwrap();
        assert_eq!(
            reader.drain(),
            vec![MetadataEvent::Command(b"echo two".to_vec())]
        );
    }

    #[test]
    fn draining_never_truncates_a_concurrent_writers_channel() {
        let mut channel = MetadataChannel::create().unwrap();
        let path = channel.path().to_path_buf();
        let mut writer = OpenOptions::new().append(true).open(&path).unwrap();
        writer.write_all(b"C\0echo fast\0").unwrap();
        writer.flush().unwrap();

        let mut reader = channel.take_reader().unwrap();
        assert_eq!(
            reader.drain(),
            vec![MetadataEvent::Command(b"echo fast".to_vec())]
        );
        let first_len = std::fs::metadata(&path).unwrap().len();
        assert!(first_len > 0, "drain must not truncate the shared file");

        writer.write_all(b"R\x000\x00/tmp\x001\x00").unwrap();
        writer.flush().unwrap();
        assert_eq!(
            reader.drain(),
            vec![MetadataEvent::Result {
                pipeline_statuses: vec![0],
                cwd: b"/tmp".to_vec(),
                exit_code: 1,
            }]
        );
        assert!(std::fs::metadata(path).unwrap().len() > first_len);
    }
}
