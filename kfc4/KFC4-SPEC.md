# KFC4 Container Format — Evidence-Tagged Specification

Status: A1 spec, Nov 1, 2026 · Reference build: Enshrouded **1273043**
(Hotfix 1.0.0.3) · Reference implementation under study: Brabb3l/kfc-parser
**v0.2.0** (commit `1670859`), `crates/kfc-base/src/`.

## How to read this spec

Every claim carries one evidence tag:

- **PROVEN** — verified byte-for-byte against the real build-1273043
  container (Eric's pristine `enshrouded.kfc` + `enshrouded.kfc_resources`,
  91,901 resources), or exercised by a test cited next to the claim.
- **INFERRED** — read from the v0.2.0 source (file:line cited); consistent
  with the data, but not independently byte-verified.
- **ASSUMED** — carried over from the KFC3-era model or from convention;
  untested. Treat as suspect.

The worked example throughout is the pristine 1273043 container. Its
directory file is 9,396,224 bytes (`header.size` 9,394,448) and its
resources stream is 216,707,072 bytes.

## 0. Provenance — what "KFC4" is

- **PROVEN** The KFC3→KFC4 transition in the reference implementation is
  a single line: upstream commit `5b5f50b` *"kfc: changed magic value from
  KFC3 to KFC4"* (Oct 16, 2026) changes `KFC_DIR_MAGIC` from `0x3343464B`
  to `0x3443464B` in `container/header.rs` and nothing else (git show,
  1 insertion / 1 deletion).
- **PROVEN** The unchanged KFC3-era layout model parses the real KFC4
  container completely: 91,901/91,901 resources decompressed and read,
  0 errors (A0 audit, `~/workspace/your_files/kfc4-audit/REPORT.md` §2).
  For this build, the KFC3 layout *is* the KFC4 layout.
- **PROVEN** Upstream v0.2.0 accepts only the KFC4 magic: `KFCHeader::read`
  returns `KFCReadError::InvalidMagic` for any other value
  (`container/header.rs`, `read`). KFC3 files are rejected outright.
- **ASSUMED** That the layout identity holds for other builds and for
  genuine KFC3 files generally. Only KFC4 / build 1273043 has been
  sampled. Re-verify per build.

## 1. File set

A KFC4 dataset named `<name>` consists of:

| File | Role | Tag |
|---|---|---|
| `<name>.kfc` | Directory: header + all metadata sections (§2–§6) | PROVEN |
| `<name>.kfc_resources` | Resource data stream: concatenated zstd chunks (§7) | PROVEN |
| `<name>_NNN.dat` (`NNN` = 3-digit index) | Content payload containers (§8) | PROVEN (naming, `container/reader.rs` `dat_path`); payload layout directory-level only |

## 2. Header — 144 bytes

**PROVEN** layout (independent Python probe of the real file + `container/header.rs`):

| Offset | Size | Field | Observed (1273043) |
|---|---|---|---|
| 0 | 4 | `magic` u32 LE = `0x3443464B` (`"KFC4"` as stored bytes `4B 46 43 34`) | `KFC4` |
| 4 | 4 | `size` u32 LE — offset of the end of the last section | 9,394,448 |
| 8 | 4 | `unk0` u32 LE | 12 |
| 12 | 4 | zero padding (reader **rejects** nonzero, `io.rs` `padding()`) | 0 |
| 16 | 8 × 16 | sixteen `KFCLocation`s, in the order of §2.1 | see §2.1 |

- **PROVEN** The physical file length is `size` rounded up to a multiple
  of 0x1000: 9,394,448 → 9,396,224. The tail beyond `size` is zero
  padding written by `align(0x1000)` in `KFCFile::write`.
- **INFERRED** `unk0`: the reader discards it (`let _unk0`,
  `header.rs` `read`) and the writer hardcodes `12` (`header.rs`
  `write`). **PROVEN** it is 12 in every observed file (pristine, .bak,
  both writer-output vectors). Its semantics are **ASSUMED** unknown.

