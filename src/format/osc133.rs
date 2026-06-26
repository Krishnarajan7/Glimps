//! OSC-133 shell-integration scanner — GLIMPS's sense of "where am I in the stream".
//!
//! Knowing whether a given byte belongs to the PROMPT, the user's typed INPUT,
//! or a command's OUTPUT is the technical core of GLIMPS: we may only ever
//! reformat OUTPUT, and must never touch the prompt or input (CLAUDE.md
//! architecture truth). The boundary is found via OSC-133 markers, not guessing.
//!
//! The markers (FinalTerm / iTerm2 / VS Code "semantic prompt" convention):
//!   OSC 133 ; A ST   prompt start          -> PROMPT zone
//!   OSC 133 ; B ST   prompt end / input    -> INPUT zone
//!   OSC 133 ; C ST   command / output start -> OUTPUT zone
//!   OSC 133 ; D ST   command end           -> UNKNOWN (between commands)
//! where OSC = `ESC ]` (0x1B 0x5D) and ST = `ESC \` (0x1B 0x5C) or BEL (0x07).
//!
//! ## Why this parses more than OSC
//! A robust scanner cannot treat the bytes inside *other* string-type escape
//! sequences — DCS (`ESC P`), SOS (`ESC X`), PM (`ESC ^`), APC (`ESC _`) — as
//! ground text. Their payloads are arbitrary and could otherwise contain bytes
//! that look like a `133;C` marker and spoof a false zone transition. We model
//! all of them as string bodies consumed up to their terminator, and classify
//! only the OSC ones. This keeps with "default to pass-through / never guess".
//!
//! ## Design invariants
//! * **Observe-only.** This scanner never produces output bytes; it only
//!   advances internal zone state. Byte-safety of the stream is therefore the
//!   trivial consequence of the caller emitting the input unchanged.
//! * **Resumable across chunk boundaries.** PTY reads split escape sequences at
//!   arbitrary points. State (`ParseState` + a tiny bounded `osc_buf`) persists
//!   between `feed` calls, so a marker split across reads is still recognized.
//!   Feeding the same total stream in any chunking yields the same final zone.
//! * **O(1) memory.** `osc_buf` captures only the first few bytes of an OSC body
//!   (enough to read `133;X`); a long payload (e.g. an OSC 52 clipboard blob) is
//!   scanned-through for its terminator without being stored.
//! * **Never panics.** Pure byte matching, no indexing that can go out of
//!   bounds, no allocation beyond the capped buffer.

/// Which part of the terminal stream the most recent bytes belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// Initial state, and the gap between a command finishing (`D`) and the next
    /// prompt starting (`A`). Treated as "do not reformat".
    Unknown,
    /// The shell prompt itself (`A`..`B`). Never reformat.
    Prompt,
    /// The command line the user typed, as echoed back (`B`..`C`). Never reformat.
    Input,
    /// A command's output (`C`..`D`). The *only* zone GLIMPS may reformat.
    Output,
}

/// How a single byte should be treated by emission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ByteClass {
    /// A ground-state content byte belonging to `zone`. Only `Content(Output)`
    /// bytes may ever be reformatted.
    Content(Zone),
    /// Part of an escape sequence (ESC, introducer, string body, terminator).
    /// Always passed through untouched.
    Control,
}

/// A segment of a chunk, as reported by [`Osc133Scanner::feed_segments`]. The
/// byte ranges are half-open indices into the chunk; concatenating the `Pass`
/// and `Output` ranges in order reconstructs the chunk exactly. `OutputStart`
/// and `OutputEnd` are zero-width markers for the rising/falling edges of the
/// OUTPUT zone (a command's output beginning / ending), used to frame output
/// (e.g. inject a separator) without scanning bytes twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seg {
    /// Pass-through bytes (markers, prompt, typed input, other zones). Verbatim.
    Pass(usize, usize),
    /// Command-output content bytes — the only bytes a formatter may touch.
    Output(usize, usize),
    /// The OUTPUT zone just began (a `C` marker completed).
    OutputStart,
    /// The OUTPUT zone just ended (a `D` marker, or any zone change out of it).
    OutputEnd,
}

/// Bytes of an OSC body we retain to classify the marker. `133;D;<code>` is the
/// longest one we care about; 16 bytes is comfortably enough, and anything
/// longer is some other (uninteresting) OSC we only scan for its terminator.
const OSC_PREFIX_CAP: usize = 16;

