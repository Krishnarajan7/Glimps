//! Lightweight source-code lexer for reader commands.

use super::super::theme::Theme;
use super::{find_sql_block_comment_end, lower_ascii, paint_bytes, split_line};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeLanguage {
    CLike,
    Css,
    Go,
    Java,
    JavaScript,
    Kotlin,
    Php,
    Python,
    Ruby,
    Rust,
    Shell,
    Swift,
    TypeScript,
}

/// Color a source-code line from reader commands (`cat`, `head`, `tail`, `sed`).
/// This is intentionally a lightweight visual lexer: it keeps layout intact,
/// avoids parser dependencies on the hot path, and passes through lines it cannot
/// improve.
pub fn colorize_code_line(line: &[u8], theme: &Theme, lang: CodeLanguage) -> Option<Vec<u8>> {
    if theme.reset.is_empty() {
        return None;
    }
    let (content, ending) = split_line(line);
    if content.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(content.len() + ending.len() + 64);
    let mut i = 0;
    let mut colored_any = false;
    while i < content.len() {
        let b = content[i];
        if let Some(end) = code_line_comment_end(content, i, lang) {
            paint_bytes(&mut out, theme.comment, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if let Some(end) = code_block_comment_end(content, i, lang) {
            paint_bytes(&mut out, theme.comment, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if let Some(end) = code_string_end(content, i, lang) {
            paint_bytes(&mut out, theme.string, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if starts_code_number(content, i) {
            let end = code_number_end(content, i);
            paint_bytes(&mut out, theme.number, &content[i..end], theme.reset);
            colored_any = true;
            i = end;
        } else if is_code_ident_start(b) {
            let end = code_ident_end(content, i);
            let word = &content[i..end];
            if is_code_keyword(word, lang) {
                paint_bytes(&mut out, theme.keyword, word, theme.reset);
                colored_any = true;
            } else if looks_like_code_constant(word) {
                paint_bytes(&mut out, theme.number, word, theme.reset);
                colored_any = true;
            } else if looks_like_function_call(content, end) {
                paint_bytes(&mut out, theme.key, word, theme.reset);
                colored_any = true;
            } else {
                out.extend_from_slice(word);
            }
            i = end;
        } else if is_code_punctuation(b, lang) {
            paint_bytes(&mut out, theme.html_delim, &content[i..i + 1], theme.reset);
            colored_any = true;
            i += 1;
        } else {
            out.push(b);
            i += 1;
        }
    }
    if !colored_any {
        return None;
    }
    out.extend_from_slice(ending);
    Some(out)
}

fn code_line_comment_end(bytes: &[u8], start: usize, lang: CodeLanguage) -> Option<usize> {
    match lang {
        CodeLanguage::Python | CodeLanguage::Ruby | CodeLanguage::Shell => {
            (bytes[start] == b'#').then_some(bytes.len())
        }
        CodeLanguage::Php => (bytes[start] == b'#'
            || (bytes[start] == b'/' && bytes.get(start + 1) == Some(&b'/')))
        .then_some(bytes.len()),
        CodeLanguage::Css => None,
        _ => (bytes[start] == b'/' && bytes.get(start + 1) == Some(&b'/')).then_some(bytes.len()),
    }
}

fn code_block_comment_end(bytes: &[u8], start: usize, lang: CodeLanguage) -> Option<usize> {
    if !matches!(
        lang,
        CodeLanguage::CLike
            | CodeLanguage::Css
            | CodeLanguage::Go
            | CodeLanguage::Java
            | CodeLanguage::JavaScript
            | CodeLanguage::Kotlin
            | CodeLanguage::Php
            | CodeLanguage::Rust
            | CodeLanguage::Swift
            | CodeLanguage::TypeScript
    ) {
        return None;
    }
    if bytes[start] != b'/' || bytes.get(start + 1) != Some(&b'*') {
        return None;
    }
    let end = find_sql_block_comment_end(&bytes[start + 2..])
        .map(|rel| start + 4 + rel)
        .unwrap_or(bytes.len());
    Some(end)
}

fn code_string_end(bytes: &[u8], start: usize, lang: CodeLanguage) -> Option<usize> {
    let quote = bytes[start];
    if quote == b'\'' && lang == CodeLanguage::Rust {
        return rust_char_literal_end(bytes, start);
    }
    if quote == b'\'' || quote == b'"' || (quote == b'`' && code_supports_backticks(lang)) {
        return Some(quoted_code_end(bytes, start, quote));
    }
    None
}

fn rust_char_literal_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let mut visible = 0;
    while i < bytes.len() && visible <= 8 {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            visible += 1;
            continue;
        }
        if bytes[i] == b'\'' {
            return Some(i + 1);
        }
        if bytes[i].is_ascii_whitespace() {
            return None;
        }
        i += 1;
        visible += 1;
    }
    None
}

fn code_supports_backticks(lang: CodeLanguage) -> bool {
    matches!(
        lang,
        CodeLanguage::Go
            | CodeLanguage::JavaScript
            | CodeLanguage::Shell
            | CodeLanguage::TypeScript
    )
}

fn quoted_code_end(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            return i + 1;
        }
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
        } else {
            i += 1;
        }
    }
    bytes.len()
}

fn starts_code_number(bytes: &[u8], start: usize) -> bool {
    let b = bytes[start];
    if b == b'.' {
        return bytes.get(start + 1).is_some_and(u8::is_ascii_digit)
            && start
                .checked_sub(1)
                .and_then(|idx| bytes.get(idx))
                .is_none_or(|prev| !is_code_ident_continue(*prev));
    }
    b.is_ascii_digit()
        && start
            .checked_sub(1)
            .and_then(|idx| bytes.get(idx))
            .is_none_or(|prev| !is_code_ident_continue(*prev))
}

fn code_number_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    if bytes.get(i) == Some(&b'0') && matches!(bytes.get(i + 1), Some(b'x' | b'X' | b'b' | b'B')) {
        i += 2;
    }
    while i < bytes.len()
        && matches!(
            bytes[i],
            b'0'..=b'9'
                | b'a'..=b'f'
                | b'A'..=b'F'
                | b'.'
                | b'_'
                | b'x'
                | b'X'
                | b'+'
                | b'-'
        )
    {
        i += 1;
    }
    i
}