### 2.1 KFCLocation — self-relative offset + count

`struct KFCLocation { u32 offset; u32 count; }` — 8 bytes.

- **PROVEN** The stored `offset` is relative to the file position of the
  offset field itself: absolute = field_position + stored
  (`io.rs:110` `read_u32_offset`; `io.rs:199` `write_offset`). Worked
  example: the `resource_chunks` location sits at header offset 136,
  stores 1124, and points at absolute 1260.
- **PROVEN** Special case: a stored offset of 0 is written for an absolute
  offset of 0 and reads back as the field's own position (`write_offset`
  early-return; harmless for the two unused locations below).
- The sixteen locations, in header order, with the pristine values
  (absolute offset, count) — all **PROVEN** by probe:

| # | Field (position) | Offset | Count | Points at |
|---|---|---|---|---|
| 1 | version (16) | 144 | 65 | version string bytes |
| 2 | containers (24) | 216 | 64 | `ContainerInfo[64]` |
| 3 | unused0 (32) | 0 | 0 | — (unused) |
| 4 | unused1 (40) | 0 | 0 | — (unused) |
| 5 | resource_locations (48) | 1240 | 1 | `ResourceLocation[1]` |
| 6 | resource_indices (56) | 2580 | 91,901 | `u32[91901]` (§6) |
| 7 | content_buckets (64) | 370,184 | 131,072 | content map buckets |
| 8 | content_keys (72) | 1,418,760 | 101,459 | `ContentHash[101459]` |
| 9 | content_values (80) | 3,042,104 | 101,459 | `ContentEntry[101459]` |
| 10 | resource_buckets (88) | 4,665,448 | 131,072 | resource map buckets |
| 11 | resource_keys (96) | 5,714,024 | 91,901 | `ResourceId[91901]` |
| 12 | resource_values (104) | 8,654,856 | 91,901 | `ResourceEntry[91901]` |
| 13 | resource_bundle_buckets (112) | 9,390,064 | 256 | bundle map buckets |
| 14 | resource_bundle_keys (120) | 9,392,112 | 146 | `Hash32[146]` (qualified type hashes) |
| 15 | resource_bundle_values (128) | 9,392,696 | 146 | `ResourceBundleEntry[146]` |
| 16 | resource_chunks (136) | 1260 | 66 | `ResourceChunkInfo[66]` (§5) |

## 3. Section order and packing

**PROVEN** — the real file's sections appear in exactly the order
`KFCFile::write` emits them (`container/file.rs` `write`), and the
sections are byte-contiguous except for the noted alignment gaps
(all ranges verified arithmetically against the §2.1 offsets):

```
144        version string (65 B)          -> align 8
216        containers (64 × 16 B)         -> align 8
1240       resource_locations (1 × 12 B)  -> align 8
1256       chunk-count prefix u32 (= 66)  (§5)
1260       resource_chunks (66 × 20 B)    (no alignment after)
2580       resource_indices (91,901 × 4 B)
370,184    content buckets (131,072 × 8 B)
1,418,760  content keys (101,459 × 16 B)  -> align 8
3,042,104  content values (101,459 × 16 B)
4,665,448  resource buckets (131,072 × 8 B)
5,714,024  resource keys (91,901 × 32 B)  -> align 8
8,654,856  resource values (91,901 × 8 B)
9,390,064  bundle buckets (256 × 8 B)
9,392,112  bundle keys (146 × 4 B)
9,392,696  bundle values (146 × 12 B)
9,394,448  = header.size; file zero-padded to 9,396,224 (0x1000)
```

- **INFERRED** Alignment is 8-byte, applied by the writer after the
  version, containers, resource_locations, content keys, and resource
  keys sections (`file.rs` `write`, `align(8)` calls). In the pristine
  file every one of those sections already ends 8-aligned, so no gap
  bytes are observable there; the only structural gap is the 4-byte
  chunk-count prefix at 1256.