/// The kind of string-type escape sequence we are consuming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StringKind {
    /// `ESC ]` — Operating System Command. We classify its body for OSC-133 and
    /// it may also be terminated by BEL (xterm convention).
    Osc,
    /// DCS / SOS / PM / APC — consumed-but-ignored so their payloads can't spoof
    /// a marker. Terminated only by ST.
    Other,
}

/// The escape-sequence parser's position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseState {
    /// Normal bytes; nothing in progress.
    Ground,
    /// Just saw `ESC`; awaiting the sequence introducer.
    Esc,
    /// Inside a string body; collecting until a terminator.
    StringBody(StringKind),
    /// Inside a string body and just saw `ESC`; `\` completes the ST terminator.
    StringEsc(StringKind),
}

const ESC: u8 = 0x1B;
const BEL: u8 = 0x07;
const OSC_INTRODUCER: u8 = b']'; // 0x5D
const DCS_INTRODUCER: u8 = b'P'; // 0x50
const SOS_INTRODUCER: u8 = b'X'; // 0x58
const PM_INTRODUCER: u8 = b'^'; // 0x5E
const APC_INTRODUCER: u8 = b'_'; // 0x5F
const ST_FINAL: u8 = b'\\'; // 0x5C, the second byte of `ESC \`

/// Incremental OSC-133 zone scanner. Cheap to feed; holds only the current zone,
/// a parse state, and a small bounded buffer.
pub struct Osc133Scanner {
    zone: Zone,
    state: ParseState,
    /// Captured prefix of the in-progress OSC body (bounded by `OSC_PREFIX_CAP`).
    osc_buf: Vec<u8>,
}

impl Osc133Scanner {
    pub fn new() -> Self {
        Osc133Scanner {
            zone: Zone::Unknown,
            state: ParseState::Ground,
            osc_buf: Vec::with_capacity(OSC_PREFIX_CAP),
        }
    }

