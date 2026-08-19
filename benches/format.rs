//! Latency-budget benchmarks for the formatting seam.
//!
//! These measure `Formatter::process` end-to-end (scan + decide + maybe format)
//! on three representative streams:
//!   * pure pass-through (no shell-integration markers) — must be ~free,
//!   * non-JSON command output wrapped in OSC-133 markers — the common case,
//!   * a JSON command output that actually gets pretty-printed.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};

use glimps::format::Formatter;

const C: &[u8] = b"\x1b]133;C\x07";
const D: &[u8] = b"\x1b]133;D\x07";

fn json_stream() -> Vec<u8> {
    let body = br#"{"login":"octocat","id":1,"node_id":"MDQ6","items":[1,2,3,4,5],"admin":true,"plan":{"name":"pro","seats":10}}"#;
    [C, body, D].concat()
}

fn plain_stream() -> Vec<u8> {
    let body = b"total 48\ndrwxr-xr-x  6 user staff   192 Jun 25 10:00 .\n-rw-r--r--  1 user staff  1024 Jun 25 09:59 Cargo.toml\n";
    [C, body.as_slice(), D].concat()
}

fn passthrough_stream() -> Vec<u8> {
    // No markers: the scanner never leaves Unknown, so this is the zero-work path.
    b"the quick brown fox jumps over the lazy dog\n".repeat(8)
}

fn diff_stream() -> Vec<u8> {
    let body = b"diff --git a/src/x.rs b/src/x.rs\nindex e69de29..4b825dc 100644\n--- a/src/x.rs\n+++ b/src/x.rs\n@@ -1,4 +1,5 @@\n fn main() {\n-    let x = 1;\n+    let x = 2;\n+    println!(\"{x}\");\n }\n";
    [C, body.as_slice(), D].concat()
}

fn stacktrace_stream() -> Vec<u8> {
    let body = b"thread 'main' panicked at src/main.rs:42:14:\ncalled `Option::unwrap()` on a `None` value\nnote: run with `RUST_BACKTRACE=1` to display a backtrace\n";
    [C, body.as_slice(), D].concat()
}

/// A command-aware table view over many rows. Bare `lsof` prints tens of
/// thousands of lines, and each one is matched against the learned schema, so
/// this is the per-line command-view cost at its worst.
fn lsof_stream() -> Vec<u8> {
    let head = b"\x1b]133;A\x07\x1b]7337;lsof\x07";
    let heading = b"COMMAND     PID   USER   FD      TYPE             DEVICE   SIZE/OFF                NODE NAME\n";
    let row = b"loginwind   588 krishv  txt       REG               1,17    3227536 1152921500312105867 /usr/lib/dyld\n";
    [head.as_slice(), C, heading.as_slice(), &row.repeat(64), D].concat()
}

/// The same table behind `lsof 2>/dev/null`. The redirect is what makes
/// `without_stderr_redirection` allocate instead of borrow, so this is the one
/// stream that measures the owned path; `lsof_stream` only ever borrows.
fn lsof_redirected_stream() -> Vec<u8> {
    let head = b"\x1b]133;A\x07\x1b]7337;lsof 2>/dev/null\x07";
    let heading = b"COMMAND     PID   USER   FD      TYPE             DEVICE   SIZE/OFF                NODE NAME\n";
    let row = b"loginwind   588 krishv  txt       REG               1,17    3227536 1152921500312105867 /usr/lib/dyld\n";
    [head.as_slice(), C, heading.as_slice(), &row.repeat(64), D].concat()
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("process");

    for (name, data) in [
        ("passthrough", passthrough_stream()),
        ("plain_output", plain_stream()),
        ("json_output", json_stream()),
        ("diff_output", diff_stream()),
        ("stacktrace_output", stacktrace_stream()),
        ("lsof_output", lsof_stream()),
        ("lsof_output_redirected", lsof_redirected_stream()),
    ] {
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_function(name, |b| {
            b.iter(|| {
                let mut f = Formatter::new();
                black_box(f.process(black_box(&data)).len())
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