## 4. Scalar record layouts

All integers are little-endian (**PROVEN** — readers use `from_le_bytes`
throughout `io.rs`, and the probe values are consistent).

### 4.1 ContainerInfo — 16 B
`{ u64 size; u64 count; }` (`header.rs`). **PROVEN** as a directory
record (64 present, 55 with nonzero size in the pristine file).
What `size`/`count` mean for the `.dat` payloads is **ASSUMED**
(directory-level only, §8).

### 4.2 ResourceLocation — 12 B
`{ u32 uncompressed_size; u32 compressed_size; u32 count; }`.
**PROVEN** exactly one is present, and its aggregates match the chunk
table of the pristine file: uncompressed 501,156,384; compressed
216,562,036 (sum of chunk `compressed_size`, §5); count 91,901
(= resource count).

### 4.3 ResourceId — 32 B (resource map key)
`{ Guid guid[16]; u32 type_hash; u32 part_index; u32 reserved_0; u32 reserved_1; }`
(`guid/resource.rs`).

- **PROVEN** `guid` is 16 raw bytes, written/read verbatim
  (`guid/base.rs` `Guid([u8; 16])`).
- **PROVEN** `type_hash` is the type's *qualified* hash — every one of
  the 146 distinct values in the pristine file resolves in the build's
  type registry by `qualifiedHash` (registry cross-check, A1).
- **PROVEN** `part_index` distinguishes parts of one logical resource
  (same GUID, multiple parts).
- **PROVEN** Both reserved fields are zero in the real file: the reader
  validates them as zero padding (`padding(4)` twice in
  `ResourceId::read`) and the full parse succeeds.

### 4.4 ResourceEntry — 8 B (resource map value)
`{ u32 offset; u32 size; }` — `offset` is the resource's byte offset in
the *uncompressed* resources stream; `size` is the stored size, subject
to the 24-bit caveat of §7.2. **PROVEN** as the directory record the
reader resolves every resource through.

### 4.5 ContentHash — 16 B (content map key)
`{ u32 size; u32 hash0; u32 hash1; u32 hash2; }` (`guid/content.rs`).
**INFERRED** `size` is the payload length — the reader seeks into a
`.dat` container and reads exactly `hash.size()` bytes
(`container/reader.rs` `open_content`/`read_content_into`); payload
bytes themselves are untested locally (§8).

### 4.6 ContentEntry — 16 B (content map value)
`{ u32 offset; u16 flags; u16 container_index; u8 padding[8] }`
(`header.rs`). **PROVEN** as a directory record (101,459 present,
carried identically through round-trips); the trailing 8 bytes are
validated zero on read. Field semantics beyond `offset` /
`container_index` addressing are **INFERRED** from reader usage;
`flags` semantics are **ASSUMED** unknown.

### 4.7 ResourceBundleEntry — 12 B (bundle map value)
`{ u32 internal_hash; u32 index; u32 count; }` (`header.rs`).

- **PROVEN** The bundle map's *key* is the qualified type hash (§4.3);
  the value's `internal_hash` equals the registry's `internalHash` for
  that type: the A0 round-trip rebuilt all 146 bundle entries from the
  registry and produced zero `internal_hash` differences.
- **PROVEN** `index`/`count` delimit a contiguous segment of
  `resource_indices` (§6): for all 146 bundles, every resource position
  in the segment belongs to a resource of that bundle's type
  (verified exhaustively on the pristine file, A1).
- **ASSUMED** The *order* in which the game lays out the segments
  (equivalently, the order in which bundle `index` bases are assigned)
  follows no rule we could find: it correlates with neither the bundle
  map's key sequence, first occurrence in the resource key sequence,
  data offsets, registry order, type names, nor either hash
  (inversion counts ≈ random for all candidates, A1). Writers must
  *preserve* the loaded order rather than derive it (§9.2).

## 5. Chunk table

