//! `.glimpsrc` — optional TOML configuration.
//!
//! Loaded once at startup from `$GLIMPSRC` (if set) or `~/.glimpsrc`. Everything
//! has a sensible default, so the file is entirely optional. A missing file uses
//! defaults silently; an *unparseable* file uses defaults and warns to stderr —
//! a broken config must never break the terminal (safety over cleverness).
//!
//! Example `~/.glimpsrc`:
//! ```toml
//! enabled = true     # master switch (like GLIMPS=0, but persistent)
//! color = true       # false => no color codes anywhere (still indents/frames)
//! separator = true   # the command/output divider line
//! timestamp = true   # include HH:MM:SS in the separator
//!
//! [formatters]
//! json = true
//! html = true
//! logs = true        # ERROR/WARN/INFO/DEBUG line coloring
//! http = true        # HTTP status line coloring
//! diff = true        # unified-diff coloring
//! stacktrace = true  # stack-trace / panic highlighting
//!
//! [limits]
//! buffer_cap = 1048576   # max bytes buffered to detect JSON/HTML (1 MiB)
//! line_cap   = 65536     # max bytes of one un-terminated streamed line (64 KiB)
//! sniff_cap  = 64        # max leading whitespace held while deciding
//! ```

use std::path::PathBuf;

use serde::Deserialize;

/// Defaults mirror the formatter's built-in constants.
const DEFAULT_BUFFER_CAP: usize = 1024 * 1024;
const DEFAULT_LINE_CAP: usize = 64 * 1024;
const DEFAULT_SNIFF_CAP: usize = 64;

/// Top-level configuration. Cloned once into the reader thread at startup.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Master switch. `false` makes GLIMPS a pure pass-through.
    pub enabled: bool,
    /// Emit color escapes. `false` keeps structure (indent/frame) but no color.
    pub color: bool,
    /// Show the command header / separator line before output.
    pub separator: bool,
    /// Include a timestamp in the command header.
    pub timestamp: bool,
    /// Command names whose output is passed through untouched (interactive /
    /// full-screen programs). Matched against the command's basename.
    pub bypass: Vec<String>,
    pub formatters: Formatters,
    pub limits: Limits,
}

/// Per-content-type enable switches.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Formatters {
    pub json: bool,
    pub html: bool,
    pub logs: bool,
    pub http: bool,
    /// Color unified diffs (added/removed/hunk/file-header lines).
    pub diff: bool,
    /// Highlight stack traces / panics (Rust panics, Python tracebacks).
    pub stacktrace: bool,
}

/// Buffering / streaming size limits.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub buffer_cap: usize,
    pub line_cap: usize,
    pub sniff_cap: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            color: true,
            separator: true,
            timestamp: true,
            bypass: default_bypass(),
            formatters: Formatters::default(),
            limits: Limits::default(),
        }
    }
}

/// Interactive / full-screen programs whose output we pass through untouched.
/// Full-screen apps are also caught by alt-screen detection; this additionally
/// covers ones that don't use it (notably `ssh`).
fn default_bypass() -> Vec<String> {
    [
        "vim", "nvim", "vi", "nano", "emacs", "less", "more", "man", "htop", "top", "btop", "fzf",
        "tmux", "screen", "ssh", "watch", "ncdu", "lazygit", "tig", "ranger",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

impl Default for Formatters {
    fn default() -> Self {
        Formatters {
            json: true,
            html: true,
            logs: true,
            http: true,
            diff: true,
            stacktrace: true,
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            buffer_cap: DEFAULT_BUFFER_CAP,
            line_cap: DEFAULT_LINE_CAP,
            sniff_cap: DEFAULT_SNIFF_CAP,
        }
    }
}

impl Config {
    /// Load configuration, falling back to defaults on any problem, and clamp
    /// user-supplied limits to sane bounds (a `.glimpsrc` must not be able to
    /// defeat GLIMPS's bounded-buffering safety property).
    pub fn load() -> Self {
        let mut config = Self::load_raw();
        config.clamp_limits();
        config
    }

    fn load_raw() -> Self {
        let Some(path) = config_path() else {
            return Config::default();
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            // Absent / unreadable: silent defaults (the file is optional).
            Err(_) => return Config::default(),
        };
        Self::parse(&text).unwrap_or_else(|err| {
            eprintln!("glimps: ignoring {} ({err})", path.display());
            Config::default()
        })
    }

    /// Parse TOML into a `Config`. Separated out for testing.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Clamp limits to sane bounds so a hand-edited (or huge) value can't make
    /// buffering effectively unbounded or pathologically tiny. `sniff_cap = 0`
    /// is allowed (it just means "never wait on leading whitespace").
    fn clamp_limits(&mut self) {
        self.limits.buffer_cap = self.limits.buffer_cap.clamp(4 * 1024, 64 * 1024 * 1024);
        self.limits.line_cap = self.limits.line_cap.clamp(256, 16 * 1024 * 1024);
        self.limits.sniff_cap = self.limits.sniff_cap.min(64 * 1024);
    }
}

/// `$GLIMPSRC` if set, else `~/.glimpsrc`. `None` if `HOME` is also unset.
fn config_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("GLIMPSRC") {
        return Some(PathBuf::from(p));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".glimpsrc"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_everything_on() {
        let c = Config::default();
        assert!(c.enabled && c.color && c.separator && c.timestamp);
        assert!(c.formatters.json && c.formatters.html && c.formatters.logs && c.formatters.http);
        assert!(c.formatters.diff && c.formatters.stacktrace);
        assert_eq!(c.limits.buffer_cap, DEFAULT_BUFFER_CAP);
    }

    #[test]
    fn empty_config_is_defaults() {
        let c = Config::parse("").unwrap();
        assert!(c.enabled && c.formatters.json);
    }

    #[test]
    fn partial_config_keeps_other_defaults() {
        let c = Config::parse("color = false\n[formatters]\nhtml = false\n").unwrap();
        assert!(!c.color);
        assert!(!c.formatters.html);
        // Untouched keys keep their defaults.
        assert!(c.enabled);
        assert!(c.formatters.json);
        assert_eq!(c.limits.line_cap, DEFAULT_LINE_CAP);
    }

    #[test]
    fn limits_override() {
        let c = Config::parse("[limits]\nbuffer_cap = 2048\nsniff_cap = 8\n").unwrap();
        assert_eq!(c.limits.buffer_cap, 2048);
        assert_eq!(c.limits.sniff_cap, 8);
        assert_eq!(c.limits.line_cap, DEFAULT_LINE_CAP);
    }

    #[test]
    fn invalid_toml_is_an_error() {
        // `load()` turns this into a warning + defaults; `parse` surfaces it.
        assert!(Config::parse("enabled = notabool").is_err());
        assert!(Config::parse("enabled = = =").is_err());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        // Typos surface as errors (-> warning + defaults) instead of silently
        // reverting to defaults with no feedback.
        assert!(Config::parse("colour = false\n").is_err());
        assert!(Config::parse("[formatter]\njson = false\n").is_err());
        assert!(Config::parse("[formatters]\njsoon = false\n").is_err());
    }

    #[test]
    fn limits_are_clamped_to_sane_bounds() {
        let mut c = Config::default();
        c.limits.buffer_cap = usize::MAX;
        c.limits.line_cap = 1;
        c.limits.sniff_cap = usize::MAX;
        c.clamp_limits();
        assert_eq!(c.limits.buffer_cap, 64 * 1024 * 1024);
        assert_eq!(c.limits.line_cap, 256);
        assert_eq!(c.limits.sniff_cap, 64 * 1024);
    }
}
