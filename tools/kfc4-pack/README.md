# kfc4-pack

Repacks an A2 extraction folder into a container through the
A1-patched writer, with the folder proving itself first:

```text
kfc4-pack pack <extract-dir> <reference-dir> <output-dir> [--types <types.json>] [--allow-modified]
```

Pre-flight (fail-closed): every manifest entry's file must exist,
match its manifest length, and match its FNV fingerprint — checked
against the manifest *and* the A0 baseline. Any mismatch aborts
naming the GUID; `--allow-modified` proceeds past flagged edits
only. Subcommands `fps`, `cmpdir`, `bytecmp`, `analyze`, `probe`
are the comparison/analysis tools used in the A3 report.

Build: `cargo build --release` (Rust 1.88); edit the `kfc-base`
path in `Cargo.toml` to your A1-patched checkout.

A3 findings (build 1273043): the folder is a complete, faithful
payload representation (91,901/91,901 fingerprints), but the current
incremental writer is append-only — it relocates every written
resource after the reference stream, so a full repack is not
byte-identical. Byte-identity needs an in-place writer mode plus
24-bit size truncation on write. Full report:
`../../kfc4/a3-repack.md`.
