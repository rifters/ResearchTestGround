# kfc4-pack-standalone

Rebuilds the container from an A2.5 extraction folder **alone** —
the pack code path takes no reference file; the pristine pair is
only ever used afterward, for comparison.

```text
kfc4-pack-standalone pack <extract-full-dir> <output-dir> [--allow-modified]
```

Pre-flight is fail-closed (every resource file against the
manifest, every directory record against `_directory.json`, orphan
bytes against their index; a length mismatch is always fatal). The
stream is assembled from resources + orphan dumps + the zero-gap
rule, re-encoded chunk-by-chunk at the recorded zstd levels into
the recorded slots, and the `.kfc` is assembled from the verbatim
directory records at their recorded offsets. Subcommands `fps`
(fingerprint pass via kfc-base) and `bytecmp` support verification.

On build 1273043 the outputs are **bit-identical** to the game's
files (SHA-256 match on both). The `.dat` payloads are not produced
— they were never captured on this machine (PC-side only). Full
report: `../../kfc4/standalone-pack.md`.

Build: `cargo build --release` (Rust 1.88, zstd pinned 0.13.3);
edit the `kfc-base` path in `Cargo.toml` to the patched checkout.
