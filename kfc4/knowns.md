# KFC4 — Knowns

Distilled from the A0 round-trip audit (Nov 1, 2026, build 1273043).
Full evidence: `round-trip-audit.md` in this folder. Tags: **PROVEN** =
byte-verified on the real container · **INFERRED** = code reading ·
**ASSUMED** = carried from KFC3, untested.

## Provenance

- **PROVEN** — Upstream kfc-parser's KFC3→KFC4 transition is commit
  `5b5f50b` *"changed magic value from KFC3 to KFC4"* (Oct 16, 2026):
  one line, the magic constant in `header.rs`. No layout work.
- **PROVEN** — The KFC3 layout model parses the real KFC4 container
  completely: 91,901/91,901 resources read, 0 errors, 0 missing; the
  66-chunk table tiles the 216,707,072-byte resources stream exactly.
  Keen changed the magic, not the container.
- **INFERRED** — In the fork's older lineage KFC4 was treated as the
  dedicated-server magic (`KFC_DIR_MAGIC_SERVER`) for the same layout.

## Container layout (all PROVEN on build 1273043 unless noted)

- Header is 144 bytes: `u32` magic `0x3443464B` "KFC4", `u32 size`,
  `u32 unk0`, 4 bytes zero padding (validated on read), then 16
  `KFCLocation`s `{u32 self-relative offset, u32 count}` — the stored
  offset is relative to its own field position.
- `header.size` = end offset of the last section; file length is `size`
  rounded up to 0x1000.
- Version tag is a raw string carried verbatim (for this build:
  `1273043|^/game38/branches/release_1.0|2026-10-20...`).
- Section order: version → containers → resource_locations →
  resource_chunks → resource_indices → content map → resource map →
  bundle map.
- Chunk table: a `u32` count sits 4 bytes *before* the location the
  header points to. Records are 20-byte `ResourceChunkInfo {offset,
  size, compressed_size, uncompressed_offset, uncompressed_size}`.
- Compression: one **zstd** frame per chunk. Uncompressed ranges are
  contiguous from 0 (501,156,384 bytes total here); compressed ranges
  are 4096-aligned and tile the stream with zero slack.
- ResourceId is 32 bytes: GUID (16) + type hash + part + reserved.
- ResourceEntry is 8 bytes `{offset, size}`; ContentHash 16 B;
  ContentEntry 16 B; ContainerInfo 16 B; ResourceLocation 12 B.
- Static maps (contents / resources / bundles) are parallel
  buckets/keys/values arrays; bucket count = next power of two of the
  entry count.

## Known writer behaviors (the lossy part)

- **PROVEN** — Content round-trips 100% losslessly: per-resource FNV-1a
  fingerprints identical for all 91,901 resources after a zero-change
  write.
- **PROVEN** — The written `.kfc` is **not** byte-faithful (~70% of
  bytes differ): static maps are rebuilt through a `HashMap` (key order
  lost), all 146 bundle `index` fields are recomputed, and a zero-change
  write appends a **spurious empty zstd chunk** (+4,096 bytes,
  chunks 66→67, `header.size` +24).
- **PROVEN** — Tool-written files (81- and 99-clone outputs) parse
  cleanly and consistently; their differences from pristine are pure
  writer layout — a dialect, not corruption.
- **PROVEN (A1, Nov 1, 2026)** — The three dialect behaviors above are
  fixable: with the patch series in `tools/kfc4-writer-fixes/`
  (IndexMap map order, bundle preservation, no empty chunk), a
  zero-change round-trip of the pristine pair is **byte-identical**
  (0 differing bytes, SHA-256 `7f6130bb…` / `1cd249c3…`). The bundle
  segment order itself follows no derivable rule found (not map order,
  first occurrence, offsets, registry order, names, or hashes) —
  preservation, not derivation, is the sound strategy.

## Resource identity (A2, Nov 1, 2026 — all PROVEN)

- The 91,901 ResourceIds cover only **74,542 distinct GUIDs**: a GUID is
  a logical *group* (scene/world/atlas) spanning multiple types;
  `part_index` enumerates repeated same-type constituents (299 repeating
  (GUID, type) pairs, parts exactly 0..n−1 in 298 of them).
- The main world GUID `509feadb…` owns 8,109 ResourceIds across 15
  types — 1,601 chunks × the per-chunk types, part = chunk index. Its
  VoxelTemperatureResource part 3 is the 44,659,852-byte resource behind
  the 24-bit size recovery.
- Type-name resolution is exact: FNV-1a-32(qualified name) == type_hash
  for all 146 resource types.
- One anomaly is the game's own data: GUID `4ae2414d…`'s
  RenderTextureResource parts run 1..9 with no part 0
  (fingerprint-verified, not extraction loss).
- The resource-types CSV export is mostly sound (names 146/146, counts
  agree 143/146) but its `FirstSampleGUID` column is wrong for 40/146
  rows — don't join on it.

## Writer mechanics and stream facts (A3, Nov 1, 2026 — all PROVEN)

- The incremental writer is **append-only**: every written resource is
  relocated into chunks appended after the reference stream. A full
  repack from the A2 folder reproduces all content (91,901/91,901
  fingerprints) and the entire directory except resource offsets —
  but is not byte-identical. Byte-identity needs an in-place writer
  mode (write into the resource's own chunks when sizes fit).
- The game **truncates stored resource sizes to 24 bits** on write;
  kfc-base writes the full u32. Exactly one resource in this container
  is affected (the 44,659,852-byte world temperature resource).
- Game zstd frames are reproducible with the stock encoder, but not
  at one level: **level 9 reproduces 61/66 chunks byte-for-byte**;
  the final five (58, 60, 61, 62, 64) reproduce exactly at **level 3**
  (smaller window). Per-chunk level must be recorded at extraction.
- The uncompressed stream contains **orphan data**: 3,960 large gaps
  (16,989,572 B total) referenced by no directory entry, all
  data-bearing; the two largest are not near-duplicates of each
  other, so they aren't simply superseded copies. Nature
  undetermined — a full-fidelity extraction must dump them verbatim
  with offsets. (The 64,139 small gaps ≤15 B are 100% zero —
  alignment padding, reproducible by rule.)
- A folder-only byte-exact rebuild needs ≈25 MB of additional
  records beyond the payload half (containers block, key sequence,
  indices, chunk table, contents map, orphan dump) plus the `.dat`
  files — the "A2.5" extraction defined in the A3 report.

## Assumed / heuristic / unknown

- **INFERRED (heuristic)** — Resource sizes are stored truncated to
  24 bits; sizes ≥16 MiB are recovered from the gap to the next
  resource (`actual_resource_size`). Fires exactly once in this
  container (stored 11,105,420 → actual 44,659,852).
- **ASSUMED** — `unk0` semantics unknown; read and discarded, hardcoded
  to 12 on write. 12 in every observed file.
- **ASSUMED** — The game's own chunking policy (~8 MiB uncompressed
  chunks) is observed, not specified; the tooling's writer matches it
  on this build.
- **UNTESTED** — `.dat` content *payloads*: directory entries (101,459)
  verified; payload data not yet tested (payloads weren't available
  off-PC).
- **NOTE** — Upstream 0.2 now *rejects* KFC3 files outright
  (`InvalidMagic`); the fork accepts both magics and preserves the
  existing file's magic on write.
