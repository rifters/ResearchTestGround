# kfc4-extract

Unpacks a KFC4 container to a folder tree, fully verified:

```text
kfc4-extract <container.kfc> <output-dir> [--types <types.json>]
```

Output: `resources/<TypeName>/<guid>__p<part>.bin` (one decompressed
file per resource part), `_manifest.json` (guid, type hash/name, part,
sizes, FNV-1a fingerprint, chunk indices, path), and `_summary.json`
(per-type counts/bytes, unresolved hashes). Fail-closed: any missing,
errored, or length-mismatched resource aborts the run naming the GUID.

Build: `cargo build --release` (Rust 1.88). The `Cargo.toml` points
its `kfc-base` path dependency at a local checkout of the A1-patched
kfc-parser tree — edit that one path to your own checkout. Reading is
behavior-identical to upstream v0.2.0; the A1 writer patches don't
affect extraction.

Verified run (build 1273043, Nov 1, 2026): 91,901 files, every
fingerprint identical to the A0 audit baseline, extracted byte total
483,669,781. Full report: `../../kfc4/a2-full-unpack.md`.
