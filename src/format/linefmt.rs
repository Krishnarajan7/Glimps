//! Streaming line colorizer — log severity + HTTP status.
//!
//! Unlike the buffered JSON/HTML formatters, this works **one complete line at a
//! time**, so it suits live/unbounded output (`tail -f`, `docker logs -f`). The
//! Formatter line-buffers across chunks and calls [`colorize_line`] on each
//! finished line.
//!
//! Precision first (charter invariant #2 — a false positive is worse than a miss):
//! * Severity matches only an **uppercase, word-delimited** level token near the
//!   start of the line (so prose like "an error occurred" is never colored).
//! * HTTP matches only lines beginning `HTTP/` with a real 3-digit status.
//!
//! On a match the whole line's content is wrapped in the level/class color (the
//! line ending is preserved outside the color). On no match, returns `None` so
//! the caller emits the line verbatim. With the plain theme the colors are empty,
//! so a "match" reproduces the input exactly — keeping the path byte-safe.

#[cfg(test)]
use super::theme::Theme;
#[cfg(test)]
use super::StreamingFormatter;
mod code;
mod command_views;
mod common;
mod diagnostics;
mod diskutil;
mod documents;
mod git;
mod json_lines;
mod network;
mod streaming;
mod tables;

pub use code::{colorize_code_line, CodeLanguage};
pub use command_views::{
    colorize_curl_header_line, colorize_curl_progress_line, colorize_df_line, colorize_du_line,
    colorize_find_line, colorize_getfileinfo_line, colorize_history_count_line,
    colorize_history_line, colorize_kubectl_pods_line, colorize_ls_line, colorize_ps_line,
    colorize_whereis_line, colorize_whois_line, colorize_xattr_line,
};
pub(crate) use command_views::{DfColumn, PsColumnRole};
pub(crate) use common::{
    colorize_size_path_line, colorize_words, contains_ascii, paint_bytes, paint_span, paint_whole,
    split_line, trim_ascii, trim_ascii_end, trim_ascii_start, word_spans,
};
pub use diagnostics::colorize_cli_diagnostic_line;
pub(crate) use diagnostics::is_cli_error_line;
pub use diskutil::colorize_diskutil_info_line;
pub use documents::{
    colorize_config_line, colorize_dotenv_line, colorize_gitignore_line,
    colorize_gitleaks_ignore_line, colorize_man_index_line, colorize_markdown_line,
    format_man_line, markdown_fence_language, MarkdownEmbeddedLanguage,
};
pub use git::{colorize_git_line, GitView};
pub use json_lines::{colorize_json_fragment_line, colorize_json_line, is_json_line};
pub use network::{
    colorize_dns_line, colorize_ifconfig_line, colorize_launchctl_line, colorize_lsof_line,
    colorize_netstat_line, colorize_networksetup_line, colorize_ping_line, colorize_pmset_line,
    colorize_route_line, colorize_scutil_dns_line,
};
pub use streaming::{colorize_line, Http, Logs, StackTrace};
pub(crate) use streaming::{is_error_log_line, is_exception_line, ltrim};
pub(crate) use tables::AUTO_DELIMITER;
pub use tables::{
    colorize_delimited_line, colorize_sql_line, colorize_sql_result_line, format_delimited_document,
};
pub(crate) use tables::{find_sql_block_comment_end, lower_ascii, split_unquoted};

#[cfg(test)]
mod tests {
    use super::*;

    /// All streaming formatters enabled, in priority order.
    fn all() -> [&'static dyn StreamingFormatter; 3] {
        [&Http, &Logs, &StackTrace]
    }

    fn colored(line: &[u8]) -> Option<String> {
        colorize_line(line, &Theme::default_colored(), &all())
            .map(|v| String::from_utf8(v).unwrap())
    }

    #[test]
    fn colors_severity_lines_without_case_inconsistency() {
        assert!(colored(b"ERROR: boom\n").unwrap().starts_with("\x1b[31m"));
        assert!(colored(b"[WARN] low disk\n")
            .unwrap()
            .contains("\x1b[38;5;220mWARN\x1b[0m"));
        assert!(colored(b"2026-01-01 INFO started\n")
            .unwrap()
            .contains("\x1b[38;2;39;135;51mINFO\x1b[0m started"));
        assert!(colored(b"DEBUG x=1\n").unwrap().starts_with("\x1b[2m"));
        assert!(colored(b"2026-08-05 09:00:09 Error invalid password\n")
            .unwrap()
            .contains("\x1b[31mError\x1b[0m invalid password"));
        assert!(colored(b"2026-08-05 09:00:15 Exception unavailable\n")
            .unwrap()
            .contains("\x1b[31mException\x1b[0m unavailable"));
        assert!(colored(b"[error] background sync failed\n")
            .unwrap()
            .contains("\x1b[31merror\x1b[0m] background sync failed"));
    }

