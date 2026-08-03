//! Unit, golden, and property tests for the formatter seam.
//!
//! Split out of `mod.rs` to keep the seam's logic readable; as a child module
//! it still has `use super::*` access to the formatter's private items.

use super::*;
use proptest::prelude::*;
use std::io::Write;

const C: &[u8] = b"\x1b]133;C\x07"; // command output start
const A: &[u8] = b"\x1b]133;A\x07"; // prompt start / next command cycle
const D: &[u8] = b"\x1b]133;D\x07"; // command output end
const D0: &[u8] = b"\x1b]133;D;0\x07"; // command output end, success
const D1: &[u8] = b"\x1b]133;D;1\x07"; // command output end, failure

/// The header a fresh Formatter injects when NO command was captured (the dim
/// rule fallback), for the given clock. Tests without a command marker frame
/// output with this.
fn sep_with(clock: Clock) -> Vec<u8> {
    Formatter::with_clock(clock).render_header()
}

/// The default (timestamp-less) separator.
fn sep() -> Vec<u8> {
    sep_with(Clock::Off)
}

/// Convert GLIMPS-generated `\n` to `\r\n`, mirroring what the Formatter does
/// to formatted JSON before it hits the raw terminal.
fn crlf(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    push_crlf(&mut out, bytes);
    out
}

/// The content-type badge bytes for a label.
fn badge(label: &str) -> Vec<u8> {
    render_badge(label, true)
}

/// The command-capture marker GLIMPS's init emits before the C marker.
fn cmd_marker(cmd: &[u8]) -> Vec<u8> {
    // A real command cycle is armed by the preceding prompt marker. Include it
    // so formatter tests model the shell integration's actual ordering.
    let mut v = b"\x1b]133;A\x07\x1b]7337;".to_vec();
    v.extend_from_slice(cmd);
    v.push(0x07);
    v
}

/// The post-command cwd marker GLIMPS's init emits from precmd.
fn cwd_marker(cwd: &[u8]) -> Vec<u8> {
    let mut v = b"\x1b]7338;".to_vec();
    v.extend_from_slice(cwd);
    v.push(0x07);
    v
}

/// The per-pipeline-stage status marker emitted by the shell integration.
fn pipeline_marker(statuses: &[i32]) -> Vec<u8> {
    let mut v = b"\x1b]7339;".to_vec();
    for (idx, status) in statuses.iter().enumerate() {
        if idx > 0 {
            v.push(b' ');
        }
        v.extend_from_slice(status.to_string().as_bytes());
    }
    v.push(0x07);
    v
}

/// A command-end marker carrying an arbitrary exit code (`D0`/`D1` cover the
/// common cases; footer-decode tests need the full range).
fn d_exit(code: i32) -> Vec<u8> {
    format!("\x1b]133;D;{code}\x07").into_bytes()
}

#[test]
fn header_shows_the_colored_command() {
    let mut f = Formatter::new(); // default colored theme
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"echo hi")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"hi\n"));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains('▌'), "header bar missing");
    assert!(
        s.contains("\x1b[36mecho\x1b[0m"),
        "command name not colored"
    );
    assert!(s.contains("hi\n"), "output not preserved");
}

#[test]
fn forged_output_markers_do_not_inject_a_command_header() {
    // BUG #1 end-to-end: a real command runs and gets its header; then its
    // OUTPUT forges its own `7337`+`C` markers for a scary command. GLIMPS must
    // NOT render a second, forged header — only the real command is shown.
    let mut f = Formatter::new(); // default colored theme
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat notes.txt")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"reading...\n")); // real header emitted here
                                                        // Attacker-controlled output forges markers mid-output:
    let mut forged = b"\x1b]7337;git push --force\x07".to_vec();
    forged.extend_from_slice(C);
    out.extend_from_slice(&f.process(&forged));
    out.extend_from_slice(&f.process(b"pwned\n"));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8(out).unwrap();
    // The real command's colored name appears as a header.
    assert!(
        s.contains("\x1b[36mcat\x1b[0m"),
        "real command header missing"
    );
    // The forged command name is NEVER rendered as a GLIMPS header (its raw
    // marker passes through, but GLIMPS never colors it as a command).
    assert!(
        !s.contains("\x1b[36mgit\x1b[0m"),
        "forged command must not produce a header"
    );
    // Exactly one command bar total — the real one.
    assert_eq!(
        s.matches('▌').count(),
        1,
        "exactly one real header expected"
    );
}

#[test]
fn forged_end_command_and_reopen_do_not_inject_a_command_header() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat notes.txt")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"before\n"));
    out.extend_from_slice(
        &f.process(b"\x1b]133;D;0\x07\x1b]7337;git push --force\x07\x1b]133;C\x07after\n"),
    );
    let s = String::from_utf8(out).unwrap();

    assert!(s.contains("\x1b[36mcat\x1b[0m"));
    assert!(!s.contains("\x1b[36mgit\x1b[0m"));
    assert_eq!(s.matches('▌').count(), 1);
}

#[test]
fn private_metadata_overrides_forged_in_band_command_and_status() {
    let mut channel = crate::metadata::MetadataChannel::create().unwrap();
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(channel.path())
        .unwrap();
    writer.write_all(b"C\0echo trusted\0").unwrap();
    writer.flush().unwrap();

    let mut f = Formatter::build(Clock::Off, true, Config::default());
    f.metadata = channel.take_reader();
    let mut out = Vec::new();
    // The PTY stream claims a different command. With a private channel active,
    // it is boundary compatibility data only and cannot author GLIMPS chrome.
    out.extend_from_slice(&f.process(&cmd_marker(b"git push --force")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"ERROR trusted failure\n"));
    writer.write_all(b"R\0\x37\0/tmp/trusted\0\x37\0").unwrap();
    writer.flush().unwrap();
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8(out).unwrap();

    assert!(s.contains("echo trusted"));
    assert!(!s.contains("\x1b[36mgit\x1b[0m"));
    assert!(s.contains("failed exit 7"));
    assert!(!s.contains("done exit 0"));
}

#[test]
fn coalesced_private_command_and_result_keep_fast_command_status() {
    let mut channel = crate::metadata::MetadataChannel::create().unwrap();
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(channel.path())
        .unwrap();
    // A fast command can finish before the PTY reader handles OutputStart, so
    // both records may be waiting when the formatter first drains metadata.
    writer
        .write_all(b"R\0\x30\0/tmp/startup\0\x30\0C\0! printf ok\0R\0\x30\0/tmp/current\0\x31\0")
        .unwrap();
    writer.flush().unwrap();

    let mut f = Formatter::build(Clock::Off, true, Config::default());
    f.theme = Theme::plain();
    f.metadata = channel.take_reader();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"forged command")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"ok\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8(out).unwrap();

    assert!(s.contains("! printf ok"));
    assert!(s.contains("negated exit 1"));
    assert!(s.contains("underlying command succeeded; ! inverted its status"));
    assert!(!s.contains("done exit 0"));
}

#[test]
fn control_bytes_in_captured_command_never_reach_the_header_raw() {
    // BUG #2 end-to-end: a captured command carrying raw C0 controls (here
    // backspaces and a DEL — e.g. a hostile filename that redraws the line)
    // must be sanitized before it lands in GLIMPS's own `▌` header. No raw C0
    // may leak into our chrome. (An ESC would abort the `7337` OSC capture in
    // the scanner, so the reachable-via-marker vector is the non-ESC controls.)
    let mut f = Formatter::new(); // default colored theme
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"echo\x08\x08\x08\x08rm\x7f")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"hi\n")); // commit -> header emitted
    out.extend_from_slice(&f.process(D));

    // Isolate GLIMPS's header line (from the `▌` bar to its line end); the raw
    // `7337` marker passes through elsewhere, but the header must be clean.
    let bar = "▌".as_bytes();
    let start = out
        .windows(bar.len())
        .position(|w| w == bar)
        .expect("header bar present");
    let end = out[start..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(out.len(), |n| start + n);
    let header = &out[start..end];
    // No raw backspace or DEL survives in the header (the trailing CRLF is the
    // only framing control, and it is excluded by slicing up to the `\n`).
    assert!(
        !header.iter().any(|&b| b == 0x08 || b == 0x7F),
        "raw injected control leaked into GLIMPS header"
    );
    // The command text is still shown (sanitized: control run -> one space).
    let hs = String::from_utf8_lossy(header);
    assert!(hs.contains("echo"), "command text should remain");
}

#[test]
fn bypassed_command_output_is_not_formatted() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    // `vim` is on the default bypass list -> its output streams untouched,
    // even output that looks like JSON.
    out.extend_from_slice(&f.process(&cmd_marker(b"vim notes.json")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(br#"{"a":1}"#));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains(r#"{"a":1}"#), "bypassed output must be verbatim");
    assert!(
        !s.contains("\"a\": 1"),
        "bypassed output must NOT be pretty-printed"
    );
}

#[test]
fn no_command_marker_means_no_bypass_and_dash_header() {
    // A shell without `glimps init`'s command marker: even if you run `vim`,
    // GLIMPS can't know the name, so it must NOT bypass and the header falls
    // back to the dim rule. (Here output IS formatted, proving no bypass.)
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C)); // no 7337 marker
    out.extend_from_slice(&f.process(br#"{"a":1}"#));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8(out).unwrap();
    assert!(
        s.contains("\"a\": 1"),
        "without a command, output still formats"
    );
    assert!(
        !s.contains('▌'),
        "no command -> dim-rule header, not a command bar"
    );
}

#[test]
fn alt_screen_does_not_leak_command_into_next_header() {
    // A bypassed TUI whose exit (`133;D`) lands in the alt-screen chunk must
    // not leave its command captured for the NEXT command's header.
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    // vim session: command marker, C, enter alt-screen, exit alt-screen + D
    // arriving while still bypassing.
    let _ = f.process(&cmd_marker(b"vim notes.json"));
    let _ = f.process(C);
    let _ = f.process(b"\x1b[?1049h"); // enter alt screen -> bypass latches
    let _ = f.process(&cat(&[b"\x1b[?1049l", D])); // exit alt screen + output end
                                                   // Next command (no marker): its header must be the dim rule, not "vim …".
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"plain output\n"));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8(out).unwrap();
    assert!(
        !s.contains("vim notes.json"),
        "stale command leaked into next header"
    );
    assert!(s.contains("plain output\n"));
}

#[test]
fn alt_screen_entry_gets_a_tui_boundary_without_touching_redraw_bytes() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();

    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"vim README.md")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"\x1b[?1049hredraw bytes"));

    let s = String::from_utf8_lossy(&out);
    assert!(
        s.contains("vim README.md"),
        "TUI command boundary should be visible in scrollback"
    );
    assert!(s.contains("TUI"), "TUI output should be badged");
    assert!(
        out.ends_with(b"\x1b[?1049hredraw bytes"),
        "alt-screen bytes must pass through untouched"
    );
}

#[test]
fn non_bypassed_command_still_formats_json() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain(); // so the pretty JSON is contiguous (no color codes)
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"curl x")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(br#"{"a":1}"#));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("curl x"), "command header missing");
    assert!(
        s.contains("\"a\": 1"),
        "JSON should still be pretty-printed"
    );
}

/// Drive a sequence of chunks through one (timestamp-less, plain-theme)
/// Formatter and return all emitted bytes concatenated. Plain theme means
/// line coloring adds no bytes, so verbatim assertions stay exact.
fn run(chunks: &[&[u8]]) -> Vec<u8> {
    let mut f = Formatter::new();
    f.theme = Theme::plain();
    let mut out = Vec::new();
    for c in chunks {
        out.extend_from_slice(&f.process(c));
    }
    out
}

/// Drive chunks through one plain-theme Formatter, then flush (PTY EOF).
fn run_flush(chunks: &[&[u8]]) -> Vec<u8> {
    let mut f = Formatter::new();
    let mut out = Vec::new();
    for c in chunks {
        out.extend_from_slice(&f.process(c));
    }
    out.extend_from_slice(&f.flush());
    out
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

#[test]
fn empty_chunk_is_safe() {
    let mut f = Formatter::new();
    assert!(f.process(b"").is_empty());
}

#[test]
fn zone_advances_through_process() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.process(C);
    assert_eq!(f.zone(), Zone::Output);
    f.process(b"some command output\n");
    assert_eq!(f.zone(), Zone::Output);
    f.process(D);
    assert_eq!(f.zone(), Zone::Unknown);
}

#[test]
fn json_output_is_pretty_printed_with_separator_and_crlf() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(br#"{"a":1,"b":[2,3]}"#));
    out.extend_from_slice(&f.process(D)); // command end -> flush + format
    let pretty = crlf(b"{\n  \"a\": 1,\n  \"b\": [2, 3]\n}");
    let expected = cat(&[C, &sep(), &badge("JSON"), &pretty, D]);
    assert_eq!(out, expected);
}

#[test]
fn json_split_across_chunks_is_still_formatted() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    for part in [C, br#"{"a":"#, br#"1}"#, D] {
        out.extend_from_slice(&f.process(part));
    }
    let pretty = crlf(b"{\n  \"a\": 1\n}");
    assert_eq!(out, cat(&[C, &sep(), &badge("JSON"), &pretty, D]));
}

#[test]
fn non_json_output_is_framed_but_unchanged() {
    let body = b"total 12\ndrwxr-xr-x  3 user staff\n";
    let input = cat(&[C, body, D]);
    // The user's bytes are untouched; only a separator is inserted at output start.
    let expected = cat(&[C, &sep(), body, D]);
    assert_eq!(run(&[&input]), expected);
}

#[test]
fn output_that_looks_like_json_but_isnt_passes_through() {
    let body = b"{this is not json}";
    let input = cat(&[C, body, D]);
    assert_eq!(run(&[&input]), cat(&[C, &sep(), body, D]));
}

#[test]
fn angle_bracket_output_that_isnt_html_passes_through() {
    // A `<`-leading run is buffered (the loose sniff trigger), but if it isn't
    // HTML it must be emitted verbatim (only framed by the separator).
    let body = b"<stdin>: not actually html\n";
    let input = cat(&[C, body, D]);
    assert_eq!(run(&[&input]), cat(&[C, &sep(), body, D]));
}

#[test]
fn output_outside_any_command_is_untouched() {
    // No C marker -> zone stays Unknown -> pure pass-through, no separator.
    let stream = br#"{"a":1}"#;
    assert_eq!(run(&[stream]), stream);
}

#[test]
fn prompt_and_input_are_never_formatted() {
    // The prompt/input zones (incl. a `{`-leading typed command) pass through
    // untouched; only the command's OUTPUT ("plain\n") is framed.
    let a = b"\x1b]133;A\x07";
    let b = b"\x1b]133;B\x07";
    let input = cat(&[a, b"{prompt} $ ", b, br#"echo {"x":1}"#, C, b"plain\n", D]);
    let expected = cat(&[
        a,
        b"{prompt} $ ",
        b,
        br#"echo {"x":1}"#,
        C,
        &sep(),
        b"plain\n",
        D,
    ]);
    assert_eq!(run(&[&input]), expected);
}

#[test]
fn no_separator_for_empty_command_output() {
    // A command that produces no output gets no separator at all.
    let input = cat(&[C, D]);
    assert_eq!(run(&[&input]), input);
}

#[test]
fn whitespace_only_output_is_still_framed() {
    // Whitespace counts as output: a command that prints only a blank line is
    // framed (its bytes are preserved verbatim behind the separator). Pins the
    // documented behavior.
    let body = b"\n";
    let input = cat(&[C, body, D]);
    assert_eq!(run(&[&input]), cat(&[C, &sep(), body, D]));
}

#[test]
fn log_and_http_lines_are_colored_streaming() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    // Default (colored) theme; lines arrive in separate chunks (tail -f style).
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"INFO starting up\n"));
    out.extend_from_slice(&f.process(b"ERROR boom\n"));
    out.extend_from_slice(&f.process(b"HTTP/1.1 404 Not Found\n"));
    out.extend_from_slice(&f.process(b"just a plain line\n"));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8(out).unwrap();
    // Separator once, then each recognized line wrapped in its color; the
    // plain line is untouched.
    assert!(s.contains("\x1b[32mINFO starting up\x1b[0m\n")); // green
    assert!(s.contains("\x1b[31mERROR boom\x1b[0m\n")); // red
    assert!(s.contains("\x1b[38;5;220mHTTP/1.1 404 Not Found\x1b[0m\n")); // yellow
    assert!(s.contains("just a plain line\n"));
    assert!(!s.contains("\x1b[31mjust a plain line")); // plain line not colored
}

