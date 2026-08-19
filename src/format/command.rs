//! Typed command policy and conservative shell-word parsing.
//!
//! This module decides whether a captured command is ordinary, interactive
//! bypass, or sensitive pass-through. It never formats output itself.

use super::cmdline;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommandTrust {
    Normal,
    InteractiveBypass,
    /// Line-oriented pager output: completed file lines may be styled, while
    /// unterminated interaction prompts stay live and failure pinning stays off.
    PagerText,
    /// Deliberately displayed sensitive text (currently dotenv files): allow
    /// byte-preserving line coloring, but never inspect/copy it into error pins.
    SensitiveText,
    Sensitive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CommandPolicy {
    pub trust: CommandTrust,
}

pub(super) fn classify(
    command: &[u8],
    bypass_names: &[String],
    sensitive_rules: &[String],
) -> CommandPolicy {
    let name = cmdline::first_word(command);
    let custom_sensitive = is_custom_sensitive(command, sensitive_rules);
    let built_in_sensitive = is_builtin_sensitive(command);
    let on_bypass_list = name
        .as_ref()
        .is_some_and(|name| bypass_names.iter().any(|bypass| bypass == name));
    // Order matters, and the bypass list has to outrank `SensitiveText`.
    //
    // `is_builtin_sensitive` scans every word, so `ssh myserver cat .env` looks
    // like a dotenv read. `SensitiveText` permits byte-preserving line coloring,
    // while `InteractiveBypass` is full pass-through — so with the dotenv arm
    // first, GLIMPS would start reformatting an ssh session, which invariant 3
    // names outright and which the alt-screen latch does not cover for ssh (see
    // the note in `config.rs` on the bypass defaults).
    //
    // Both `Sensitive` arms deliberately stay above the bypass list. Bypass
    // losing to them is behaviourally identical anyway: both route to
    // `Collect::Passthrough`.
    //
    // `reads_dotenv_file` is deliberately given the RAW command while
    // `is_builtin_sensitive` sees past a stderr redirect. The asymmetry is the
    // point, not an oversight: it means `cat .env 2>/dev/null` lands in
    // `Sensitive` (raw pass-through) rather than `SensitiveText` (coloring
    // permitted). Teaching this call to strip the redirect too would be the
    // *looser* direction on a file full of credentials, so the dotenv view
    // simply does not run for that spelling. Do not "fix" this for symmetry.
    let trust = if custom_sensitive || (built_in_sensitive && !reads_dotenv_file(command)) {
        CommandTrust::Sensitive
    } else if on_bypass_list {
        CommandTrust::InteractiveBypass
    } else if built_in_sensitive {
        CommandTrust::SensitiveText
    } else if name.as_deref() == Some("more") {
        CommandTrust::PagerText
    } else {
        CommandTrust::Normal
    };
    CommandPolicy { trust }
}

pub(super) fn silent_breadcrumb(command: &[u8]) -> Option<Vec<u8>> {
    let words = shell_words(command)?;
    // Stay deliberately conservative: only infer UX for the actual executable,
    // never for a word that merely appears later as an argument.
    let executable = std::str::from_utf8(words.first()?).ok()?;
    let name = executable.rsplit('/').next()?;
    if !matches!(name, "touch" | "mkdir" | "rm" | "killall") {
        return None;
    }
    let args = &words[1..];
    let targets = match name {
        "touch" => command_targets(args, &["-t", "-d", "-r", "--date", "--reference"]),
        "mkdir" => command_targets(args, &["-m", "--mode", "-Z", "--context"]),
        "rm" => command_targets(args, &[]),
        // `killall` flags differ across platforms and some take values that
        // look like process names. Only summarize the unambiguous positional
        // form rather than risk naming an option value as a killed process.
        "killall" => simple_targets(args)?,
        _ => Vec::new(),
    };
    if targets.is_empty() {
        return None;
    }

    let mut out = Vec::new();
    match (name, targets.len()) {
        ("touch", 1) => {
            out.extend_from_slice(b"touch processed path: ");
            out.extend_from_slice(&cmdline::sanitize_display(&targets[0]));
        }
        ("mkdir", 1) | ("rm", 1) => {
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(b" completed for ");
            out.extend_from_slice(&cmdline::sanitize_display(&targets[0]));
        }
        ("touch", n) => {
            push_target_summary(&mut out, b"touch processed", n, b"path arguments", &targets);
        }
        ("mkdir", n) | ("rm", n) => {
            let mut verb = name.as_bytes().to_vec();
            verb.extend_from_slice(b" completed for");
            push_target_summary(&mut out, &verb, n, b"targets", &targets);
        }
        ("killall", 1) => {
            out.extend_from_slice(b"killall completed for ");
            out.extend_from_slice(&cmdline::sanitize_display(&targets[0]));
        }
        ("killall", n) => {
            push_target_summary(
                &mut out,
                b"killall completed for",
                n,
                b"process names",
                &targets,
            );
        }
        _ => return None,
    }
    Some(out)
}

/// Return the destination of a simple `cat > file` / `cat >> file` command
/// that will read interactively from the terminal. Commands that also name an
/// input file, contain another shell operator, or have multiple redirections
/// decline the hint rather than guessing about their stdin behavior.
pub(super) fn cat_stdin_redirect_target(command: &[u8]) -> Option<Vec<u8>> {
    let (left, right) = split_simple_stdout_redirect(command)?;
    let command_words = shell_words(left)?;
    let executable = command_words.first()?.rsplit(|byte| *byte == b'/').next()?;
    if executable != b"cat"
        || command_words[1..]
            .iter()
            .any(|arg| !cat_stdin_argument(arg))
    {
        return None;
    }

    let target_words = shell_words(right)?;
    (target_words.len() == 1).then(|| target_words[0].clone())
}

fn cat_stdin_argument(argument: &[u8]) -> bool {
    if matches!(argument, b"-" | b"--") {
        return true;
    }
    let Some(flags) = argument.strip_prefix(b"-") else {
        return false;
    };
    !flags.is_empty() && flags.iter().all(|flag| b"AETbenstuv".contains(flag))
}

fn split_simple_stdout_redirect(command: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut quote = None;
    let mut escaped = false;
    let mut redirect = None;
    let mut i = 0;
    while i < command.len() {
        let byte = command[i];
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if delimiter == b'"' && byte == b'\\' {
                escaped = true;
            } else if byte == delimiter {
                quote = None;
            }
        } else if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
        } else if byte == b'>' {
            if redirect.is_some() {
                return None;
            }
            let width = if command.get(i + 1) == Some(&b'>') {
                2
            } else {
                1
            };
            redirect = Some((i, width));
            i += width - 1;
        } else if matches!(byte, b'|' | b'&' | b';' | b'<' | b'(' | b')') {
            return None;
        }
        i += 1;
    }
    if quote.is_some() || escaped {
        return None;
    }
    let (at, width) = redirect?;
    Some((
        command[..at].trim_ascii(),
        command[at + width..].trim_ascii(),
    ))
}

