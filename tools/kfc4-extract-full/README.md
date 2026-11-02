# kfc4-extract-full

The A2.5 full-fidelity extractor: everything `kfc4-extract` writes,
plus the complete directory side, so a folder-only rebuild has all
its inputs.

```text
kfc4-extract-full <container.kfc> <output-dir> [--types <types.json>]
```

Beyond the A2 layout (and a manifest that now also records each
resource's absolute stream offset), the output carries a
`directory/` folder: verbatim `.bin` slices of the header, version,
containers, resource locations, chunk table (with its count
prefix), resource indices, and all three static maps; the resource
key sequence in file order; per-chunk zstd levels *determined by
re-encoding test*; and the orphan gap dump (index + bytes) for the
stream regions no resource covers. `_directory.json` indexes every
record with offset, length, and SHA-256, and the run verifies each
dump byte-for-byte against the source file. The `.dat` payloads are
inventoried in `dat_files.json` but not captured (PC-side only).

Build: `cargo build --release` (Rust 1.88); edit the `kfc-base`
path in `Cargo.toml` to your A1-patched checkout. Full report:
`../../kfc4/a2-5-full-fidelity-extraction.md` — on build 1273043 the
byte budgets of both game files close exactly.
