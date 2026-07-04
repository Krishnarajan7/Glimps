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
    pub html_delim: &'static str,
    pub html_name: &'static str,
    pub html_attr: &'static str,
    pub html_value: &'static str,
    pub html_raw: &'static str,
    pub comment: &'static str,
    // Log severity / HTTP status classes
    pub error: &'static str,
    pub warn: &'static str,
    pub info: &'static str,
    pub debug: &'static str,
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
            html_delim: "",
            html_name: "",
            html_attr: "",
            html_value: "",
            html_raw: "",
            comment: "",
            error: "",
            warn: "",
            info: "",
            debug: "",
            reset: "",
        }
    }

    /// The default colored theme. Conservative, readable on both light and dark
    /// backgrounds: cyan keys, green strings, yellow numbers, magenta keywords;
    /// HTML gets a richer semantic palette (dim brackets, cyan element names,
    /// yellow attributes, green values/raw text), with dim comments.
    pub const fn default_colored() -> Self {
        Theme {
            key: "\x1b[36m",        // cyan
            string: "\x1b[32m",     // green
            number: "\x1b[33m",     // yellow
            keyword: "\x1b[35m",    // magenta
            html_delim: "\x1b[2m",  // dim brackets / punctuation
            html_name: "\x1b[36m",  // cyan element names
            html_attr: "\x1b[33m",  // yellow attributes
            html_value: "\x1b[32m", // green quoted values
            html_raw: "\x1b[32m",   // green CSS/JS/title raw text
            comment: "\x1b[2m",     // dim
            error: "\x1b[31m",      // red    (ERROR / 5xx)
            warn: "\x1b[33m",       // yellow (WARN / 4xx)
            info: "\x1b[32m",       // green  (INFO / 2xx)
            debug: "\x1b[2m",       // dim    (DEBUG/TRACE / 3xx)
            reset: "\x1b[0m",
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::default_colored()
    }
}