pub(super) fn breadcrumb_path_start(message: &[u8]) -> Option<usize> {
    const SINGULAR_PREFIXES: &[&[u8]] = &[
        b"touch processed path: ",
        b"mkdir completed for ",
        b"rm completed for ",
        b"killall completed for ",
    ];
    message
        .windows(2)
        .position(|window| window == b": ")
        .map(|position| position + 2)
        .or_else(|| {
            SINGULAR_PREFIXES
                .iter()
                .find_map(|prefix| message.starts_with(prefix).then_some(prefix.len()))
        })
}

#[cfg(test)]
pub(super) fn is_sensitive(command: &[u8], custom_rules: &[String]) -> bool {
    is_builtin_sensitive(command) || is_custom_sensitive(command, custom_rules)
}

fn is_builtin_sensitive(command: &[u8]) -> bool {
    // Two independent recognizers, OR-ed, because neither is a superset of the
    // other and a secret only needs one of them to fire.
    //
    // The structured pass sees past a redirection of stderr, because otherwise
    // that redirection *declassifies*: `gh auth token` is recognized while
    // `gh auth token 2>/dev/null` was not, since the operator diverted it to
    // the loose pass, and the loose pass only knows secret-reading *files*,
    // never secret-printing tools.
    //
    // The loose pass then runs unconditionally as a backstop, on the ORIGINAL
    // command. It is not merely a fallback for unparseable input: it matches a
    // reader by basename, so it catches `/bin/cat .env` where the structured
    // pass — which compares the whole word — does not. Making it a fallback
    // rather than a backstop is what turned `/bin/cat .env 2>/dev/null` from
    // Sensitive into Normal. OR-ing the two guarantees this function can only
    // ever recognize *more* than either pass alone, which is the direction the
    // contract below requires.
    let cleaned = without_stderr_redirection(command);
    let structured = shell_words(&cleaned).is_some_and(|words| {
        words.iter().enumerate().any(|(idx, word)| {
            let Ok(name) = std::str::from_utf8(word) else {
                return false;
            };
            // Compare the basename, the way `contains_sensitive_file_reader_loose`
            // and `reads_dotenv_file` already do. Matching the whole word made
            // `/bin/cat`, `./scripts/cat` and `/usr/local/bin/op` invisible to
            // this pass — the divergence from the loose pass that let a stderr
            // redirect declassify a secret reader.
            //
            // Known cost, accepted deliberately: this applies to every word, not
            // just command positions, so `ls -l /usr/bin/pass` is now Sensitive
            // and loses formatting. That is over-classification — output stays
            // raw, which is the safe direction — and narrowing it to word 0
            // would give up `sudo /usr/local/bin/gh auth token`, which is the
            // unsafe direction. The real cause is the `|| !args.is_empty()`
            // catch-all in `sensitive_pass_args`; tightening that is a loosening
            // edit and needs its own audit, so it is not done here.
            let name = name.rsplit('/').next().unwrap_or(name);
            let args = &words[idx + 1..];
            match name {
                "security" => sensitive_security_args(args),
                "gh" => sensitive_gh_args(args),
                "op" => sensitive_1password_args(args),
                "bw" => sensitive_bitwarden_args(args),
                "pass" => sensitive_pass_args(args),
                "aws" => sensitive_aws_args(args),
                "gcloud" => sensitive_gcloud_args(args),
                "doppler" => sensitive_doppler_args(args),
                "cat" | "head" | "tail" | "sed" | "more" => args
                    .iter()
                    .any(|arg| std::str::from_utf8(arg).is_ok_and(secret_file_argument)),
                _ => false,
            }
        })
    });
    // Operators deliberately make the structured parser decline. For secrets,
    // decline must mean *more* conservative, not less: a compound
    // `cat .env; ...` command stays raw rather than accidentally enabling
    // formatting and failure pinning.
    structured || contains_sensitive_file_reader_loose(command)
}

