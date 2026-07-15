//! Typed command policy and conservative shell-word parsing.
//!
//! This module decides whether a captured command is ordinary, interactive
//! bypass, or sensitive pass-through. It never formats output itself.

use super::cmdline;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommandTrust {
    Normal,
    InteractiveBypass,
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
    let trust = if is_sensitive(command, sensitive_rules) {
        CommandTrust::Sensitive
    } else if name
        .as_ref()
        .is_some_and(|name| bypass_names.iter().any(|bypass| bypass == name))
    {
        CommandTrust::InteractiveBypass
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
    if !matches!(name, "touch" | "mkdir" | "rm") {
        return None;
    }
    let args = &words[1..];
    let targets = match name {
        "touch" => command_targets(args, &["-t", "-d", "-r", "--date", "--reference"]),
        "mkdir" => command_targets(args, &["-m", "--mode", "-Z", "--context"]),
        "rm" => command_targets(args, &[]),
        _ => Vec::new(),
    };
    if targets.is_empty() {
        return None;
    }

    let mut out = Vec::new();
    match (name, targets.len()) {
        ("touch", 1) | ("mkdir", 1) | ("rm", 1) => {
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(b" completed for ");
            out.extend_from_slice(&cmdline::sanitize_display(&targets[0]));
        }
        ("touch", n) | ("mkdir", n) | ("rm", n) => {
            let mut verb = name.as_bytes().to_vec();
            verb.extend_from_slice(b" completed for");
            push_target_summary(&mut out, &verb, n, b"targets", &targets);
        }
        _ => return None,
    }
    Some(out)
}

pub(super) fn breadcrumb_path_start(message: &[u8]) -> Option<usize> {
    const SINGULAR_PREFIXES: &[&[u8]] = &[
        b"touch completed for ",
        b"mkdir completed for ",
        b"rm completed for ",
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

pub(super) fn is_sensitive(command: &[u8], custom_rules: &[String]) -> bool {
    let Some(words) = shell_words(command) else {
        return false;
    };
    let built_in = words.iter().enumerate().any(|(idx, word)| {
        let Ok(name) = std::str::from_utf8(word) else {
            return false;
        };
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
            "cat" | "head" | "tail" | "sed" => args
                .iter()
                .any(|arg| std::str::from_utf8(arg).is_ok_and(secret_file_argument)),
            _ => false,
        }
    });
    built_in
        || custom_rules
            .iter()
            .any(|rule| sensitive_rule_matches(&words, rule))
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