#[test]
fn log_line_split_across_chunks_is_colored_once_whole() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"ERR")); // partial line, no newline yet
    out.extend_from_slice(&f.process(b"OR boom\n")); // completes the line
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("\x1b[31mERROR boom\x1b[0m\n"));
}

#[test]
fn stalled_no_newline_prompt_is_released_before_command_end() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"ask-user")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"Continue with operation? [yn] "));
    assert!(
        !out.ends_with(b"Continue with operation? [yn] "),
        "the line formatter should initially coalesce a partial line"
    );

    let visible = f.flush_stalled_output();
    assert_eq!(visible, b"Continue with operation? [yn] ");

    // The reply completes the already-visible physical line verbatim. Once its
    // newline arrives, normal line formatting resumes without byte loss.
    out.extend_from_slice(&visible);
    out.extend_from_slice(&f.process(b"n\r\nINFO next line\r\n"));
    out.extend_from_slice(&f.process(D0));
    let rendered = String::from_utf8(out).unwrap();
    assert!(rendered.contains("Continue with operation? [yn] n\r\n"));
    assert!(rendered.contains("INFO next line\r\n"));
}

#[test]
fn stalled_buffer_candidate_declines_formatting_for_prompt_liveness() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"<choose a value> "));
    out.extend_from_slice(&f.flush_stalled_output());
    out.extend_from_slice(&f.process(b"yes\r\n"));
    out.extend_from_slice(&f.process(D));
    assert_eq!(out, cat(&[C, &sep(), b"<choose a value> yes\r\n", D]));
}

#[test]
fn http_status_split_across_chunks_is_colored_whole() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"HTTP/1.1 4")); // code split mid-number
    out.extend_from_slice(&f.process(b"04 Not Found\n"));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("\x1b[38;5;220mHTTP/1.1 404 Not Found\x1b[0m\n"));
}

#[test]
fn crlf_log_line_is_colored_with_ending_preserved() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"ERROR x\r\n")); // CRLF, as from a real PTY
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("\x1b[31mERROR x\x1b[0m\r\n")); // reset before \r\n
}

#[test]
fn unterminated_final_line_flushes_verbatim_uncolored() {
    // A colorable-looking line with no trailing newline at EOF is flushed
    // verbatim (we only color complete lines) — and no bytes are lost.
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"ERROR boom")); // no newline
    out.extend_from_slice(&f.flush()); // EOF
    let s = String::from_utf8(out).unwrap();
    assert!(s.ends_with("ERROR boom"));
    assert!(!s.contains("\x1b[31m")); // not colored (partial line)
}

#[test]
fn very_long_line_streams_verbatim_without_coloring() {
    // A line longer than LINE_CAP with no newline overflows the line buffer:
    // it must be streamed verbatim (no coloring, no byte loss).
    let mut body = b"ERROR ".to_vec(); // would match if it were a complete line
    body.extend(std::iter::repeat_n(
        b'x',
        Config::default().limits.line_cap + 16,
    ));
    let input = cat(&[C, &body]); // no D; overflow forces verbatim, then EOF
    assert_eq!(run_flush(&[&input]), cat(&[C, &sep(), &body]));
}

#[test]
fn binary_output_is_passed_through_without_a_separator() {
    // Output beginning with a NUL byte is binary: no separator, no buffering,
    // streamed exactly (invariant #3: never reformat binary).
    let body = b"\x7fELF\x00\x01\x02\x00\x00rest";
    let input = cat(&[C, body, D]);
    assert_eq!(run(&[&input]), input);
}

#[test]
fn whitespace_then_binary_gets_no_separator() {
    // Output that begins with whitespace and THEN reveals a NUL is still
    // binary: because the separator is deferred until a text commit, it is
    // correctly suppressed (not injected ahead of the binary).
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"  ")); // whitespace (undecided)
    out.extend_from_slice(&f.process(b"\x00\x01bin")); // NUL -> binary
    out.extend_from_slice(&f.process(D));
    assert_eq!(out, cat(&[C, b"  ", b"\x00\x01bin", D]));
}

#[test]
fn http_response_is_structured_with_body_formatting() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let body = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: sid=1\r\n\r\n{\"ok\":true}";
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body));
    out.extend_from_slice(&f.process(D));
    assert_eq!(
        out,
        cat(&[
            C,
            &sep(),
            &badge("HTTP"),
            &crlf(
                b"HTTP/1.1 200 OK\nContent-Type: application/json\nSet-Cookie: sid=1\n\nJSON body\n{\n  \"ok\": true\n}"
            ),
            D,
        ])
    );
}

#[test]
fn successful_silent_cd_gets_a_moved_breadcrumb() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cd docs")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(&cwd_marker(b"/Users/apple/Projects/Glimps/docs")));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("cd docs"));
    assert!(!s.contains("[CD]"));
    assert!(!s.contains("\x1b[7m CD \x1b[0m"));
    assert!(s.contains("moved to "));
    assert!(s.contains("/Users/apple/Projects/Glimps/docs"));
    assert!(s.contains(
        "\x1b[38;2;5;130;202mmoved to \x1b[38;2;142;202;230m/Users/apple/Projects/Glimps/docs\x1b[0m"
    ));
    assert!(!s.contains("done exit 0"));
}

#[test]
fn successful_pwd_gets_working_directory_without_done_footer() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"pwd")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"/Users/apple/Projects/Glimps\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("pwd"));
    assert!(s.contains("working directory "));
    assert!(s.contains("/Users/apple/Projects/Glimps"));
    assert!(
        s.contains(
            "\x1b[38;2;5;130;202mworking directory \x1b[38;2;142;202;230m/Users/apple/Projects/Glimps\x1b[0m"
        )
    );
    assert!(!s.contains("done exit 0"));
}

#[test]
fn failed_pwd_still_gets_failure_footer() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"pwd")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("failed exit 1"));
    assert!(s.contains("command failed: pwd"));
}

#[test]
fn successful_touch_gets_a_file_breadcrumb_without_done_footer() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"touch 'hello world.txt'")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("touch 'hello world.txt'"));
    assert!(s.contains("touch completed for "));
    assert!(s.contains("hello world.txt"));
    assert!(s.contains(
        "\x1b[38;2;5;130;202mtouch completed for \x1b[38;2;142;202;230mhello world.txt\x1b[0m"
    ));
    assert!(!s.contains("done exit 0"));
}

#[test]
fn successful_mkdir_gets_a_folder_breadcrumb_without_done_footer() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"mkdir -p logs/cache tmp/out")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(
        s.contains("mkdir completed for 2 targets: "),
        "captured: {s:?}"
    );
    assert!(s.contains("logs/cache, tmp/out"));
    assert!(s.contains(
        "\x1b[38;2;5;130;202mmkdir completed for 2 targets: \x1b[38;2;142;202;230mlogs/cache, tmp/out\x1b[0m"
    ));
    assert!(!s.contains("done exit 0"));
}

#[test]
fn successful_rm_gets_a_conservative_target_breadcrumb_without_done_footer() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"rm -rf target/cache old.log")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(
        s.contains("rm completed for 2 targets: "),
        "captured: {s:?}"
    );
    assert!(s.contains("target/cache, old.log"));
    assert!(s.contains(
        "\x1b[38;2;5;130;202mrm completed for 2 targets: \x1b[38;2;142;202;230mtarget/cache, old.log\x1b[0m"
    ));
    assert!(!s.contains("done exit 0"));
}

#[test]
fn successful_killall_gets_a_conservative_process_breadcrumb() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"killall Finder")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("killall Finder"));
    assert!(
        s.contains("\x1b[38;2;5;130;202mkillall completed for \x1b[38;2;142;202;230mFinder\x1b[0m")
    );
    assert!(!s.contains("done exit 0"));
}

#[test]
fn killall_failure_and_flagged_forms_do_not_claim_completion() {
    let mut failed = Formatter::new();
    if !failed.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&failed.process(&cmd_marker(b"killall MissingProcess")));
    out.extend_from_slice(&failed.process(C));
    out.extend_from_slice(&failed.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(!s.contains("killall completed for"));
    assert!(s.contains("failed exit 1"));

    let mut flagged = Formatter::new();
    let mut out = Vec::new();
    out.extend_from_slice(&flagged.process(&cmd_marker(b"killall -u krish Finder")));
    out.extend_from_slice(&flagged.process(C));
    out.extend_from_slice(&flagged.process(D0));
    assert!(!String::from_utf8_lossy(&out).contains("killall completed for"));
}

#[test]
fn failed_rm_keeps_failure_footer_and_no_success_breadcrumb() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"rm missing.txt")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(!s.contains("rm completed for missing.txt"));
    assert!(s.contains("failed exit 1"));
    assert!(s.contains("command failed: rm missing.txt"));
}

#[test]
fn compound_file_commands_do_not_get_guessed_breadcrumbs() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"touch a && rm b")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(!s.contains("touch completed for"));
    assert!(!s.contains("rm completed for"));
    assert!(!s.contains("done exit 0"));
}

#[test]
fn successful_noop_prone_file_commands_make_no_state_change_claim() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();

    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"rm -f missing.txt")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(D0));
    out.extend_from_slice(&f.process(&cmd_marker(b"mkdir -p existing-dir")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("rm completed for missing.txt"));
    assert!(s.contains("mkdir completed for existing-dir"));
    assert!(!s.contains("removed target"));
    assert!(!s.contains("created folder"));
}

#[test]
fn file_command_name_used_as_an_argument_gets_no_breadcrumb() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"printf rm target.txt")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(!s.contains("rm completed for"));
}

#[test]
fn find_output_gets_path_coloring_without_text_changes() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"find src -name '*.rs'")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"src/format/html.rs\nsrc/main.rs\n"));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("src/format"));
    assert!(s.contains("src"));
    assert!(s.contains("\x1b[36mhtml.rs\x1b[0m"));
    assert!(s.contains("\x1b[36mmain.rs\x1b[0m"));
}

#[test]
fn command_diagnostics_override_find_path_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"find -maxdepth 1 -type f | wc -l")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"find: illegal option -- m\n"));
    out.extend_from_slice(
        &f.process(b"usage: find [-H | -L | -P] [-EXdsx] [-f path] path ... [expression]\n"),
    );
    out.extend_from_slice(&f.process(b"0\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[31mfind: illegal option -- m\x1b[0m\n"));
    assert!(s.contains("\x1b[38;5;220musage: find "));
    assert!(s.contains("\n\x1b[36m0\x1b[0m\n"));
}

/// macOS reports TCC-protected directories (~/Desktop, ~/Documents, iCloud)
/// as EPERM, not EACCES. The diagnostic pass must claim the line first — the
/// `find` path view would otherwise paint it in the filename color, making a
/// hard failure read like a result.
#[test]
fn find_eperm_diagnostic_is_not_colored_like_a_path() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"find . -name '*.mov'")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"find: .: Operation not permitted\n"));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(
        s.contains("\x1b[31mfind: .: Operation not permitted\x1b[0m\n"),
        "EPERM line not painted as an error: {s:?}"
    );
    assert!(
        !s.contains("\x1b[36mfind: .: Operation not permitted"),
        "EPERM line painted in the filename color: {s:?}"
    );
}

/// ENOTDIR / EISDIR are the other two errno strings a user meets constantly
/// (`cd` onto a file, `cat` onto a directory). Both carry a real path, so the
/// `find` view's parent/leaf split would happily color them as results.
#[test]
fn directory_errno_diagnostics_are_colored_as_errors() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"find src/main.rs -name '*.rs'")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"find: src/main.rs: Not a directory\n"));
    out.extend_from_slice(&f.process(b"cat: src/format: Is a directory\n"));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(
        s.contains("\x1b[31mfind: src/main.rs: Not a directory\x1b[0m\n"),
        "ENOTDIR line not painted as an error: {s:?}"
    );
    assert!(
        s.contains("\x1b[31mcat: src/format: Is a directory\x1b[0m\n"),
        "EISDIR line not painted as an error: {s:?}"
    );
}

/// The new fragments must not turn prose red. The `tool:` slot is what makes a
/// line a diagnostic; without it the wording alone proves nothing, and a false
/// positive mangles real output (invariant #2).
#[test]
fn errno_wording_without_a_tool_prefix_is_left_alone() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat notes.txt")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"The last attempt failed: Operation not permitted\n"));
    out.extend_from_slice(&f.process(b"note that /etc is a directory, not a file\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(
        !s.contains("\x1b[31mThe last attempt failed"),
        "prose painted as a diagnostic: {s:?}"
    );
    assert!(
        !s.contains("\x1b[31mnote that /etc"),
        "prose painted as a diagnostic: {s:?}"
    );
}

/// `is_cli_error_line` also gates pin candidate selection, so the EPERM miss
/// cost the failure footer its pinned error too — invisible in a two-line
/// failure, but the whole point of the pin when the error scrolled away.
#[test]
fn eperm_error_is_pinned_when_it_scrolled_away() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"find . -name '*.mov'")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"find: ./Library: Operation not permitted\n"));
    out.extend_from_slice(&f.process(b"./clip-one.mov\n"));
    out.extend_from_slice(&f.process(b"./clip-two.mov\n"));
    out.extend_from_slice(&f.process(b"./clip-three.mov\n"));
    out.extend_from_slice(&f.process(b"./clip-four.mov\n"));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(
        s.contains("\u{21b3} find: ./Library: Operation not permitted"),
        "EPERM error was not pinned: {s:?}"
    );
    assert!(
        s.contains("(\u{2191} 4 lines up)"),
        "wrong pin distance: {s:?}"
    );
}

#[test]
fn pipeline_stage_failure_warns_even_when_final_exit_is_zero() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"find -maxdepth 1 -type f | wc -l")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"find: illegal option -- m\n"));
    out.extend_from_slice(
        &f.process(b"usage: find [-H | -L | -P] [-EXdsx] [-f path] path ... [expression]\n"),
    );
    out.extend_from_slice(&f.process(b"0\n"));
    out.extend_from_slice(&f.process(&pipeline_marker(&[1, 0])));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("pipeline stage failed: stage 1 exit 1; final exit 0 in "));
    assert!(
        !s.contains('\u{21b3}'),
        "an error only two lines above the warning should not be repeated: {s:?}"
    );
    assert!(s.contains("find: illegal option -- m"));
    assert!(!s.contains("done exit 0"));
    assert!(!s.contains("command failed: find -maxdepth"));
}

#[test]
fn pipeline_status_does_not_replace_real_nonzero_failure() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"printf ok | false")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(&pipeline_marker(&[0, 1])));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("failed exit 1 in "));
    assert!(s.contains("command failed: printf ok | false"));
    assert!(!s.contains("pipeline stage failed"));
}

#[test]
fn successful_negated_command_is_a_notice_not_a_failure() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"! ssh-copy-id root@example")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"Permission denied, please try again.\n"));
    out.extend_from_slice(&f.process(b"Number of key(s) added: 1\n"));
    out.extend_from_slice(&f.process(&pipeline_marker(&[0])));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\u{2298} negated exit 1 in "));
    assert!(s.contains("underlying command succeeded; ! inverted its status"));
    assert!(!s.contains("\u{2717} failed exit"));
    assert!(!s.contains("command failed:"));
    assert!(!s.contains("\u{21b3} Permission denied"));
}

#[test]
fn leading_negation_does_not_hide_a_later_real_failure() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"! true; false")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(&pipeline_marker(&[1])));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\u{2717} failed exit 1 in "));
    assert!(s.contains("command failed: ! true; false"));
    assert!(!s.contains("underlying command succeeded"));
}