/// Test-only view of the loose backstop, so the monotonicity contract in
/// `is_builtin_sensitive` ("this function recognizes at least what the loose
/// pass recognizes") can be asserted as a property rather than by example.
#[cfg(test)]
pub(super) fn contains_sensitive_file_reader_loose_for_test(command: &[u8]) -> bool {
    contains_sensitive_file_reader_loose(command)
}

fn contains_sensitive_file_reader_loose(command: &[u8]) -> bool {
    // The strict parser declines compound commands. Keep the conservative
    // secret-file fallback, but associate reader arguments with their own
    // shell stage. A global token scan made an unrelated grep pattern such as
    // `grep "Failed password" log | tail -40` look sensitive merely because
    // `tail` occurred later in the pipeline, disabling all formatting.
    let Some(stages) = shell_stages(command) else {
        return true;
    };
    stages.iter().any(|words| {
        words.iter().enumerate().any(|(index, word)| {
            let Ok(name) = std::str::from_utf8(word) else {
                return false;
            };
            matches!(
                name.rsplit('/').next(),
                Some("cat" | "head" | "tail" | "sed" | "more")
            ) && words[index + 1..]
                .iter()
                .filter_map(|argument| std::str::from_utf8(argument).ok())
                .any(secret_file_argument)
        })
    })
}

