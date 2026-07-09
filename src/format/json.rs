//! JSON detector + pretty-printer — GLIMPS's first content formatter.
//!
//! Contract (per the `add-formatter` skill):
//! * [`detect`] is a cheap, high-precision O(n) gate. It only says "yes" for a
//!   buffer whose first non-whitespace byte is `{`/`[` and whose last is the
//!   matching `}`/`]`. Bare scalars (`42`, `"s"`, `true`) are intentionally NOT
//!   detected — they are rarely what a user wants reflowed and the precision
//!   guards against mangling normal output.
//! * [`format`] returns the reformatted bytes, and is **byte-safe**: on anything
//!   that does not fully parse as a single JSON value it returns the input
//!   unchanged (graceful degradation), never panics, and only ever emits valid
//!   UTF-8 (it is built from a `String`).
//!
//! Object key order is preserved (serde_json `preserve_order` feature) so we
//! never silently reorder the user's data.

use serde_json::Value;

use super::theme::Theme;

/// Cheap precheck: is this buffer plausibly a single JSON object or array?
/// Whitespace-trimmed, the first byte must open and the last must close the same
/// kind of container. This rejects the vast majority of normal output in O(n)
/// without any parsing.
pub fn detect(bytes: &[u8]) -> bool {
    let trimmed = trim_ascii_ws(bytes);
    matches!(
        (trimmed.first(), trimmed.last()),
        (Some(b'{'), Some(b'}')) | (Some(b'['), Some(b']'))
    )
}

/// Reformat `bytes` as pretty, colored JSON, returning `Some` only when it
/// actually reformatted — i.e. `bytes` is exactly one JSON value (per [`detect`]
/// plus a full parse). Returns `None` for anything else, so the caller can tell
/// "I generated these bytes" (newlines need CRLF on a raw terminal) from "pass
/// the user's bytes through verbatim".
///
/// The produced JSON uses `\n` line breaks, matching `serde_json`; turning them
/// into `\r\n` for the terminal is the caller's job (only GLIMPS-generated
/// newlines get that treatment, never the user's passed-through bytes).
pub fn try_format(bytes: &[u8], theme: &Theme) -> Option<Vec<u8>> {
    if !detect(bytes) {
        return None;
    }
    // Looked like JSON but didn't fully parse (truncated, trailing data, too
    // deep, …) -> None -> pass-through.
    let value = serde_json::from_slice::<Value>(bytes).ok()?;
    let mut out = String::with_capacity(bytes.len() * 2);
    write_value(&mut out, &value, 0, theme);
    Some(out.into_bytes())
}

/// Registry entry for the JSON formatter. See [`super::BufferedFormatter`].
pub struct Json;

impl super::BufferedFormatter for Json {
    fn could_start(&self, head: &[u8]) -> bool {
        matches!(head.first(), Some(b'{' | b'['))
    }
    fn try_format(&self, bytes: &[u8], theme: &Theme) -> Option<Vec<u8>> {
        try_format(bytes, theme)
    }
    fn label(&self) -> &'static str {
        "JSON"
    }
    fn needs_crlf(&self) -> bool {
        true // regenerates content with bare `\n`
    }
}

/// Convenience: reformat, or return the input unchanged if it isn't JSON. Test
/// scaffolding for golden comparisons; the live path uses [`try_format`] via the
/// `BufferedFormatter` registry.
#[cfg(test)]
pub fn format(bytes: &[u8], theme: &Theme) -> Vec<u8> {
    try_format(bytes, theme).unwrap_or_else(|| bytes.to_vec())
}

/// Trim leading/trailing ASCII whitespace without allocating.
fn trim_ascii_ws(mut bytes: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = bytes {
        if first.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    while let [rest @ .., last] = bytes {
        if last.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    bytes
}

const COMPACT_ARRAY_MAX_CHARS: usize = 96;

/// Recursively render `value` with 2-space indentation. Objects and nested
/// containers use a familiar pretty layout, while short scalar arrays stay on a
/// single line to keep terminal scrollback readable.
fn write_value(out: &mut String, value: &Value, depth: usize, theme: &Theme) {
    match value {
        Value::Null => paint(out, theme.keyword, theme.reset, "null"),
        Value::Bool(true) => paint(out, theme.keyword, theme.reset, "true"),
        Value::Bool(false) => paint(out, theme.keyword, theme.reset, "false"),
        Value::Number(n) => {
            out.push_str(theme.number);
            // `Number`'s Display is serde_json's canonical JSON form (identical to
            // its serializer for a given value), written without a temporary
            // allocation. Parity with `to_string_pretty` is proven by
            // `prop_matches_serde_pretty`.
            use std::fmt::Write as _;
            let _ = write!(out, "{n}");
            out.push_str(theme.reset);
        }
        Value::String(s) => {
            out.push_str(theme.string);
            escape_into(out, s);
            out.push_str(theme.reset);
        }
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            if write_compact_scalar_array(out, items, depth, theme) {
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                out.push('\n');
                indent(out, depth + 1);
                write_value(out, item, depth + 1, theme);
                if i + 1 != items.len() {
                    out.push(',');
                }
            }
            out.push('\n');
            indent(out, depth);
            out.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            let len = map.len();
            for (i, (key, val)) in map.iter().enumerate() {
                out.push('\n');
                indent(out, depth + 1);
                out.push_str(theme.key);
                escape_into(out, key);
                out.push_str(theme.reset);
                out.push_str(": ");
                write_value(out, val, depth + 1, theme);
                if i + 1 != len {
                    out.push(',');
                }
            }
            out.push('\n');
            indent(out, depth);
            out.push('}');
        }
    }
}

fn write_compact_scalar_array(
    out: &mut String,
    items: &[Value],
    depth: usize,
    theme: &Theme,
) -> bool {
    if !items.iter().all(is_scalar) {
        return false;
    }

    let plain_len = scalar_array_plain_len(items);
    if depth * 2 + plain_len > COMPACT_ARRAY_MAX_CHARS {
        return false;
    }

    out.push('[');
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_scalar_value(out, item, theme);
    }
    out.push(']');
    true
}

