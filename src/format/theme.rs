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
    // Low-priority readable text
    pub muted: &'static str,
    // GLIMPS-authored action confirmations
    pub action: &'static str,
    // Filesystem paths inside GLIMPS-authored messages
    pub path: &'static str,
    // Dot-prefixed files and folders
    pub hidden: &'static str,
    // Known directories in typed filesystem output
    pub folder: &'static str,
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
            muted: "",
            action: "",
            path: "",
            hidden: "",
            folder: "",
            error: "",
            warn: "",
            info: "",
            debug: "",
            reset: "",
        }
    }

    /// The default colored theme. Cyan marks structure, pale blue carries
    /// readable content, bright gold marks numbers and attributes, and magenta
    /// marks keywords. Green is reserved for successful states and additions,
    /// so long HTML or JSON values do not compete with status information.
    pub const fn default_colored() -> Self {
        Theme {
            key: "\x1b[36m",                    // cyan
            string: "\x1b[38;5;117m",           // pale sky blue content
            number: "\x1b[38;5;220m",           // bright gold numbers
            keyword: "\x1b[35m",                // magenta
            html_delim: "\x1b[2m",              // dim brackets / punctuation
            html_name: "\x1b[38;2;224;82;125m", // #e0527d HTML element names
            html_attr: "\x1b[38;5;220m",        // bright gold attributes
            html_value: "\x1b[38;5;117m",       // pale sky blue quoted values
            html_raw: "\x1b[38;5;117m",         // pale sky blue CSS/JS/title text
            comment: "\x1b[2m",                 // dim
            muted: "\x1b[38;5;153m",            // pale blue-gray for long low-priority text
            action: "\x1b[38;2;5;130;202m",     // #0582ca action confirmations
            path: "\x1b[38;2;142;202;230m",     // #8ecae6 filesystem paths
            hidden: "\x1b[38;2;69;73;85m",      // #454955 hidden files and folders
            folder: "\x1b[38;2;122;162;247m",   // #7aa2f7 visible folders
            error: "\x1b[31m",                  // red    (ERROR / 5xx)
            warn: "\x1b[38;5;220m",             // bright gold (WARN / 4xx)
            info: "\x1b[32m",                   // green  (INFO / 2xx)
            debug: "\x1b[2m",                   // dim    (DEBUG/TRACE / 3xx)
            reset: "\x1b[0m",
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::default_colored()
    }
}