/// Split valid-enough compound shell text into words grouped by execution
/// stage. Quotes and escapes are honored; pipes, lists, and grouping boundaries
/// end a stage, while redirection operands stay with the command they affect.
fn shell_stages(command: &[u8]) -> Option<Vec<Vec<Vec<u8>>>> {
    let mut stages = Vec::new();
    let mut words = Vec::new();
    let mut word = Vec::new();
    let mut quote = None;
    let mut index = 0;

    while index < command.len() {
        let byte = command[index];
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            } else if delimiter == b'"' && byte == b'\\' && index + 1 < command.len() {
                index += 1;
                word.push(command[index]);
            } else {
                word.push(byte);
            }
        } else if byte.is_ascii_whitespace() {
            push_shell_word(&mut words, &mut word);
        } else if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
        } else if byte == b'\\' && index + 1 < command.len() {
            index += 1;
            word.push(command[index]);
        } else if matches!(byte, b'|' | b'&' | b';' | b'(' | b')') {
            push_shell_word(&mut words, &mut word);
            push_shell_stage(&mut stages, &mut words);
            if command.get(index + 1) == Some(&byte) {
                index += 1;
            }
        } else if matches!(byte, b'<' | b'>') {
            push_shell_word(&mut words, &mut word);
            if command.get(index + 1) == Some(&byte) {
                index += 1;
            }
        } else {
            word.push(byte);
        }
        index += 1;
    }

    if quote.is_some() {
        return None;
    }
    push_shell_word(&mut words, &mut word);
    push_shell_stage(&mut stages, &mut words);
    Some(stages)
}

fn push_shell_word(words: &mut Vec<Vec<u8>>, word: &mut Vec<u8>) {
    if !word.is_empty() {
        words.push(std::mem::take(word));
    }
}

fn push_shell_stage(stages: &mut Vec<Vec<Vec<u8>>>, words: &mut Vec<Vec<u8>>) {
    if !words.is_empty() {
        stages.push(std::mem::take(words));
    }
}

fn is_custom_sensitive(command: &[u8], custom_rules: &[String]) -> bool {
    // Same reasoning as `is_builtin_sensitive`: a user's `sensitive_commands`
    // rule must not stop matching because the command redirected its stderr.
    let cleaned = without_stderr_redirection(command);
    let Some(words) = shell_words(&cleaned) else {
        return false;
    };
    custom_rules
        .iter()
        .any(|rule| sensitive_rule_matches(&words, rule))
}

fn reads_dotenv_file(command: &[u8]) -> bool {
    let Some(words) = shell_words(command) else {
        return false;
    };
    words.iter().enumerate().any(|(idx, word)| {
        let Ok(name) = std::str::from_utf8(word) else {
            return false;
        };
        if !matches!(
            name.rsplit('/').next(),
            Some("cat" | "head" | "tail" | "sed" | "more")
        ) {
            return false;
        }
        let sensitive_files = words[idx + 1..]
            .iter()
            .filter_map(|arg| std::str::from_utf8(arg).ok())
            .filter(|text| secret_file_argument(text))
            .collect::<Vec<_>>();
        !sensitive_files.is_empty()
            && sensitive_files
                .iter()
                .all(|text| is_dotenv_file_argument(text))
    })
}