#[test]
fn command_status_footer_shows_exit_and_duration_after_output() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"echo hi")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"hi\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("echo hi"));
    assert!(s.contains("hi\n"));
    assert!(s.contains("done exit 0 in "));
}

#[test]
fn silent_nonzero_command_gets_failure_summary() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"false")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("false"));
    assert!(s.contains("failed exit 1 in "));
    assert!(s.contains("command failed: false"));
}

#[test]
fn exit_137_footer_decodes_sigkill() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"docker build -t api .")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"Step 1/9 : FROM rust\n"));
    out.extend_from_slice(&f.process(&d_exit(137)));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\u{2717} killed exit 137 in "));
    assert!(s.contains("\u{2014} SIGKILL: force-killed, often out of memory"));
    assert!(s.contains("command failed: docker build -t api ."));
}

#[test]
fn exit_127_footer_explains_command_not_found() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"gti status")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"zsh: command not found: gti\n"));
    out.extend_from_slice(&f.process(&d_exit(127)));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\u{2717} failed exit 127 in "));
    assert!(s.contains("\u{2014} command not found on PATH"));
}

#[test]
fn ctrl_c_footer_is_a_neutral_notice_never_red() {
    // The alarm-fatigue rule: a deliberate Ctrl-C must not be styled like a
    // failure, or the red footer stops meaning anything.
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"sleep 100")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"^C\n"));
    out.extend_from_slice(&f.process(&d_exit(130)));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\u{2298} interrupted exit 130 in "));
    assert!(s.contains("Ctrl-C, not an error"));
    // Dim (theme.debug), not red (theme.error), and no failure recap line.
    assert!(s.contains("\x1b[2m\u{2298} interrupted"));
    assert!(!s.contains("\x1b[31m"));
    assert!(!s.contains("command failed"));
}

#[test]
fn sigterm_footer_is_a_notice_without_failure_recap() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"npm run dev")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"listening on :3000\n"));
    out.extend_from_slice(&f.process(&d_exit(143)));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\u{2298} terminated exit 143 in "));
    assert!(s.contains("SIGTERM: asked to stop"));
    assert!(!s.contains("command failed"));
}

#[test]
fn config_explain_off_keeps_raw_exit_codes() {
    let cfg = Config {
        failures: crate::config::Failures {
            explain: false,
            ..crate::config::Failures::default()
        },
        ..Config::default()
    };
    let mut f = fmt_with(cfg);
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"docker build .")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"Step 1/9\n"));
    out.extend_from_slice(&f.process(&d_exit(137)));
    let s = String::from_utf8_lossy(&out);
    // The class verb survives (it is styling, not a story) …
    assert!(s.contains("\u{2717} killed exit 137 in "));
    // … but the decode text is gone.
    assert!(!s.contains("SIGKILL"));
    assert!(!s.contains("out of memory"));
}

#[test]
fn config_failures_disabled_suppresses_footer_but_not_breadcrumbs() {
    let cfg = Config {
        failures: crate::config::Failures {
            enabled: false,
            ..crate::config::Failures::default()
        },
        ..Config::default()
    };
    // No footer, even for a hard failure.
    let mut f = fmt_with(cfg.clone());
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"false")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(&d_exit(1)));
    let s = String::from_utf8_lossy(&out);
    assert!(!s.contains("failed exit"));
    assert!(!s.contains("command failed"));
    // Silent-cd breadcrumbs are separator chrome, not failure intelligence —
    // they survive the failures switch.
    let mut f = fmt_with(cfg);
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cd docs")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(&cwd_marker(b"/Users/apple/Projects/Glimps/docs")));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("moved to "));
}

#[test]
fn failed_colored_cargo_build_pins_the_error_line() {
    // The flagship: rustc errors are COLORED under a PTY, so their bytes
    // travel as Pass segments. The pin must still assemble, strip, and
    // quote them — with the `-->` location attached and a distance hint.
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cargo build")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"   Compiling glimps v0.0.1\n"));
    out.extend_from_slice(
        &f.process(b"\x1b[1m\x1b[31merror[E0308]\x1b[0m\x1b[1m: mismatched types\x1b[0m\n"),
    );
    out.extend_from_slice(&f.process(b"\x1b[1m\x1b[34m  --> \x1b[0msrc/pty.rs:214:18\n"));
    out.extend_from_slice(&f.process(b"   |\n214 |     let n: usize = read_result;\n   |\n"));
    out.extend_from_slice(&f.process(b"error: could not compile `glimps` due to 1 error\n"));
    out.extend_from_slice(&f.process(&d_exit(101)));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\u{2717} failed exit 101 in "));
    assert!(
        s.contains("\u{21b3} error[E0308]: mismatched types \u{2192} src/pty.rs:214:18"),
        "pin line missing or wrong: {s:?}"
    );
    assert!(s.contains("(\u{2191} 5 lines up)"));
    assert!(s.contains("command failed: cargo build"));
}

#[test]
fn failed_python_script_does_not_repeat_the_visible_final_exception() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"python app.py")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"Traceback (most recent call last):\n"));
    out.extend_from_slice(&f.process(b"  File \"app.py\", line 7, in <module>\n"));
    out.extend_from_slice(&f.process(b"ValueError: broken config\n"));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("ValueError: broken config"));
    assert!(
        !s.contains('\u{21b3}'),
        "the final exception is already visible directly above the footer: {s:?}"
    );
}

#[test]
fn nearby_error_is_never_repeated_for_any_exit_class() {
    // The same ERROR line is directly above the footer in every case. Even a
    // real failure should not quote information that is already in view.
    for marker in [d_exit(1), d_exit(0), d_exit(130)] {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        f.theme = Theme::plain();
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(&cmd_marker(b"./job.sh")));
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(b"ERROR connection reset by peer\n"));
        out.extend_from_slice(&f.process(&marker));
        let s = String::from_utf8_lossy(&out);
        assert!(
            !s.contains('\u{21b3}'),
            "nearby error was needlessly repeated for marker {marker:?}: {s:?}"
        );
    }
}

#[test]
fn config_pin_errors_off_keeps_footer_but_drops_quote() {
    let cfg = Config {
        failures: crate::config::Failures {
            pin_errors: false,
            ..crate::config::Failures::default()
        },
        ..Config::default()
    };
    let mut f = fmt_with(cfg);
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"./job.sh")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"ERROR boom\n"));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("failed exit 1 in "));
    assert!(!s.contains('\u{21b3}'));
}

#[test]
fn bypassed_command_is_never_pinned() {
    // ssh is on the default bypass list: minimal chrome, no quoting of
    // remote output — even when it fails.
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"ssh host")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"error: remote thing broke\n"));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(!s.contains('\u{21b3}'), "bypass must not pin: {s:?}");
}

#[test]
fn binary_output_is_never_pinned() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat blob.bin")));
    out.extend_from_slice(&f.process(C));
    // Binary from the first bytes; an "error:" string embedded in it must
    // not surface in the footer.
    out.extend_from_slice(&f.process(b"\x00\x01\x02error: fake\n\x03\x04\n"));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(!s.contains('\u{21b3}'), "binary must not pin: {s:?}");
}

#[test]
fn pinned_line_is_truncated_on_a_char_boundary() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    // A confidently-matched error line far longer than the display cap,
    // ending in multibyte chars right around the cut.
    let mut long = b"error: ".to_vec();
    long.extend_from_slice("é".repeat(200).as_bytes());
    long.push(b'\n');
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"make")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(&long));
    out.extend_from_slice(&f.process(b"context one\ncontext two\ncontext three\n"));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    let pin_line = s
        .lines()
        .find(|l| l.contains('\u{21b3}'))
        .expect("pin line present");
    assert!(
        pin_line.contains("\u{2026}  (\u{2191} 3 lines up)"),
        "truncated with … before the distance hint: {pin_line:?}"
    );
    assert!(
        !pin_line.contains('\u{fffd}'),
        "no split chars: {pin_line:?}"
    );
}

#[test]
fn config_on_success_off_silences_done_but_not_failures() {
    let cfg = Config {
        failures: crate::config::Failures {
            on_success: crate::config::SuccessFooter::Off,
            ..crate::config::Failures::default()
        },
        ..Config::default()
    };
    let mut f = fmt_with(cfg.clone());
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"echo hi")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"hi\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("hi\n"));
    assert!(!s.contains("done exit 0"));
    // Failures stay loud regardless.
    let mut f = fmt_with(cfg);
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"false")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("failed exit 1 in "));
}

#[test]
fn cat_markdown_gets_project_doc_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat README.md")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        b"# GLIMPS\n- use `cat README.md`, `git status`, and **safe pass-through**.\nSee [`docs/SAFETY_INVARIANTS.md`](./docs/SAFETY_INVARIANTS.md) and [`ROADMAP.md`](./ROADMAP.md).\n```bash\nGLIMPS=0 zsh     # start a raw shell\n```\n",
    ));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[36m# GLIMPS\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m- \x1b[0muse \x1b[35m`cat README.md`\x1b[0m, \x1b[35m`git status`\x1b[0m, and \x1b[38;5;117m**safe pass-through**\x1b[0m."));
    assert!(s.contains("\x1b[38;5;117m[`docs/SAFETY_INVARIANTS.md`]\x1b[0m\x1b[2m(./docs/SAFETY_INVARIANTS.md)\x1b[0m and \x1b[38;5;117m[`ROADMAP.md`]\x1b[0m\x1b[2m(./ROADMAP.md)\x1b[0m."));
    assert!(s.contains("\x1b[35m```bash\x1b[0m"));
    assert!(s.contains("GLIMPS"));
    assert!(s.contains("zsh"));
    assert!(s.contains("\x1b[2m# start a raw shell\x1b[0m"));
}

#[test]
fn nl_markdown_colors_the_gutter_and_reuses_the_document_formatter() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let body = concat!(
        "     1\t<p align=\"center\">\n",
        "     2\t**Readable terminal output.**\n",
        "      \t\n",
        "     3\t```bash\n",
        "     4\tgit status --short\n",
        "     5\t```\n",
    );
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"nl README.md")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body.as_bytes()));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("     \x1b[2m1\x1b[0m\x1b[2m\t\x1b[0m"));
    assert!(s.contains("\x1b[38;2;224;82;125mp\x1b[0m"));
    assert!(s.contains("\x1b[38;5;117m**Readable terminal output.**\x1b[0m"));
    assert!(s.contains("\x1b[35m```bash\x1b[0m"));
    assert!(s.contains("\x1b[36mgit\x1b[0m status \x1b[38;5;220m--short\x1b[0m"));
    assert!(strip_sgr(&out)
        .windows(body.len())
        .any(|window| window == body.as_bytes()));
}

#[test]
fn nl_flags_and_safe_pipelines_keep_numbered_code_semantics() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let body = b"001 | pub fn main() {\n002 |     let answer = 42;\n";
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(
        b"nl -ba -w3 -nrz -s ' | ' src/main.rs | head -2",
    )));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("\x1b[2m001\x1b[0m\x1b[2m | \x1b[0m"));
    assert!(s.contains("\x1b[35mpub\x1b[0m \x1b[35mfn\x1b[0m \x1b[36mmain\x1b[0m"));
    assert!(s.contains("\x1b[35mlet\x1b[0m answer \x1b[2m=\x1b[0m \x1b[38;5;220m42\x1b[0m"));
    assert!(strip_sgr(&out)
        .windows(body.len())
        .any(|window| window == body));

    let mut left_aligned = Formatter::new();
    let left_body = b"7   ::# Heading\n";
    let mut left_out = Vec::new();
    left_out.extend_from_slice(&left_aligned.process(&cmd_marker(
        b"nl -ba -w 4 -n ln -s :: -v 7 -i 2 -l 1 README.md",
    )));
    left_out.extend_from_slice(&left_aligned.process(C));
    left_out.extend_from_slice(&left_aligned.process(left_body));
    left_out.extend_from_slice(&left_aligned.process(D0));
    let left_text = String::from_utf8_lossy(&left_out);
    assert!(left_text.contains("\x1b[2m7\x1b[0m   \x1b[2m::\x1b[0m"));
    assert!(left_text.contains("\x1b[36m# Heading\x1b[0m"));
    assert!(strip_sgr(&left_out)
        .windows(left_body.len())
        .any(|window| window == left_body));
}

#[test]
fn nl_without_a_known_file_only_styles_its_metadata_gutter() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let body = b"  7::plain stdin text\n";
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"nl --number-width=3 --separator='::' -")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("  \x1b[2m7\x1b[0m\x1b[2m::\x1b[0mplain stdin text"));
    assert!(strip_sgr(&out)
        .windows(body.len())
        .any(|window| window == body));
}

#[test]
fn nl_known_json_and_html_files_keep_content_aware_coloring() {
    let mut json = Formatter::new();
    if !json.is_enabled() {
        return;
    }
    let json_body = b" 1\t{\n 2\t  \"status\": \"error\",\n 3\t  \"retry\": false\n 4\t}\n";
    let mut json_out = Vec::new();
    json_out.extend_from_slice(&json.process(&cmd_marker(b"nl -w2 response.json")));
    json_out.extend_from_slice(&json.process(C));
    json_out.extend_from_slice(&json.process(json_body));
    json_out.extend_from_slice(&json.process(D0));
    let json_text = String::from_utf8_lossy(&json_out);
    assert!(json_text.contains("\x1b[36m\"status\"\x1b[0m"));
    assert!(json_text.contains("\x1b[38;5;117m\"error\"\x1b[0m"));
    assert!(json_text.contains("\x1b[35mfalse\x1b[0m"));
    assert!(strip_sgr(&json_out)
        .windows(json_body.len())
        .any(|window| window == json_body));

    let mut html = Formatter::new();
    let html_body = b" 1\t<div class=\"notice\">Ready</div>\n";
    let mut html_out = Vec::new();
    html_out.extend_from_slice(&html.process(&cmd_marker(b"nl -w2 page.html")));
    html_out.extend_from_slice(&html.process(C));
    html_out.extend_from_slice(&html.process(html_body));
    html_out.extend_from_slice(&html.process(D0));
    let html_text = String::from_utf8_lossy(&html_out);
    assert!(html_text.contains("\x1b[38;2;224;82;125mdiv\x1b[0m"));
    assert!(html_text.contains("\x1b[38;5;220mclass\x1b[0m"));
    assert!(strip_sgr(&html_out)
        .windows(html_body.len())
        .any(|window| window == html_body));
}

#[test]
fn more_formats_complete_file_lines_but_keeps_pager_prompts_live() {
    let cfg = Config::default();
    assert_eq!(
        command::classify(b"more README.md", &cfg.bypass, &cfg.sensitive_commands).trust,
        CommandTrust::PagerText
    );
    assert_eq!(
        command::classify(b"more .env", &cfg.bypass, &cfg.sensitive_commands).trust,
        CommandTrust::SensitiveText
    );
    assert_eq!(
        command::classify(b"more id_ed25519", &cfg.bypass, &cfg.sensitive_commands).trust,
        CommandTrust::Sensitive
    );

    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"more README.md")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"<p align=\"center\">\n**Readable output**\n"));
    let prompt = b"--More--(10%)";
    assert!(
        f.process(prompt).is_empty(),
        "partial prompt should await the liveness flush"
    );
    assert_eq!(f.flush_stalled_output(), prompt);

    let prompt_erase_and_line = b"\r             \rfirst line after prompt\n";
    assert_eq!(
        f.process(prompt_erase_and_line).as_ref(),
        prompt_erase_and_line
    );
    out.extend_from_slice(&f.process(b"# Formatting resumes\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[38;2;224;82;125mp\x1b[0m"));
    assert!(s.contains("\x1b[38;5;117m**Readable output**\x1b[0m"));
    assert!(s.contains("\x1b[36m# Formatting resumes\x1b[0m"));
}

#[test]
fn an_explicit_more_bypass_still_wins_over_pager_formatting() {
    let cfg = Config::default();
    assert_eq!(
        command::classify(
            b"more README.md",
            &["more".to_string()],
            &cfg.sensitive_commands,
        )
        .trust,
        CommandTrust::InteractiveBypass
    );
}

