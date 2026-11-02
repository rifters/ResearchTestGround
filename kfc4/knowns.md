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