/// Drop a redirection of stderr from a captured command, for **view selection
/// only**.
///
/// `shell_words` declines any command carrying an operator, which is right for
/// the two cases it was written for: with `cmd | other` the text on screen is
/// `other`'s output, and with `cmd > file` the only thing reaching the terminal
/// is stderr — in both, `cmd`'s view would be applied to bytes it does not own.
/// A redirection of *stderr* is different: `lsof 2>/dev/null` still writes its
/// own stdout to the terminal, so `lsof`'s view is exactly the right one, and
/// declining it is collateral damage from a check that cannot tell `2>` from
/// `>`. Recognized forms are `2>target`, `2>>target`, `2> target` and `2>&1`.
///
/// Anything else — a pipe, a stdout redirect, a quoted target, a malformed
/// trailer — returns the command untouched, so the caller declines exactly as
/// it does today. Borrowing on the common path keeps this free for the vast
/// majority of commands, which carry no `2>` at all.
///
/// This deliberately does NOT relax `shell_words` itself: `is_builtin_sensitive`
/// depends on an operator making the parser decline *into a stricter* loose
/// check, and that behavior must not change.
pub(super) fn without_stderr_redirection(command: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    if !command.windows(2).any(|pair| pair == b"2>") {
        return std::borrow::Cow::Borrowed(command);
    }
    let mut out = Vec::with_capacity(command.len());
    let mut index = 0;
    let mut quote: Option<u8> = None;
    let mut at_word_start = true;
    while index < command.len() {
        let byte = command[index];
        if let Some(open) = quote {
            // Escape handling must match `shell_words` exactly, or the two
            // disagree about what is quoted and this function can strip a `2>`
            // the shell would treat as ordinary text.
            if open == b'"' && byte == b'\\' && index + 1 < command.len() {
                out.push(byte);
                out.push(command[index + 1]);
                index += 2;
                continue;
            }
            if byte == open {
                quote = None;
            }
            out.push(byte);
            index += 1;
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
            out.push(byte);
            index += 1;
            at_word_start = false;
            continue;
        }
        if byte == b'\\' && index + 1 < command.len() {
            out.push(byte);
            out.push(command[index + 1]);
            index += 2;
            at_word_start = false;
            continue;
        }
        if byte.is_ascii_whitespace() {
            out.push(byte);
            index += 1;
            at_word_start = true;
            continue;
        }
        if at_word_start && command[index..].starts_with(b"2>") {
            match stderr_redirection_end(command, index) {
                Some(end) => {
                    index = end;
                    continue;
                }
                None => return std::borrow::Cow::Borrowed(command),
            }
        }
        if is_shell_operator(byte) {
            return std::borrow::Cow::Borrowed(command);
        }
        out.push(byte);
        index += 1;
        at_word_start = false;
    }
    if quote.is_some() {
        return std::borrow::Cow::Borrowed(command);
    }
    std::borrow::Cow::Owned(out)
}

/// End of the stderr redirection starting at `start`, or `None` when it is not
/// one this may safely drop (quoted or operator-bearing target, dangling `2>`).
fn stderr_redirection_end(command: &[u8], start: usize) -> Option<usize> {
    let mut index = start + 2;
    if command.get(index) == Some(&b'>') {
        index += 1;
    }
    if command
        .get(index..)
        .is_some_and(|rest| rest.starts_with(b"&1"))
    {
        // `2>&1` must end the word. Without this, `ls 2>&1extra` would strip
        // only the `2>&1` and hand back `ls extra` — a command the user never
        // typed, with a word that could carry a flag and steer view selection.
        let end = index + 2;
        return match command.get(end) {
            None => Some(end),
            Some(byte) if byte.is_ascii_whitespace() => Some(end),
            Some(_) => None,
        };
    }
    // `2> target` — skip the gap to reach a separated target.
    if command
        .get(index)
        .is_none_or(|byte| byte.is_ascii_whitespace())
    {
        while command
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            index += 1;
        }
        // A dangling `2>` with no target is malformed; leave the command alone.
        command.get(index)?;
    }
    let target_start = index;
    while let Some(byte) = command.get(index) {
        if byte.is_ascii_whitespace() {
            break;
        }
        // Backtick and `$` are rejected here, in the TARGET only — not added to
        // `is_shell_operator`, which the main loop uses: a command may perfectly
        // well contain `$PID`, and bailing on that would give up the whole
        // feature. A substituting target is harmless today (its output becomes
        // a filename and never reaches the terminal), but a target this
        // function cannot evaluate is one it should not silently drop.
        if matches!(byte, b'\'' | b'"' | b'`' | b'$') || is_shell_operator(*byte) {
            return None;
        }
        index += 1;
    }
    (index > target_start).then_some(index)
}

fn is_shell_operator(byte: u8) -> bool {
    matches!(byte, b'|' | b'&' | b';' | b'<' | b'>' | b'(' | b')')
}

