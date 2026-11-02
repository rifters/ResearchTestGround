# A3 — Repack the A2 extraction and test it against pristine

- **When:** 2026-11-01 (VM, local; game build 1273043 container)
- **Inputs (all read-only, unmodified):** A2 extraction
  `~/workspace/your_files/kfc4-a2/extracted/`; pristine pair
  `~/workspace/repro-gamedir/` (`.kfc` SHA-256 `7f6130bb…e544f79e9`,
  `_resources` `1cd249c3…0290afe341`); A0 baseline
  `~/workspace/your_files/kfc4-audit/work/fp-pristine.txt`.
- **Tool built here:** `kfc4-pack` (`~/workspace/your_files/kfc4-a3/kfc4-pack/`),
  on the A1-patched kfc-base.
- **Artifacts kept:** `work/` (logs, fingerprints, hashes), the two output
  `.kfc` files (`work/grade1-out/`, `work/edit-out/`). The two 448 MB output
  streams were deleted after hash-verified comparison; every number below is
  in the logs they were checked against.

## Verdict

**The folder is a complete and faithful representation of the resource
payload half — proven. Byte-identity of a Grade 1 repack is blocked by the
writer's mechanics, not by the folder.**

- Grade 1 (reference-assisted repack from folder bytes): **ran clean**.
  Before any byte was written, the tool verified all **91,901** manifest
  entries against the folder — file sizes, per-file FNV fingerprints, and
  the A0 baseline — **0 mismatches, 0 missing, 483,669,781 bytes verified**.
  The pack then wrote every resource from its extracted file and finalized.
- The output is **not byte-identical** to pristine, for exactly two
  mechanical reasons in the A1-proven incremental writer (see §1):
  append-only relocation of every written resource, and one full-width
  size field where the game truncates to 24 bits. Neither is a folder
  deficiency: re-reading the output reproduces the A0 baseline fingerprints
  for **91,901 / 91,901** resources.
- Grade 2 (folder-only rebuild): **not possible from the A2 folder** — the
  directory half (map key orders, chunk table, contents map, stream
  offsets, ~17 MB of unreferenced stream data, the `.dat` payloads) is
  simply not in it. §3 gives the section-by-section account and the exact
  extra data an "A2.5 full-fidelity extraction" would need (≈ 25 MB of
  records on top of the 483 MB payload half). With that data recorded,
  every byte of the pair is accounted for — including the compression,
  which §3.2 shows is reproducible chunk-for-chunk from the stock zstd
  encoder once the per-chunk level is known.

## 1. Grade 1 run

Command (tool usage in §5):

```
kfc4-pack pack <A2 extracted> <pristine dir> work/grade1-out \
  --types repro-gamedir/.cache/types.json --baseline fp-pristine.txt
```

Exit 0, 93 s. Output: `.kfc` 9,396,224 bytes (same length as pristine),
SHA-256 `ba3c439f…ca41c6`; stream 448,466,944 bytes, SHA-256
`990aa5cd…9ad05`.

### 1.1 Why it cannot be byte-identical (writer semantics, from source)

The incremental writer (`KFCWriter::new_incremental_with_options`) seeds
itself from the reference, positions the stream writer **at the end of the
reference stream**, and `write_resource` *always* appends the written
resource's data to newly created chunks — there is no in-place path.
Writing all 91,901 resources therefore produces: pristine stream prefix
(untouched) + a second, complete copy of the payload appended. The
directory is rewritten pointing at the appended copies. That is what A1
proved of this writer (byte-faithful for *zero-change* and *sparse-edit*
writes); a write-everything pack is the opposite regime.

### 1.2 Comparison against pristine (the precise diff)

| Check | Result |
|---|---|
| Stream byte compare | **First 216,707,072 bytes byte-identical** (the whole pristine stream); differing bytes = 231,759,872 = exactly the appended tail; single range |
| `.kfc` byte compare | 6,255,641 differing bytes of 9,396,224 (positional measure — includes the +1,160 B chunk-table growth shifting later sections; the semantic diff below is the meaningful one) |
| Semantic directory diff | **version identical; containers identical; contents map: key sequence and all 101,459 values identical; bundles: all 146 entries identical in every field** |
| Resource entries | key sequence identical; **offsets differ for 91,901 / 91,901** (relocated to appended region); **sizes differ for exactly 1** |
| The 1 size diff | world GUID `509feadb-…`: pristine stores 11,105,420 (24-bit-truncated), output stores the true 44,659,852 — kfc-base's `ResourceEntry::write` writes the full u32, the game's writer truncates (spec §4.4). The content is identical (same fingerprint, below) |
| Chunks | 66 → 124 (58 appended); first 66 chunk records field-identical; `ResourceLocation` aggregates grow accordingly |
| Content proof | Output re-read: 91,901 OK / 0 missing / 0 errors; fingerprints identical to the A0 baseline on (guid, type, actual size, fingerprint) for **91,901 / 91,901**. The single full-line difference is the stored-size *column* of the resource above — fingerprint hash `f4af3fa9fe6bf4f4` matches |

