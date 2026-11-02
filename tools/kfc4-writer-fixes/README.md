# kfc4 writer fixes — byte-faithful round-trip

Three patches against upstream Brabb3l/kfc-parser **v0.2.0** that make a
zero-change re-serialization of a pristine KFC4 container **byte-identical**
(A0 baseline: 6,545,161 differing bytes; A1 result: 0, SHA-256 verified).
Full evidence: `../../kfc4/a1-byte-faithful-writer.md`.

Apply in order, to the crates' source tree:

1. `0001-static-map-order.patch` — `StaticMapBuilder` uses an `IndexMap`
   instead of a `HashMap`, so within-bucket key order survives a
   load → write cycle.
2. `0002-bundle-preservation.patch` — incremental finalize carries the
   reference file's `resource_indices` + bundle map over verbatim when
   the resource key sequence is unchanged; when changed, existing types
   keep their previous segment order and new types append ascending by
   type hash. (No derivable game ordering rule for bundle segments was
   found — preservation is the strategy; see the spec, §4.7.)
3. `0003-no-empty-chunk.patch` — `finalize()` no longer appends an
   empty zstd chunk when nothing is pending. Also applies standalone.

Each patch is independently gated; only the combination is
byte-faithful. `cargo test -p kfc-base`: 21/21 with the series applied.