/// Parse enough shell syntax for conservative command classification. Any
/// operator or unterminated quote declines classification rather than guessing.
pub(super) fn shell_words(command: &[u8]) -> Option<Vec<Vec<u8>>> {
    let mut words = Vec::new();
    let mut word = Vec::new();
    let mut quote = None;
    let mut i = 0;
    while i < command.len() {
        let b = command[i];
        if let Some(q) = quote {
            if b == q {
                quote = None;
            } else if q == b'"' && b == b'\\' && i + 1 < command.len() {
                i += 1;
                word.push(command[i]);
            } else {
                word.push(b);
            }
        } else if b.is_ascii_whitespace() {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else if matches!(b, b'\'' | b'"') {
            quote = Some(b);
        } else if matches!(b, b'|' | b'&' | b';' | b'<' | b'>' | b'(' | b')') {
            return None;
        } else if b == b'\\' && i + 1 < command.len() {
            i += 1;
            word.push(command[i]);
        } else {
            word.push(b);
        }
        i += 1;
    }
    if quote.is_some() {
        return None;
    }
    if !word.is_empty() {
        words.push(word);
    }
    Some(words)
}

fn sensitive_rule_matches(command_words: &[Vec<u8>], rule: &str) -> bool {
    let Some(rule_words) = shell_words(rule.as_bytes()) else {
        return false;
    };
    if rule_words.is_empty() || rule_words.len() > command_words.len() {
        return false;
    }
    command_words.windows(rule_words.len()).any(|window| {
        window
            .iter()
            .zip(&rule_words)
            .enumerate()
            .all(|(idx, (command_word, rule_word))| {
                if idx == 0 {
                    command_word.rsplit(|byte| *byte == b'/').next()
                        == rule_word.rsplit(|byte| *byte == b'/').next()
                } else {
                    command_word == rule_word
                }
            })
    })
}

fn simple_targets(args: &[Vec<u8>]) -> Option<Vec<Vec<u8>>> {
    let mut targets = Vec::new();
    let mut end_options = false;
    for arg in args {
        let text = std::str::from_utf8(arg).ok()?;
        if !end_options && text == "--" {
            end_options = true;
        } else if !end_options && text.starts_with('-') {
            return None;
        } else {
            targets.push(arg.clone());
        }
    }
    Some(targets)
}

fn push_target_summary(
    out: &mut Vec<u8>,
    verb: &[u8],
    count: usize,
    noun: &[u8],
    targets: &[Vec<u8>],
) {
    out.extend_from_slice(verb);
    out.push(b' ');
    out.extend_from_slice(count.to_string().as_bytes());
    out.push(b' ');
    out.extend_from_slice(noun);
    out.extend_from_slice(b": ");
    for (idx, target) in targets.iter().take(3).enumerate() {
        if idx > 0 {
            out.extend_from_slice(b", ");
        }
        out.extend_from_slice(&cmdline::sanitize_display(target));
    }
    if count > 3 {
        out.extend_from_slice(b", +");
        out.extend_from_slice((count - 3).to_string().as_bytes());
        out.extend_from_slice(b" more");
    }
}

fn command_targets(args: &[Vec<u8>], options_with_value: &[&str]) -> Vec<Vec<u8>> {
    let mut targets = Vec::new();
    let mut end_options = false;
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        let text = std::str::from_utf8(arg).unwrap_or("");
        if !end_options && text == "--" {
            end_options = true;
            continue;
        }
        if !end_options && text.starts_with('-') && text.len() > 1 {
            if options_with_value
                .iter()
                .any(|opt| text == *opt || text.starts_with(&format!("{opt}=")))
                && !text.contains('=')
            {
                skip_next = true;
            }
            continue;
        }
        targets.push(arg.clone());
    }
    targets
}

fn sensitive_security_args(args: &[Vec<u8>]) -> bool {
    let finds_password = args.iter().any(|arg| {
        matches!(
            std::str::from_utf8(arg),
            Ok("find-generic-password" | "find-internet-password")
        )
    });
    let prints_password = args
        .iter()
        .any(|arg| std::str::from_utf8(arg).is_ok_and(is_short_password_print_flag));
    finds_password && prints_password
}

fn sensitive_gh_args(args: &[Vec<u8>]) -> bool {
    arg_text(args, 0) == Some("auth") && arg_text(args, 1) == Some("token")
}