So the output is a valid, self-consistent container carrying exactly the
folder's bytes for every resource, with a directory that differs from
pristine only in where the payload lives (plus the one un-truncated size).
Byte-identity would require an in-place writer (replace data within the
reference's own chunks when sizes fit) or the Grade 2 route.

## 2. Edit test

Setup: hardlink copy of the extraction (`cp -rl`; the VM filesystem refuses
`cp -al`'s ownership preservation — see §6), then one file replaced by a
modified copy (write-temp + rename, so the original inode — and therefore
the real extraction — is untouched; verified: the original still
fingerprints to its manifest value afterwards).

- **Edited resource:** `000c4c03-5677-4399-863e-4c5f2d14a50c`
  (`keen::JointAnimation`, 9,376 bytes); one byte at offset 4,688 flipped
  (252 → 3, XOR 0xFF), length unchanged.
- **Without `--allow-modified`:** the tool's verification pass flagged
  exactly this entry — `modified: 000c4c03-…` — and **aborted, exit 1, no
  output directory created**. The folder must prove itself before a byte is
  written; it did, by refusing.
- **With `--allow-modified`:** pack succeeded; sizes identical to Grade 1.
- **Verification of the edit output:**
  - Parses; fingerprint pass reads 91,901 / 91,901.
  - Against the A0 baseline, exactly **one** fingerprint differs — the
    target's, and it equals the fingerprint computed independently from the
    edited file (`a6d318fe83428382`). The edited resource reads back with
    the new bytes; the other 91,900 are untouched.
  - Directory vs the Grade 1 output: **byte-identical** (`.kfc` compare:
    0 differing bytes; same SHA-256 `ba3c439f…`). A same-length content
    edit is invisible to the directory — as the format says it must be.
  - Stream vs the Grade 1 output: **exactly 1 differing byte** in
    448,466,944, at offset 216,711,911 — inside the first appended chunk
    (the target sits at appended uncompressed offset 9,984, chunk 66).
    The zstd frame length is unchanged; the flipped content byte was
    carried as a literal, and no match decision changed, so the compressed
    form differs in precisely the one literal byte.

## 3. Grade 2 — section-by-section recoverability from the A2 folder

Legend: **exact** = byte-for-byte reconstructible from folder data;
**partial** = some fields exact; **no** = not derivable from the folder.
"Closing data" is what an A2.5 extraction must record to make the section
exact. Verdicts rest on the A1 spec plus the measurements in §3.1–3.3,
taken on the pristine pair with the tool's `analyze`/`probe` subcommands.

| Section | Folder data | Verdict | Closing data for A2.5 |
|---|---|---|---|
| Header magic + `unk0` | constants (magic string; `unk0` = 12 as the tooling writes it — byte known, semantics still unknown) | exact (byte) | — |
| `header.size` + 16 locations + 0x1000 zero padding | pure functions of the section bytes (packing algorithm proven in A1) | exact iff all sections exact | — |
| Version string (65 B) | `_summary.json` records `version_tag` verbatim (`1273043\|^/game38/…`) | **exact** | — |
| Containers [64] × {u64 size, u64 count} | nothing | **no** | dump the 64 records verbatim (1,024 B). (Extracting the `.dat` files would give sizes but not counts.) |
| ResourceLocation [1] × {count, uncompressed, compressed} | count = 91,901 ✓ (manifest); sums ✗ | partial | follows from the chunk table once recorded |
| Chunk-count prefix | = chunk count | exact iff chunk table known | with chunk table |
| Resource chunks [66] × 20 B | manifest records only each resource's chunk *span indices*; summary records the count (66). Chunk uncompressed sizes are **not** uniformly 8 MiB (last chunks: 448,792 / 2,976 / 5,616,320 / 5,168,560) and compressed sizes depend on per-chunk zstd level (§3.2) | **no** | dump the chunk table verbatim (1,320 B) + per-chunk zstd level (66 B) |
| Resource indices [91,901] × u32 | each resource's type ✓; key sequence ✗; bundle segment order ✗ (proven non-derivable, A1 §4.7) | **no** | dump verbatim (367,604 B). (Alternative: key sequence + bundle table + the within-segment-matches-key-order rule — one assumption more than the dump.) |
| Resources map — buckets [131,072] | bucket-count rule proven (next pow2); hashing deterministic | exact given key sequence | key sequence (below) |
| Resources map — keys (91,901 ResourceIds, in bucket order) | manifest order is GUID-ascending, **not** the file's key order; within-bucket order is the game's insertion order | **no** | dump the key sequence verbatim (91,901 × 32 B ≈ 2.94 MB) |
| Resources map — values {u32 offset, u32 size} | size ✓ per resource (manifest); **offset ✗** (manifest records chunk spans, not stream offsets); array order needs key sequence | partial | add `offset` (u32) to each manifest entry |
| Contents map — buckets [131,072], keys [101,459], values (101,459 × {u32, u32}) | nothing at all — never extracted | **no** (0%) | dump the whole map verbatim (≈ 4.3 MB). Payload files alone would not suffice: how a `ContentHash` derives from payload bytes is not established in kfc-base (the writer receives hashes precomputed) |
| Bundles map — buckets [256] | rule proven | exact given keys | — |
| Bundles map — keys [146 type hashes] | all 146 known (manifest/summary) but not their file order | partial | record the 146 keys in file order (584 B), or the whole map verbatim |
| Bundles map — values {internal_hash, index, count} | count ✓ (proven equal to per-type resource counts: **0 mismatches / 146**); internal_hash = the registry's `internalHash` (registry is a pack *input*, not folder data); index ✗ (segment order) | partial | dump the 146 entries verbatim (1,752 B) closes all three fields |
| `.kfc` tail | alignment rule proven (0x1000, zeros) | exact | — |
| Stream, uncompressed | resource bytes ✓ (483,669,781 B at **unknown offsets**); inter-resource bytes split into two proven classes (§3.1): ≤15 B pads are zero; **3,960 orphan regions (16,989,572 B) are data-bearing and referenced by no directory entry** | **no** | per-resource offsets (above) + dump the orphan ranges verbatim with their offsets (≈ 17 MB). Then every stream byte is accounted: resources + zero pads + orphans |
| Stream, compressed frames | chunk contents (given the uncompressed reconstruction) | exact **given per-chunk level**: all 66 frames reproduce byte-for-byte with the stock zstd crate — 61 at level 9, 5 at level 3 (§3.2) | record per-chunk zstd level (66 B) at extraction time; the level split is only discoverable by trial against the original frames |
| `.dat` payload files [64] | nothing (this VM holds only zero-byte stubs `enshrouded_055–063.dat`) | **no** | copy the `.dat` files into the extraction wholesale (opaque containers; real files exist only on Eric's PC) |

**A2.5 total:** ≈ 25 MB of recorded directory/stream data (≈ 5% on top of
the payload half) plus the `.dat` files themselves — and then a folder-only
rebuild is byte-exact in every section, with the writer laying sections out
in the canonical order A1 already proved byte-faithful.

### 3.1 Stream layout census (pristine, measured)

Resources sorted by offset tile the uncompressed stream with **0
overlaps**, ending at 501,156,369 + a final 15-byte pad = 501,156,384.
Between consecutive resources there are **68,099 gap regions totaling
17,486,603 bytes**:

- **64,139 small gaps (≤ 15 B), 497,031 bytes — 100% zero.** The 16-byte
  alignment padding. Exactly reproducible by rule.
- **3,960 large gaps (> 15 B), 16,989,572 bytes — every one contains
  nonzero data** (5,381,900 nonzero bytes in total; first at uncompressed
  offset 25,027,488). These regions are referenced by *no* resource entry
  — orphan data inside the stream. The two largest (927,567 B @
  485,469,553 and 927,560 B @ 130,631,352) are **not** near-duplicates of
  each other (556,141 of 927,560 bytes differ; 8-byte common prefix), so
  the "superseded copies of one another" guess is not supported; what they
  are (data addressed by some other index, abandoned writes, …) is
  undetermined. Operative fact only: a folder-only rebuild cannot invent
  them — they must be dumped.

### 3.2 The game's zstd settings (measured)

Re-encoding each pristine chunk's uncompressed bytes with the stock zstd
crate (the same encoder kfc-base uses):

- level 0 (the tooling's `copy_encode`): reproduces **8 / 66** frames.
- **level 9: reproduces 61 / 66 frames byte-for-byte.**
- The remaining 5 chunks (indices 58, 60, 61, 62, 64 — all in the file's
  final stretch) reproduce **exactly at level 3** (chunk 60 also at 4).
  Their frames carry window descriptor `0x58` (2 MiB window) vs `0x60`
  (4 MiB) for the level-9 chunks — the level difference is visible in the
  frame headers themselves.

So the game compresses with (at least) two settings — consistent with a
main pass at level 9 and a tail/finalization pass at level 3 — and every
frame is reproducible with the stock encoder *provided the per-chunk level
is known*. A standalone rebuilder cannot derive the split; A2.5 must
record it.

### 3.3 Bundle counts (measured)

Bundle `count` equals the per-type resource count for **146 / 146** types
— that field is derivable from the manifest. (The `index` field remains
non-derivable per A1; `internal_hash` comes from the type registry.)

## 4. What this settles, and what it doesn't

- The A2 folder is **proven complete for content**: 91,901 resources,
  byte-exact, verified twice independently (pre-flight against the A0
  baseline; post-pack re-read). Any future tool — in-place writer,
  standalone rebuilder with A2.5 data, per-type decoders (A5) — can trust
  it as the payload source.
- Byte-identity via the *current* writer is a writer feature away:
  in-place replacement (write into the resource's own chunks when the new
  data fits the old allocation) would make a folder-sourced pack
  byte-identical to pristine for unchanged content. That is a kfc-base
  change, not an extraction change.
- Grade 2 byte-identity needs the A2.5 records in §3 (≈ 25 MB + `.dat`
  files). The `.dat`/contents half can only be captured on Eric's PC,
  where the real payload files exist.
- Neither output pair has been near the game; in-game acceptance remains
  Eric-side, as with A1.

## 5. Tool usage

```
kfc4-pack pack <extract-dir> <reference-dir> <output-dir>
               [--types <types.json>] [--baseline <fp-file>] [--allow-modified]
    Verify the folder (manifest sizes + FNV fingerprints; with --baseline,
    every entry must also match the A0 audit line — an entry that matches
    neither is a hard error unless --allow-modified, and baseline entries
    with no folder counterpart are fatal regardless), then drive the
    A1-patched incremental writer from the folder's bytes, in manifest
    order, over a copy of the reference pair.

kfc4-pack fps    <dir> <name> <fp-out>     fingerprint pass over a pair (A0 line format)
kfc4-pack cmpdir <dirA> <dirB> <name>      semantic directory comparison
kfc4-pack bytecmp <fileA> <fileB>          streaming byte comparison with diff ranges
kfc4-pack analyze <dir> <name>             layout census + zstd level-9 identity test
kfc4-pack probe   <dir> <name>             orphan-region table + zstd level sweep
```

Build: `cargo build --release` in `kfc4-pack/` (path dependency on the A1
tree's kfc-base; zstd pinned `=0.13.3`, the A1 lock's version).

Logs: `work/pack-grade1.log`, `pack-edit-denied.log`, `pack-edit.log`,
`sha-grade1.log`, `sha-all.log`, `bytecmp-grade1-kfc.log`,
`bytecmp-grade1-stream.log`, `cmpdir-grade1.log`, `fps-grade1.log`,
`cmpdir-edit-vs-grade1.log`, `bytecmp-edit-stream.log`, `fps-edit.log`,
`analyze-pristine.log`, `probe-pristine.log`, `analyze-level9.log`,
`probe-level9.log`, `probe-level9-sweep.log`, `edit-target.json`.

## 6. Run notes (honest incidents)

- The first `analyze` run encoded at zstd level 0 (the tooling's default)
  and reproduced only 8/66 frames; the level sweep in `probe` found the
  9/3 split. No conclusion was drawn from the level-0 result.
- `cp -al` fails on this VM filesystem ("failed to preserve ownership",
  the same EPERM class A2 hit); `cp -rl` (hardlinks, no preservation)
  works and was used. The partial first copy was deleted before any edit.
- The first edit-denial run aborted on the *incompleteness* check before
  naming the modified GUID: a content edit leaves its original baseline
  line unconsumed, which the leftover counter misread as a missing entry.
  Fixed in the tool (a flagged modification explains one leftover;
  unexplained leftovers remain fatal even with `--allow-modified`) and
  re-run — the denial now names the GUID and no output is created. The
  Grade 1 (unedited) verification was unaffected (0 modified, 0 leftover
  under both versions).