#[test]
fn markdown_composes_raw_html_and_shell_formatters() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let body = concat!(
        "<p align=\"center\">\n",
        "  <a href=\"https://glimps.dev\">Website</a>\n",
        "</p>\n",
        "Inline <kbd>Ctrl-C</kbd> stays Markdown text.\n",
        "```bash\n",
        "GLIMPS=0 zsh # raw shell\n",
        "git --no-pager diff -- README.md\n",
        "```\n",
    );
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat README.md")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body.as_bytes()));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("\x1b[2m<\x1b[0m\x1b[38;2;224;82;125mp\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220malign\x1b[0m\x1b[2m=\x1b[0m\x1b[38;5;117m\"center\"\x1b[0m"));
    assert!(s.contains("Inline \x1b[2m<\x1b[0m\x1b[38;2;224;82;125mkbd\x1b[0m"));
    assert!(
        s.contains("\x1b[35mGLIMPS\x1b[0m\x1b[2m=\x1b[0m\x1b[38;5;117m0\x1b[0m \x1b[36mzsh\x1b[0m")
    );
    assert!(
        s.contains(
            "\x1b[36mgit\x1b[0m \x1b[38;5;220m--no-pager\x1b[0m diff \x1b[38;5;220m--\x1b[0m README.md"
        ),
        "unexpected shell fence output: {s:?}"
    );
    assert!(!s.contains("\x1b[38;5;220mREADME\x1b[0m"));
    assert!(strip_sgr(&out)
        .windows(body.len())
        .any(|window| window == body.as_bytes()));
}

#[test]
fn markdown_tracks_html_and_unknown_fences_without_markdown_leakage() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let body = concat!(
        "```html\n",
        "<strong class=\"signal\">Ready</strong>\n",
        "```\n",
        "```unknown\n",
        "**not Markdown emphasis inside code**\n",
        "```\n",
        "**Markdown again**\n",
    );
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat README.md")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body.as_bytes()));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("\x1b[38;2;224;82;125mstrong\x1b[0m"));
    assert!(!s.contains("\x1b[38;5;117m**not Markdown emphasis inside code**\x1b[0m"));
    assert!(s.contains("\x1b[38;5;117m**Markdown again**\x1b[0m"));
    assert!(strip_sgr(&out)
        .windows(body.len())
        .any(|window| window == body.as_bytes()));
}

#[test]
fn cat_config_gets_key_value_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat Cargo.toml")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"[package]\nname = \"glimps\"\nversion = 1\n# comment\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[35m[package]\x1b[0m"));
    assert!(s.contains("\x1b[36mname\x1b[0m \x1b[2m=\x1b[0m\x1b[38;5;117m \"glimps\"\x1b[0m"));
    assert!(s.contains("\x1b[36mversion\x1b[0m \x1b[2m=\x1b[0m\x1b[38;5;220m 1\x1b[0m"));
    assert!(s.contains("\x1b[2m# comment\x1b[0m"));
}

#[test]
fn cat_dotenv_gets_semantic_coloring_without_secret_pinning() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let body = concat!(
        "# local development\n",
        "export API_URL=\"https://localhost:3000/v1#fragment\"\n",
        "PORT=3000\n",
        "DEBUG=true\n",
        "EMPTY=\n",
        "ERROR=super-secret-value\n",
        "MODE=development # selected profile\n",
        "context one\ncontext two\ncontext three\n",
    );
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat .env.local")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body.as_bytes()));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("\x1b[2m# local development\x1b[0m"));
    assert!(s.contains(concat!(
        "\x1b[35mexport\x1b[0m ",
        "\x1b[36mAPI_URL\x1b[0m",
        "\x1b[2m=\x1b[0m",
        "\x1b[38;5;117m\"https://localhost:3000/v1#fragment\"\x1b[0m"
    )));
    assert!(s.contains("\x1b[36mPORT\x1b[0m\x1b[2m=\x1b[0m\x1b[38;5;220m3000\x1b[0m"));
    assert!(s.contains("\x1b[36mDEBUG\x1b[0m\x1b[2m=\x1b[0m\x1b[35mtrue\x1b[0m"));
    assert!(s.contains("\x1b[36mEMPTY\x1b[0m\x1b[2m=\x1b[0m\n"));
    assert!(s.contains("\x1b[2m# selected profile\x1b[0m"));
    assert!(s.contains("command failed: cat .env.local"));
    assert!(
        !s.contains('\u{21b3}'),
        "dotenv secrets must never be copied into a failure pin: {s:?}"
    );
    assert_eq!(
        s.matches("super-secret-value").count(),
        1,
        "secret value should appear only in the requested file output"
    );
}

#[test]
fn dotenv_examples_use_the_same_view_but_unrelated_output_does_not() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"head .env.example")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"DATABASE_URL=postgres://localhost/app\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[36mDATABASE_URL\x1b[0m\x1b[2m=\x1b[0m"));

    let mut f = Formatter::new();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"printf config-like-text")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"DATABASE_URL=postgres://localhost/app\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(!s.contains("\x1b[36mDATABASE_URL"));
}

#[test]
fn custom_sensitive_rule_can_force_dotenv_back_to_raw_passthrough() {
    let cfg = Config {
        sensitive_commands: vec!["cat .env".to_string()],
        ..Config::default()
    };
    let mut f = fmt_with(cfg);
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat .env")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"API_TOKEN=secret\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("API_TOKEN=secret\n"));
    assert!(!s.contains("\x1b[36mAPI_TOKEN"));
}

#[test]
fn dotenv_view_does_not_weaken_other_secret_file_passthrough() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat .env id_ed25519")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"API_TOKEN=secret\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("API_TOKEN=secret\n"));
    assert!(!s.contains("\x1b[36mAPI_TOKEN"));
}

#[test]
fn compound_dotenv_reader_stays_raw_and_never_pins_secrets() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat .env; false")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(
        &f.process(b"ERROR=compound-secret\ncontext one\ncontext two\ncontext three\n"),
    );
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("ERROR=compound-secret\n"));
    assert!(!s.contains("\x1b[36mERROR"));
    assert!(!s.contains('\u{21b3}'));
    assert_eq!(s.matches("compound-secret").count(), 1);
}

#[test]
fn cat_gitleaksignore_colors_comments_and_fingerprint_fields() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat .gitleaksignore")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        b"# Expired dev-session JWT\n4646bf87405b2073d83fc4dbc1e5e3c5beff2cb8:backend/cookies.txt:jwt:6\n",
    ));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[2m# Expired dev-session JWT\x1b[0m"));
    assert!(s.contains(
        "\x1b[38;5;220m4646bf87405b2073d83fc4dbc1e5e3c5beff2cb8\x1b[0m\
\x1b[2m:\x1b[0m\x1b[36mbackend/cookies.txt\x1b[0m\
\x1b[2m:\x1b[0m\x1b[35mjwt\x1b[0m\
\x1b[2m:\x1b[0m\x1b[38;5;220m6\x1b[0m"
    ));
}

#[test]
fn cat_gitignore_colors_comments_negation_paths_and_globs() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let body = b"# generated files\n/target/\n*.log\n!important.log\nsrc/**/generated?.rs\n\\#literal-name\n";
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat .gitignore")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("\x1b[2m# generated files\x1b[0m"));
    assert!(s.contains(concat!(
        "\x1b[2m/\x1b[0m",
        "\x1b[38;5;117mtarget\x1b[0m",
        "\x1b[2m/\x1b[0m"
    )));
    assert!(s.contains("\x1b[38;5;220m*\x1b[0m\x1b[38;5;117m.log\x1b[0m"));
    assert!(s.contains("\x1b[35m!\x1b[0m\x1b[38;5;117mimportant.log\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m**\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m?\x1b[0m"));
    assert!(s.contains("\x1b[38;5;117m\\#\x1b[0m"));
    assert!(strip_sgr(&out)
        .windows(body.len())
        .any(|window| window == body));
}

#[test]
fn getfileinfo_colors_metadata_labels_paths_attributes_and_dates() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let body = concat!(
        "directory: \"/Volumes/One Touch\"\n",
        "attributes: avbstClinmedz\n",
        "created: 01/01/1904 05:21:10\n",
        "modified: 01/01/1904 05:21:10\n",
    );
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"GetFileInfo /Volumes/One\\ Touch")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body.as_bytes()));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("\x1b[36mdirectory\x1b[0m\x1b[2m:\x1b[0m"));
    assert!(s.contains("\x1b[38;2;142;202;230m \"/Volumes/One Touch\"\x1b[0m"));
    assert!(s.contains("\x1b[35m avbstClinmedz\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m 01/01/1904 05:21:10\x1b[0m"));
}

#[test]
fn xattr_long_view_colors_attribute_names_and_values_only_with_long_flag() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"xattr -l /Volumes/One\\ Touch")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"com.apple.FinderInfo:\ncom.apple.test: 0A FF 12\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[36mcom.apple.FinderInfo\x1b[0m\x1b[2m:\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m 0A FF 12\x1b[0m"));

    let mut f = Formatter::new();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"xattr /tmp/file")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"com.apple.FinderInfo\n"));
    out.extend_from_slice(&f.process(D0));
    assert!(!String::from_utf8_lossy(&out).contains("\x1b[36mcom.apple.FinderInfo"));
}

#[test]
fn diskutil_info_colors_aligned_typed_fields() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let body = concat!(
        "   Device Identifier:        disk6s1\n",
        "   Device Node:              /dev/disk6s1\n",
        "   Mounted:                  Yes\n",
        "   Mount Point:              /Volumes/One Touch\n",
        "   File System Personality:  ExFAT\n",
        "   SMART Status:             Not Supported\n",
        "   Volume UUID:              396DF0B8-CE18-3EDA-8486-7295049E9D8A\n",
        "   Disk Size:                2.0 TB (2000397795328 Bytes)\n",
        "   Volume Used Space:        332.7 GB (16.6%)\n",
    );
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"diskutil info /Volumes/One\\ Touch")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body.as_bytes()));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("\x1b[36mDevice Identifier\x1b[0m\x1b[2m:\x1b[0m"));
    assert!(s.contains("\x1b[38;2;142;202;230m/dev/disk6s1\x1b[0m"));
    assert!(s.contains("\x1b[32mYes\x1b[0m"));
    assert!(s.contains("\x1b[35mExFAT\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220mNot Supported\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m396DF0B8-CE18-3EDA-8486-7295049E9D8A\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m2.0\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m16.6%\x1b[0m"));
}

#[test]
fn piped_diskutil_info_keeps_the_report_view_but_plist_declines_it() {
    let mut piped = Formatter::new();
    if !piped.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&piped.process(&cmd_marker(
        b"diskutil info /Volumes/One\\ Touch | grep 'File System'",
    )));
    out.extend_from_slice(&piped.process(C));
    out.extend_from_slice(&piped.process(b"   File System Personality:  ExFAT\n"));
    out.extend_from_slice(&piped.process(D0));
    assert!(String::from_utf8_lossy(&out).contains("\x1b[35mExFAT\x1b[0m"));

    let mut plist = Formatter::new();
    let mut out = Vec::new();
    out.extend_from_slice(&plist.process(&cmd_marker(b"diskutil info -plist disk6")));
    out.extend_from_slice(&plist.process(C));
    out.extend_from_slice(&plist.process(b"<key>DeviceIdentifier</key>\n"));
    out.extend_from_slice(&plist.process(D0));
    assert!(!String::from_utf8_lossy(&out).contains("\x1b[36mDeviceIdentifier"));
}

#[test]
fn cat_csv_gets_header_and_cell_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat users.csv")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(
        &f.process(b"name,age,active\nAda,37,true\n\"Lovelace, Ada\",12,false\n"),
    );
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[36mname\x1b[0m\x1b[2m,\x1b[0m\x1b[36mage\x1b[0m"));
    assert!(s.contains("\x1b[38;5;117mAda\x1b[0m\x1b[2m,\x1b[0m\x1b[38;5;220m37\x1b[0m"));
    assert!(s.contains("\x1b[35mtrue\x1b[0m"));
    assert!(s.contains("\x1b[38;5;117m\"Lovelace, Ada\"\x1b[0m"));
}

#[test]
fn cat_tsv_gets_tabular_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat report.tsv")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"service\tlatency_ms\tok\napi\t42\ttrue\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[36mservice\x1b[0m\x1b[2m\t\x1b[0m\x1b[36mlatency_ms\x1b[0m"));
    assert!(s.contains("\x1b[38;5;117mapi\x1b[0m\x1b[2m\t\x1b[0m\x1b[38;5;220m42\x1b[0m"));
}

#[test]
fn cat_sql_gets_query_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat schema.sql")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        b"-- users table\nCREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);\nselect * from users where id = 42 and name = 'Ada''s';\n",
    ));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[2m-- users table\x1b[0m"));
    assert!(s.contains("\x1b[35mCREATE\x1b[0m \x1b[35mTABLE\x1b[0m users"));
    assert!(s.contains("\x1b[35mselect\x1b[0m \x1b[2m*\x1b[0m \x1b[35mfrom\x1b[0m users"));
    assert!(s.contains("\x1b[38;5;220m42\x1b[0m"));
    assert!(s.contains("\x1b[38;5;117m'Ada''s'\x1b[0m"));
}

#[test]
fn psql_result_table_gets_value_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"psql -c 'select * from users'")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(
        &f.process(b" id | name | active\n----+------+--------\n  1 | Ada  | t\n(1 row)\n"),
    );
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[36m id \x1b[0m\x1b[2m|\x1b[0m\x1b[36m name \x1b[0m"));
    assert!(s.contains("\x1b[2m----+------+--------\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m  1 \x1b[0m\x1b[2m|\x1b[0m\x1b[38;5;117m Ada  \x1b[0m"));
    assert!(s.contains("\x1b[2m(1 row)\x1b[0m"));
}

#[test]
fn mysql_boxed_result_table_gets_value_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"mysql -e 'select id,name from users'")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        b"+----+--------+\n| id | name   |\n+----+--------+\n|  2 | Grace  |\n+----+--------+\n",
    ));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[2m+----+--------+\x1b[0m"));
    assert!(s.contains("\x1b[2m|\x1b[0m\x1b[36m id \x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m  2 \x1b[0m\x1b[2m|\x1b[0m\x1b[38;5;117m Grace  \x1b[0m"));
}

#[test]
fn sqlite_pipe_result_table_gets_value_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(
        b"sqlite3 app.db 'select id,name,ok from users'",
    )));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"id|name|ok\n1|Ada|true\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[36mid\x1b[0m\x1b[2m|\x1b[0m\x1b[36mname\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m1\x1b[0m\x1b[2m|\x1b[0m\x1b[38;5;117mAda\x1b[0m"));
    assert!(s.contains("\x1b[35mtrue\x1b[0m"));
}

#[test]
fn git_short_status_gets_status_and_path_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"git status --short")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        b"## main...origin/main [ahead 1]\n M README.md\nA  src/new.rs\n?? scratch.txt\nD  old.rs\nR  old.rs -> new.rs\n",
    ));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[2m## \x1b[0m\x1b[36mmain\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m M\x1b[0m\x1b[2m \x1b[0m\x1b[36mREADME.md\x1b[0m"));
    assert!(s.contains("\x1b[32mA \x1b[0m\x1b[2m \x1b[0m\x1b[36msrc/new.rs\x1b[0m"));
    assert!(s.contains("\x1b[38;5;117m??\x1b[0m\x1b[2m \x1b[0m\x1b[36mscratch.txt\x1b[0m"));
    assert!(s.contains("\x1b[31mD \x1b[0m\x1b[2m \x1b[0m\x1b[36mold.rs\x1b[0m"));
    assert!(s.contains("\x1b[35mR \x1b[0m\x1b[2m \x1b[0m\x1b[36mold.rs\x1b[0m\x1b[2m -> \x1b[0m\x1b[36mnew.rs\x1b[0m"));
}

#[test]
fn git_status_long_gets_branch_headings_and_paths() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"git status")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        b"On branch main\nChanges not staged for commit:\n  (use \"git add <file>...\" to update what will be committed)\n\tmodified:   README.md\nUntracked files:\n\tnew.txt\nnothing to commit, working tree clean\n",
    ));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[2mOn branch \x1b[0m\x1b[36mmain\x1b[0m"));
    assert!(s.contains("\x1b[35mChanges not staged for commit:\x1b[0m"));
    assert!(
        s.contains("\x1b[2m  (use \"git add <file>...\" to update what will be committed)\x1b[0m")
    );
    assert!(s.contains("\x1b[38;5;220mmodified:\x1b[0m\x1b[2m   \x1b[0m\x1b[36mREADME.md\x1b[0m"));
    assert!(s.contains("\x1b[35mUntracked files:\x1b[0m"));
    assert!(s.contains("\x1b[32mnothing to commit, working tree clean\x1b[0m"));
}

#[test]
fn git_log_oneline_gets_hash_and_ref_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"git --no-pager log --oneline --decorate -2")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(
        &f.process(b"1a2b3c4 (HEAD -> main, origin/main) Add git polish\n5d6e7f8 Previous work\n"),
    );
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains(
        "\x1b[38;5;220m1a2b3c4\x1b[0m \x1b[36m(HEAD -> main, origin/main)\x1b[0m Add git polish"
    ));
    assert!(s.contains("\x1b[38;5;220m5d6e7f8\x1b[0m Previous work"));
}

