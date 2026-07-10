//! Unit, golden, and property tests for the formatter seam.
//!
//! Split out of `mod.rs` to keep the seam's logic readable; as a child module
//! it still has `use super::*` access to the formatter's private items.

use super::*;
use proptest::prelude::*;

const C: &[u8] = b"\x1b]133;C\x07"; // command output start
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
    let mut v = b"\x1b]7337;".to_vec();
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
    let mut forged = cmd_marker(b"git push --force");
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
    assert!(s.contains("touched file "));
    assert!(s.contains("hello world.txt"));
    assert!(
        s.contains("\x1b[38;2;5;130;202mtouched file \x1b[38;2;142;202;230mhello world.txt\x1b[0m")
    );
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
    assert!(s.contains("created 2 folders: "));
    assert!(s.contains("logs/cache, tmp/out"));
    assert!(s.contains(
        "\x1b[38;2;5;130;202mcreated 2 folders: \x1b[38;2;142;202;230mlogs/cache, tmp/out\x1b[0m"
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
    assert!(s.contains("removed 2 targets: "));
    assert!(s.contains("target/cache, old.log"));
    assert!(s.contains(
        "\x1b[38;2;5;130;202mremoved 2 targets: \x1b[38;2;142;202;230mtarget/cache, old.log\x1b[0m"
    ));
    assert!(!s.contains("done exit 0"));
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
    assert!(!s.contains("removed target missing.txt"));
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
    assert!(!s.contains("touched file"));
    assert!(!s.contains("removed target"));
    assert!(!s.contains("done exit 0"));
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
    assert!(s.contains("\u{21b3} find: illegal option -- m"));
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
fn failed_python_script_pins_the_final_exception() {
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
    assert!(s.contains("\u{21b3} ValueError: broken config \u{2192} app.py:7"));
    // The exception was the last line: no distance hint.
    assert!(!s.contains("lines up"));
}

#[test]
fn pin_is_failure_only_never_success_or_notice() {
    // The same ERROR line in the output; only a Failure exit quotes it.
    for (marker, expect_pin) in [(d_exit(1), true), (d_exit(0), false), (d_exit(130), false)] {
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
        assert_eq!(
            s.contains('\u{21b3}'),
            expect_pin,
            "exit marker {marker:?}: {s:?}"
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
    out.extend_from_slice(&f.process(D1));
    let s = String::from_utf8_lossy(&out);
    let pin_line = s
        .lines()
        .find(|l| l.contains('\u{21b3}'))
        .expect("pin line present");
    assert!(
        pin_line.ends_with('\u{2026}'),
        "truncated with …: {pin_line:?}"
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
    assert!(s.contains("\x1b[36mREADME.md\x1b[0m"));
    assert!(!s.contains("\x1b[38;2;69;73;85m.\x1b[0m"));
    assert!(!s.contains("\x1b[38;2;69;73;85m..\x1b[0m"));
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
    assert!(s.contains("\x1b[36m./src\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m203Ki\x1b[0m"));
    assert!(s.contains("\x1b[38;5;220m100%\x1b[0m"));
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
    for p in [C, br#"{"a":1}"#.as_slice(), D] {
        out.extend_from_slice(&f.process(p));
    }
    assert_eq!(
        out,
        cat(&[C, &sep(), &badge("JSON"), &crlf(b"{\n  \"a\": 1\n}"), D])
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
    for part in [C, br#"{"a":1}"#, D, C, b"[1,2]", D] {
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
    let stream = cat(&[C, prompt, D]);
    // No trailing newline, prompt waits inline -> Stream holds it, flushed on D.
    assert_eq!(run(&[&stream]), cat(&[C, &sep(), prompt, D]));
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
            bypass: Vec::new(),
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
            b"cat data.csv",
            b"cat schema.sql",
            b"cat notes.md",
            b"cat main.rs",
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
