# KFC4 Round-Trip Audit — Report

Date: Nov 1, 2026 · Game build 1273043 · Tooling under test: Brabb3l/kfc-parser **v0.2.0** (upstream) and Eric's M182 fork (EML 0.1.2 snapshot + ports)
Harness: `harness/` (upstream kfc-base) and `harness-fork/` (fork kfc-base) in this folder; raw logs and per-resource fingerprints in `work/`.

## Verdict

**Reading KFC4 is native and complete. Writing is semantically lossless but byte-level lossy — there is no byte-faithful round-trip. And Eric's suspicion about provenance is literally true: the "KFC4 support" in the current tooling is the KFC3 format model with the magic constant changed — one line — which happens to be correct, because Keen changed the magic without changing the container layout.**

Three layers, separately:

1. **Format model — correct, and proven on real data.** Upstream's KFC3→KFC4 transition is commit `5b5f50b` *"kfc: changed magic value from KFC3 to KFC4"* (Oct 16, 2026): 1 insertion, 1 deletion, the magic constant in `header.rs` and nothing else. The carried-over layout parses Eric's real KFC4 container perfectly: **91,901 of 91,901 resources decompressed and read, zero errors, zero missing**; the 66-chunk table tiles the 216,707,072-byte resources stream *exactly*; section layout of the game-era files matches the writer's canonical layout section for section. So it is not a fake header swap that limps — the KFC3 model *is* the KFC4 format, as far as every byte in this container can testify. (In the fork's older lineage, KFC4 was even treated as the dedicated-server magic for the same layout — `KFC_DIR_MAGIC_SERVER` in the fork's `header.rs:8`.)
2. **Content fidelity — 100% lossless.** A zero-change re-serialization preserves every resource's bytes (FNV-1a fingerprints identical for all 91,901 resources), every directory entry value, every content entry, and all 66 original chunk records verbatim.
3. **Container fidelity — lossy.** The written `.kfc` differs from the source in **6,545,161 of 9,396,224 bytes (~70%)**: static-map key order is not preserved (the maps are rebuilt through a `HashMap`), bundle index bases are recomputed in arbitrary type order, and a zero-change write still **appends a spurious empty chunk** (+4,096 bytes to the resources stream, chunk count 66→67, header size +24). The tooling writes its own dialect of the directory — internally flawless, accepted by the game, but not byte-equivalent to what it read.

One-sentence version: **KFC4 support is real for reading and patching; it is not a faithful serializer, and never was — the writer rebuilds rather than reproduces.**

## Inputs and provenance

| File | Role | Resources | Chunks | Notes |
|---|---|---|---|---|
| `~/workspace/repro-gamedir/enshrouded.kfc` + `_resources` (216,707,072 B) | Primary test pair | 91,901 | 66 | Real build-1273043 KFC4 container from Eric's local repro staging |
| `~/workspace/repro-gamedir/enshrouded.kfc.bak` | Pre-write reference (EML backup) | 91,901 | 65 | Same counts as the primary; see §4 item 5 — it carries the fingerprint of one earlier zero-change write |
| `~/workspace/user/files/enshrouded.kfc` | Writer-output vector (99-clone session) | 91,916 | 60 | Directory only (no `_resources` locally) |
| `~/workspace/user/files/enshrouded-2.kfc` | Writer-output vector (81-clone session) | 91,898 | 60 | Directory only |

Caveats: the `.dat` content payloads are **not** present locally (only nine zero-byte stubs, `enshrouded_055.dat`–`_063.dat`), so content *data* reads/writes could not be tested — only content *directory entries* (101,459 of them, carried exactly). The fork results were produced against the local clone's `ports-work` working tree (0.1.2 + the uncommitted PR #3/#4 ports), not the byte-exact shipped release DLL; the container read/write code paths exercised are, per the earlier verified fork↔upstream diff, identical except magic handling.

## 1. Format inventory — proven vs assumed

Layout as implemented in `crates/kfc-base/src/container/` (upstream v0.2.0) and confirmed against the real files with an independent Python header probe (`work/` logs):

**Proven (explicit in code and verified byte-for-byte on the real container):**

