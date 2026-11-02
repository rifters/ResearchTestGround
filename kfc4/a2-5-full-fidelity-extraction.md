# A2.5 — Full-fidelity extraction: closing the Grade 2 gaps

**Date:** Nov 1, 2026 (run completed 20:41 EST) · **Game build:** 1273043 · **Verdict: PASS — every
verification check green (`VERIFY: ALL OK`, exit 0).** The A2 folder's missing closing data is now
captured: the complete directory sections verbatim, per-chunk zstd levels determined by test, the
large orphan gaps byte-for-byte, and per-resource stream offsets in the manifest. With this folder,
**every byte of both game files is either present or reproducible by a stated rule** — the only
exception is the `.dat` payloads, which do not exist on this VM (see §5).

Inputs were the same pristine pair as A0–A3 (read-only; SHA-256 re-confirmed before and after the
run: `.kfc` `7f6130bb…e544f79e9`, `_resources` `1cd249c3…0290afe341` — unchanged).

## 1. What was built and run

- **Tool:** `kfc4-extract-full` at `kfc4-a25/kfc4-extract-full/` — extends a copy of A2's
  `kfc4-extract`; the A1-patched `kfc-base` is path-depended, unmodified. It parses the 144-byte
  header itself (kfc-base doesn't expose it), cross-checks every header count and the version bytes
  against kfc-base's parsed view (fail-closed on any mismatch), extracts the payload half exactly
  like A2, then captures the directory records, chunk levels, and orphan gaps, and finishes with a
  built-in verification phase that re-reads everything from disk against fresh parses.
- **Run:** `kfc4-extract-full enshrouded.kfc extracted-full --types types.json --baseline
  fp-pristine.txt` — 114.6 s, exit 0, final line `VERIFY: ALL OK`.
- **Output:** `kfc4-a25/extracted-full/` — 555,584,633 bytes on disk: the A2 layout
  (`resources/` + `_manifest.json` + `_summary.json`) plus `directory/`. The manifest now also
  carries each resource's **absolute uncompressed stream `offset`** (the field A3 found missing).

## 2. Directory capture — every record, verified

All `.bin` records are raw slices of the pristine `.kfc` at the header-recorded locations. Each was
re-sliced from a **fresh header parse** during verification and byte-compared: **all equal**.

| Record | Bytes | Source section (file offset) | Verification |
|---|---|---|---|
| `directory/header.bin` | 144 | whole header `[0, 144)` | byte-equal on re-slice ✓ |
| `directory/version.bin` | 65 | version `[144, 209)` | byte-equal ✓; bytes also cross-checked vs kfc-base `game_version()` ✓ |
| `directory/containers.bin` | 1,024 | containers `[216, 1240)` (64 × 16 B) | byte-equal ✓ |
| `directory/resource_locations.bin` | 12 | resource_locations `[1240, 1252)` (1 × 12 B) | byte-equal ✓; values match the stream file (501,156,384 / 216,562,036 / 91,901) ✓ |
| `directory/chunk_table.bin` | 1,324 | count prefix `[1256, 1260)` + 66 records `[1260, 2580)` | byte-equal ✓; prefix asserted = 66 = header count; raw parse re-derived and matched kfc-base on all 66 chunks ✓ |
| `directory/resource_indices.bin` | 367,604 | resource_indices `[2580, 370,184)` | byte-equal ✓ |
| `directory/contents_map.bin` | 4,295,264 | full span `[370,184, 4,665,448)`: buckets 1,048,576 + keys 1,623,344 + values 1,217,508 + 405,836 span-tail* | byte-equal ✓; arrays contiguous, 0 inter-array gap bytes |
| `directory/resource_map.bin` | 4,724,616 | full span `[4,665,448, 9,390,064)`: buckets 1,048,576 + keys 2,940,832 + values 735,208 | byte-equal ✓; contiguous, 0 gap bytes |
| `directory/bundle_map.bin` | 4,384 | full span `[9,390,064, 9,394,448)`: buckets 2,048 + keys 584 + values 1,752 | byte-equal ✓; contiguous, 0 gap bytes |
| `directory/resource_keys.json` | 16,751,383 (JSON) | the resource map's keys array, in file order (91,901 × 32 B records) | re-derived from a fresh slice in verification: 91,901 entries, **0 mismatches** (guid, type_hash, part, container_index); also entry-for-entry identical to kfc-base's parsed key order at extract time ✓ |
| `directory/chunks.json` | 19,818 (JSON) | chunk table + stream frames | per-chunk levels re-encoded and re-compared in verification (§3) ✓ |
| `directory/dat_files.json` | 15,009 (JSON) | containers section | inventory only — payloads **not captured** (§5) |
| `directory/orphans/orphans.bin` + `orphans.json` | 16,989,572 + 886,468 | uncompressed stream gaps | every gap re-hashed against a fresh stream (§4) ✓ |

\* The contents map's span extends 405,836 bytes past its values array to the next section's start —
bytes that belong to no named array. Capturing the **full span** (rather than buckets+keys+values
only) means nothing between sections is lost; all three maps were captured this way, and in each
case `buckets.end == keys.start` and `keys.end == values.start` exactly.

SHA-256 for every record is in `directory/_directory.json`. Three of them (header, version,
containers) were additionally re-hashed with an independent tool (Python `hashlib`) and match.

**Complete byte accounting of the `.kfc`:** captured records total **9,394,437** of the header's
`size` field 9,394,448. The remaining **11 bytes are the only bytes in the directory not inside a
record**: two zero alignment gaps — `[209, 216)` after the version section (7 B) and `[1252, 1256)`
after resource_locations (4 B) — verified all-zero. Past `size`, the file carries 1,776 zero tail
bytes to its 4,096 alignment (also verified all-zero). A rebuild therefore needs nothing beyond
these records plus "write zeros in the two gaps and the tail."

**Format JSON vs `.bin`:** the key sequence is stored as JSON (16.7 MB) rather than a 2,940,832-byte
raw slice because the raw bytes are already preserved verbatim inside `resource_map.bin` (its keys
array *is* that slice, at a recorded offset); the JSON adds the parsed, order-preserving form a
builder actually consumes. No information is lost either way.

## 3. Chunk zstd levels — determined by test, 66/66 reproduced

For each chunk, the tool decompressed the game's frame, re-encoded the data with
`zstd::stream::copy_encode` (the encoder kfc-base's writer uses) at candidate levels — 9 and 3
first (A3's finding), then ascending 1..=19 — and recorded every level that reproduces the original
frame **byte-for-byte**; `zstd_level` in `chunks.json` is the smallest matching level, per the
requirements. The verification phase re-encoded all 66 chunks at their recorded levels from fresh
reads: **66/66 frames reproduced byte-for-byte. No chunk needed a fallback frame** (`chunks_frames/`
was not created; no null levels).

| Recorded level | Chunks | Count |
|---|---|---|
| 9 | 0–57 | 58 |
| 3 | 58, 60, 61, 62, 64 | 5 |
| 1 (also matches 3 and 9) | 59, 63, 65 | 3 |

**Reconciliation with A3:** A3 reported "level 9 reproduces 61/66, level 3 the final five
(58, 60, 61, 62, 64)". That is exactly what the test finds: the 61 level-9 chunks are the 58 above
plus chunks 59, 63, 65, whose frames are reproduced identically by levels 1, 3, **and** 9 — the
smallest-match rule records them as 1, with the full matching set kept in
`zstd_levels_matching`. The five level-3-only chunks match A3's list exactly. (One completeness
note: for a chunk whose smallest match is found early, higher untested levels aren't enumerated —
A3 additionally observed chunk 60 matching at level 4; that doesn't change its recorded level, 3.)

**Compressed-stream accounting:** frames sum to 216,562,036 bytes; chunk slots (4096-aligned) sum
to 216,707,072 = the stream file's exact size; the 145,036 padding bytes between each frame and its
slot end were checked and are **all zero** — reproducible by rule from the chunk table.

## 4. Orphan gaps — recomputed from the directory, exact match with A3

Gaps were recomputed from scratch: resources sorted by their recorded stream offsets, coverage
walked against the uncompressed stream length (501,156,384). Resource overlaps: **0**.

| Class | This run | A3 | Match |
|---|---|---|---|
| Large gaps (>15 B, or ≤15 B nonzero) | **3,960 regions / 16,989,572 B** | 3,960 / 16,989,572 | exact ✓ |
| Small gaps (≤15 B, all zero) | **64,139 / 497,031 B** | 64,139 / 497,031 | exact ✓ |

- Large gaps are dumped concatenated in `directory/orphans/orphans.bin` (16,989,572 B) with
  `orphans.json` carrying each gap's stream offset, length, bin offset, and SHA-256. Verification
  re-hashed **all 3,960 gaps against a freshly decompressed stream: 0 mismatches**.
- Small gaps are **not** dumped; the reproduction rule is recorded in `orphans.json` and
  `_directory.json`: *zero-fill up to the next resource's recorded offset* (16-byte alignment
  padding; the trailing gap after the last resource follows the same rule). Every small gap was
  asserted 100% zero during the census (a nonzero one would have been classified large and dumped).
- **Stream byte accounting closes exactly:** resource bytes 483,669,781 + large gaps 16,989,572 +
  small gaps 497,031 = **501,156,384** = the uncompressed stream length.

## 5. The one remaining gap: `.dat` payloads (PC-side)

`directory/dat_files.json` records the full 64-container inventory from the directory (index, file
name `enshrouded_NNN.dat`, the directory's own `count` and `size` fields), each entry marked
`payload_captured: false`, under a top-level `payloads_captured: false`. The payloads themselves are
**not captured and not claimed**: on this VM only zero-byte stubs for indices 055–063 exist, and the
repro game dir has none of the real files. Capturing them is a PC-side copy of the 55 nonzero
containers' `.dat` files. Two consistency notes from this run:

- The containers' `count` fields sum to **101,459 — exactly the contents-map entry count**: the
  containers partition the contents.
- **Open question (flagged, not resolved):** the raw `size` fields sum to 51,500,314,624
  (container 0 alone: 1,060,708,352), so that field is *not* a plain payload byte count, and the
  "74,410,422 bytes total" figure carried in the A0 report and the spec does not follow from it.
  Nothing is lost by this — `containers.bin` preserves the bytes verbatim — but the field's
  semantics (and the 74,410,422 derivation) need one more look before anyone sizes a `.dat`
  capture by it.

## 6. Payload half — verification

- **91,901 files** under `resources/` (independently counted), total decompressed bytes
  **483,669,781** — identical to A2/A3.
- **Fingerprints:** the tool's set comparison against `fp-pristine.txt` passed; an independent
  Python re-check compared **multisets** (91,901 lines each, 88,392 distinct — identical lines recur
  where distinct ResourceIds hold identical bytes): **exactly equal**.
- **Manifest offsets:** verification compared **every** entry's file bytes against the freshly
  decompressed stream at its recorded offset: **91,901/91,901 match**. Named spot cases:
  - first by offset: `4ff44bc6…` (AchievementDefinitionResource) @ 0;
  - last by end: `fd993990…` (TemplateResource) ends @ 501,156,369 — 15 B before the stream end,
    i.e. the trailing small gap;
  - the 44,659,852-byte resource: `509feadb…` VoxelTemperatureResource part 3 @ 25,175,536,
    spanning chunks 3–8;
  - orphan-adjacent: `a430a506…` (RenderSettingsGpuPresetsResource) @ 25,169,696, starting exactly
    at a large gap's end.

## 7. Closing-data total vs A3's estimate

| Component | Bytes |
|---|---|
| Verbatim directory records (9 `.bin` files) | 9,394,437 |
| Orphan gap bytes (`orphans.bin`) | 16,989,572 |
| **Closing records total** | **26,384,009** |
| JSON artifacts (resource_keys, chunks, dat_files, orphans index) | 17,672,678 |
| `_directory.json` (written after the tool's own accounting) | 5,022 |
| **`directory/` total on disk** | **44,061,709** |

A3's estimate was ≈25 MB. The captured binary closing data is **26,384,009 B (~26.4 MB)** — about
1.8 MB over, fully itemized: A3's list counted the resource key sequence (2,940,832) and indices
separately but did not count the resource map's bucket and value arrays (1,048,576 + 735,208) or the
bundle map's buckets (2,048), all of which A2.5 captures as complete map spans; the chunk table is
captured with its count prefix (1,324 vs A3's 1,320 records-only). The JSON artifacts are builder
convenience on top; the *binary* closing data is the 26.4 MB figure.

## 8. What a standalone pack still needs beyond this folder

**Nothing on the VM side except the `.dat` payloads.** Precisely:

1. **The `.dat` payloads** (55 nonzero containers) — PC-side capture only; §5.
2. **A stream/directory builder** (the remaining A3/A4 work, not data): a from-scratch or in-place
   writer that consumes this folder — payloads at their recorded offsets, orphan bytes at theirs,
   small gaps zero-filled, per-chunk frames encoded at the recorded zstd levels with slot padding
   zeroed, directory sections written verbatim (or rebuilt from `resource_keys.json` order), the
   two zero alignment gaps and tail reproduced, and the game's 24-bit stored-size truncation (A3
   §3.3) applied for the one affected resource.

Every other byte of both files is in this folder or reproducible by a rule stated above:
directory bytes (§2), stream resource bytes (§6), orphan bytes (§4), small-gap/slot/tail zeros
(§§2–4), chunk frames (§3).

## 9. Deliverables

- Tool: `kfc4-a25/kfc4-extract-full/` (binary `target/release/kfc4-extract-full`); run log at
  `kfc4-a25/extract-full-run.log`.
- Extraction: `kfc4-a25/extracted-full/` (555,584,633 B; `resources/` 91,901 files,
  `_manifest.json` with stream offsets, `_summary.json`, `directory/` per §2).
- No GitHub pushes were made; no clones, A1/A2/A3 trees, or pristine inputs were modified.