fn is_code_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || matches!(b, b'_' | b'$')
}

fn is_code_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$')
}

fn code_ident_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && is_code_ident_continue(bytes[i]) {
        i += 1;
    }
    i
}

fn looks_like_code_constant(word: &[u8]) -> bool {
    let trimmed = word.strip_prefix(b"$").unwrap_or(word);
    trimmed.len() > 1
        && trimmed
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
        && trimmed.iter().any(u8::is_ascii_uppercase)
}

fn looks_like_function_call(bytes: &[u8], mut end: usize) -> bool {
    while end < bytes.len() && bytes[end].is_ascii_whitespace() {
        end += 1;
    }
    bytes.get(end) == Some(&b'(')
}

fn is_code_punctuation(b: u8, lang: CodeLanguage) -> bool {
    matches!(
        b,
        b'{' | b'}'
            | b'['
            | b']'
            | b'('
            | b')'
            | b','
            | b';'
            | b':'
            | b'.'
            | b'='
            | b'+'
            | b'-'
            | b'*'
            | b'/'
            | b'%'
            | b'<'
            | b'>'
            | b'!'
            | b'?'
            | b'&'
            | b'|'
            | b'^'
            | b'~'
            | b'@'
    ) || (lang == CodeLanguage::Shell && matches!(b, b'$'))
}

fn is_code_keyword(word: &[u8], lang: CodeLanguage) -> bool {
    match lang {
        CodeLanguage::Rust => is_rust_keyword(word),
        CodeLanguage::Python => is_python_keyword(word),
        CodeLanguage::Shell => is_shell_keyword(word),
        CodeLanguage::JavaScript => is_javascript_keyword(word),
        CodeLanguage::TypeScript => is_typescript_keyword(word),
        CodeLanguage::Go => is_go_keyword(word),
        CodeLanguage::Java => is_java_keyword(word),
        CodeLanguage::Kotlin => is_kotlin_keyword(word),
        CodeLanguage::Swift => is_swift_keyword(word),
        CodeLanguage::Ruby => is_ruby_keyword(word),
        CodeLanguage::Php => is_php_keyword(word),
        CodeLanguage::Css => is_css_keyword(word),
        CodeLanguage::CLike => is_c_like_keyword(word),
    }
}

fn is_rust_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"as"
            | b"async"
            | b"await"
            | b"break"
            | b"const"
            | b"continue"
            | b"crate"
            | b"dyn"
            | b"else"
            | b"enum"
            | b"extern"
            | b"false"
            | b"fn"
            | b"for"
            | b"if"
            | b"impl"
            | b"in"
            | b"let"
            | b"loop"
            | b"match"
            | b"mod"
            | b"move"
            | b"mut"
            | b"pub"
            | b"ref"
            | b"return"
            | b"self"
            | b"Self"
            | b"static"
            | b"struct"
            | b"super"
            | b"trait"
            | b"true"
            | b"type"
            | b"unsafe"
            | b"use"
            | b"where"
            | b"while"
    )
}

