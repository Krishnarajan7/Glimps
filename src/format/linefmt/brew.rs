//! Command-aware coloring for Homebrew's plain listing output.
//!
//! Homebrew paints what it considers important itself whenever stdout is a
//! terminal: `==>` headings, the status cell of `brew services`, `Error:`
//! prefixes. Those escapes reach the terminal untouched — the scanner routes
//! every ESC past the formatters. The views here only touch the lines Homebrew
//! leaves plain: formula and cask listings (`brew list --versions`,
//! `brew outdated`, `brew leaves`, `brew tap`, `brew deps --tree`) and the rows
//! of `brew services list`. Anything sentence-shaped is declined.

use super::super::theme::Theme;
use super::{colorize_spans, paint_whole, split_line, word_spans};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrewView {
    /// `brew services [list]`: service name, status, user, service file.
    Services,
    /// Formula and cask listings: one name per line, optionally followed by
    /// versions (`brew list --versions`, `brew outdated`) or preceded by tree
    /// glyphs (`brew deps --tree`).
    Packages,
}

/// The longest row a listing produces: a name, a few versions, and the
/// comparator of `brew outdated --verbose`, or a nested `deps --tree` line.
/// Prose runs longer.
const MAX_PACKAGE_ROW_WORDS: usize = 8;

/// Color one line of Homebrew output under `view`. Only SGR escapes are
/// inserted; the line's own bytes are never changed, and any line that does
/// not fit the view's shape is declined.
pub fn colorize_brew_line(line: &[u8], theme: &Theme, view: BrewView) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    if content.iter().any(|&byte| is_control_byte(byte)) {
        return None;
    }
    let words = word_spans(content);
    if words.is_empty() {
        return None;
    }
    let word = |index: usize| &content[words[index].0..words[index].1];
    // A section heading (`==> Formulae`). Homebrew colors it itself on a
    // terminal, so this only fires for a heading that arrived plain.
    if word(0) == b"==>" {
        return Some(paint_whole(content, ending, theme.key, theme.reset));
    }
    match view {
        BrewView::Services => colorize_services_row(content, ending, &words, theme),
        BrewView::Packages => {
            if !is_package_row(content, &words) {
                return None;
            }
            Some(colorize_spans(content, ending, &words, theme, |_, word| {
                Some(package_word_color(word, theme))
            }))
        }
    }
}

/// C0 controls other than TAB, plus DEL. A listing never carries them; a line
/// that does is not ours to paint (invariant #3).
fn is_control_byte(byte: u8) -> bool {
    (byte < 0x20 && byte != b'\t') || byte == 0x7f
}

fn colorize_services_row(
    content: &[u8],
    ending: &[u8],
    words: &[(usize, usize)],
    theme: &Theme,
) -> Option<Vec<u8>> {
    let word = |index: usize| &content[words[index].0..words[index].1];
    if words.len() >= 2 && word(0) == b"Name" && word(1) == b"Status" {
        return Some(paint_whole(content, ending, theme.debug, theme.reset));
    }
    if words.len() >= 2 {
        if let Some(status_color) = service_status_color(word(1), theme) {
            return Some(colorize_spans(
                content,
                ending,
                words,
                theme,
                |idx, word| {
                    Some(match idx {
                        0 => theme.key,
                        1 => status_color,
                        _ if is_service_file(word) => theme.path,
                        // `error  256`: the exit status Homebrew prints after a
                        // failed launch.
                        _ if word.iter().all(u8::is_ascii_digit) => theme.number,
                        _ => theme.muted,
                    })
                },
            ));
        }
    }
    // On a terminal Homebrew paints the status cell itself. The escape splits
    // the row, so this view sees only the plain remainder: `<user> <file>`.
    if words.len() == 2 && is_service_file(word(1)) {
        return Some(colorize_spans(content, ending, words, theme, |idx, _| {
            Some(if idx == 0 { theme.muted } else { theme.path })
        }));
    }
    None
}

/// The status vocabulary of `brew services list`. Anything else means the line
/// is not a service row (usage text, a diagnostic, prose) and is declined.
fn service_status_color(status: &[u8], theme: &Theme) -> Option<&'static str> {
    Some(match status {
        b"started" => theme.info,
        b"scheduled" => theme.warn,
        b"stopped" | b"none" => theme.comment,
        b"error" => theme.error,
        b"unknown" | b"other" => theme.warn,
        _ => return None,
    })
}

/// A launchd plist or systemd unit path, the only file `brew services` lists.
fn is_service_file(word: &[u8]) -> bool {
    word.contains(&b'/') && (word.ends_with(b".plist") || word.ends_with(b".service"))
}

/// A listing row is a handful of package tokens. Prose — `You have 13
/// outdated formulae installed.`, `Error: No such keg: …`, the git progress a
/// tap action prints — is declined by its sentence punctuation and length.
fn is_package_row(content: &[u8], words: &[(usize, usize)]) -> bool {
    words.len() <= MAX_PACKAGE_ROW_WORDS
        && words
            .iter()
            .all(|&(start, end)| is_package_token(&content[start..end]))
}

