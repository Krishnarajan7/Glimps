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

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("process");

    for (name, data) in [
        ("passthrough", passthrough_stream()),
        ("plain_output", plain_stream()),
        ("json_output", json_stream()),
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