    #[test]
    fn colors_openssh_authentication_records_by_event_semantics() {
        let failed = colored(
            b"2026-08-07T14:59:12.573694+00:00 server sshd[493736]: Failed password for invalid user centos from 172.182.10.105 port 61497 ssh2\n",
        )
        .unwrap();
        assert!(failed.contains("\x1b[2m2026-08-07T14:59:12.573694+00:00\x1b[0m"));
        assert!(failed.contains("\x1b[38;5;220m493736\x1b[0m"));
        assert!(failed.contains("\x1b[31mFailed password\x1b[0m"));
        assert!(failed.contains("\x1b[38;5;117mcentos\x1b[0m"));
        assert!(failed.contains("\x1b[38;2;142;202;230m172.182.10.105\x1b[0m"));
        assert!(failed.contains("port \x1b[38;5;220m61497\x1b[0m"));

        let accepted = colored(
            b"Aug  7 06:38:50 server sshd[208163]: Accepted publickey for root from 10.10.10.68 port 51501 ssh2\r\n",
        )
        .unwrap();
        assert!(accepted.contains("\x1b[2mAug  7 06:38:50\x1b[0m"));
        assert!(accepted.contains("\x1b[38;2;39;135;51mAccepted publickey\x1b[0m"));
        assert!(accepted.ends_with("ssh2\r\n"));
    }

    #[test]
    fn ssh_auth_formatter_rejects_unstructured_lookalikes() {
        assert!(colored(b"Failed password for a test account\n").is_none());
        assert!(colored(b"2026-08-07T14:59:12Z server app[42]: Failed password\n").is_none());
        assert!(colored(b"server sshd[42]: Accepted password for root\n").is_none());
        assert!(colored(b"2026-08-07T14:59:12Z server sshd[42]: session opened\n").is_none());
    }

    #[test]
    fn preserves_content_and_line_ending() {
        let out = colorize_line(b"ERROR: boom\r\n", &Theme::default_colored(), &all()).unwrap();
        assert_eq!(out, b"\x1b[31mERROR\x1b[0m: boom\r\n");
        let out = colorize_line(b"ERROR: boom\n", &Theme::default_colored(), &all()).unwrap();
        assert_eq!(out, b"\x1b[31mERROR\x1b[0m: boom\n");
        // No trailing newline is fine too.
        let out = colorize_line(b"ERROR: boom", &Theme::default_colored(), &all()).unwrap();
        assert_eq!(out, b"\x1b[31mERROR\x1b[0m: boom");
    }

    #[test]
    fn http_status_lines_color_by_class() {
        assert!(colored(b"HTTP/1.1 200 OK\n")
            .unwrap()
            .starts_with("\x1b[38;2;39;135;51m"));
        assert!(colored(b"HTTP/2 301 Moved\n")
            .unwrap()
            .starts_with("\x1b[2m"));
        assert!(colored(b"HTTP/1.1 404 Not Found\n")
            .unwrap()
            .starts_with("\x1b[38;5;220m"));
        assert!(colored(b"HTTP/1.1 500 Boom\n")
            .unwrap()
            .starts_with("\x1b[31m"));
    }

    #[test]
    fn does_not_color_prose_or_embedded_level_words() {
        // Case variants are supported in a log-level slot, but mid-sentence
        // words must NOT trigger (precision).
        assert!(colored(b"an error occurred while connecting\n").is_none());
        assert!(colored(b"No errors found.\n").is_none());
        // Level beyond the SEVERITY_WINDOW (50-char prefix, then ERROR) is ignored.
        assert!(
            colored(b"this is a long informational sentence with no level... ERROR\n").is_none()
        );
        assert!(colored(b"MYERROR is a variable\n").is_none()); // not delimited
        assert!(colored(b"ERROR_CODE = 5\n").is_none()); // underscore = word char
        assert!(colored(b"plain text line\n").is_none());
        assert!(colored(b"\n").is_none());
    }