/// A formula or cask name (`apr-util`, `python@3.11`, `user/tap/name`), a
/// version (`1.6.3_1`, `(2026-03-19,`), an installed file (`/opt/…`), a tree
/// glyph (`├──`) or a comparator (`<`, `!=`). Never a word that ends in
/// sentence punctuation or carries quotes or a colon.
fn is_package_token(word: &[u8]) -> bool {
    let bare = strip_version_punctuation(word);
    let Some(&last) = bare.last() else {
        return false;
    };
    if !bare.iter().any(u8::is_ascii_alphanumeric) {
        return bare.iter().all(|&byte| {
            !byte.is_ascii() || matches!(byte, b'<' | b'>' | b'!' | b'=' | b'|' | b'-' | b'`')
        });
    }
    (last.is_ascii_alphanumeric() || last == b'+')
        && bare.iter().all(|&byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'@' | b'.' | b'_' | b'-' | b'+' | b'/' | b'~')
        })
}

fn package_word_color(word: &[u8], theme: &Theme) -> &'static str {
    // Tree glyphs (`├──`, `│`) and comparison tokens (`<`, `!=`) carry no
    // letters or digits.
    if !word.iter().any(u8::is_ascii_alphanumeric) {
        return theme.comment;
    }
    // `brew list <formula>` prints the files it installed.
    if word.starts_with(b"/") || word.starts_with(b"~/") {
        return theme.path;
    }
    if looks_like_version(strip_version_punctuation(word)) {
        return theme.number;
    }
    theme.key
}

/// `brew outdated` wraps the installed version in parentheses and separates
/// several with commas: `(2026-03-19, 2026-05-14)`.
fn strip_version_punctuation(mut word: &[u8]) -> &[u8] {
    while let Some(rest) = word.strip_prefix(b"(") {
        word = rest;
    }
    while let Some(rest) = word.strip_suffix(b")").or_else(|| word.strip_suffix(b",")) {
        word = rest;
    }
    word
}