    /// The zone the stream is currently in, i.e. the zone the *next* byte fed
    /// would belong to. Wired into emission by the formatters that arrive in the
    /// next increment; until then it is read only by tests.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn zone(&self) -> Zone {
        self.zone
    }

    /// Advance the scanner over a chunk of stream bytes. Pure state update; emits
    /// nothing. Safe on arbitrary input. (Observe-only convenience; production
    /// emission goes through [`feed_segments`]. Currently exercised by tests.)
    ///
    /// [`feed_segments`]: Self::feed_segments
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn feed(&mut self, chunk: &[u8]) {
        for &b in chunk {
            self.feed_byte(b);
        }
    }

    /// Partition the chunk into [`Seg`]s: maximal `Pass`/`Output` byte runs plus
    /// zero-width `OutputStart`/`OutputEnd` edge markers at the boundaries of the
    /// OUTPUT zone. Concatenating the `Pass` and `Output` ranges in order
    /// reconstructs the chunk exactly; the edge markers carry no bytes. Resumes
    /// correctly across chunk boundaries (a marker split across reads still flips
    /// the zone on the byte that completes it).
    pub fn feed_segments(&mut self, chunk: &[u8]) -> Vec<Seg> {
        let mut segs: Vec<Seg> = Vec::new();
        let mut run_start = 0usize;
        let mut run_output: Option<bool> = None;
        let mut prev_zone = self.zone();

        for (i, &b) in chunk.iter().enumerate() {
            let is_output = matches!(self.feed_byte(b), ByteClass::Content(Zone::Output));

            // Group consecutive bytes of the same kind into a run.
            match run_output {
                None => {
                    run_output = Some(is_output);
                    run_start = i;
                }
                Some(prev) if prev != is_output => {
                    push_run(&mut segs, prev, run_start, i);
                    run_start = i;
                    run_output = Some(is_output);
                }
                _ => {}
            }

            // Detect an OUTPUT-zone edge caused by the byte just consumed (the
            // marker's terminator). The byte belongs to the current run (it is a
            // control byte), so flush up to and including it, then emit the edge.
            let zone = self.zone();
            let entered = zone == Zone::Output && prev_zone != Zone::Output;
            let left = zone != Zone::Output && prev_zone == Zone::Output;
            if entered || left {
                push_run(&mut segs, run_output.unwrap_or(false), run_start, i + 1);
                run_start = i + 1;
                run_output = None;
                segs.push(if entered {
                    Seg::OutputStart
                } else {
                    Seg::OutputEnd
                });
            }
            prev_zone = zone;
        }

        if let Some(prev) = run_output {
            push_run(&mut segs, prev, run_start, chunk.len());
        }
        segs
    }

    /// Run the state machine over one byte and report how that byte should be
    /// treated by emission: ground-state, non-ESC bytes are `Content(zone)`;
    /// everything else (ESC, sequence introducers, string bodies, terminators)
    /// is `Control` and must pass through untouched.
    fn feed_byte(&mut self, b: u8) -> ByteClass {
        // Classify from the state *entering* this byte. Zone transitions only
        // happen while consuming control bytes (`finish_string`), so a
        // `Content` byte's zone is stable across this call.
        let class = match self.state {
            ParseState::Ground if b != ESC => ByteClass::Content(self.zone),
            _ => ByteClass::Control,
        };

        // The only re-dispatch is StringEsc -> Esc (an ESC inside a string that
        // wasn't ST starts a fresh sequence). That's a single retry, so two
        // iterations is a hard upper bound — no risk of an unbounded loop.
        for _ in 0..2 {
            match self.state {
                ParseState::Ground => {
                    if b == ESC {
                        self.state = ParseState::Esc;
                    }
                    break;
                }
                ParseState::Esc => {
                    self.state = match b {
                        OSC_INTRODUCER => {
                            self.osc_buf.clear();
                            ParseState::StringBody(StringKind::Osc)
                        }
                        DCS_INTRODUCER | SOS_INTRODUCER | PM_INTRODUCER | APC_INTRODUCER => {
                            ParseState::StringBody(StringKind::Other)
                        }
                        // ESC ESC: stay primed on the latest ESC.
                        ESC => ParseState::Esc,
                        // Any other escape (CSI `ESC [`, two-char escapes, …) is
                        // not a string sequence we track; its introducer byte is
                        // consumed and we return to ground.
                        _ => ParseState::Ground,
                    };
                    break;
                }
                ParseState::StringBody(kind) => {
                    match b {
                        // BEL terminates an OSC (xterm); other string types use
                        // ST only, so for them BEL is just payload.
                        BEL if kind == StringKind::Osc => self.finish_string(kind),
                        ESC => self.state = ParseState::StringEsc(kind),
                        _ => {
                            if kind == StringKind::Osc && self.osc_buf.len() < OSC_PREFIX_CAP {
                                self.osc_buf.push(b);
                            }
                            // Past the cap (or for ignored string types) we keep
                            // scanning for the terminator without storing —
                            // bounds memory on long payloads.
                        }
                    }
                    break;
                }
                ParseState::StringEsc(kind) => {
                    if b == ST_FINAL {
                        // `ESC \` = ST: the string is complete.
                        self.finish_string(kind);
                        break;
                    }
                    // The ESC began a new control sequence rather than completing
                    // ST. Treat it as a fresh `ESC` introducer and re-handle this
                    // byte so a marker starting immediately after (e.g.
                    // `… ESC ]133;C`) isn't dropped.
                    self.state = ParseState::Esc;
                    // loop once more to reprocess `b` in the Esc state
                }
            }
        }

        class
    }

    /// A string sequence terminated. If it was an OSC, classify its body and
    /// update the zone; either way reset to Ground.
    fn finish_string(&mut self, kind: StringKind) {
        if kind == StringKind::Osc {
            if let Some(zone) = classify_osc133(&self.osc_buf) {
                self.zone = zone;
            }
        }
        self.osc_buf.clear();
        self.state = ParseState::Ground;
    }
}