#[test]
fn git_branch_gets_current_branch_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"git branch -a")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"* main\n  feature/git-polish\n  remotes/origin/main\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[32m*\x1b[0m \x1b[36mmain\x1b[0m"));
    assert!(s.contains("\x1b[36mfeature/git-polish\x1b[0m"));
    assert!(s.contains("\x1b[2mremotes/\x1b[0m\x1b[36morigin/main\x1b[0m"));
}

#[test]
fn git_branch_delete_warning_is_gold_not_branch_cyan() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"git branch -d fix/ci-backend-pipeline")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(
        &f.process(
            concat!(
            "warning: deleting branch 'fix/ci-backend-pipeline' that has been merged to\n",
            "         'refs/remotes/origin/fix/ci-backend-pipeline', but not yet merged to HEAD\n",
            "Deleted branch fix/ci-backend-pipeline (was 59d1c84).\n",
        )
            .as_bytes(),
        ),
    );
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains(
        "\x1b[38;5;220mwarning: deleting branch 'fix/ci-backend-pipeline' that has been merged to\x1b[0m"
    ));
    assert!(s.contains(
        "\x1b[38;5;220m         'refs/remotes/origin/fix/ci-backend-pipeline', but not yet merged to HEAD\x1b[0m"
    ));
    assert!(s.contains(concat!(
        "\x1b[32mDeleted branch\x1b[0m ",
        "\x1b[36mfix/ci-backend-pipeline\x1b[0m",
        "\x1b[2m (was \x1b[0m",
        "\x1b[38;5;220m59d1c84\x1b[0m",
        "\x1b[2m).\x1b[0m"
    )));
    assert!(!s.contains("\x1b[36mwarning:"));
}

#[test]
fn git_diff_stat_gets_file_count_and_change_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"git diff --stat")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        b" README.md       | 10 +++++-----\n src/main.rs    |  2 ++\n 2 files changed, 7 insertions(+), 5 deletions(-)\n",
    ));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[36m README.md       \x1b[0m\x1b[2m|\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m10\x1b[0m"));
    assert!(s.contains("\x1b[32m+++++\x1b[0m\x1b[31m-----\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m2\x1b[0m files changed"));
    assert!(s.contains("\x1b[32minsertions(+)\x1b[0m"));
    assert!(s.contains("\x1b[31mdeletions(-)\x1b[0m"));
}

#[test]
fn git_numstat_and_name_status_get_value_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"git diff --numstat")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"7\t5\tREADME.md\n-\t-\tassets/logo.png\n"));
    out.extend_from_slice(&f.process(D0));
    out.extend_from_slice(&f.process(&cmd_marker(b"git diff --name-status")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"M\tREADME.md\nR100\told.rs\tnew.rs\n"));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[32m7\x1b[0m\x1b[2m\t\x1b[0m\x1b[31m5\x1b[0m"));
    assert!(s.contains("\x1b[36mREADME.md\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220mM\x1b[0m\x1b[2m\t\x1b[0m\x1b[36mREADME.md\x1b[0m"));
    assert!(s.contains("\x1b[35mR100\x1b[0m\x1b[2m\t\x1b[0m\x1b[36mold.rs\tnew.rs\x1b[0m"));
}

#[test]
fn git_show_stat_keeps_commit_header_and_colors_stats() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"git show --stat --oneline")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        b"commit 1a2b3c4d5e6f7890\n README.md | 3 ++-\n 1 file changed, 2 insertions(+), 1 deletion(-)\n",
    ));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[35mcommit\x1b[0m \x1b[38;5;220m1a2b3c4d5e6f7890\x1b[0m"));
    assert!(s.contains("\x1b[36m README.md \x1b[0m\x1b[2m|\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m1\x1b[0m file changed"));
    assert!(s.contains("\x1b[31mdeletion(-)\x1b[0m"));
}

#[test]
fn cat_jsonl_gets_streaming_json_line_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat events.jsonl")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        br#"{"level":"info","count":2}
{"level":"error","ok":false}
"#,
    ));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(
        !s.contains("JSON\r\n"),
        "JSONL should not get a buffered JSON badge"
    );
    assert!(s.contains("\x1b[36m\"level\"\x1b[0m"));
    assert!(s.contains("\x1b[38;5;117m\"info\"\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m2\x1b[0m"));
    assert!(s.contains("\x1b[35mfalse\x1b[0m"));
}

#[test]
fn cat_rust_source_gets_syntax_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"cat src/main.rs")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        b"// boot path\npub fn main() {\n    let answer = 42;\n    println!(\"ok\");\n}\n",
    ));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[2m// boot path\x1b[0m"));
    assert!(s.contains("\x1b[35mpub\x1b[0m \x1b[35mfn\x1b[0m \x1b[36mmain\x1b[0m"));
    assert!(s.contains("\x1b[35mlet\x1b[0m answer \x1b[2m=\x1b[0m \x1b[38;5;220m42\x1b[0m"));
    assert!(s.contains("\x1b[38;5;117m\"ok\"\x1b[0m"));
}

#[test]
fn head_python_source_gets_syntax_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"head -20 app.py")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(
        &f.process(b"# deploy helper\ndef greet(name):\n    return f\"hi {name}\"\n"),
    );
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[2m# deploy helper\x1b[0m"));
    assert!(s.contains("\x1b[35mdef\x1b[0m \x1b[36mgreet\x1b[0m"));
    assert!(s.contains("\x1b[35mreturn\x1b[0m f\x1b[38;5;117m\"hi {name}\"\x1b[0m"));
}

#[test]
fn generic_json_lines_stream_instead_of_buffering_whole_output() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"printf json lines")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        br#"{"a":1}
{"b":2}
"#,
    ));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);
    assert!(
        !s.contains("JSON\r\n"),
        "JSON-lines must not be buffered as one document"
    );
    assert!(s.contains("\x1b[36m\"a\"\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m1\x1b[0m"));
    assert!(s.contains("\x1b[36m\"b\"\x1b[0m"));
}

#[test]
fn ls_output_gets_command_aware_columns() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"ls -la")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"drwxr-xr-x   8 krishv  staff   256 Jun 28 09:10 src\n"));
    out.extend_from_slice(&f.process(b"-rw-r--r--   1 krishv  staff   312 Jun 28 09:10 .env\n"));
    out.extend_from_slice(&f.process(b"drwxr-xr-x   3 krishv  staff    96 Jun 28 09:10 .vscode\n"));
    out.extend_from_slice(&f.process(b"drwxr-xr-x  18 krishv  staff   576 Jun 28 09:10 .\n"));
    out.extend_from_slice(&f.process(b"drwxr-xr-x  30 krishv  staff   960 Jun 28 09:10 ..\n"));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[2mdrwxr-xr-x\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m256\x1b[0m"));
    assert!(s.contains("\x1b[38;2;122;162;247msrc\x1b[0m"));
    assert!(s.contains("\x1b[38;2;69;73;85m.env\x1b[0m"));
    assert!(s.contains("\x1b[38;2;69;73;85m.vscode\x1b[0m"));
    assert!(!s.contains("\x1b[38;2;69;73;85m.\x1b[0m"));
    assert!(!s.contains("\x1b[38;2;69;73;85m..\x1b[0m"));
}

#[test]
fn ls_simple_output_distinguishes_hidden_names() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"ls -a")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b".  ..  .git  README.md\n"));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[38;2;69;73;85m.git\x1b[0m"));
    assert!(s.contains("\x1b[38;5;117mREADME.md\x1b[0m"));
    assert!(s.contains("\x1b[2m.\x1b[0m"));
    assert!(s.contains("\x1b[2m..\x1b[0m"));
}

#[test]
fn ls_simple_and_long_views_share_filename_semantics() {
    let mut simple = Formatter::new();
    if !simple.is_enabled() {
        return;
    }
    let mut simple_out = Vec::new();
    simple_out.extend_from_slice(&simple.process(&cmd_marker(b"ls -F")));
    simple_out.extend_from_slice(&simple.process(C));
    simple_out.extend_from_slice(&simple.process(b"README.md  src/  run*  current@  .git/\n"));
    simple_out.extend_from_slice(&simple.process(D));
    let simple_text = String::from_utf8_lossy(&simple_out);
    assert!(simple_text.contains("\x1b[38;5;117mREADME.md\x1b[0m"));
    assert!(simple_text.contains("\x1b[38;2;122;162;247msrc/\x1b[0m"));
    assert!(simple_text.contains("\x1b[32mrun*\x1b[0m"));
    assert!(simple_text.contains("\x1b[35mcurrent@\x1b[0m"));
    assert!(simple_text.contains("\x1b[38;2;69;73;85m.git/\x1b[0m"));

    let mut long = Formatter::new();
    let mut long_out = Vec::new();
    long_out.extend_from_slice(&long.process(&cmd_marker(b"ls -la")));
    long_out.extend_from_slice(&long.process(C));
    long_out.extend_from_slice(
        &long.process(b"-rw-r--r--  1 krishv staff 17230 Aug 1 23:34 README.md\n"),
    );
    long_out
        .extend_from_slice(&long.process(b"-rwxr-xr-x  1 krishv staff   512 Aug 1 23:34 run\n"));
    long_out.extend_from_slice(
        &long.process(b"lrwxr-xr-x  1 krishv staff     3 Aug 1 23:34 current -> src\n"),
    );
    long_out.extend_from_slice(&long.process(D));
    let long_text = String::from_utf8_lossy(&long_out);
    assert!(long_text.contains("\x1b[38;5;117mREADME.md\x1b[0m"));
    assert!(long_text.contains("\x1b[32mrun\x1b[0m"));
    assert!(long_text.contains("\x1b[35mcurrent\x1b[0m"));
    assert!(long_text.contains("\x1b[2m->\x1b[0m"));
    assert!(long_text.contains("\x1b[38;2;142;202;230msrc\x1b[0m"));
}

#[test]
fn kubectl_get_pods_colors_running_status() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"kubectl get pods")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"NAME READY STATUS RESTARTS AGE\nnginx 1/1 Running 0 2m\n"));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[2mNAME READY STATUS RESTARTS AGE\x1b[0m"));
    assert!(s.contains("\x1b[32mRunning\x1b[0m"));
}

#[test]
fn kubectl_get_pods_colors_failing_status() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"kubectl get pods")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(
        &f.process(b"NAME READY STATUS RESTARTS AGE\napi 0/1 CrashLoopBackOff 5 10m\n"),
    );
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[31mCrashLoopBackOff\x1b[0m"));
}

#[test]
fn kubectl_non_pod_output_passes_through() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"kubectl config current-context")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"kind-dev\n"));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("kind-dev\n"));
    assert!(!s.contains("\x1b[36mkind-dev\x1b[0m"));
    assert!(!s.contains("\x1b[32mkind-dev\x1b[0m"));
}

#[test]
fn du_and_df_outputs_highlight_sizes_and_capacity() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"du -sh src")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b" 12K\t./src\n"));
    out.extend_from_slice(&f.process(D));
    out.extend_from_slice(&f.process(&cmd_marker(b"df -h")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"devfs 203Ki 203Ki 0Bi 100% /dev\n"));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[38;5;220m12K\x1b[0m"));
    assert!(s.contains("\x1b[38;2;142;202;230m./src\x1b[0m"));
    assert!(!s.contains("\x1b[36m./src\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m203Ki\x1b[0m"));
    assert!(s.contains("\x1b[31m100%\x1b[0m"));
}

#[test]
fn df_uses_semantic_storage_colors_and_handles_multiword_filesystems() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let body = concat!(
        "Filesystem        Size Used Avail Capacity iused ifree %iused Mounted on\n",
        "/dev/disk3s1s1    245G  13G   49G      21%  459k  483M      0% /\n",
        "/dev/disk3s5      245G 173G   49G      78%  2.8M  483M      1% /System/Volumes/Data\n",
        "map auto_home       0B   0B    0B     100%     0     0       - /System/Volumes/Data/home\n",
        "/dev/disk5s1       18G  18G  466M      98%  627k  4.6M     12% /Library/Developer\n",
    );
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"df -H")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body.as_bytes()));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("\x1b[36mmap\x1b[0m \x1b[36mauto_home\x1b[0m"));
    assert!(s.contains("\x1b[38;5;153m245G\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m173G\x1b[0m"));
    assert!(s.contains("\x1b[32m466M\x1b[0m"));
    assert!(s.contains("\x1b[32m21%\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m78%\x1b[0m"));
    assert!(s.contains("\x1b[31m98%\x1b[0m"));
    assert!(s.contains("\x1b[38;2;142;202;230m/Library/Developer\x1b[0m"));
    assert!(strip_sgr(&out)
        .windows(body.len())
        .any(|window| window == body.as_bytes()));
}

#[test]
fn ps_output_highlights_process_columns() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"ps aux")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(
        &f.process(b"krishv   42311   0.4  0.3 412899200  54128 s001  S    9:10AM   0:01.23 zsh\n"),
    );
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[36mkrishv\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m42311\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m0.4\x1b[0m"));
    assert!(s.contains("zsh"));
    assert!(s.contains("\x1b[38;5;153mzsh\x1b[0m"));
    assert!(!s.contains("\x1b[32mzsh\x1b[0m"));
}

#[test]
fn dig_output_highlights_dns_sections_and_records() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"dig 360astra.io")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(
        &f.process(b";; ANSWER SECTION:\n360astra.io. 1767 IN A 82.180.142.20\n"),
    );
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[35m;; ANSWER SECTION:\x1b[0m"));
    assert!(s.contains("\x1b[36m360astra.io.\x1b[0m"));
    assert!(s.contains("\x1b[35mA\x1b[0m"));
    assert!(s.contains("\x1b[36m82.180.142.20\x1b[0m"));
}

#[test]
fn ping_colors_live_replies_and_macos_statistics_semantically() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let body = concat!(
        "PING 360astra.io (82.180.142.20): 56 data bytes\n",
        "64 bytes from 82.180.142.20: icmp_seq=0 ttl=49 time=75.102 ms\n",
        "64 bytes from 82.180.142.20: icmp_seq=1 ttl=49 time=180.500 ms\n",
        "64 bytes from 82.180.142.20: icmp_seq=2 ttl=49 time=320.750 ms\n",
        "--- 360astra.io ping statistics ---\n",
        "5 packets transmitted, 5 packets received, 0.0% packet loss\n",
        "round-trip min/avg/max/stddev = 61.847/75.815/83.587/7.533 ms\n",
    );
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"ping -c 5 360astra.io")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body.as_bytes()));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("\x1b[35mPING\x1b[0m"));
    assert!(s.contains("\x1b[38;2;142;202;230m82.180.142.20:\x1b[0m"));
    assert!(s.contains("\x1b[32mtime=75.102\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220mtime=180.500\x1b[0m"));
    assert!(s.contains("\x1b[31mtime=320.750\x1b[0m"));
    assert!(s.contains("\x1b[32m0.0%\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m61.847/75.815/83.587/7.533\x1b[0m"));
    assert!(strip_sgr(&out)
        .windows(body.len())
        .any(|window| window == body.as_bytes()));
}