fn is_scalar(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

fn scalar_array_plain_len(items: &[Value]) -> usize {
    let mut out = String::new();
    out.push('[');
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_scalar_value(&mut out, item, &Theme::plain());
    }
    out.push(']');
    out.len()
}

fn write_scalar_value(out: &mut String, value: &Value, theme: &Theme) {
    match value {
        Value::Null => paint(out, theme.keyword, theme.reset, "null"),
        Value::Bool(true) => paint(out, theme.keyword, theme.reset, "true"),
        Value::Bool(false) => paint(out, theme.keyword, theme.reset, "false"),
        Value::Number(n) => {
            out.push_str(theme.number);
            use std::fmt::Write as _;
            let _ = write!(out, "{n}");
            out.push_str(theme.reset);
        }
        Value::String(s) => {
            out.push_str(theme.string);
            escape_into(out, s);
            out.push_str(theme.reset);
        }
        Value::Array(_) | Value::Object(_) => unreachable!("caller checked scalar values"),
    }
}

/// Wrap `text` in `color`…`reset`. For the plain theme both are empty, so this
/// is just `out.push_str(text)`.
fn paint(out: &mut String, color: &str, reset: &str, text: &str) {
    out.push_str(color);
    out.push_str(text);
    out.push_str(reset);
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// Append `s` as a JSON string literal (quoted + escaped), matching
/// serde_json's escaping: only `"`, `\`, and control characters `< 0x20` are
/// escaped; valid UTF-8 passes through unchanged.
fn escape_into(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(bytes: &[u8]) -> Vec<u8> {
        format(bytes, &Theme::plain())
    }

    // ---- golden-file tests -------------------------------------------------

    #[test]
    fn golden_object_plain() {
        let input = include_bytes!("../../tests/corpus/json/object.json");
        let expected = include_bytes!("../../tests/corpus/json/object.expected");
        assert_eq!(plain(input), trim_final_lf(expected));
    }

    #[test]
    fn golden_array_plain() {
        let input = include_bytes!("../../tests/corpus/json/array.json");
        let expected = include_bytes!("../../tests/corpus/json/array.expected");
        assert_eq!(plain(input), trim_final_lf(expected));
    }

    #[test]
    fn golden_malformed_passes_through_unchanged() {
        // A buffer that looks like JSON (starts `{`, ends `}`) but doesn't parse
        // must come back byte-for-byte.
        let input = include_bytes!("../../tests/corpus/json/malformed.json");
        assert_eq!(plain(input), input);
    }

    // ---- colored output ----------------------------------------------------

    #[test]
    fn colored_output_contains_ansi_and_is_valid_utf8() {
        let out = format(br#"{"k":1}"#, &Theme::default_colored());
        let s = std::str::from_utf8(&out).expect("valid utf8");
        assert!(s.contains("\x1b[36m")); // key color
        assert!(s.contains("\x1b[38;5;220m")); // number color
        assert!(s.contains("\x1b[0m")); // reset
    }

    #[test]
    fn preserves_object_key_order() {
        let out = plain(br#"{"zebra":1,"apple":2}"#);
        let s = std::str::from_utf8(&out).unwrap();
        let zebra = s.find("zebra").unwrap();
        let apple = s.find("apple").unwrap();
        assert!(zebra < apple, "key order must be preserved, not sorted");
    }

    #[test]
    fn empty_containers_render_inline() {
        assert_eq!(plain(b"{}"), b"{}");
        assert_eq!(plain(b"[]"), b"[]");
        assert_eq!(plain(b"{\"a\":{}}"), b"{\n  \"a\": {}\n}");
    }

    #[test]
    fn short_scalar_arrays_render_inline() {
        assert_eq!(
            plain(br#"{"items":[2,3,4,5,6,7,8,9,8,7,6,54,3]}"#),
            br#"{
  "items": [2, 3, 4, 5, 6, 7, 8, 9, 8, 7, 6, 54, 3]
}"#
        );
    }

    #[test]
    fn arrays_with_objects_stay_expanded() {
        assert_eq!(
            plain(br#"[{"id":1},{"id":2}]"#),
            br#"[
  {
    "id": 1
  },
  {
    "id": 2
  }
]"#
        );
    }

    #[test]
    fn surrounding_whitespace_is_tolerated() {
        assert_eq!(plain(b"  {\"a\":1}\n"), b"{\n  \"a\": 1\n}");
    }

    // ---- negative / precision tests ---------------------------------------

    #[test]
    fn does_not_detect_non_containers() {
        for s in [
            &b"42"[..],
            b"\"a string\"",
            b"true",
            b"null",
            b"hello world",
            b"",
            b"   ",
        ] {
            assert!(!detect(s), "should not detect: {s:?}");
            assert_eq!(format(s, &Theme::plain()), s); // unchanged
        }
    }

    #[test]
    fn looks_like_json_but_isnt_passes_through() {
        for s in [
            &b"{not json}"[..],
            b"{\"a\":1}{\"b\":2}", // two values: trailing data
            b"[1,2,",              // truncated
            b"{\"a\":}",           // missing value
        ] {
            // detect may say yes (first/last brackets match) but format must
            // return the bytes unchanged because the full parse fails.
            assert_eq!(format(s, &Theme::plain()), s, "must pass through: {s:?}");
        }
    }

    #[test]
    fn does_not_detect_partial_brackets() {
        assert!(!detect(b"{\"a\":1")); // no closing brace
        assert!(!detect(b"1,2]")); // no opening bracket
        assert!(!detect(b"hello {}")); // doesn't start with a container
    }

    // ---- property tests ----------------------------------------------------

    proptest::proptest! {
        /// For arbitrary bytes, `format` never panics and obeys its contract:
        /// when it actually reformats (detected AND parses) the output is valid
        /// UTF-8; otherwise it is a byte-identical pass-through (which may itself
        /// be invalid UTF-8 — preserving the stream is the point, and not a
        /// violation since we didn't introduce the invalid bytes).
        #[test]
        fn prop_byte_safe_and_never_panics(bytes: Vec<u8>) {
            let out = format(&bytes, &Theme::plain());
            let reformatted = detect(&bytes) && serde_json::from_slice::<Value>(&bytes).is_ok();
            if reformatted {
                proptest::prop_assert!(std::str::from_utf8(&out).is_ok());
            } else {
                proptest::prop_assert_eq!(out, bytes);
            }
        }

        /// Re-pretty-printing already-pretty JSON is a fixed point (idempotent),
        /// which also exercises the parse+print path on real JSON.
        #[test]
        fn prop_idempotent_on_valid_json(
            obj in proptest::collection::vec(
                (proptest::string::string_regex("[a-z]{1,6}").unwrap(), 0i64..1000),
                0..8,
            )
        ) {
            // Build a JSON object from generated key/value pairs.
            let map: serde_json::Map<String, Value> =
                obj.into_iter().map(|(k, v)| (k, Value::from(v))).collect();
            let source = serde_json::to_vec(&Value::Object(map)).unwrap();
            let once = format(&source, &Theme::plain());
            let twice = format(&once, &Theme::plain());
            proptest::prop_assert_eq!(once, twice);
        }

        /// The plain-theme printer must still emit semantically identical JSON on
        /// arbitrary JSON values, even though GLIMPS intentionally keeps short
        /// scalar arrays inline instead of matching serde_json's exact layout.
        ///
        /// Both sides start from the SAME parsed value (`from_slice(source)`):
        /// serde_json's float serialization is not round-trip-idempotent, so
        /// comparing against `to_string_pretty(original)` would compare two
        /// different values rather than testing our printer.
        #[test]
        fn prop_plain_output_round_trips_to_same_value(value in arb_container()) {
            let source = serde_json::to_vec(&value).unwrap();
            let parsed: Value = serde_json::from_slice(&source).unwrap();
            let ours = format(&source, &Theme::plain());
            let reparsed: Value = serde_json::from_slice(&ours).unwrap();
            proptest::prop_assert_eq!(reparsed, parsed);
        }
    }

    /// An arbitrary JSON value of bounded depth (used inside containers).
    fn arb_value() -> impl proptest::strategy::Strategy<Value = Value> {
        use proptest::prelude::*;
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(Value::from),
            any::<String>().prop_map(Value::String),
        ];
        leaf.prop_recursive(4, 48, 6, |inner| {
            prop_oneof![
                proptest::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
                proptest::collection::vec((any::<String>(), inner), 0..6)
                    .prop_map(|kvs| Value::Object(kvs.into_iter().collect())),
            ]
        })
    }

    /// A top-level JSON container (object or array) — the only thing `detect`
    /// accepts, so the only thing `format` will reformat.
    fn arb_container() -> impl proptest::strategy::Strategy<Value = Value> {
        use proptest::prelude::*;
        prop_oneof![
            proptest::collection::vec(arb_value(), 0..6).prop_map(Value::Array),
            proptest::collection::vec((any::<String>(), arb_value()), 0..6)
                .prop_map(|kvs| Value::Object(kvs.into_iter().collect())),
        ]
    }

    fn trim_final_lf(bytes: &[u8]) -> &[u8] {
        bytes.strip_suffix(b"\n").unwrap_or(bytes)
    }
}