impl Default for Osc133Scanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Push a non-empty byte run as the appropriate [`Seg`]. Empty runs (which occur
/// right after an edge marker) are skipped so segments never carry zero bytes.
fn push_run(segs: &mut Vec<Seg>, is_output: bool, start: usize, end: usize) {
    if end > start {
        segs.push(if is_output {
            Seg::Output(start, end)
        } else {
            Seg::Pass(start, end)
        });
    }
}

/// Map an OSC body to the zone it begins, if it is an OSC-133 marker we track.
/// Returns `None` for any non-133 or unrecognized body (leave zone unchanged).
fn classify_osc133(body: &[u8]) -> Option<Zone> {
    // Expect `133;<C>` optionally followed by `;...`. Match the prefix exactly.
    let rest = body.strip_prefix(b"133;")?;
    match rest.first()? {
        b'A' => Some(Zone::Prompt),
        b'B' => Some(Zone::Input),
        b'C' => Some(Zone::Output),
        b'D' => Some(Zone::Unknown),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ESC ] 133 ; <c> BEL`
    fn marker_bel(c: char) -> Vec<u8> {
        let mut v = vec![ESC, OSC_INTRODUCER];
        v.extend_from_slice(format!("133;{c}").as_bytes());
        v.push(BEL);
        v
    }

    /// `ESC ] 133 ; <c> ESC \`
    fn marker_st(c: char) -> Vec<u8> {
        let mut v = vec![ESC, OSC_INTRODUCER];
        v.extend_from_slice(format!("133;{c}").as_bytes());
        v.push(ESC);
        v.push(ST_FINAL);
        v
    }

    /// Marker bytes for code 0..4 mapped to (bytes, resulting zone). Alternates
    /// BEL/ST terminators to exercise both.
    fn marker_for(idx: u8) -> (Vec<u8>, Zone) {
        match idx % 4 {
            0 => (marker_bel('A'), Zone::Prompt),
            1 => (marker_st('B'), Zone::Input),
            2 => (marker_bel('C'), Zone::Output),
            _ => (marker_st('D'), Zone::Unknown),
        }
    }

    #[test]
    fn starts_unknown() {
        assert_eq!(Osc133Scanner::new().zone(), Zone::Unknown);
    }

    #[test]
    fn bel_terminated_transitions() {
        let mut s = Osc133Scanner::new();
        s.feed(&marker_bel('A'));
        assert_eq!(s.zone(), Zone::Prompt);
        s.feed(&marker_bel('B'));
        assert_eq!(s.zone(), Zone::Input);
        s.feed(&marker_bel('C'));
        assert_eq!(s.zone(), Zone::Output);
        s.feed(&marker_bel('D'));
        assert_eq!(s.zone(), Zone::Unknown);
    }

    #[test]
    fn st_terminated_transitions() {
        let mut s = Osc133Scanner::new();
        s.feed(&marker_st('A'));
        assert_eq!(s.zone(), Zone::Prompt);
        s.feed(&marker_st('C'));
        assert_eq!(s.zone(), Zone::Output);
        s.feed(&marker_st('D'));
        assert_eq!(s.zone(), Zone::Unknown);
    }

    #[test]
    fn d_with_exit_code_payload() {
        let mut s = Osc133Scanner::new();
        s.feed(&marker_bel('C'));
        assert_eq!(s.zone(), Zone::Output);
        // OSC 133 ; D ; 0  BEL
        let mut v = vec![ESC, OSC_INTRODUCER];
        v.extend_from_slice(b"133;D;0");
        v.push(BEL);
        s.feed(&v);
        assert_eq!(s.zone(), Zone::Unknown);
    }

    #[test]
    fn output_surrounds_real_content() {
        let mut s = Osc133Scanner::new();
        s.feed(&marker_bel('C'));
        s.feed(b"{\"hello\": \"world\"}\n");
        assert_eq!(s.zone(), Zone::Output);
        s.feed(&marker_bel('D'));
        assert_eq!(s.zone(), Zone::Unknown);
    }

    #[test]
    fn marker_split_across_every_byte_boundary() {
        let mut stream = Vec::new();
        stream.extend_from_slice(&marker_bel('A'));
        stream.extend_from_slice(b"prompt$ ");
        stream.extend_from_slice(&marker_st('B'));
        stream.extend_from_slice(b"curl example.com");
        stream.extend_from_slice(&marker_bel('C'));
        stream.extend_from_slice(b"body");

        let mut byte_at_a_time = Osc133Scanner::new();
        for &b in &stream {
            byte_at_a_time.feed(&[b]);
        }
        let mut whole = Osc133Scanner::new();
        whole.feed(&stream);

        assert_eq!(byte_at_a_time.zone(), Zone::Output);
        assert_eq!(whole.zone(), byte_at_a_time.zone());
    }

    #[test]
    fn st_marker_split_exactly_at_esc_backslash_seam() {
        // The highest-risk boundary: a read ends between the ESC and the `\` of
        // an ST terminator. The scanner must resume in StringEsc and still
        // recognize the marker.
        let m = marker_st('C'); // … ESC \
        let split = m.len() - 1; // boundary right before the final `\`
        let mut s = Osc133Scanner::new();
        s.feed(&m[..split]);
        assert_eq!(s.zone(), Zone::Unknown); // not yet terminated
        s.feed(&m[split..]);
        assert_eq!(s.zone(), Zone::Output);
    }

    #[test]
    fn non_133_osc_is_ignored() {
        let mut s = Osc133Scanner::new();
        s.feed(&marker_bel('C'));
        s.feed(b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07");
        s.feed(b"\x1b]0;my title\x07");
        assert_eq!(s.zone(), Zone::Output);
    }

    #[test]
    fn dcs_payload_cannot_spoof_a_marker() {
        // A DCS string whose payload literally contains `]133;A...` must NOT be
        // read as a marker — that is the whole point of modeling string types.
        let mut s = Osc133Scanner::new();
        s.feed(&marker_bel('C')); // we are in Output
        let mut v = vec![ESC, DCS_INTRODUCER];
        v.extend_from_slice(b"q]133;A;evil"); // looks marker-ish, but it's DCS payload
        v.push(ESC);
        v.push(ST_FINAL); // ST closes the DCS
        s.feed(&v);
        // Still Output — the fake `133;A` inside the DCS was ignored.
        assert_eq!(s.zone(), Zone::Output);
    }

    #[test]
    fn long_osc_payload_stays_within_cap_mid_scan() {
        // Feed a huge, *unterminated* OSC body and assert the buffer never grows
        // past the cap while we are still mid-OSC (the moment the cap must bind).
        let mut s = Osc133Scanner::new();
        let mut v = vec![ESC, OSC_INTRODUCER];
        v.extend_from_slice(b"52;c;"); // OSC 52 clipboard
        v.extend(std::iter::repeat_n(b'Q', 100_000));
        s.feed(&v); // no terminator yet
        assert!(matches!(s.state, ParseState::StringBody(StringKind::Osc)));
        assert!(s.osc_buf.len() <= OSC_PREFIX_CAP);
        assert_eq!(s.zone(), Zone::Unknown);
    }

    #[test]
    fn esc_inside_osc_starting_new_marker_is_recovered() {
        let mut s = Osc133Scanner::new();
        let mut v = vec![ESC, OSC_INTRODUCER];
        v.extend_from_slice(b"0;title-no-terminator");
        // ESC that is NOT followed by '\\' -> abandons, then a real C marker.
        v.extend_from_slice(&marker_bel('C'));
        s.feed(&v);
        assert_eq!(s.zone(), Zone::Output);
    }

    /// Reconstruct a chunk from the byte-bearing segments to prove nothing is
    /// dropped or reordered.
    fn rebuild(chunk: &[u8], segs: &[Seg]) -> Vec<u8> {
        let mut out = Vec::new();
        for seg in segs {
            match *seg {
                Seg::Pass(a, b) | Seg::Output(a, b) => out.extend_from_slice(&chunk[a..b]),
                Seg::OutputStart | Seg::OutputEnd => {}
            }
        }
        out
    }

    #[test]
    fn feed_segments_partitions_output_and_marks_edges() {
        let mut s = Osc133Scanner::new();
        let mut stream = Vec::new();
        stream.extend_from_slice(b"prompt$ "); // Unknown-zone content -> Pass
        stream.extend_from_slice(&marker_bel('C')); // -> OutputStart after it
        let out_start = stream.len();
        stream.extend_from_slice(b"hello output"); // Output content
        let out_end = stream.len();
        stream.extend_from_slice(&marker_bel('D')); // -> OutputEnd after it

        let segs = s.feed_segments(&stream);

        // Exact reconstruction (byte-safety of the partition).
        assert_eq!(rebuild(&stream, &segs), stream);

        // The output content is reported as exactly one Output run.
        let output_runs: Vec<(usize, usize)> = segs
            .iter()
            .filter_map(|seg| match *seg {
                Seg::Output(a, b) => Some((a, b)),
                _ => None,
            })
            .collect();
        assert_eq!(output_runs, vec![(out_start, out_end)]);

        // Exactly one rising and one falling edge, in order, surrounding output.
        let edges: Vec<Seg> = segs
            .iter()
            .copied()
            .filter(|s| matches!(s, Seg::OutputStart | Seg::OutputEnd))
            .collect();
        assert_eq!(edges, vec![Seg::OutputStart, Seg::OutputEnd]);
    }

    #[test]
    fn feed_segments_edges_survive_chunk_splits() {
        // The C marker is split across two feeds; the rising edge must still fire
        // exactly once, on the chunk that completes it.
        let mut s = Osc133Scanner::new();
        let m = marker_bel('C');
        let cut = m.len() - 1;
        let first = s.feed_segments(&m[..cut]);
        assert!(!first.iter().any(|x| matches!(x, Seg::OutputStart)));
        let second = s.feed_segments(&m[cut..]);
        let starts = second
            .iter()
            .filter(|x| matches!(x, Seg::OutputStart))
            .count();
        assert_eq!(starts, 1);
    }

    #[test]
    fn feed_segments_no_edge_for_ansi_within_output() {
        // An ANSI escape mid-output must NOT produce a spurious OutputStart/End:
        // the zone stays Output throughout.
        let mut s = Osc133Scanner::new();
        s.feed(&marker_bel('C')); // enter output (separate feed)
        let segs = s.feed_segments(b"ab\x1b[31mcd");
        assert!(!segs
            .iter()
            .any(|x| matches!(x, Seg::OutputStart | Seg::OutputEnd)));
    }

    #[test]
    fn arbitrary_garbage_never_panics() {
        let mut s = Osc133Scanner::new();
        for b in 0u16..=255 {
            s.feed(&[b as u8]);
        }
        let _ = s.zone();
    }

    /// Feed `stream` split at the given chunk sizes (cycled), returning the
    /// final zone. Used to prove chunking doesn't change the result.
    fn zone_after_chunks(stream: &[u8], sizes: &[usize]) -> Zone {
        let mut s = Osc133Scanner::new();
        let mut i = 0;
        let mut si = 0;
        while i < stream.len() {
            let want = sizes.get(si % sizes.len().max(1)).copied().unwrap_or(1);
            let step = want.clamp(1, stream.len() - i);
            s.feed(&stream[i..i + step]);
            i += step;
            si += 1;
        }
        s.zone()
    }

    proptest::proptest! {
        /// Arbitrary bytes, fed in arbitrary chunks, never panic.
        #[test]
        fn prop_never_panics(stream: Vec<u8>, sizes in proptest::collection::vec(1usize..8, 1..8)) {
            let _ = zone_after_chunks(&stream, &sizes);
        }

        /// Chunk-boundary invariance on arbitrary bytes: the final zone is
        /// independent of how the stream was split across `feed` calls.
        #[test]
        fn prop_chunking_invariant(stream: Vec<u8>, sizes in proptest::collection::vec(1usize..8, 1..8)) {
            let whole = {
                let mut s = Osc133Scanner::new();
                s.feed(&stream);
                s.zone()
            };
            proptest::prop_assert_eq!(zone_after_chunks(&stream, &sizes), whole);
        }

        /// Real markers interleaved with ESC-free filler: the final zone must
        /// equal the last marker's zone, regardless of chunking. This actually
        /// exercises the marker-recognition path (random bytes almost never form
        /// a valid `ESC ]133;` prefix on their own).
        #[test]
        fn prop_real_markers_detected_and_chunk_invariant(
            items in proptest::collection::vec(
                (proptest::option::of(0u8..4), proptest::collection::vec(0u8..=255, 0..8)),
                0..24,
            ),
            sizes in proptest::collection::vec(1usize..7, 1..7),
        ) {
            let mut stream = Vec::new();
            let mut expected = Zone::Unknown;
            for (marker, filler) in &items {
                // Strip ESC from filler so it cannot start an escape sequence
                // and perturb the deterministic expectation.
                stream.extend(filler.iter().copied().filter(|&b| b != ESC));
                if let Some(code) = marker {
                    let (bytes, zone) = marker_for(*code);
                    stream.extend_from_slice(&bytes);
                    expected = zone;
                }
            }
            let whole = {
                let mut s = Osc133Scanner::new();
                s.feed(&stream);
                s.zone()
            };
            proptest::prop_assert_eq!(whole, expected);
            proptest::prop_assert_eq!(zone_after_chunks(&stream, &sizes), expected);
        }
    }
}