#[test]
fn ping_handles_linux_loss_ipv6_timeouts_and_unreachable_errors() {
    let cases: &[(&[u8], &[u8], &str)] = &[
        (
            b"ping example.com",
            b"4 packets transmitted, 3 received, 25% packet loss, time 3004ms\n",
            "\x1b[31m25%\x1b[0m",
        ),
        (
            b"ping6 ::1",
            b"64 bytes from ::1: icmp_seq=1 ttl=64 time=0.042 ms\n",
            "\x1b[38;2;142;202;230m::1:\x1b[0m",
        ),
        (
            b"ping example.com",
            b"Request timeout for icmp_seq 2\n",
            "\x1b[38;5;220mRequest timeout",
        ),
        (
            b"ping example.com",
            b"From 192.0.2.1 icmp_seq=1 Destination Host Unreachable\n",
            "\x1b[31mFrom 192.0.2.1",
        ),
    ];
    for (command, line, expected) in cases {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(&cmd_marker(command)));
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(line));
        out.extend_from_slice(&f.process(D0));
        assert!(
            String::from_utf8_lossy(&out).contains(expected),
            "missing ping formatting for {:?}: {:?}",
            String::from_utf8_lossy(command),
            String::from_utf8_lossy(&out)
        );
    }
}

#[test]
fn mac_networking_commands_get_command_aware_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"ifconfig")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        b"en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500\n\tinet 192.168.1.9 netmask 0xffffff00 broadcast 192.168.1.255\n\tether a0:9a:8e:8b:b1:26\n\tstatus: active\n",
    ));
    out.extend_from_slice(&f.process(D));

    out.extend_from_slice(&f.process(&cmd_marker(b"scutil --dns")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        b"DNS configuration\n\nresolver #1\n  nameserver[0] : 192.168.1.1\n  if_index : 14 (en0)\n",
    ));
    out.extend_from_slice(&f.process(D));

    out.extend_from_slice(&f.process(&cmd_marker(b"route get default")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        b"   route to: default\ndestination: default\n    gateway: 192.168.1.1\n  interface: en0\n",
    ));
    out.extend_from_slice(&f.process(D));

    out.extend_from_slice(&f.process(&cmd_marker(b"netstat -rn")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        b"Routing tables\n\nInternet:\nDestination        Gateway            Flags        Netif Expire\ndefault            192.168.1.1        UGSc           en0\n",
    ));
    out.extend_from_slice(&f.process(D));

    out.extend_from_slice(&f.process(&cmd_marker(b"lsof -i")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        b"COMMAND   PID   USER   FD   TYPE DEVICE SIZE/OFF NODE NAME\nCode     5146 krishv   42u  IPv4 0xabcd      0t0  TCP localhost:5173 (LISTEN)\n",
    ));
    out.extend_from_slice(&f.process(D));

    out.extend_from_slice(&f.process(&cmd_marker(b"launchctl list")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(
        &f.process(b"PID\tStatus\tLabel\n-\t0\tcom.apple.Finder\n513\t-9\tcom.example.crashed\n"),
    );
    out.extend_from_slice(&f.process(D));

    out.extend_from_slice(&f.process(&cmd_marker(b"pmset -g")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(
        b"System-wide power settings:\nCurrently in use:\n sleep      10\n powernap   0\n",
    ));
    out.extend_from_slice(&f.process(D));

    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[36men0:\x1b[0m"));
    assert!(s.contains("\x1b[38;2;142;202;230m192.168.1.9\x1b[0m"));
    assert!(s.contains("\x1b[38;2;142;202;230ma0:9a:8e:8b:b1:26\x1b[0m"));
    assert!(s.contains("\x1b[32mactive\x1b[0m"));
    assert!(s.contains("\x1b[36mDNS configuration\x1b[0m"));
    assert!(s.contains("\x1b[35m  nameserver[0] :\x1b[0m\x1b[38;2;142;202;230m 192.168.1.1\x1b[0m"));
    assert!(s.contains("\x1b[35m    gateway:\x1b[0m\x1b[38;2;142;202;230m 192.168.1.1\x1b[0m"));
    assert!(s.contains("\x1b[36mRouting tables\x1b[0m"));
    assert!(s.contains("\x1b[36mdefault\x1b[0m"));
    assert!(s.contains("\x1b[38;2;142;202;230m192.168.1.1\x1b[0m"));
    assert!(s.contains("\x1b[36mCode\x1b[0m"));
    assert!(s.contains("\x1b[38;2;142;202;230mlocalhost:5173\x1b[0m"));
    assert!(s.contains("\x1b[2mPID\tStatus\tLabel\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m-9\x1b[0m"));
    assert!(s.contains("\x1b[36mSystem-wide power settings:\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m10\x1b[0m"));
}

#[test]
fn networksetup_output_gets_label_and_value_coloring() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"networksetup -listallhardwareports")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(
        &f.process(b"Hardware Port: Wi-Fi\nDevice: en0\nEthernet Address: a0:9a:8e:8b:b1:26\n"),
    );
    out.extend_from_slice(&f.process(D));
    out.extend_from_slice(&f.process(&cmd_marker(
        b"networksetup -listpreferredwirelessnetworks en0",
    )));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"Preferred networks on en0:\n\tHomeWiFi\n\t.Office\n"));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[36mHardware Port:\x1b[0m\x1b[38;2;142;202;230m Wi-Fi\x1b[0m"));
    assert!(s.contains("\x1b[36mDevice:\x1b[0m\x1b[38;5;117m en0\x1b[0m"));
    assert!(s.contains(
        "\x1b[36mEthernet Address:\x1b[0m\x1b[38;2;142;202;230m a0:9a:8e:8b:b1:26\x1b[0m"
    ));
    assert!(s.contains("\x1b[36mPreferred networks on \x1b[0m\x1b[38;5;117men0:\x1b[0m"));
    assert!(s.contains("\t\x1b[38;5;117mHomeWiFi\x1b[0m"));
    assert!(s.contains("\t\x1b[38;2;69;73;85m.Office\x1b[0m"));
}

#[test]
fn security_password_reveal_output_is_passthrough_and_never_pinned() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(
        br#"security find-generic-password -D "AirPort network password" -a "HomeWiFi" -gw"#,
    )));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(br#"{"secret":"do-not-format-this"}"#));
    out.extend_from_slice(&f.process(b"\nERROR still just secret-shaped output\n"));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains(r#"{"secret":"do-not-format-this"}"#));
    assert!(!s.contains(r#""secret": "do-not-format-this""#));
    assert!(!s.contains("JSON\n"));
    assert!(
        !s.contains('\u{21b3}'),
        "sensitive output must not be pinned: {s:?}"
    );
    assert!(s.contains("command failed: security find-generic-password"));
}

#[test]
fn sensitive_command_registry_catches_secret_printing_tools() {
    let commands: &[&[u8]] = &[
        br#"security find-generic-password -D "AirPort network password" -a "HomeWiFi" -gw"#,
        b"gh auth token",
        b"op read op://Private/GitHub/token",
        b"op item get GitHub --fields password --reveal",
        b"bw get password github",
        b"pass show github/token",
        b"aws configure get aws_secret_access_key",
        b"aws secretsmanager get-secret-value --secret-id prod/db",
        b"aws ssm get-parameter --name /prod/db/password --with-decryption",
        b"gcloud auth print-access-token",
        b"doppler secrets get DATABASE_URL",
        b"cat .env.local",
        b"head -20 ~/.aws/credentials",
        b"tail ~/.kube/config",
        b"sed -n 1,20p id_ed25519",
    ];
    for command in commands {
        assert!(
            is_sensitive_command(command, &[]),
            "expected sensitive command: {}",
            String::from_utf8_lossy(command)
        );
    }
}

#[test]
fn sensitive_command_registry_does_not_catch_neighbor_commands() {
    let commands: &[&[u8]] = &[
        b"gh issue list",
        b"aws configure get region",
        b"aws ssm get-parameter --name /prod/plain",
        b"gcloud config list",
        b"doppler run -- cargo test",
        b"cat .env.example",
        b"cat README.md",
    ];
    for command in commands {
        assert!(
            !is_sensitive_command(command, &[]),
            "unexpected sensitive command: {}",
            String::from_utf8_lossy(command)
        );
    }
}

#[test]
fn custom_sensitive_commands_are_token_aware_and_passthrough() {
    let rules = vec!["vault kv get".to_string(), "kubectl get secret".to_string()];
    assert!(is_sensitive_command(
        b"sudo vault kv get secret/app",
        &rules
    ));
    assert!(is_sensitive_command(
        b"/usr/local/bin/kubectl get secret db -o yaml",
        &rules
    ));
    assert!(!is_sensitive_command(b"vault status", &rules));
    assert!(!is_sensitive_command(b"echo 'vault kv get'", &rules));

    let cfg = Config {
        sensitive_commands: rules,
        ..Config::default()
    };
    let mut f = fmt_with(cfg);
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"vault kv get secret/app")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(br#"{"token":"do-not-format"}"#));
    out.extend_from_slice(&f.process(b"\nERROR secret output\n"));
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains(r#"{"token":"do-not-format"}"#));
    assert!(!s.contains(r#""token": "do-not-format""#));
    assert!(!s.contains('\u{21b3}'));
}

#[test]
fn other_sensitive_commands_are_passthrough_and_never_pinned() {
    for command in [
        &b"gh auth token"[..],
        &b"aws secretsmanager get-secret-value --secret-id prod/db"[..],
    ] {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        f.theme = Theme::plain();
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(&cmd_marker(command)));
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(br#"{"token":"do-not-format-this"}"#));
        out.extend_from_slice(&f.process(b"\nERROR secret-shaped output\n"));
        out.extend_from_slice(&f.process(D1));
        let s = String::from_utf8_lossy(&out);
        assert!(
            s.contains(r#"{"token":"do-not-format-this"}"#),
            "raw output missing for {}: {s:?}",
            String::from_utf8_lossy(command)
        );
        assert!(!s.contains(r#""token": "do-not-format-this""#));
        assert!(!s.contains("JSON\n"));
        assert!(
            !s.contains('\u{21b3}'),
            "sensitive output must not be pinned for {}: {s:?}",
            String::from_utf8_lossy(command)
        );
    }
}

#[test]
fn man_overstrike_output_is_cleaned_and_highlighted() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"man glimps")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"N\x08NA\x08AM\x08ME\x08E\n"));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[36mNAME\x1b[0m\n"));
    assert!(!s.contains('\u{8}'));
}

#[test]
fn whatis_colors_names_sections_aliases_and_descriptions() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let body = concat!(
        "cat(1)                    - concatenate and print files\n",
        "man(1), apropos(1), whatis(1) - display online manual documentation pages\n",
        "DateTime::Locale::en_IM(3pm) - Locale data examples for the English Isle of Man locale\n",
    );
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"whatis man")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body.as_bytes()));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("\x1b[36mcat\x1b[0m\x1b[2m(\x1b[0m\x1b[38;5;220m1\x1b[0m"));
    assert!(s.contains("\x1b[2m, \x1b[0m\x1b[36mapropos\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m3pm\x1b[0m"));
    assert!(s.contains("\x1b[2m - \x1b[0m\x1b[38;5;153mdisplay online manual"));
    assert!(strip_sgr(&out)
        .windows(body.len())
        .any(|window| window == body.as_bytes()));
}

#[test]
fn whereis_distinguishes_command_executable_and_manual_locations() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let body = b"ls: /bin/ls /usr/share/man/man1/ls.1\nmissing:\n";
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"whereis ls missing")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("\x1b[36mls\x1b[0m\x1b[2m:\x1b[0m"));
    assert!(s.contains("\x1b[38;2;142;202;230m/bin/ls\x1b[0m"));
    assert!(s.contains("\x1b[35m/usr/share/man/man1/ls.1\x1b[0m"));
    assert!(s.contains("\x1b[36mmissing\x1b[0m\x1b[2m:\x1b[0m"));
    assert!(strip_sgr(&out)
        .windows(body.len())
        .any(|window| window == body));
}

#[test]
fn history_colors_event_numbers_and_command_syntax() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let body = b"    42  git status --short\n    43  echo 'hello world'\n";
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"history 1")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("\x1b[38;5;220m42\x1b[0m  \x1b[36mgit\x1b[0m status"));
    assert!(s.contains("\x1b[38;5;220m--short\x1b[0m"));
    assert!(s.contains("\x1b[36mecho\x1b[0m \x1b[38;5;117m'hello world'\x1b[0m"));
    assert!(strip_sgr(&out)
        .windows(body.len())
        .any(|window| window == body));
}

#[test]
fn history_frequency_pipeline_colors_counts_and_commands() {
    let command = b"history 1 | awk '{print $2}' | sort | uniq -c | sort -nr | head -100";
    let body = b"  128 git\n   64 cargo\n    9 code\n";
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(command)));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body));
    out.extend_from_slice(&f.process(D0));
    let s = String::from_utf8_lossy(&out);

    assert!(s.contains("\x1b[38;5;220m128\x1b[0m \x1b[36mgit\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m64\x1b[0m \x1b[36mcargo\x1b[0m"));
    assert!(strip_sgr(&out)
        .windows(body.len())
        .any(|window| window == body));
}

#[test]
fn history_unknown_transform_pipeline_stays_conservative() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let body = b"git\ncargo\n";
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(&cmd_marker(b"history 1 | awk '{print $2}'")));
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body));
    out.extend_from_slice(&f.process(D0));

    assert!(strip_sgr(&out)
        .windows(body.len())
        .any(|window| window == body));
    assert!(!String::from_utf8_lossy(&out).contains("\x1b[38;5;220mgit"));
}

#[test]
fn apropos_and_man_index_flags_share_the_whatis_view() {
    for command in [
        &b"apropos terminal"[..],
        b"man -k terminal",
        b"man --apropos terminal",
        b"man -f cat",
        b"man --whatis cat",
    ] {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return;
        }
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(&cmd_marker(command)));
        out.extend_from_slice(&f.process(C));
        out.extend_from_slice(&f.process(b"cat(1) - concatenate and print files\n"));
        out.extend_from_slice(&f.process(D0));
        assert!(
            String::from_utf8_lossy(&out).contains("\x1b[36mcat\x1b[0m"),
            "missing manual-index coloring for {command:?}"
        );
    }
}

#[test]
fn binary_control_bytes_without_nul_pass_through_unframed() {
    // Binary that contains NO NUL but other C0 control bytes (an image/gzip
    // dump, a compiled binary) is still binary: no separator, no formatting,
    // exact bytes (invariant #3). This is the gap NUL-only detection missed.
    let body = b"\x02\x03\x04 raw \x10\x11\x16 bytes \x1f";
    let input = cat(&[C, body, D]);
    assert_eq!(run(&[&input]), input);
}

#[test]
fn invalid_utf8_output_passes_through_unframed() {
    // High bytes that don't form valid UTF-8 (e.g. Latin-1, or a truncated
    // binary) are not text we frame or color: passed through verbatim.
    let body = b"caf\xe9 menu \xff\xfe rows"; // lone 0xe9, then 0xff 0xfe
    let input = cat(&[C, body, D]);
    assert_eq!(run(&[&input]), input);
}

#[test]
fn valid_utf8_unicode_output_is_framed_as_text() {
    // Valid multibyte UTF-8 (accents, CJK, emoji) is ordinary text — it must
    // still be framed with the header, never misread as binary.
    let body = "café ☕ 日本語\n".as_bytes();
    let input = cat(&[C, body, D]);
    assert_eq!(run(&[&input]), cat(&[C, &sep(), body, D]));
}