/// `2.72`, `1.6.3_1`, `20250814.1`, `2026-05-14`, `9.0.1_1`. A bare short
/// number stays a name (`7zip` is a formula; so, in principle, is `2fa`), so a
/// version must either carry a separator or be a long all-digit stamp.
fn looks_like_version(word: &[u8]) -> bool {
    let Some(first) = word.first() else {
        return false;
    };
    if !first.is_ascii_digit() {
        return false;
    }
    if !word.iter().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+' | b'~')
    }) {
        return false;
    }
    word.iter().any(|byte| matches!(byte, b'.' | b'_' | b'-'))
        || (word.len() >= 4 && word.iter().all(u8::is_ascii_digit))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colored(line: &str, view: BrewView) -> String {
        let out = colorize_brew_line(line.as_bytes(), &Theme::default_colored(), view)
            .expect("line is colored");
        String::from_utf8(out).expect("valid utf-8")
    }

    fn declined(line: &[u8], view: BrewView) -> bool {
        colorize_brew_line(line, &Theme::default_colored(), view).is_none()
    }

    #[test]
    fn plain_theme_declines_every_line() {
        for view in [BrewView::Services, BrewView::Packages] {
            assert_eq!(
                colorize_brew_line(b"mysql started krishv ~/x.plist\n", &Theme::plain(), view),
                None
            );
        }
    }

    #[test]
    fn services_rows_key_name_status_user_and_file() {
        let s = colored(
            "mysql         started         krishv ~/Library/LaunchAgents/homebrew.mxcl.mysql.plist\n",
            BrewView::Services,
        );
        assert!(s.starts_with("\x1b[36mmysql\x1b[0m         "));
        assert!(s.contains("\x1b[38;2;39;135;51mstarted\x1b[0m"));
        assert!(s.contains("\x1b[38;5;153mkrishv\x1b[0m"));
        assert!(s.contains(
            "\x1b[38;2;142;202;230m~/Library/LaunchAgents/homebrew.mxcl.mysql.plist\x1b[0m\n"
        ));
    }

    #[test]
    fn services_error_row_paints_status_red_and_exit_code_as_number() {
        let s = colored(
            "unbound error  256 krishv ~/Library/LaunchAgents/homebrew.mxcl.unbound.plist\n",
            BrewView::Services,
        );
        assert!(s.contains("\x1b[31merror\x1b[0m"));
        assert!(s.contains("\x1b[38;5;220m256\x1b[0m"));
    }

    #[test]
    fn services_header_is_dimmed_and_prose_is_declined() {
        assert_eq!(
            colored("Name          Status User   File\n", BrewView::Services),
            "\x1b[2mName          Status User   File\x1b[0m\n"
        );
        for line in [
            &b"Example usage:\n"[..],
            b"  brew services list\n",
            b"Error: Invalid usage: Unknown command: brew service\n",
            b"Did you mean services?\n",
            b"\n",
        ] {
            assert!(
                declined(line, BrewView::Services),
                "{:?} is not a service row",
                String::from_utf8_lossy(line)
            );
        }
    }

    #[test]
    fn services_tail_after_brews_own_status_color_is_still_painted() {
        // What the view actually receives on a terminal, once brew's
        // `\x1b[32mstarted\x1b[0m` has split the row.
        let s = colored(
            " krishv ~/Library/LaunchAgents/homebrew.mxcl.mysql.plist\r\n",
            BrewView::Services,
        );
        assert_eq!(
            s,
            " \x1b[38;5;153mkrishv\x1b[0m \x1b[38;2;142;202;230m~/Library/LaunchAgents/homebrew.mxcl.mysql.plist\x1b[0m\r\n"
        );
        // The `none` rows leave only padding behind, which is not a row.
        assert!(declined(b"           \r\n", BrewView::Services));
    }

    #[test]
    fn packages_paint_names_versions_and_comparators() {
        let s = colored("apr-util (1.6.3_1) < 1.6.5\n", BrewView::Packages);
        assert_eq!(
            s,
            "\x1b[36mapr-util\x1b[0m \x1b[38;5;220m(1.6.3_1)\x1b[0m \x1b[2m<\x1b[0m \x1b[38;5;220m1.6.5\x1b[0m\n"
        );
        let s = colored(
            "ca-certificates (2026-03-19, 2026-05-14) < 2026-08-13\n",
            BrewView::Packages,
        );
        assert!(s.contains("\x1b[38;5;220m(2026-03-19,\x1b[0m \x1b[38;5;220m2026-05-14)\x1b[0m"));
        let s = colored("postman (11.69.2) != 12.27.5\n", BrewView::Packages);
        assert!(s.contains("\x1b[2m!=\x1b[0m"));
    }

    #[test]
    fn packages_keep_digit_led_names_as_names() {
        let s = colored("7zip 25.01\n", BrewView::Packages);
        assert_eq!(s, "\x1b[36m7zip\x1b[0m \x1b[38;5;220m25.01\x1b[0m\n");
        let s = colored("argon2 20190702_1\n", BrewView::Packages);
        assert!(s.contains("\x1b[38;5;220m20190702_1\x1b[0m"));
    }

    #[test]
    fn packages_dim_tree_glyphs_and_paint_installed_files_as_paths() {
        let s = colored("│   └── openssl@3\n", BrewView::Packages);
        assert_eq!(
            s,
            "\x1b[2m│\x1b[0m   \x1b[2m└──\x1b[0m \x1b[36mopenssl@3\x1b[0m\n"
        );
        let s = colored(
            "/opt/homebrew/Cellar/curl/8.16.0/bin/curl\n",
            BrewView::Packages,
        );
        assert!(s.starts_with("\x1b[38;2;142;202;230m/opt/homebrew"));
        let s = colored("krishnarajan7/tap/glimps\n", BrewView::Packages);
        assert!(s.starts_with("\x1b[36mkrishnarajan7/tap/glimps"));
    }

    #[test]
    fn packages_decline_prose_diagnostics_and_control_bytes() {
        for line in [
            &b"You have 13 outdated formulae installed.\n"[..],
            b"Error: No such keg: /opt/homebrew/Cellar/foo\n",
            b"Warning: foo is deprecated\n",
            b"Cloning into '/opt/homebrew/Library/Taps/user/homebrew-tap'...\n",
            b"remote: Enumerating objects: 3512, done.\n",
            b"Tapped 1 formula (12 files, 100KB).\n",
            b"Updating Homebrew...\n",
            b"  brew search TEXT|/REGEX/\n",
            b"Did you mean services?\n",
            b"a b c d e f g h i\n",
            b"abseil\x0120250814.1\n",
            b"abseil 20250814.1\x7f\n",
        ] {
            assert!(
                declined(line, BrewView::Packages),
                "{:?} is not a listing row",
                String::from_utf8_lossy(line)
            );
        }
        // Still a row: bare names, tap slugs, versions, glyphs, comparators.
        for line in [
            "abseil\n",
            "python@3.11 3.11.13_1\n",
            "mongodb/brew\n",
            "├── brotli\n",
            "postman (11.69.2) != 12.27.5\n",
            "libstdc++ 1.0\n",
        ] {
            assert!(
                !declined(line.as_bytes(), BrewView::Packages),
                "{line:?} is a listing row"
            );
        }
    }

    #[test]
    fn plain_heading_is_keyed() {
        assert_eq!(
            colored("==> Formulae\n", BrewView::Packages),
            "\x1b[36m==> Formulae\x1b[0m\n"
        );
    }
}
