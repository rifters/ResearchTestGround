# KFC4 — In-Place Writer Mode + 24-Bit Stored-Size Truncation: Report

Date: Nov 1, 2026 · Game build 1273043 · Bases: upstream Brabb3l/kfc-parser
**v0.2.0 + A1 patches 0001–0003** (`~/workspace/your_files/kfc4-a1/src/`,
git `21972fc`) and the **M182 fork at the `fix/byte-faithful-writer`
branch state** (fork head `168f132c` + the three ported A1 fixes,
`~/workspace/your_files/kfc4-port-rebase/src/`). Builds on the A3 report
(`~/workspace/your_files/kfc4-a3/A3-REPORT.md`), which isolated the two
writer mechanics that blocked byte-identity of a full repack.

## Verdict

**Both fixes are in, and every gate passes on both trees.**

- **Patch 0005 (24-bit truncation):** `ResourceEntry::write` now stores
  sizes truncated to 24 bits, exactly like the game's writer. Rewriting
  the one affected resource (44,659,852 B) with its own content now
  stores 11,105,420 (A3 stored the full 44,659,852); via the in-place
  writer the whole `.kfc` comes out **byte-identical** to pristine.
- **Patch 0004 (in-place writer):** a new opt-in incremental mode
  (`KFCWriteOptions::in_place`, default off — default behavior is
  unchanged, proven by the append-path regression runs). A zero-change
  in-place write is **byte-identical in both files**; a same-size edit
  is confined to the edited chunk's frame (the `.kfc` differs in
  exactly **2 bytes**); a shrink stays in place with the remainder
  zeroed; a growth relocates only that one resource and leaves the
  pristine stream as a byte-identical prefix.