#[test]
fn utf8_multibyte_split_across_chunks_is_not_treated_as_binary() {
    // "é" (0xC3 0xA9) split across two chunks: the first output chunk ends
    // mid-character. That incomplete tail must NOT be misclassified as binary
    // (invariant #4) — the run stays text and every byte survives.
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"caf\xc3")); // 'é' first byte (incomplete)
    out.extend_from_slice(&f.process(b"\xa9\n")); // 'é' second byte + newline
    out.extend_from_slice(&f.process(D));
    assert_eq!(out, cat(&[C, &sep(), b"caf\xc3\xa9\n", D]));
}

#[test]
fn nul_after_text_committed_still_streams_verbatim() {
    // Once a run has committed to text (Passthrough), a later NUL just streams
    // through. The separator was already shown for the text — that's correct.
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"hello ")); // commits to text -> separator
    out.extend_from_slice(&f.process(b"\x00\x00")); // NUL later -> verbatim
    out.extend_from_slice(&f.process(D));
    assert_eq!(out, cat(&[C, &sep(), b"hello ", b"\x00\x00", D]));
}

/// Build a Formatter with a specific config (plain timestamp, treated as a TTY).
fn fmt_with(config: Config) -> Formatter {
    Formatter::build(Clock::Off, true, config)
}

#[test]
fn config_master_disable_is_pure_passthrough() {
    let cfg = Config {
        enabled: false,
        ..Config::default()
    };
    let mut f = fmt_with(cfg);
    let stream = cat(&[C, br#"{"a":1}"#, D]);
    assert_eq!(&*f.process(&stream), stream.as_slice());
}

#[test]
fn config_json_off_passes_json_through_verbatim() {
    let cfg = Config {
        formatters: crate::config::Formatters {
            json: false,
            ..crate::config::Formatters::default()
        },
        ..Config::default()
    };
    let mut f = fmt_with(cfg);
    let body = br#"{"a":1}"#;
    let stream = cat(&[C, body, D]);
    // JSON disabled -> not reformatted; still framed by the separator.
    assert_eq!(f.process(&stream).into_owned(), cat(&[C, &sep(), body, D]));
}

#[test]
fn config_logs_off_does_not_color_log_lines() {
    let cfg = Config {
        formatters: crate::config::Formatters {
            logs: false,
            ..crate::config::Formatters::default()
        },
        ..Config::default()
    };
    let mut f = fmt_with(cfg);
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"ERROR boom\n"));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("ERROR boom\n"));
    assert!(!s.contains("\x1b[31m")); // not colored red
}

#[test]
fn config_color_off_emits_no_ansi_but_still_structures() {
    let cfg = Config {
        color: false,
        ..Config::default()
    };
    let mut f = fmt_with(cfg);
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(br#"{"a":1}"#));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8(out).unwrap();
    // No SGR color codes (`\x1b[`). The OSC-133 markers use `\x1b]` and are
    // passed through, so we don't assert against bare ESC.
    assert!(!s.contains("\x1b[")); // no color codes (separator/badge/json)
    assert!(s.contains("[JSON]")); // plain badge instead of inverse
    assert!(s.contains("{\r\n  \"a\": 1\r\n}")); // still indented (CRLF)
}

#[test]
fn config_separator_off_hides_the_divider() {
    let cfg = Config {
        separator: false,
        ..Config::default()
    };
    let mut f = fmt_with(cfg);
    let body = b"plain output\n";
    let stream = cat(&[C, body, D]);
    // No separator, and (plain log line) no coloring change -> verbatim.
    assert_eq!(f.process(&stream).into_owned(), stream);
}

#[test]
fn non_tty_supervisor_output_disables_formatting() {
    // Under `cargo test`, stdout is not a terminal, so the supervisor
    // constructor must disable formatting (raw pass-through).
    if std::io::stdout().is_terminal() {
        return; // can't assert the gate when run attached to a real tty
    }
    let mut f = Formatter::for_supervisor(Clock::Off, Config::default());
    assert!(!f.is_enabled());
    // And it truly passes through, markers/JSON included.
    let stream = cat(&[C, br#"{"a":1}"#, D]);
    assert_eq!(&*f.process(&stream), stream.as_slice());
}

#[test]
fn alt_screen_app_is_passed_through_untouched() {
    // A full-screen app (vim): enter alt screen, draw content that even looks
    // like JSON, exit. GLIMPS may leave a boundary breadcrumb before the
    // alt-screen switch, but the app's redraw stream itself is pure verbatim.
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let alt_on = b"\x1b[?1049h";
    let alt_off = b"\x1b[?1049l";
    let parts: [&[u8]; 5] = [C, alt_on, br#"{"a":1}"#, alt_off, D];
    let mut out = Vec::new();
    for p in parts {
        out.extend_from_slice(&f.process(p));
    }
    assert_eq!(
        out,
        cat(&[C, &sep(), &badge("TUI"), alt_on, br#"{"a":1}"#, alt_off, D])
    );
}

#[test]
fn alt_screen_entering_mid_buffer_flushes_without_byte_loss() {
    // Defensive path: a buffered JSON-candidate run is pending when alt-screen
    // is entered. The withheld bytes must be flushed verbatim (incomplete
    // JSON doesn't format), then the alt-screen chunk streamed — nothing lost.
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C)); // OutputStart, separator owed
    out.extend_from_slice(&f.process(br#"{"a":1"#)); // buffered (incomplete)
    out.extend_from_slice(&f.process(b"\x1b[?1049h")); // alt-screen enters
                                                       // Separator was emitted lazily before the buffered bytes; on alt-enter the
                                                       // buffer is flushed verbatim and the chunk streamed.
    assert_eq!(out, cat(&[C, &sep(), br#"{"a":1"#, b"\x1b[?1049h"]));
}

#[test]
fn formatting_resumes_after_alt_screen_exits() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    // A TUI session, discarded.
    for p in [C, b"\x1b[?1049h".as_slice(), b"\x1b[?1049l", D] {
        let _ = f.process(p);
    }
    // The next command's JSON output is formatted normally again.
    let mut out = Vec::new();
    for p in [A, C, br#"{"a":1}"#.as_slice(), D] {
        out.extend_from_slice(&f.process(p));
    }
    assert_eq!(
        out,
        cat(&[A, C, &sep(), &badge("JSON"), &crlf(b"{\n  \"a\": 1\n}"), D,])
    );
}

#[test]
fn separator_owed_across_a_chunk_boundary() {
    // OutputStart arrives at the end of one chunk (C marker), the first output
    // byte in the next. The owed separator must still be emitted once, before
    // that byte — exercising the `pending_separator` Cow-fast-path guard.
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C)); // OutputStart, separator owed
    out.extend_from_slice(&f.process(b"hello\n")); // first output byte next chunk
    out.extend_from_slice(&f.process(D));
    assert_eq!(out, cat(&[C, &sep(), b"hello\n", D]));
}

#[test]
fn separator_carries_timestamp_with_a_clock() {
    let mut f = Formatter::with_clock(Clock::Fixed("12:34:56"));
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"hi\n"));
    out.extend_from_slice(&f.process(D));
    let expected = cat(&[C, &sep_with(Clock::Fixed("12:34:56")), b"hi\n", D]);
    assert_eq!(out, expected);
    // The timestamp text is present in the emitted separator.
    assert!(out.windows(8).any(|w| w == b"12:34:56"));
}

#[test]
fn eof_flush_emits_withheld_non_json_unchanged() {
    // Output that starts like JSON but never closes (no `D`), then EOF. The
    // withheld bytes survive verbatim, behind the separator.
    let body = br#"{"a":1"#; // incomplete: no closing brace
    let input = cat(&[C, body]);
    assert_eq!(run_flush(&[&input]), cat(&[C, &sep(), body]));
}

#[test]
fn eof_flush_formats_withheld_complete_json() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(br#"{"a":1}"#)); // buffered, no D yet
    out.extend_from_slice(&f.flush()); // EOF -> format the complete value
    assert_eq!(
        out,
        cat(&[C, &sep(), &badge("JSON"), &crlf(b"{\n  \"a\": 1\n}")])
    );
}

#[test]
fn two_consecutive_json_outputs_each_framed_and_formatted() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    for part in [C, br#"{"a":1}"#, D, A, C, b"[1,2]", D] {
        out.extend_from_slice(&f.process(part));
    }
    let one = crlf(b"{\n  \"a\": 1\n}");
    let two = crlf(b"[1, 2]");
    let expected = cat(&[
        C,
        &sep(),
        &badge("JSON"),
        &one,
        D,
        A,
        C,
        &sep(),
        &badge("JSON"),
        &two,
        D,
    ]);
    assert_eq!(out, expected);
}

#[test]
fn html_output_is_indented_with_badge() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"<p>hi</p>"));
    out.extend_from_slice(&f.process(D));
    let indented = crlf(b"<p>\n  hi\n</p>");
    assert_eq!(out, cat(&[C, &sep(), &badge("HTML"), &indented, D]));
}

#[test]
fn diff_output_is_badged_and_preserves_crlf_without_doubling() {
    // A unified diff as it arrives off a PTY (CRLF line endings). It gets a
    // DIFF badge and (plain theme) is otherwise byte-identical — crucially the
    // diff colorizer preserves line endings, so no CR is doubled.
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let body = b"--- a/x\r\n+++ b/x\r\n@@ -1 +1 @@\r\n-old\r\n+new\r\n";
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(body));
    out.extend_from_slice(&f.process(D));
    assert_eq!(out, cat(&[C, &sep(), &badge("DIFF"), body, D]));
}

#[test]
fn diff_like_text_without_a_hunk_is_not_reformatted() {
    // A `-`/`+` list with NO `@@` hunk header must not be mistaken for a diff:
    // framed by the separator, but byte-preserved and no DIFF badge.
    let body = b"- buy milk\n+ add sugar to the list\n";
    let input = cat(&[C, body, D]);
    let out = run(&[&input]);
    assert_eq!(out, cat(&[C, &sep(), body, D]));
    assert!(
        !out.windows(4).any(|w| w == b"DIFF"),
        "must not badge a non-diff"
    );
}

#[test]
fn stack_trace_panic_line_is_highlighted_streaming() {
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    // Colored theme: the panic header line is wrapped in the error color.
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(b"thread 'main' panicked at src/main.rs:1:1:\n"));
    out.extend_from_slice(&f.process(b"called `Option::unwrap()` on a `None` value\n"));
    out.extend_from_slice(&f.process(D));
    let s = String::from_utf8(out).unwrap();
    assert!(
        s.contains("\x1b[31mthread 'main' panicked at src/main.rs:1:1:\x1b[0m\n"),
        "panic header not highlighted"
    );
    // The message line below is ordinary text — left untouched.
    assert!(s.contains("called `Option::unwrap()` on a `None` value\n"));
}

#[test]
fn buffer_cap_overflow_streams_verbatim() {
    // A `{`-leading run larger than BUFFER_CAP gives up and streams the bytes
    // unchanged (behind the separator) rather than holding them.
    let mut body = vec![b'{'];
    body.extend(std::iter::repeat_n(
        b'x',
        Config::default().limits.buffer_cap + 1,
    ));
    let input = cat(&[C, &body]); // no D; overflow forces verbatim
    assert_eq!(run_flush(&[&input]), cat(&[C, &sep(), &body]));
}