fn is_python_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"and"
            | b"as"
            | b"assert"
            | b"async"
            | b"await"
            | b"break"
            | b"class"
            | b"continue"
            | b"def"
            | b"del"
            | b"elif"
            | b"else"
            | b"except"
            | b"False"
            | b"finally"
            | b"for"
            | b"from"
            | b"global"
            | b"if"
            | b"import"
            | b"in"
            | b"is"
            | b"lambda"
            | b"None"
            | b"nonlocal"
            | b"not"
            | b"or"
            | b"pass"
            | b"raise"
            | b"return"
            | b"True"
            | b"try"
            | b"while"
            | b"with"
            | b"yield"
    )
}

fn is_shell_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"case"
            | b"do"
            | b"done"
            | b"elif"
            | b"else"
            | b"esac"
            | b"export"
            | b"fi"
            | b"for"
            | b"function"
            | b"if"
            | b"in"
            | b"local"
            | b"readonly"
            | b"return"
            | b"select"
            | b"set"
            | b"then"
            | b"until"
            | b"while"
    )
}

fn is_javascript_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"await"
            | b"async"
            | b"break"
            | b"case"
            | b"catch"
            | b"class"
            | b"const"
            | b"continue"
            | b"debugger"
            | b"default"
            | b"delete"
            | b"do"
            | b"else"
            | b"export"
            | b"extends"
            | b"false"
            | b"finally"
            | b"for"
            | b"from"
            | b"function"
            | b"if"
            | b"import"
            | b"in"
            | b"instanceof"
            | b"let"
            | b"new"
            | b"null"
            | b"of"
            | b"return"
            | b"super"
            | b"switch"
            | b"this"
            | b"throw"
            | b"true"
            | b"try"
            | b"typeof"
            | b"undefined"
            | b"var"
            | b"void"
            | b"while"
            | b"with"
            | b"yield"
    )
}

fn is_typescript_keyword(word: &[u8]) -> bool {
    is_javascript_keyword(word)
        || matches!(
            word,
            b"any"
                | b"boolean"
                | b"declare"
                | b"enum"
                | b"implements"
                | b"interface"
                | b"keyof"
                | b"namespace"
                | b"never"
                | b"number"
                | b"private"
                | b"protected"
                | b"public"
                | b"readonly"
                | b"string"
                | b"type"
                | b"unknown"
        )
}

fn is_go_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"break"
            | b"case"
            | b"chan"
            | b"const"
            | b"continue"
            | b"default"
            | b"defer"
            | b"else"
            | b"fallthrough"
            | b"false"
            | b"for"
            | b"func"
            | b"go"
            | b"goto"
            | b"if"
            | b"import"
            | b"interface"
            | b"map"
            | b"nil"
            | b"package"
            | b"range"
            | b"return"
            | b"select"
            | b"struct"
            | b"switch"
            | b"true"
            | b"type"
            | b"var"
    )
}

fn is_java_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"abstract"
            | b"assert"
            | b"boolean"
            | b"break"
            | b"byte"
            | b"case"
            | b"catch"
            | b"char"
            | b"class"
            | b"const"
            | b"continue"
            | b"default"
            | b"do"
            | b"double"
            | b"else"
            | b"enum"
            | b"extends"
            | b"false"
            | b"final"
            | b"finally"
            | b"float"
            | b"for"
            | b"if"
            | b"implements"
            | b"import"
            | b"instanceof"
            | b"int"
            | b"interface"
            | b"long"
            | b"new"
            | b"null"
            | b"package"
            | b"private"
            | b"protected"
            | b"public"
            | b"return"
            | b"short"
            | b"static"
            | b"strictfp"
            | b"super"
            | b"switch"
            | b"synchronized"
            | b"this"
            | b"throw"
            | b"throws"
            | b"transient"
            | b"true"
            | b"try"
            | b"void"
            | b"volatile"
            | b"while"
    )
}

