//! Color theme for formatters.
//!
//! A `Theme` is just a set of ANSI SGR escape strings the formatters wrap tokens
//! in. The `plain` theme uses empty strings, so the same formatting code path
//! produces uncolored output — which is what golden-file tests assert against
//! (deterministic, human-readable) and what a future `--no-color` / non-TTY mode
//! will use.

/// ANSI escapes used to paint formatted output. All fields are escape strings
/// (or empty for "no color"); `reset` returns to the terminal default and must
/// be empty whenever the color fields are, so plain output contains no escapes.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    // JSON
    pub key: &'static str,
    pub string: &'static str,
    pub number: &'static str,
    pub keyword: &'static str,
    // HTML
    pub tag: &'static str,
    pub comment: &'static str,
    pub reset: &'static str,
}

impl Theme {
    /// No color: every wrap is empty, so output is plain text. Used by golden
    /// tests today and by the no-color / non-TTY mode that lands next.
    #[cfg_attr(not(test), allow(dead_code))]
    pub const fn plain() -> Self {
        Theme {
            key: "",
            string: "",
            number: "",
            keyword: "",
            tag: "",
            comment: "",
            reset: "",
        }
    }

    /// The default colored theme. Conservative, readable on both light and dark
    /// backgrounds: cyan keys, green strings, yellow numbers, magenta keywords;
    /// blue HTML tags, dim comments.
    pub const fn default_colored() -> Self {
        Theme {
            key: "\x1b[36m",     // cyan
            string: "\x1b[32m",  // green
            number: "\x1b[33m",  // yellow
            keyword: "\x1b[35m", // magenta
            tag: "\x1b[34m",     // blue
            comment: "\x1b[2m",  // dim
            reset: "\x1b[0m",
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::default_colored()
    }
}