#[test]
fn ansi_escape_mid_json_keeps_bytes_intact() {
    // Output containing an ANSI escape can't be one clean JSON value; the
    // user's bytes pass through unchanged (invariant #3) behind the separator.
    let body = cat(&[br#"{"a":1"#, b"\x1b[31m", b"}"]);
    let input = cat(&[C, &body, D]);
    assert_eq!(run(&[&input]), cat(&[C, &sep(), &body, D]));
}

/// A token used to build fuzz bodies that interleave plain text, ANSI SGR
/// sequences, and newlines — mimicking real colored command output.
#[derive(Debug, Clone)]
enum Tok {
    Text(Vec<u8>),
    Sgr(u8),
    Newline,
}

/// Remove the first occurrence of `needle` from `haystack`.
fn strip_first(haystack: &[u8], needle: &[u8]) -> Vec<u8> {
    match haystack.windows(needle.len()).position(|w| w == needle) {
        Some(pos) => {
            let mut v = haystack[..pos].to_vec();
            v.extend_from_slice(&haystack[pos + needle.len()..]);
            v
        }
        None => haystack.to_vec(),
    }
}

#[test]
fn corpus_common_commands_preserve_every_byte() {
    // Zero interference across a corpus of real-world command output —
    // including ANSI-colored, Unicode, man-overstrike, control-only, tables,
    // and empty/whitespace cases. GLIMPS may only INSERT one separator; every
    // user byte must survive. Plain theme so line coloring adds nothing; we
    // then strip the single injected separator and require the original back.
    // (Stripping handles ANSI-leading output, where the separator lands after
    // the leading escape rather than right after the C marker.)
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/corpus/commands");
    let sep = sep();
    let mut count = 0;
    for entry in std::fs::read_dir(dir).expect("corpus dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let sample = std::fs::read(&path).expect("read fixture");
        let input = cat(&[C, &sample, D]);
        let out = run(&[&input]);
        let recovered = strip_first(&out, &sep);
        assert_eq!(
            recovered,
            cat(&[C, &sample, D]),
            "interference on corpus fixture {:?}",
            path.file_name()
        );
        count += 1;
    }
    assert!(count >= 30, "expected a sizable corpus, found only {count}");
}

#[test]
fn password_prompt_output_is_never_touched() {
    // A no-echo password prompt is just command output (the program prints
    // "Password:"); GLIMPS must pass it through unchanged. The typed password
    // is no-echo, so it never appears in the stream at all — GLIMPS can't see
    // it. This pins the "password prompts never touched" promise.
    let prompt = b"Password:";
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let mut out = Vec::new();
    out.extend_from_slice(&f.process(C));
    out.extend_from_slice(&f.process(prompt));
    out.extend_from_slice(&f.flush_stalled_output());
    // Crucially, the password question is visible while the command is still
    // waiting; command end has not been sent yet.
    assert_eq!(out, cat(&[C, &sep(), prompt]));
    out.extend_from_slice(&f.process(D));
    assert_eq!(out, cat(&[C, &sep(), prompt, D]));
}

#[test]
fn latency_budget_no_pathological_blowup() {
    use std::time::Instant;
    // ~4 MiB of line-oriented output fed in PTY-sized chunks through the
    // streaming (per-line) path — the realistic hot path. A generous wall
    // budget catches O(n^2)/pathological regressions (criterion measures the
    // real micro-latency separately). Debug builds are slow, hence 10s.
    let mut f = Formatter::new();
    if !f.is_enabled() {
        return;
    }
    f.theme = Theme::plain();
    let line = b"the quick brown fox jumps over the lazy dog 0123456789\n";
    let mut body = Vec::with_capacity(4 * 1024 * 1024);
    while body.len() < 4 * 1024 * 1024 {
        body.extend_from_slice(line);
    }
    let start = Instant::now();
    let _ = f.process(C);
    for chunk in body.chunks(8192) {
        let _ = f.process(chunk);
    }
    let _ = f.process(D);
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 10,
        "processed {} bytes in {elapsed:?} (budget 10s — suspect a complexity regression)",
        body.len()
    );
}

proptest::proptest! {
    /// Arbitrary config (random toggles + small/zero caps) over arbitrary
    /// command output never panics, and when the master switch is off the
    /// output is byte-identical to the input (pure pass-through).
    #[test]
    #[allow(clippy::too_many_arguments)]
    fn prop_arbitrary_config_is_safe(
        enabled: bool,
        color: bool,
        separator: bool,
        json: bool,
        html: bool,
        logs: bool,
        http: bool,
        diff: bool,
        stacktrace: bool,
        buffer_cap in 0usize..2048,
        line_cap in 0usize..2048,
        sniff_cap in 0usize..128,
        failures_enabled: bool,
        success_off: bool,
        explain: bool,
        pin_errors: bool,
        exit_code in proptest::option::of(-300i32..400),
        cmd in proptest::option::of(proptest::collection::vec(0u8..=255, 0..64)),
        body in proptest::collection::vec(0u8..=255, 0..256),
    ) {
        let cfg = Config {
            enabled,
            color,
            separator,
            timestamp: false,
            farewell: false,
            bypass: Vec::new(),
            sensitive_commands: Vec::new(),
            formatters: crate::config::Formatters { json, html, logs, http, diff, stacktrace },
            failures: crate::config::Failures {
                enabled: failures_enabled,
                on_success: if success_off {
                    crate::config::SuccessFooter::Off
                } else {
                    crate::config::SuccessFooter::Dim
                },
                explain,
                pin_errors,
            },
            limits: crate::config::Limits { buffer_cap, line_cap, sniff_cap },
        };
        let mut f = Formatter::build(Clock::Off, true, cfg);
        // Half the runs end with a bare `D`, half with `D;<code>` across the
        // full (incl. out-of-range/negative) exit-code space, so the footer
        // path itself is fuzzed alongside the formatters.
        let end = match exit_code {
            Some(code) => d_exit(code),
            None => D.to_vec(),
        };
        // An optional command capture (arbitrary bytes, incl. control chars)
        // makes the footer actually fire — without a captured command the
        // status path returns early — and fuzzes its sanitization (BUG #2).
        let start = match &cmd {
            Some(cmd) => cmd_marker(cmd),
            None => Vec::new(),
        };
        let stream = [&start, C, &body, &end].concat();
        let mut out = f.process(&stream).into_owned();
        out.extend_from_slice(&f.flush()); // also exercises EOF flush; must not panic
        if !enabled {
            proptest::prop_assert_eq!(out, stream); // off => verbatim
        }
    }

    /// Fuzz: realistic colored output — arbitrary interleavings of plain
    /// text, ANSI SGR sequences, and newlines — is preserved byte-for-byte
    /// (plain theme; the one injected separator stripped back out), and never
    /// panics. Exercises the ESC-splitting + line-streaming paths on
    /// adversarial-but-realistic input.
    #[test]
    fn prop_text_and_ansi_preserve_every_byte(
        ops in proptest::collection::vec(
            proptest::prop_oneof![
                proptest::collection::vec(
                    proptest::prop_oneof![Just(b' '), 0x61u8..0x7b, 0x30u8..0x3a],
                    1..12,
                ).prop_map(Tok::Text),
                (0u8..8).prop_map(Tok::Sgr),
                Just(Tok::Newline),
            ],
            0..40,
        )
    ) {
        // Lead with a plain byte so the run is classified as text (not
        // JSON/HTML/binary); tokens contain no NUL, no `ESC ]`, no `─`/`\r`,
        // so the body can neither form a marker nor collide with a separator.
        let mut body = vec![b'x'];
        for op in &ops {
            match op {
                Tok::Text(bytes) => body.extend_from_slice(bytes),
                Tok::Sgr(n) => {
                    body.extend_from_slice(b"\x1b[");
                    body.extend_from_slice(n.to_string().as_bytes());
                    body.push(b'm');
                }
                Tok::Newline => body.push(b'\n'),
            }
        }
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return Ok(());
        }
        f.theme = Theme::plain();
        let stream = [C, &body, D].concat();
        let out = f.process(&stream).into_owned();
        let recovered = strip_first(&out, &sep());
        proptest::prop_assert_eq!(recovered, stream);
    }

    /// Byte-safety invariant #4 for the pass-through path: with no escape
    /// sequences in the stream (so the zone never leaves Unknown, and no
    /// separator is ever inserted), the concatenation of outputs equals the
    /// concatenation of inputs exactly.
    #[test]
    fn prop_passthrough_is_byte_identical(
        chunks in proptest::collection::vec(proptest::collection::vec(0u8..=255, 0..64), 0..16)
    ) {
        let mut f = Formatter::new();
        let mut out = Vec::new();
        let mut expected = Vec::new();
        for chunk in &chunks {
            // Strip ESC so no escape sequence (and thus no zone change) can form.
            let clean: Vec<u8> = chunk.iter().copied().filter(|&b| b != 0x1b).collect();
            expected.extend_from_slice(&clean);
            out.extend_from_slice(&f.process(&clean));
        }
        proptest::prop_assert_eq!(out, expected);
    }

    /// Non-JSON command output (arbitrary ESC-free bytes wrapped in C/D
    /// markers) is preserved byte-for-byte, with only the GLIMPS separator
    /// inserted at output start. Proves the buffering path withholds-then-
    /// flushes without altering the user's bytes.
    #[test]
    fn prop_non_json_output_preserves_user_bytes(
        body in proptest::collection::vec(0u8..=255, 0..256)
    ) {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return Ok(());
        }
        f.theme = Theme::plain(); // line coloring adds no bytes -> exact assertions
        // Drop only ESC (so no escape sequence / zone change / marker can form);
        // binary bytes are kept and recovered via `strip_first` below (verbatim,
        // no separator), so this exercises both the text-framing and binary paths.
        let clean: Vec<u8> = body.iter().copied().filter(|&b| b != 0x1b).collect();
        // Exclude anything a formatter would reformat — those are meant to change.
        proptest::prop_assume!(format_recognized(&clean, &Theme::plain(), &crate::config::Formatters::default()).is_none());

        let input = [C, &clean, D].concat();
        let mut out = Vec::new();
        out.extend_from_slice(&f.process(&input));
        // At most one separator is inserted (on a text commit; binary inserts
        // none). Removing one recovers the input exactly — no byte lost/changed.
        let recovered = strip_first(&out, &sep());
        proptest::prop_assert_eq!(recovered, input);
    }

    /// Binary command output is passed through byte-for-byte with NO separator,
    /// badge, or color — even with the COLORED theme on. The body is built from
    /// printable bytes plus C0 controls (and is asserted to contain at least one
    /// binary byte, and never ESC so no marker can form), so the whole run is
    /// classified binary. Pins invariant #3 for the non-NUL binary case.
    #[test]
    fn prop_binary_output_is_passed_through_verbatim(
        body in proptest::collection::vec(
            proptest::prop_oneof![1u8..=6, 0x20u8..=0x7e], 1..200)
    ) {
        proptest::prop_assume!(body.iter().copied().any(is_binary_byte));
        let mut f = Formatter::new(); // colored theme — proves even color is suppressed
        if !f.is_enabled() {
            return Ok(());
        }
        let stream = [C, &body, D].concat();
        let mut out = f.process(&stream).into_owned();
        out.extend_from_slice(&f.flush());
        proptest::prop_assert_eq!(out, stream);
    }

    /// The CLI-diagnostic colorizer never panics on arbitrary input, and when
    /// it claims a line it only *wraps* it: stripping SGR recovers the input
    /// byte-for-byte, so nothing is dropped, reordered, or truncated and no
    /// invalid UTF-8 can be introduced (invariant #4). ESC is filtered out of
    /// the generated body so the generator cannot smuggle in a sequence that
    /// `strip_sgr` would then remove from the payload itself.
    #[test]
    fn prop_cli_diagnostic_line_is_byte_safe(
        body in proptest::collection::vec(0u8..=255, 0..200)
    ) {
        let clean: Vec<u8> = body.iter().copied().filter(|&b| b != 0x1b).collect();
        let theme = Theme::default_colored();
        if let Some(out) = linefmt::colorize_cli_diagnostic_line(&clean, &theme) {
            proptest::prop_assert_eq!(strip_sgr(&out), clean);
        }
    }

    /// The same byte-safety guarantee for inputs shaped like real diagnostics,
    /// so the property actually exercises the matching path instead of bailing
    /// out on `None` for nearly every generated case.
    #[test]
    fn prop_diagnostic_shaped_line_is_byte_safe(
        tool in proptest::collection::vec(
            proptest::prop_oneof![Just(b'.'), Just(b'-'), Just(b'/'), 0x61u8..0x7b], 0..12),
        fragment in proptest::sample::select(vec![
            &b"Operation not permitted"[..],
            &b"Not a directory"[..],
            &b"Is a directory"[..],
            &b"No such file or directory"[..],
            &b"Permission denied"[..],
            &b"illegal option -- m"[..],
            &b"perfectly ordinary output"[..],
        ]),
        path in proptest::collection::vec(0x20u8..0x7f, 0..24),
        ending in proptest::sample::select(vec![&b""[..], &b"\n"[..], &b"\r\n"[..]]),
    ) {
        let mut line = tool;
        line.extend_from_slice(b": ");
        line.extend_from_slice(&path);
        line.extend_from_slice(b": ");
        line.extend_from_slice(fragment);
        line.extend_from_slice(ending);
        let theme = Theme::default_colored();
        if let Some(out) = linefmt::colorize_cli_diagnostic_line(&line, &theme) {
            proptest::prop_assert_eq!(strip_sgr(&out), line);
        }
    }

    /// The EOF-flush path preserves non-JSON user bytes even when the stream
    /// is split at arbitrary boundaries and never closed by a `D` marker (the
    /// shell-crash scenario). Directly guards the truncation bug the audit
    /// caught.
    #[test]
    fn prop_eof_flush_preserves_user_bytes(
        body in proptest::collection::vec(0u8..=255, 0..256),
        splits in proptest::collection::vec(1usize..40, 1..10),
    ) {
        let mut f = Formatter::new();
        if !f.is_enabled() {
            return Ok(());
        }
        f.theme = Theme::plain(); // line coloring adds no bytes -> exact assertions
        // Drop only ESC (so no escape sequence / zone change / marker can form);
        // binary bytes are kept and recovered via `strip_first` below (verbatim,
        // no separator), exercising both the text-framing and binary EOF paths.
        let clean: Vec<u8> = body.iter().copied().filter(|&b| b != 0x1b).collect();
        // Exclude anything a formatter would reformat — those are meant to change.
        proptest::prop_assume!(format_recognized(&clean, &Theme::plain(), &crate::config::Formatters::default()).is_none());

        // C + body, but NO closing D — then flush, simulating PTY EOF.
        let input = [C, &clean].concat();
        let mut out = Vec::new();
        let (mut i, mut si) = (0usize, 0usize);
        while i < input.len() {
            let step = splits[si % splits.len()].min(input.len() - i).max(1);
            out.extend_from_slice(&f.process(&input[i..i + step]));
            i += step;
            si += 1;
        }
        out.extend_from_slice(&f.flush());
        // At most one separator is inserted; removing it recovers every input
        // byte — the truncation/loss guard, robust to binary and split points.
        let recovered = strip_first(&out, &sep());
        proptest::prop_assert_eq!(recovered, input);
    }
}

/// Remove every SGR/CSI color escape (`ESC [ … final-byte`) from `bytes`,
/// leaving all other bytes — text, OSC-133 markers (`ESC ]`), box-drawing —
/// intact. A byte-safe colorizer only ever *wraps* the user's bytes in these
/// escapes, so `strip_sgr(colorized) == original`.
fn strip_sgr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && bytes.get(i + 1) == Some(&b'[') {
            // CSI: skip params/intermediates up to and including the final byte.
            let mut j = i + 2;
            while j < bytes.len() && !(0x40..=0x7e).contains(&bytes[j]) {
                j += 1;
            }
            i = (j + 1).min(bytes.len());
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// One line of fuzz for the colored command-view byte-safety proptest: either
/// arbitrary safe text (printable ASCII + tab, no ESC/NUL/newline so it can't
/// form a marker, look binary, or split a line), or a realistic git
/// branch-tracking line whose whitespace run before `[ahead/behind …]` is
/// fuzzed to 1..=4 bytes — the exact shape that exposed the branch-meta
/// whitespace byte-loss bug (`## main...origin/main␠␠[ahead 1]`).
fn cmd_view_line() -> impl Strategy<Value = Vec<u8>> {
    let text = proptest::collection::vec(
        proptest::prop_oneof![Just(b'\t'), Just(b' '), 0x21u8..=0x7e],
        0..24,
    );
    let branch = (
        proptest::collection::vec(0x61u8..=0x7a, 1..6),
        proptest::option::of(proptest::collection::vec(0x61u8..=0x7a, 1..6)),
        1usize..=4,
        proptest::prop_oneof![
            Just(&b"[ahead 1]"[..]),
            Just(&b"[behind 2]"[..]),
            Just(&b"[ahead 3, behind 4]"[..]),
        ],
    )
        .prop_map(|(name, upstream, gap, meta)| {
            let mut line = b"## ".to_vec();
            line.extend_from_slice(&name);
            if let Some(up) = upstream {
                line.extend_from_slice(b"...");
                line.extend_from_slice(&up);
            }
            line.extend(std::iter::repeat_n(b' ', gap));
            line.extend_from_slice(meta);
            line
        });
    proptest::prop_oneof![text, branch]
}

proptest::proptest! {
    /// Byte-safety invariant #4 for the COLORED command-view family. The other
    /// process-level byte-preservation proptests all run with `Theme::plain()`
    /// AND no captured command, so the colored command-view colorizers
    /// (`colorize_git_*`, `colorize_delimited_*`, `colorize_sql*`, markdown,
    /// code, …) get ZERO coverage there: under a plain theme they early-return,
    /// and with no command `command_view()` is `None`. Here we inject the
    /// `7337;<cmd>` command marker (so `command_view` resolves) and the `133;C`
    /// output-start marker under a COLORED theme (so the colorizers actually
    /// paint), then prove that stripping the SGR color escapes back out recovers
    /// the user's bytes EXACTLY — for git status, CSV, SQL, Markdown, and Rust
    /// source — and that nothing ever panics. Regression guard for the
    /// `## …␠␠[ahead]` branch-meta whitespace-loss bug.
    #[test]
    fn prop_colored_command_views_preserve_every_byte(
        lines in proptest::collection::vec(cmd_view_line(), 1..12),
    ) {
        // One newline-terminated body reused across every view, so no partial
        // line is ever left dangling at the D marker (finalize emits nothing).
        let mut body = Vec::new();
        for line in &lines {
            body.extend_from_slice(line);
            body.push(b'\n');
        }
        // Need >=1 non-whitespace byte so the owed header is emitted during the
        // body chunk (not deferred to the D flush), keeping the frame a clean
        // prefix of the body output.
        proptest::prop_assume!(body.iter().any(|b| !b.is_ascii_whitespace()));

        for cmd in [
            &b"git status --short"[..],
            b"cat .gitignore",
            b"cat .gitleaksignore",
            b"cat .env.example",
            b"cat data.csv",
            b"cat schema.sql",
            b"cat notes.md",
            b"cat main.rs",
            b"kubectl get pods",
            b"GetFileInfo /tmp/example",
            b"xattr -l /tmp/example",
            b"diskutil info /dev/disk1",
            b"whatis cat",
            b"whereis cat",
            b"history 1",
            b"ping -c 1 127.0.0.1",
        ] {
            let mut f = Formatter::build(Clock::Off, true, Config::default());
            if !f.is_enabled() {
                return Ok(());
            }
            // 7337;<cmd> -> command_view resolves; 133;C -> output zone begins.
            let mut prefix = f.process(&cmd_marker(cmd)).into_owned();
            prefix.extend_from_slice(&f.process(C));
            // The exact header the formatter will emit lazily before the body.
            let header = f.render_header();
            let body_out = f.process(&body).into_owned();
            let tail = f.process(D).into_owned();

            let mut full = prefix.clone();
            full.extend_from_slice(&body_out);
            full.extend_from_slice(&tail);

            // A byte-safe colorizer only ADDS SGR escapes, so stripping them must
            // recover exactly: <marker passthroughs> <header> <body> <D marker>.
            let stripped = strip_sgr(&full);
            let mut want_prefix = strip_sgr(&prefix);
            want_prefix.extend_from_slice(&strip_sgr(&header));
            let core = stripped
                .strip_prefix(&want_prefix[..])
                .expect("frame is a clean prefix of the colored output");
            let recovered = core
                .strip_suffix(D)
                .expect("colored output ends with the 133;D marker");
            proptest::prop_assert_eq!(
                recovered,
                &body[..],
                "byte loss under command {:?}",
                String::from_utf8_lossy(cmd)
            );
        }
    }
}