fn is_kotlin_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"as"
            | b"break"
            | b"class"
            | b"continue"
            | b"data"
            | b"do"
            | b"else"
            | b"false"
            | b"for"
            | b"fun"
            | b"if"
            | b"in"
            | b"interface"
            | b"is"
            | b"null"
            | b"object"
            | b"package"
            | b"return"
            | b"super"
            | b"this"
            | b"throw"
            | b"true"
            | b"try"
            | b"typealias"
            | b"val"
            | b"var"
            | b"when"
            | b"while"
    )
}

fn is_swift_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"as"
            | b"associatedtype"
            | b"break"
            | b"case"
            | b"catch"
            | b"class"
            | b"continue"
            | b"default"
            | b"defer"
            | b"do"
            | b"else"
            | b"enum"
            | b"extension"
            | b"false"
            | b"for"
            | b"func"
            | b"guard"
            | b"if"
            | b"import"
            | b"in"
            | b"init"
            | b"inout"
            | b"let"
            | b"nil"
            | b"private"
            | b"protocol"
            | b"public"
            | b"return"
            | b"self"
            | b"static"
            | b"struct"
            | b"super"
            | b"switch"
            | b"throw"
            | b"true"
            | b"try"
            | b"typealias"
            | b"var"
            | b"where"
            | b"while"
    )
}

fn is_ruby_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"alias"
            | b"and"
            | b"begin"
            | b"break"
            | b"case"
            | b"class"
            | b"def"
            | b"defined?"
            | b"do"
            | b"else"
            | b"elsif"
            | b"end"
            | b"ensure"
            | b"false"
            | b"for"
            | b"if"
            | b"in"
            | b"module"
            | b"next"
            | b"nil"
            | b"not"
            | b"or"
            | b"redo"
            | b"rescue"
            | b"retry"
            | b"return"
            | b"self"
            | b"super"
            | b"then"
            | b"true"
            | b"undef"
            | b"unless"
            | b"until"
            | b"when"
            | b"while"
            | b"yield"
    )
}

fn is_php_keyword(word: &[u8]) -> bool {
    matches!(
        lower_ascii(word).as_slice(),
        b"abstract"
            | b"and"
            | b"array"
            | b"as"
            | b"break"
            | b"callable"
            | b"case"
            | b"catch"
            | b"class"
            | b"clone"
            | b"const"
            | b"continue"
            | b"declare"
            | b"default"
            | b"die"
            | b"do"
            | b"echo"
            | b"else"
            | b"elseif"
            | b"empty"
            | b"endfor"
            | b"endforeach"
            | b"endif"
            | b"endswitch"
            | b"endwhile"
            | b"extends"
            | b"false"
            | b"final"
            | b"finally"
            | b"for"
            | b"foreach"
            | b"function"
            | b"global"
            | b"if"
            | b"implements"
            | b"include"
            | b"instanceof"
            | b"interface"
            | b"isset"
            | b"namespace"
            | b"new"
            | b"null"
            | b"or"
            | b"print"
            | b"private"
            | b"protected"
            | b"public"
            | b"require"
            | b"return"
            | b"static"
            | b"switch"
            | b"throw"
            | b"trait"
            | b"true"
            | b"try"
            | b"unset"
            | b"use"
            | b"var"
            | b"while"
            | b"xor"
    )
}

fn is_css_keyword(word: &[u8]) -> bool {
    matches!(
        lower_ascii(word).as_slice(),
        b"absolute"
            | b"auto"
            | b"block"
            | b"border-box"
            | b"center"
            | b"flex"
            | b"fixed"
            | b"grid"
            | b"hidden"
            | b"important"
            | b"inline"
            | b"none"
            | b"relative"
            | b"solid"
            | b"static"
            | b"sticky"
            | b"transparent"
    )
}

fn is_c_like_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"auto"
            | b"bool"
            | b"break"
            | b"case"
            | b"char"
            | b"class"
            | b"const"
            | b"constexpr"
            | b"continue"
            | b"default"
            | b"delete"
            | b"do"
            | b"double"
            | b"else"
            | b"enum"
            | b"extern"
            | b"false"
            | b"float"
            | b"for"
            | b"if"
            | b"inline"
            | b"int"
            | b"long"
            | b"namespace"
            | b"new"
            | b"nullptr"
            | b"private"
            | b"protected"
            | b"public"
            | b"return"
            | b"short"
            | b"signed"
            | b"sizeof"
            | b"static"
            | b"struct"
            | b"switch"
            | b"template"
            | b"this"
            | b"true"
            | b"typedef"
            | b"typename"
            | b"union"
            | b"unsigned"
            | b"using"
            | b"void"
            | b"volatile"
            | b"while"
    )
}