- **Fork port:** both patches carried over with no structural
  adaptation (the touched code is identical to upstream except the
  import line and Carlos's magic plumbing, both untouched). Fork gates
  reproduce the upstream numbers exactly; KFC4 magic is preserved, and
  a synthetic KFC3-tagged input round-trips byte-identically with its
  tag preserved.

## 1. Patch 0005 — 24-bit stored-size truncation

**Where:** `crates/kfc-base/src/container/header.rs`,
`ResourceEntry::write` — the single serialization point for resource
map values (the writer path in `container/file.rs` serializes entries
exclusively through it). The size write becomes
`self.size as u32 & 0x00FF_FFFF`.

**Arithmetic (verified in a unit test):**
44,659,852 = 0x2A9748C; 44,659,852 & 0xFF_FFFF = 0xA9748C =
**11,105,420** — the value the game stored for resource
`509feadb-4c60-425f-9c7c-deeefd9b6920`. Exactly 16 MiB truncates to 0;
16 MiB−1 is kept whole; small sizes are unchanged (two new unit tests,
`resource_entry_write_truncates_size_to_24_bits`,
`resource_entry_write_keeps_small_sizes`).

**Why A1's zero-change round-trip was already byte-identical without
this fix, and why A3 exposed it:** the directory reader
(`ResourceEntry::read`) loads the *stored* value as-is — the in-memory
entry for the big resource carries size 11,105,420. The reader's 24-bit
recovery (`actual_resource_size` in `reader.rs`) is computed lazily at
content-read time only and never written back into the directory map.
A zero-change incremental write seeds its builder from the loaded map
(`resources().as_builder()`) and serializes the loaded value verbatim —
the truncation survived by round-tripping, not by being re-derived.
A3's pack instead called `write_resource` for every resource, and
`write_resource` inserts `size = bytes.len()` — the *actual* 44,659,852 —
which the unpatched `write` then serialized in full. Any path that
re-derives an entry from data hit the bug; paths that carried entries
through did not.

## 2. Patch 0004 — in-place writer mode

**API:** `KFCWriteOptions::in_place: bool` (default `false`), honored
by `new_incremental_with_options`; ignored for non-incremental writes.
`write_resource` dispatches on it; `write_content` (`.dat` payloads)
is untouched.

**Construction (`InPlaceData::load`)** snapshots everything from the
reference before any output byte exists — mandatory, because the
output stream file *is* the reference stream file, rewritten from
offset 0 at finalize:

- the reference chunk table (output geometry is taken from it);
- the full decompressed stream (501,156,384 B on this container) as
  the working copy edits are spliced into;
- each reference chunk's zstd level, **detected from the reference
  file itself**: the chunk is decompressed, re-encoded at candidate
  levels (order 9, 3, 1, 2, 4, …, 19) and compared byte-for-byte with
  the chunk's original frame; first match wins. A chunk no candidate
  reproduces keeps its original frame bytes as a verbatim fallback
  (used only if the chunk is never edited);
- every resource's allocation (offset + span), with stored sizes
  recovered from the 24-bit truncation by the reader's own rule,
  mirrored in the writer (bounds = sorted distinct offsets + stream
  end; a gap that equals a multiple of 16 MiB plus <16 B of alignment
  is added back).

**Edits (`write_resource_inplace`):**

- new size ≤ allocation ⇒ splice at the original offset, zero the
  remainder of the allocation, directory entry keeps the offset and
  takes the new size (serialized per patch 0005);
- otherwise (and for brand-new resources) ⇒ append after the
  reference stream's end, 16-aligned; the appended span becomes the
  resource's allocation for later rewrites in the same session, and
  the vacated original span stays behind as unreferenced data — the
  same orphan pattern A3 found in the game's own stream;
- repeated writes compose through the allocation table.

**Finalize (`flush_inplace_stream`)** writes the whole stream from
offset 0 and truncates the file to the new end:

- reference chunks keep their uncompressed offset/size and are encoded
  at **their own detected level**, so an unchanged chunk reproduces
  its original frame byte-for-byte (the property that keeps offsets
  stable). A *changed* chunk whose level was undetectable is encoded
  at level 9 (content stays correct; only the frame bytes are the
  fallback's). On this container no chunk needed either fallback —
  detection succeeded 66/66;
- **slot stability:** if a chunk's new frame fits its original
  4096-aligned slot, the slot size is kept and no later chunk moves;
  only a frame that outgrows its slot shifts the tail;
- appended data is chunked at the usual 8 MiB and encoded at level 9
  (the game's main-pass level per A2.5) — a documented choice;
- the directory is then built by the unchanged A1 path
  (`set_resources_preserving_bundles`, map order, bundle layout).

## 3. Gates — upstream tree

Harness: `harness/` (A1 harness + `bytecmp`, `inplace-roundtrip`,
`trunc-test`, `inplace-edit`, `dumpchunk`, `levelcheck`). All runs on
copies of the pristine pair (SHA-256 `7f6130bb…e544f79e9` /
`1cd249c3…0290afe341`). Logs and fingerprints in `work/`; the 216 MB
output streams were deleted after hash/byte comparison (`.kfc`
outputs kept).

| Gate | Result |
|---|---|
| **G5** tests | `cargo test -p kfc-base` **23/23** (21 at the A1 state + 2 new truncation tests; patch 0004 adds no unit tests — its proof is this harness, same as A1 fix 2). `cargo check -p mod-loader-lua` clean (2 warnings are the pre-existing upstream ones) |
| **G0** zero-change, in-place | **byte-identical, both files** (SHA-256 equal; `cmp` clean), 66 chunks, ~20 s |
| G0 regression, append path | zero-change write with default options also byte-identical — default behavior untouched |
| **G4-append** (isolates 0005) | rewriting the 44.6 MB resource identically via the append writer: stored size now **11,105,420** (A3: 44,659,852); semantic directory diff vs pristine is exactly **1 resource value — its offset** (25,175,536 → 501,156,384, the append relocation); bundles/contents 0 diffs; chunks 66 → 72. The `.kfc` is therefore *not* byte-identical on this path — the remaining difference is purely the relocation + chunk-table growth that define the append writer |
| **G4-inplace** | same rewrite via the in-place writer: entry unchanged (offset 25,175,536, stored 11,105,420), 66 chunks, **`.kfc` and stream both byte-identical to pristine** — the gate as specified |
| **G1** same-size edit | target `06e4721d-…` (69,472 B, inside chunk 20), bytes XOR-flipped at stride 997. Output: offset/size unchanged; **exactly 1 of 66 chunk records differs** — chunk 20 `compressed_size` 5,398,802 → 5,398,935 (+133), slot kept (5,402,624), all chunk offsets unchanged. Stream: only chunk slot 20 differs; differing bytes span [22,006,111, 26,632,596], entirely inside chunk 20's frame (slot [21,233,664, 26,636,288)); 4,607,772 positional byte diffs, all within that frame; lengths equal. **`.kfc`: exactly 2 differing bytes** — [1244] the resource-location aggregate `compressed_size` (216,562,036 → 216,562,169) and [1668] chunk 20's `compressed_size` field, low byte each. Fingerprints vs A0 baseline: exactly the target's line differs; readback == new content; 91,901/91,901 reads OK |
| **G2** smaller edit | same target, content = first half (34,736 B): stays in place, stored size 34,736, chunk 20 frame shrinks within its kept slot, only chunk slot 20 differs in the stream. **Zero-padding proven directly:** decompressed output chunk 20 shows the remainder of the old span (34,736 B) is **100 % zero** (pristine: 25,202 nonzero bytes); bytes past the old span untouched. Fingerprints: only the target's line |
| **G3** larger edit | target `2bbef7a7-…` (1,416 B) grown to 101,416 B: relocates to 501,156,384 (the reference stream end); **all 66 original chunk records field-identical**, one chunk appended (67 total); directory value diffs = exactly 1 (the target); stream diff = a single range, the appended slot [216,707,072, 216,711,168) — the pristine stream is a byte-identical prefix; readback == new content; fingerprints: only the target's line |

**Detector cross-check** (harness `levelcheck`, replicating the
writer's candidate order independently): first-match levels =
**61 chunks at level 9** (the 58 level-9 chunks of A2.5 plus the three
empty chunks 59/63/65, which match {1, 3, 9} and take 9 first in this
order), **5 chunks at level 3** (58, 60, 61, 62, 64), **0 undetected**;
the {1, 3, 9} match sets equal A2.5's `chunks.json` records for all 66
chunks. G0/G4-inplace byte-identity confirms the writer's internal
detection end-to-end: every unchanged chunk was re-encoded at its
detected level and the stream still came out byte-identical.

**Differing-range note (G1 expectation):** the task's G1 expectation
allowed knock-on compressed-offset shifts; the slot-stability rule
eliminated them for this edit (the +133 B frame growth fit the slot's
3,822 B of slack), so the actual differing ranges are *narrower* than
the expectation — one frame in the stream, two bytes in the `.kfc`.
An edit whose frame outgrows its slot would shift subsequent chunk
offsets, as the expectation described; the mechanism is the slot rule
in §2, and G3's appended chunk shows the corresponding tail behavior.

## 4. Port to the M182 fork

**Branch base:** `src-fork/` is a copy of
`~/workspace/your_files/kfc4-port-rebase/src/` — fork head `168f132c`
with the three A1 fixes ported and rebased, i.e. exactly the state of
branch **`fix/byte-faithful-writer`** (PR #5). The new patches sit on
top of that state. `patches-fork/` apply at the **`eml-fork/`**
directory level (paths are `crates/kfc-base/...`), in numeric order
(0004, then 0005 — disjoint files).

**Region diff (before porting):** the fork's `header.rs` differs from
upstream only in the magic plumbing (dual KFC3/KFC4 constants, `magic`
field, header read/write) — `ResourceEntry::write` is byte-identical.
The fork's `writer.rs` differs only in (a) the import line — the fork
already imports `Read`, so its 0004 import hunk adds `BufReader` only —
and (b) the finalize magic block (reads the existing file's 4-byte tag,
calls `file.set_magic(magic)`), which sits *after* the chunk-preparation
region and is not touched by either patch (the fork's 0004 patch
contains zero `set_magic` references). Everything else the patches
touch was byte-identical to upstream pre-patch, so the changes were
applied directly, region by region.

**Cross-verification (both directions):**

- fork-patched vs upstream-patched: `writer.rs` differs *only* by the
  magic block (9 lines); `header.rs` differs *only* by the pre-existing
  magic regions. Nothing else.
- patches-fork applied to the pristine port-rebase base reproduce
  `src-fork/`'s two files exactly (verified by application + diff).

**Fork gates (subset, harness-fork/):**

| Gate | Result |
|---|---|
| `cargo test -p kfc-base` | **25/25** (23 at the branch base + 2 new) |
| `cargo check -p mod-loader-lua` | clean |
| G0 append + in-place | byte-identical, both files; output magic **KFC4** |
| G4 append | stored size 11,105,420; offset relocated (append semantics), chunks 66 → 72 |
| G4 in-place | **byte-identical, both files** |
| G1 same-size edit | numbers identical to upstream: same target, chunk 20 `compressed_size` 5,398,802 → 5,398,935, same fingerprints |
| Magic spot-check | synthetic KFC3-tagged input (pristine pair with the first 4 bytes flipped to `KFC3`): in-place zero-change round-trip is **byte-identical to the tagged input**, output magic **KFC3** — Carlos's preservation path intact |

## 5. Caveats

- **Cost of in-place mode:** construction decompresses and level-tests
  the reference stream (~500 MB working copy, ~20 s on this VM for the
  full container) even for a one-resource edit. It is opt-in; the
  append path remains the default for the loader's patch-time use.
- **Detection fallbacks exist but were not exercised by this
  container** (66/66 detected): an undetected-and-unchanged chunk is
  written from its stored original frame; an undetected-and-changed
  chunk is encoded at level 9. Both keep content correct.
- **Slot growth:** a changed frame that outgrows its slot shifts all
  following chunk offsets (directory updated accordingly) — valid for
  the reader, but that write is then no longer offset-stable. G1/G2
  did not trigger it.
- `.dat` content payloads are untouched by both patches and remain
  directory-level only, as in A1–A3 (only zero-byte stubs exist
  locally).
- Neither output pair has been near the game; in-game acceptance
  remains Eric-side, as with A1.

## 6. Artifacts

- `patches/0004-inplace-writer.patch`, `patches/0005-truncate-stored-size-24bit.patch`
  — against upstream v0.2.0 + A1 patches 0001–0003; apply in numeric
  order (disjoint files; verified: applying them to the A1 tree
  `21972fc` reproduces `src/` exactly). The work was committed in the
  order 0005, then 0004 (commits `422a94c`, `a125ac9` on top of A1's
  `21972fc` in `src/`'s git history).
- `patches-fork/0004-inplace-writer.patch`,
  `patches-fork/0005-truncate-stored-size-24bit.patch` — fork versions,
  apply at the `eml-fork/` level on top of the `fix/byte-faithful-writer`
  branch state.
- `src/` — patched upstream tree (git history preserved).
- `src-fork/` — patched fork tree.
- `harness/`, `harness-fork/` — gate harnesses.
- `work/` — run logs, fingerprint files, audit logs, the kept `.kfc`
  outputs, G2's decompressed chunk-20 evidence (`g2-chunk20.bin`,
  `pristine-chunk20.bin`), and `levelcheck.log`.