- **PROVEN** A `u32` chunk count sits 4 bytes *before* the offset the
  header's `resource_chunks` location points to: at 1256 the value 66,
  records start at 1260. The writer creates this layout by writing the
  count first and recording `position + 4` as the location offset
  (`file.rs` `write`); the reader ignores the prefix and takes the
  count from the header location.
- **PROVEN** Record layout, 20 B (`header.rs` `ResourceChunkInfo`):
  `{ u32 offset; u32 size; u32 compressed_size; u32 uncompressed_offset; u32 uncompressed_size; }`
  — `offset`/`size` address the chunk's slot in `.kfc_resources`
  (`size` includes alignment padding, §7), `compressed_size` is the
  exact zstd frame length, `uncompressed_offset`/`uncompressed_size`
  address the decompressed stream.

## 6. Static maps and resource indices

Three sections (contents, resources, bundles) are *static maps*:
parallel arrays `buckets[]`, `keys[]`, `values[]`
(`container/static_map.rs`).

- **PROVEN** Bucket record, 8 B: `{ u32 index; u32 count; }` — the
  bucket's keys occupy `keys[index .. index+count]`, likewise values.
  The reader validates: keys/values lengths equal, bucket counts sum
  to the key count, bucket count is a power of two, every bucket range
  is in bounds (`StaticMap::from_parts`).
- **PROVEN** Bucket count = next power of two ≥ entry count:
  101,459 contents → 131,072; 91,901 resources → 131,072; 146 bundles
  → 256 (matches `StaticMapBuilder::build`'s rule and the file).
- **PROVEN** Lookup: `bucket = static_hash(key) % bucket_count`, then a
  linear scan of the bucket's key range (`StaticMap::get`). Every one
  of the 91,901 resources resolves through this in the real file (A0).
- Static hash functions (**INFERRED** from code, **PROVEN** consistent
  with the file by the successful full parse):
  - `ResourceId`: FNV-1a-32 seeded with the first 4 GUID bytes (LE),
    over the 8 bytes `type_hash LE ‖ part_index LE`
    (`guid/resource.rs:152`, `hash/fnv.rs:31`; FNV prime `0x01000193`,
    standard basis `0x811c9dc5` when unseeded).
  - `ContentHash`: the `hash0` field itself (`guid/content.rs:146`).
  - `Hash32` (bundle keys): the value itself (`lib.rs:16`).
- **PROVEN** Within a bucket, keys appear in *file order* — that order
  is part of the file's byte identity. No derivation rule for the
  game's within-bucket order is known (**ASSUMED** unknowable from the
  container alone); §9.1 states the writer rule that preserves it.
- `resource_indices` (**PROVEN**): a flat `u32[91901]`; bundle (§4.7)
  segments partition it, and segment entries are positions into the
  resource map's key array. It is the per-type resource listing the
  game enumerates through.

## 7. The resources stream (`.kfc_resources`)