    #[test]
    fn does_not_color_non_status_http_like_lines() {
        assert!(colored(b"HTTP/1.1 banana\n").is_none()); // no numeric code
        assert!(colored(b"HTTP/1.1 2000 weird\n").is_none()); // 4-digit
        assert!(colored(b"GET /api HTTP/1.1\n").is_none()); // doesn't start with HTTP/
    }

    #[test]
    fn cli_diagnostics_color_errors_and_usage() {
        let theme = Theme::default_colored();
        assert_eq!(
            colorize_cli_diagnostic_line(b"find: illegal option -- m\n", &theme).unwrap(),
            b"\x1b[31mfind: illegal option -- m\x1b[0m\n"
        );
        assert_eq!(
            colorize_cli_diagnostic_line(b"usage: find path ... [expression]\n", &theme).unwrap(),
            b"\x1b[38;5;220musage: find path ... [expression]\x1b[0m\n"
        );
        assert!(colorize_cli_diagnostic_line(b"this usage is documented here\n", &theme).is_none());
        assert!(
            colorize_cli_diagnostic_line(b"http://example.com: no such page\n", &theme).is_none()
        );
    }

    #[test]
    fn plain_theme_is_byte_identical_on_match() {
        // The whole point that keeps the streaming path byte-safe: with no colors,
        // a "matched" line reproduces the input exactly.
        let line = b"ERROR: boom\r\n";
        assert_eq!(colorize_line(line, &Theme::plain(), &all()).unwrap(), line);
        let auth = b"2026-08-07T14:59:12Z server sshd[42]: Failed password for root from 10.0.0.1 port 22 ssh2\n";
        assert_eq!(colorize_line(auth, &Theme::plain(), &all()).unwrap(), auth);
    }

    #[test]
    fn highlights_stack_traces() {
        // Rust panic header, Python traceback header + frame + exception line.
        assert!(colored(b"thread 'main' panicked at src/main.rs:42:10:\n")
            .unwrap()
            .starts_with("\x1b[31m")); // red
        assert!(colored(b"Traceback (most recent call last):\n")
            .unwrap()
            .starts_with("\x1b[31m"));
        assert!(colored(b"  File \"app.py\", line 10, in <module>\n")
            .unwrap()
            .starts_with("\x1b[2m")); // dim frame
        assert!(colored(b"ValueError: bad input\n")
            .unwrap()
            .starts_with("\x1b[31m"));
        assert!(colored(b"json.decoder.JSONDecodeError: Expecting value\n")
            .unwrap()
            .starts_with("\x1b[31m"));
    }

    #[test]
    fn does_not_mistake_prose_for_a_stack_trace() {
        // Precision: ordinary "Word: ..." lines and paths must NOT be colored.
        assert!(colored(b"Note: see the docs above\n").is_none());
        assert!(colored(b"http://example.com: reachable\n").is_none());
        assert!(colored(b"TODO: refactor this later\n").is_none());
        assert!(colored(b"Summary: 3 passed\n").is_none());
        assert!(colored(b"at the store I bought milk\n").is_none());
        // A lowercase severity at the start is a valid log-level shape.
        assert!(colored(b"error: could not compile\n")
            .unwrap()
            .contains("\x1b[31merror\x1b[0m: could not compile"));
        // Leading/trailing dot tokens are not Python exception identifiers,
        // even though the Logs formatter may independently recognize Error.
        assert!(!is_exception_line(b".Error: leading dot"));
        assert!(!is_exception_line(b"Error.: trailing dot"));
    }

    #[test]
    fn stacktrace_gate_off_disables_it() {
        // With StackTrace not in the registry, a panic header is left alone.
        let without_trace: [&dyn StreamingFormatter; 2] = [&Http, &Logs];
        assert!(colorize_line(
            b"thread 'main' panicked at x:1:1:\n",
            &Theme::default_colored(),
            &without_trace,
        )
        .is_none());
    }

    proptest::proptest! {
        /// Never panics, and with the plain theme any match is byte-identical to
        /// the input (so the streaming path can't corrupt user bytes).
        #[test]
        fn prop_plain_is_identity_and_never_panics(line: Vec<u8>) {
            let fmts: [&dyn StreamingFormatter; 3] = [&Http, &Logs, &StackTrace];
            if let Some(out) = colorize_line(&line, &Theme::plain(), &fmts) {
                proptest::prop_assert_eq!(out, line);
            }
        }
    }
}
