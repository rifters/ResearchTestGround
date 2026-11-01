# kfc4-audit harness

The Rust harness that produced the A0 round-trip audit numbers
(`../../kfc4/round-trip-audit.md`): full parse of a pristine KFC4
container, per-resource FNV-1a fingerprints, and a zero-change
re-serialization through the mod-loader's own writer path
(`KFCWriter::new_incremental_with_options` + `finalize()`), followed by
structural comparison.

- `harness/` — built against upstream Brabb3l/kfc-parser v0.2.0.
- `harness-fork/` — the same source built against the M182 fork's
  kfc-base (EML 0.1.2 snapshot + ports).

Setup: each `Cargo.toml` points its `kfc-base` path dependency at a
local checkout (upstream clone for `harness/`, fork clone for
`harness-fork/`). Edit that one path to your own checkout location,
then `cargo build --release`. The harness takes the game directory /
container paths as arguments; it always works on copies, never on the
pristine inputs.