- **PROVEN** Geometry of the pristine stream (A0 §2): 66 chunks;
  uncompressed ranges are contiguous from 0 and total 501,156,384 B;
  compressed slots are 4096-aligned, tile the stream with zero slack,
  and sum to the stream length 216,707,072 B; the zstd frames
  themselves sum to 216,562,036 B (= §4.2's `compressed_size`).
- **PROVEN** One zstd frame per chunk: reader `zstd::stream::copy_decode`
  (`container/reader.rs` `decompress_chunk`); writer `copy_encode` at
  level 0 (`container/writer.rs` `submit_resource_data`).
- **INFERRED** Chunking policy: the writer emits fixed 8 MiB
  uncompressed chunks (`writer.rs` `RESOURCE_CHUNK_SIZE = 8 * 1024 * 1024`)
  and 16-byte-aligns each resource inside the uncompressed stream
  (`RESOURCE_ALIGNMENT`). The pristine file's chunks are the same
  ~8 MiB size, so game and writer agree on this build; the game's own
  policy is observed, not specified.
- Resource data placement (**INFERRED** from reader logic, **PROVEN**
  by 91,901 successful reads): a resource's bytes are the
  `actual_size` bytes starting at its entry `offset` in the
  decompressed stream, transparently spanning chunk boundaries.

### 7.2 The 24-bit size rule

- **INFERRED** Stored `ResourceEntry.size` is the resource size
  truncated to 24 bits, so resources ≥ 16 MiB are stored short by a
  multiple of 16 MiB (`container/reader.rs:7-9` comment; modulus
  `0x1000000`).
- **PROVEN** (single instance) The pristine container contains exactly
  one affected resource: `509feadb-4c60-425f-9c7c-deeefd9b6920`, stored
  11,105,420, actual 44,659,852 = stored + 2×16 MiB (A0 §1).
- **INFERRED** Recovery heuristic (`reader.rs` `actual_resource_size`):
  take the gap to the next resource offset; if the gap equals
  `stored + k·16 MiB` plus < 16 bytes of alignment remainder, the
  actual size is `stored + k·16 MiB`; otherwise the stored size is
  used as-is. The heuristic silently falls back when the pattern does
  not match — a proper implementation must treat unrecovered sizes as
  a known risk, not a certainty. (Upstream added this after the KFC4
  magic change, commit `d71544a` / PR #35 — discovered from real KFC4
  data, not specified by Keen.)

## 8. Content containers (`.dat`) — directory level only

- **PROVEN** Directory facts: 64 `ContainerInfo` records (55 nonzero);
  101,459 content entries address payloads by `{offset, container_index}`
  (§4.6), and payload length comes from the key's `size` field (§4.5).
  Container file names are `<name>_{index:03}.dat`
  (`reader.rs`/`writer.rs` `dat_path`).
- **NOT TESTED** Payload bytes: Eric's local staging holds only
  zero-byte `.dat` stubs, so no content payload read or write is
  verified in A0/A1. Everything in this section beyond the directory
  records is **INFERRED** from reader/writer code. A full game-dir pull
  is required to promote any of it to PROVEN.

## 9. Writer rules (normative, from the A1 patch set)

These rules are what the patched v0.2.0 writer
(`~/workspace/your_files/kfc4-a1/`, patches 0001–0003) implements; with
them, a zero-change re-serialization of the pristine container is
**byte-identical** (A1-REPORT.md).

### 9.1 Map order preservation
A builder seeded from a loaded map inserts entries in the loaded key
order; `build()` groups by bucket with a stable sort, so within-bucket
order equals insertion order and an unchanged map serializes exactly as
loaded. Entries inserted later (new keys) land at the end of their
bucket group. If an entry count crosses a power-of-two boundary the
bucket count changes and regrouping is total — expected, and the only
case where order is not preserved.

### 9.2 Bundle layout preservation
- Unchanged resource key sequence ⇒ carry the loaded `resource_indices`
  and bundle map over verbatim.
- Changed ⇒ existing types keep the relative order of their previous
  bundle segments; new types are appended after them, ascending by
  qualified type hash; `index` values are reassigned as running offsets
  in that group order; `count` is the group's resource count;
  `internal_hash` comes from the type registry (fallback: previous
  bundle map).

### 9.3 No empty chunks
`finalize()` submits a trailing chunk only when uncompressed data is
actually pending. A zero-change incremental write therefore leaves the
resources stream untouched, byte for byte.

## 10. Known gaps

1. `.dat` payload half of the format is unverified (§8).
2. `unk0` (= 12) semantics unknown.
3. The game's bundle segment ordering rule is unknown (§4.7) —
  preservation is the only sound writer strategy.
4. The 24-bit recovery heuristic (§7.2) has exactly one proven instance;
   its fallback path is silent.
5. Only build 1273043 sampled; per-build re-verification required.
6. Pristine v0.2.0 exposes no public load→save API (`KFCFile::write` is
   `pub(super)`, `file.rs`); all writing goes through the incremental
   `KFCWriter`. The A1 rules above are implemented inside that path.