fn sensitive_1password_args(args: &[Vec<u8>]) -> bool {
    match (arg_text(args, 0), arg_text(args, 1)) {
        (Some("read"), _) => true,
        (Some("item"), Some("get")) => args.iter().any(|arg| {
            std::str::from_utf8(arg).is_ok_and(|text| {
                text == "--reveal"
                    || text.contains("password")
                    || text.contains("credential")
                    || text.contains("secret")
                    || text.contains("token")
            })
        }),
        _ => false,
    }
}

fn sensitive_bitwarden_args(args: &[Vec<u8>]) -> bool {
    arg_text(args, 0) == Some("get")
        && matches!(
            arg_text(args, 1),
            Some("password" | "item" | "notes" | "totp" | "attachment")
        )
}

fn sensitive_pass_args(args: &[Vec<u8>]) -> bool {
    matches!(arg_text(args, 0), Some("show") | Some("-c" | "--clip")) || !args.is_empty()
}

fn sensitive_aws_args(args: &[Vec<u8>]) -> bool {
    match (arg_text(args, 0), arg_text(args, 1), arg_text(args, 2)) {
        (Some("configure"), Some("get"), Some(key)) => contains_secret_word(key),
        (Some("secretsmanager"), Some("get-secret-value"), _) => true,
        (Some("ssm"), Some("get-parameter" | "get-parameters"), _) => args
            .iter()
            .any(|arg| std::str::from_utf8(arg).is_ok_and(|text| text == "--with-decryption")),
        _ => false,
    }
}

fn sensitive_gcloud_args(args: &[Vec<u8>]) -> bool {
    arg_text(args, 0) == Some("auth")
        && matches!(
            arg_text(args, 1),
            Some("print-access-token" | "print-identity-token")
        )
}

fn sensitive_doppler_args(args: &[Vec<u8>]) -> bool {
    arg_text(args, 0) == Some("secrets") && matches!(arg_text(args, 1), Some("get" | "download"))
}

fn arg_text(args: &[Vec<u8>], index: usize) -> Option<&str> {
    std::str::from_utf8(args.get(index)?).ok()
}

fn is_short_password_print_flag(text: &str) -> bool {
    text == "-w" || text.ends_with('w') && text.starts_with('-') && !text.starts_with("--")
}

fn secret_file_argument(text: &str) -> bool {
    if text.starts_with('-') || shell_operator_word(text) {
        return false;
    }
    let clean = text.trim_matches(|c| matches!(c, '"' | '\'' | '`' | ',' | ':' | ';' | ')' | '('));
    let lower = clean.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(lower.as_str());
    if matches!(
        name,
        ".env"
            | ".netrc"
            | ".npmrc"
            | ".pypirc"
            | ".dockercfg"
            | "id_rsa"
            | "id_dsa"
            | "id_ecdsa"
            | "id_ed25519"
    ) {
        return true;
    }
    if name.starts_with(".env.") {
        return !matches!(
            name,
            ".env.example" | ".env.sample" | ".env.template" | ".env.defaults"
        );
    }
    if name.ends_with(".env") {
        return true;
    }
    if name.ends_with(".pem") || name.ends_with(".key") {
        return true;
    }
    if lower.contains("/.aws/credentials")
        || lower.contains("/.kube/config")
        || lower.contains("/.config/gcloud/")
    {
        return true;
    }
    contains_secret_word(name)
}

fn is_dotenv_file_argument(text: &str) -> bool {
    if text.starts_with('-') || shell_operator_word(text) {
        return false;
    }
    let clean = text.trim_matches(|c| matches!(c, '"' | '\'' | '`' | ',' | ':' | ';' | ')' | '('));
    let lower = clean.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(lower.as_str());
    name == ".env" || name.starts_with(".env.") || name.ends_with(".env")
}

fn contains_secret_word(text: &str) -> bool {
    text.contains("secret")
        || text.contains("password")
        || text.contains("passwd")
        || text.contains("token")
        || text.contains("credential")
        || text.contains("private_key")
}

fn shell_operator_word(word: &str) -> bool {
    matches!(
        word,
        "|" | "||" | "&&" | ";" | ">" | ">>" | "<" | "2>" | "2>>"
    )
}
