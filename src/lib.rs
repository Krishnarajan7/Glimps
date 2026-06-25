//! GLIMPS library surface.
//!
//! The crate is built as both a library and a binary. The library exists so the
//! supervisor, terminal helpers, and the formatting seam can be exercised by
//! integration tests and `criterion` benchmarks (which are separate crates and
//! can only see the public API). `src/main.rs` is a thin binary over this.
//!
//! Formatter internals (detectors/printers) stay private inside `format`; the
//! only formatting entry point is `format::Formatter::process`.

pub mod format;
pub mod pty;
pub mod terminal;