- Header, 144 bytes: `u32 magic = 0x3443464B "KFC4"` (`header.rs:7`), `u32 size`, `u32 unk0`, 4 bytes zero padding (validated on read — `io.rs` `padding()` rejects nonzero), then **16 `KFCLocation`s** `{u32 self-relative offset, u32 count}` — the stored offset is relative to its own field position (`io.rs:110` `read_u32_offset`, `io.rs:199` `write_offset`).
- `header.size` = offset of the end of the last section; the file length is `size` rounded up to 0x1000 (observed on all four files: e.g. size 9,394,448 → length 9,396,224).
- Version tag: raw string at the version location (65 bytes: `1273043|^/game38/branches/release_1.0|2026-10-20T14:55:29.904125Z`), carried verbatim.
- Section order in real files is exactly the writer's order (`KFCFile::write`, `file.rs`): version → containers → resource_locations → resource_chunks → resource_indices → content map → resource map → bundle map. Inter-section gaps are only 8-byte alignment and the chunk table's count prefix.
- Chunk table: a `u32` count sits 4 bytes *before* the location the header points to (writer: `file.rs` writes the count, then records `position + 4` as the offset); entries are 20-byte `ResourceChunkInfo {offset, size, compressed_size, uncompressed_offset, uncompressed_size}`.
- Compression: one **zstd** frame per chunk (`reader.rs` `zstd::stream::copy_decode`; writer `copy_encode` at level 0). Chunk geometry on the real file: uncompressed ranges contiguous from 0 (501,156,384 bytes total), compressed ranges 4096-aligned and tiling the stream with zero slack (sum of chunk `size` = file length exactly).
- Static maps (contents / resources / bundles): parallel `buckets / keys / values` arrays; bucket count = next power of two of the entry count (observed 131,072 for 101,459 contents, 131,072 for 91,901 resources, 256 for 146 bundles — exactly `StaticMapBuilder`'s rule, `static_map.rs`). `StaticMap::from_parts` validates length/bucket invariants on read.
- Record sizes: `ResourceId` 32 B (GUID 16 + type hash + part + reserved), `ResourceEntry` 8 B `{offset, size}`, `ContentHash` 16 B, `ContentEntry` 16 B, `ContainerInfo` 16 B, `ResourceLocation` 12 B (exactly one present; its aggregates match the chunk table: uncompressed 501,156,384, compressed 216,562,036, count 91,901).
- Lookups work as the game expects: every one of the 91,901 resources was found through the map and read — the hash/bucket assignment the code assumes is the one the file uses.

**Assumed / heuristic / implicit (not established format knowledge):**

- **24-bit resource sizes.** `reader.rs:7-9` documents that the game stores resource sizes truncated to 24 bits, and `actual_resource_size` (`reader.rs:459`) *recovers* sizes ≥16 MiB from the gap to the next resource, accepting the correction only if the gap equals `stored + k·16 MiB` with <16 bytes remainder. This is an inference about the game's writer, not a parsed field. In this container it fires **exactly once** — resource `509feadb-4c60-425f-9c7c-deeefd9b6920`, stored 11,105,420 → actual 44,659,852 (= stored + 2×16 MiB, gap-consistent) — and upstream added it after the magic change (commit `d71544a`, PR #35), i.e. it was discovered from real KFC4 data, not specified.
- `unk0`: read and discarded (`let _unk0`), hardcoded to `12` on write (`KFCHeader::write`). Semantics unknown; 12 in every observed file.
- `unused0`/`unused1` locations: carried as zero. (Quirk: a stored offset of 0 reads back as the field's own position, `io.rs` — harmless because the fields are unused.)
- The game's *own* chunking policy is only observed, not coded: the writer uses fixed 8 MiB uncompressed chunks (`writer.rs` `RESOURCE_CHUNK_SIZE`); the observed file's chunks are also ~8 MiB (e.g. 8,388,608), so they agree here, on this build.
- `ContainerInfo {size, count}` semantics for the `.dat` containers: carried, never validated against payloads (payloads absent locally).
- That KFC3 files share this layout in general: strongly indicated (one-line upstream magic commit; fork accepts both magics with one layout) but only KFC4/build-1273043 was sampled here. Upstream 0.2 now *rejects* KFC3 outright (`InvalidMagic`).

## 2. Parse results (upstream reader, primary pair)

| Check | Result |
|---|---|
| Directory parse (`KFCFile::from_path`) | OK — version tag, 64 containers (55 nonzero), 101,459 contents, 91,901 resources, 146 bundles, 146 distinct type hashes |
| Chunk table consistency | uncompressed ranges contiguous ✓; all compressed ranges within the stream ✓; sum of chunk sizes = stream length (216,707,072) exactly ✓ |
| Full content read (every resource via `KFCCursor`) | **91,901 ok / 0 missing / 0 errors / 0 length mismatches** |
| Total decompressed resource bytes | 483,669,781 (of 501,156,384 uncompressed stream bytes; the rest is inter-resource alignment/unused space) |
| 24-bit size corrections applied | 1 (see §1) |
| Fork reader, same file | Identical results; per-resource fingerprints **byte-identical to upstream's** (`work/fp-pristine.txt` vs `fp-pristine-fork.txt`) |

## 3. Round-trip results

**Method.** There is no public "load → save unchanged" API: `KFCFile::write` is `pub(super)` (`file.rs:348`), reachable only inside the container module. The closest supported path — and the one the game sessions actually use — is the mod-loader's own construction (`mod-loader-lua/src/load.rs` `create_writer`): `KFCWriter::new_incremental_with_options(dir, name, reference_file, type_registry, {overwrite_containers: true})`, then `finalize()` with **zero** `write_resource`/`write_content` calls, run on copies in `work/rt/` (inputs untouched). Registry: Eric's `.cache/types.json` (12,646 types).

**Upstream v0.2.0:**

| Measure | Pristine | After zero-change write | Delta |
|---|---|---|---|
| `.kfc` length | 9,396,224 | 9,396,224 | 0 |
| `.kfc` header.size | 9,394,448 | 9,394,472 | **+24** (+20 chunk entry, +4 alignment) |
| `.kfc` differing bytes | — | **6,545,161 (~70%)**, first diff at offset 4 (the size field) | — |
| Resources stream length | 216,707,072 | 216,711,168 | **+4,096** |
| Resources stream, original prefix | — | **byte-identical** for all 216,707,072 bytes | appended tail only |
| Appended tail | — | 4,096 B: a 9-byte zstd frame of *empty* input (`28 b5 2f fd 20 00 01 00 00`), rest zeros | — |
| Chunk count | 66 | 67 | +1 empty chunk (uncompressed_size 0, compressed_size 9) |
| Original 66 chunk records | — | **all field-identical** | 0 diffs |
| Resource entries (offset+size per GUID) | — | **0 diffs** across all 91,901 | set equal, values equal |
| Resource key *order* | — | differs (first divergence at index 4) | — |
| Content entries | — | 0 diffs, set equal; key order differs | — |
| Bundles (146) | — | internal_hash diffs **0**, count diffs **0**, **index diffs 146/146** | index bases recomputed |
| Containers / version | — | identical | — |
| Re-read of round-tripped file | — | 91,901 ok / 0 errors; **fingerprints identical to pristine for every resource** | content lossless |

**Fork (0.1.2 + ports), same procedure:** same profile. `finalize` OK; stream +4,096 with identical original prefix; `.kfc` differs in 6,538,916 bytes; semantic compare (parsed by the upstream reader) shows the same zero value diffs, 66→67 chunks, order-only differences; output magic correctly preserved as `KFC4` (the fork's `set_magic` path, fork `file.rs:101` / `writer.rs:422`).

## 4. Where the loss comes from (code-level)

1. **No faithful serializer exists.** `KFCFile::write` (`file.rs:348`) is `pub(super)`; the only public write path is `KFCWriter`, which *rebuilds* a fresh `KFCFile::default()` in `finalize()` (`writer.rs:363`; the fresh `KFCFile::default()` is built at `writer.rs:413`) from builder state instead of reproducing the loaded file's layout.
2. **Map order is destroyed by construction.** Incremental load converts each `StaticMap` via `as_builder()` into a `HashMap` (`static_map.rs:122`), and `StaticMapBuilder::build()` (`static_map.rs:193`) re-sorts entries by `static_hash % bucket_size`. The game's within-bucket ordering is unrecoverable from that path — hence ~6.5M differing bytes concentrated in the indices/map sections (first byte divergence after the header is in `resource_indices`).
3. **Bundle indices are recomputed, in arbitrary type order.** `set_resources` → `rebuild_resource_bundles` (`file.rs:180`) regroups `resource_indices` by iterating a `HashSet` of type hashes and reassigns every bundle's `index`. Values stay *correct* (the game resolves resources through the map, not file order), but the layout is the writer's dialect, not the game's.
4. **The empty chunk.** `finalize()` unconditionally calls `submit_resource_data()` first (`writer.rs:364`; the function itself at `writer.rs:299`), which zstd-encodes the (empty) chunk buffer and appends it, 4096-aligned. A write that changes *nothing* therefore still grows the resources stream by 4,096 bytes and the chunk table by one record — every time.
5. **Corroboration in Eric's own files.** The staging pair shows the same signature without my harness: `enshrouded.kfc.bak` (65 chunks, resloc compressed 216,562,027, header.size 9,394,432) → `enshrouded.kfc` (66 chunks, 216,562,036 = +9, header.size 9,394,448): identical resource/content counts, +1 chunk, +9 compressed bytes — one earlier zero-change write through this path, fossilized.

## 5. Writer-output vectors (99-clone vs 81-clone)

Both parse cleanly through `KFCFile::from_path` (all `StaticMap::from_parts` validations pass). Cross-comparison (99 = A, 81 = B): contents identical in set *and* values (101,459); resources 91,916 vs 91,898 with 4,505 entry diffs that are pure **offset** changes at equal sizes — the incremental writer relocates patched resources to appended chunks, so layout differences between two patched outputs are expected behavior, not parse drift; only 2 of 60 chunk records differ (the tail chunks where the appended data landed); exactly 1 bundle count differs (the cloned type). The tooling reads its own outputs consistently.

Notable: both game-session outputs have **60** chunks for ~495 MB uncompressed, vs 65–66 in the staging pair for 501 MB — the writer's chunk count is a function of write history (appends), not of the data. Another marker that tool-written files are a dialect: same data, different chunking per history.

## 6. What a proper KFC4 implementation would still need

1. **An order-preserving serializer.** Keep the loaded maps' bucket/key/value arrays intact and rewrite only changed entries (or reproduce the game's ordering rule, if one exists — currently unknown). Until then, no byte-faithful round-trip is possible through this codebase, and "the file looks different" can never be used as evidence about game behavior.
2. **A no-op-safe finalize.** Skip chunk submission when nothing was written; a zero-change pass should be a true no-op (today it mutates both files of the pair, measurably).
3. **The size-storage rule, stated as format fact.** The 24-bit truncation + gap-recovery heuristic works (1/91,901 here, gap-consistent), but a proper implementation should enumerate the ≥16 MiB resources and verify recovered sizes against the game's own behavior (e.g. in-game) rather than trusting the gap pattern universally — the heuristic silently falls back to the truncated size when the gap doesn't match its pattern.
4. **Content-side coverage.** The `.dat` container half of the format (101,459 contents, 64 containers) is untested end-to-end here for lack of payloads; a proper audit needs a full game-dir pull and the same parse/round-trip treatment for content data.
5. **Cross-build sampling.** Only build 1273043 was tested. The format's stability claims (and the KFC3↔KFC4 identity claim) should be re-checked per build; upstream's reader restructures (commit `45185ab`) and validation hardenings (#24/#25/#35) show the model is still being actively corrected against real files.
6. **For the 81-clone question specifically:** this audit *removes* one suspect class and sharpens another. Tool-written files are internally flawless at every level the format defines — directory, chunks, and decompressed content all check out — so if a consumer-side cliff exists at 82 clones, it is not visible in the container bytes this tooling produces or reads. But because the writer's output is a dialect (recomputed indices, reordered maps, history-dependent chunking), "internally flawless" is a statement about the tooling's model, not proof of byte-equivalence with a game-written file carrying the same resources.

## Artifacts

- `harness/` + `harness-fork/` — audit harness sources (subcommands: `audit`, `roundtrip`, `cmpdir`); rebuild with `cargo build --release` (Rust 1.88, `~/.cargo/bin`).
- `work/audit-pristine.log`, `audit-roundtrip.log`, `audit-pristine-fork.log` — parse logs.
- `work/fp-pristine.txt`, `fp-roundtrip.txt`, `fp-pristine-fork.txt` — per-resource fingerprints (GUID, type hash, stored size, actual size, FNV-1a-64 of content); pristine and round-trip files are identical.
- `work/roundtrip.log`, `roundtrip-fork.log`, `cmpdir.log`, `cmpdir-bundles.log`, `cmpdir-fork.log`, `vector-cmp.log` — round-trip and comparison logs.
- `work/rt/enshrouded.kfc`, `work/rt-fork/enshrouded.kfc` — the zero-change re-serialized directories (the 216 MB re-serialized resources streams were deleted after comparison; they were the original bytes plus the documented 4,096-byte tail).
