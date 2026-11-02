# KFC4 contents-addressing map — the 101,459 content entries and the 64 `.dat` containers

**Date:** Nov 1–2, 2026 (VM, local) · **Game build:** 1273043
(`1273043|^/game38/branches/release_1.0|2026-10-20T14:55:29.904125Z`, as parsed)
**Scope:** directory data only. The `.dat` payloads are not on this VM and were not read;
everything below is computed from the pristine container's directory.

## Verdict

The contents addressing is **fully mapped, and it tiles perfectly**:

- Payloads are packed **back-to-back inside each `.dat` container, each padded up to a
  4,096-byte boundary**. Of 101,404 consecutive offset-sorted pairs, **101,402** satisfy
  `next_offset == align4096(offset + size)` exactly. The other two are a single shared
  anomaly (§4) involving the map's only zero-length payload.
- **`ContainerInfo.size` is solved**: it is the **byte length of that container's `.dat`
  file** — the sum of its entries' 4,096-padded payload slots. The identity holds for
  **55 of 55** nonempty containers, with the total matching to the byte
  (Σ size fields = Σ padded slots = 51,500,314,624). The earlier reading that "size is
  not a payload byte count" (A0/A2.5) was imprecise, not wrong about the sum: `size`
  counts *padded file bytes*, not raw payload bytes (raw payloads sum to
  51,283,306,666 — the difference is 217,007,958 B of alignment padding).
- The hash function behind the keys is **identified**, not just structured: the census's
  single size-0 entry is exactly `compute_content_guid` of the empty byte string as
  implemented in kfc-base (§7).
- `flags` carries no information in this build: **all 101,459 entries have flags = 0**.

## Inputs, tools, method

| Input | What it is |
|---|---|
| `~/workspace/repro-gamedir/enshrouded.kfc` | The pristine container. SHA-256 `7f6130bb3d800fd5b0f91b2644792827045acc43ccbf666255dcfa8e544f79e9` — re-verified this run; identical to the hash recorded in A2.5's `_directory.json`. The census describes *this* file. |
| `dat_files.json`, `contents_map.bin`, `containers.bin`, `_directory.json` | A2.5's verbatim directory capture (`kfc4-a25/extracted-full/directory/`), used only for cross-checks. |
| kfc-base (in-place tree) | `~/workspace/your_files/kfc4-inplace/src/crates/kfc-base` — parse source of truth; its structs and writer define the field semantics tested here. |

- **Harness** (`harness/` in this folder, path-depends on the in-place tree's kfc-base):
  `dump` parses the container with `KFCFile::from_path` and writes `contents.csv`
  (101,459 rows, the map's keys in file order) and `containers.csv` (64 rows).
  `rawcheck` re-reads the key/value arrays and the container records as raw
  little-endian bytes at the spec §2.1 offsets (keys @ 1,418,760; values @ 3,042,104;
  containers @ 216) and compares **element-by-element** against the parse — so the
  `keys[i] ↔ values[i]` pairing was verified, not assumed from the API.
  **Result: 0 key mismatches, 0 value mismatches, 0 container mismatches.**
  `emptyhash` computes the empty payload's content hash through kfc-base (§7).
- **Analysis** (`work/analyze.py`, `work/analyze2.py`): pure computation over the CSVs;
  every number below traces to `work/analysis.json` / `work/analysis2.json`, which are
  kept with the dump as evidence. No number in this report is asserted without being
  computed from `contents.csv`.
- Struct citations are to the in-place tree; line numbers as of this run.

## 1. The structures (what the directory actually carries)

- **`ContentHash` — 16 B, the map key** (`guid/content.rs:12`):
  `{ u32 size; u32 hash0; u32 hash1; u32 hash2; }`.
  **The payload length is stored in the key**: `size` is the payload's byte length.
  `ContentHash::from_data` sets it from `data.len()` (`guid/content.rs:53`), and the
  reader seeks into a `.dat` container and reads exactly `key.size()` bytes
  (`container/reader.rs`, `open_content`/`read_content_into`). Spec §4.5 said exactly
  this; the census below confirms it behaviourally — the `size` fields tile the
  containers (§3), which they could not do if they were anything else.
- **`ContentEntry` — 16 B, the map value** (`container/header.rs:304`):
  `{ u32 offset; u16 flags; u16 container_index; u8 padding[8] }`. The padding is
  validated zero on read; `rawcheck` confirmed it is zero for all 101,459 entries.
  `offset` is the byte offset of the payload **inside its `.dat` file** (not in the
  `.kfc`, not in the resources stream).
- **`ContainerInfo` — 16 B × 64** (`container/header.rs:225`): `{ u64 size; u64 count; }`.
  §8 below.
- The hash triple is the low 12 bytes of an AES-based content GUID:
  `ContentHash::from_data` computes `compute_content_guid(data, 0)`
  (`hash/content.rs:40` — four 16-byte states initialised to the byte ramps
  0x00–0x0F / 0x10–0x1F / 0x20–0x2F / 0x30–0x3F, folded over the data in 64-byte
  blocks with AES inverse cipher rounds, finalised with a seed block built from
  `seed − len` and `seed + len + 1`) and takes GUID bytes 4..16 as
  `hash0 ‖ hash1 ‖ hash2`. The map's bucket index is `hash0` itself
  (`StaticHash for ContentHash`, `guid/content.rs:146`).

## 2. Partition check — the containers partition the contents, exactly

All 64 containers compared individually: directory `count` vs entries actually
measured in the contents map. **64/64 match.** Σ count fields = Σ measured = **101,459**.
The empty containers are exactly **indices 55–63** (9 containers) — empty by `count`,
by measured entries, and by `size` alike, and the only `.dat` files present in Eric's
local staging are precisely those nine, as zero-byte stubs (`enshrouded_055.dat` …
`enshrouded_063.dat` in `~/workspace/repro-gamedir/`). Entries per nonempty container
range 168–19,369 (median 409); container 53 alone holds 19,369 entries, container 45
holds 8,937.

Cross-check vs A2.5: `dat_files.json`'s `directory_size` / `directory_count` agree with
this parse for **64/64** containers, and the pristine file's SHA-256 still matches
A2.5's record — so A2.5's census and this one describe the same bytes.

`max_end` below is the largest `offset + size` among the container's entries (the end
of its last byte of real payload).

| index | size (directory) | count | measured | match | max_end |
|---|---|---|---|---|---|
| 0 | 1,060,708,352 | 414 | 414 | ✓ | 1,060,707,000 |
| 1 | 1,011,748,864 | 409 | 409 | ✓ | 1,011,747,512 |
| 2 | 1,009,995,776 | 407 | 407 | ✓ | 1,009,994,424 |
| 3 | 1,004,273,664 | 398 | 398 | ✓ | 1,004,272,312 |
| 4 | 1,004,101,632 | 383 | 383 | ✓ | 1,004,100,280 |
| 5 | 1,011,863,552 | 377 | 377 | ✓ | 1,011,862,200 |
| 6 | 1,010,012,160 | 286 | 286 | ✓ | 1,010,010,808 |
| 7 | 1,005,117,440 | 367 | 367 | ✓ | 1,005,116,088 |
| 8 | 1,037,316,096 | 412 | 412 | ✓ | 1,037,313,384 |
| 9 | 1,000,910,848 | 365 | 365 | ✓ | 1,000,909,496 |
| 10 | 1,000,935,424 | 336 | 336 | ✓ | 1,000,934,072 |
| 11 | 1,012,752,384 | 367 | 367 | ✓ | 1,012,751,032 |
| 12 | 1,054,711,808 | 392 | 392 | ✓ | 1,054,710,456 |
| 13 | 1,000,976,384 | 350 | 350 | ✓ | 1,000,975,032 |
| 14 | 1,009,950,720 | 376 | 376 | ✓ | 1,009,949,368 |
| 15 | 1,010,335,744 | 375 | 375 | ✓ | 1,010,334,392 |
| 16 | 1,004,216,320 | 431 | 431 | ✓ | 1,004,214,968 |
| 17 | 1,033,170,944 | 298 | 298 | ✓ | 1,033,169,592 |
| 18 | 1,013,051,392 | 408 | 408 | ✓ | 1,013,050,040 |
| 19 | 1,008,046,080 | 458 | 458 | ✓ | 1,008,044,728 |
| 20 | 1,009,111,040 | 459 | 459 | ✓ | 1,009,109,688 |
| 21 | 1,006,993,408 | 390 | 390 | ✓ | 1,006,992,056 |
| 22 | 1,013,837,824 | 399 | 399 | ✓ | 1,013,836,472 |
| 23 | 1,007,153,152 | 345 | 345 | ✓ | 1,007,151,800 |
| 24 | 1,014,304,768 | 451 | 451 | ✓ | 1,014,302,064 |
| 25 | 1,014,571,008 | 377 | 377 | ✓ | 1,014,569,656 |
| 26 | 886,738,944 | 374 | 374 | ✓ | 886,737,592 |
| 27 | 1,005,580,288 | 2249 | 2249 | ✓ | 1,005,577,394 |
| 28 | 1,004,965,888 | 2243 | 2243 | ✓ | 1,004,962,044 |
| 29 | 1,005,019,136 | 2231 | 2231 | ✓ | 1,005,017,926 |
| 30 | 1,006,403,584 | 2194 | 2194 | ✓ | 1,006,401,932 |
| 31 | 1,011,777,536 | 5670 | 5670 | ✓ | 1,011,775,448 |
| 32 | 614,178,816 | 5330 | 5330 | ✓ | 614,175,719 |
| 33 | 1,005,031,424 | 216 | 216 | ✓ | 1,005,030,072 |
| 34 | 1,013,829,632 | 271 | 271 | ✓ | 1,013,828,280 |
| 35 | 1,010,716,672 | 196 | 196 | ✓ | 1,010,715,320 |
| 36 | 1,003,167,744 | 204 | 204 | ✓ | 1,003,166,392 |
| 37 | 768,745,472 | 168 | 168 | ✓ | 768,744,120 |
| 38 | 1,007,460,352 | 3591 | 3591 | ✓ | 1,007,456,760 |
| 39 | 1,007,534,080 | 3574 | 3574 | ✓ | 1,007,531,448 |
| 40 | 1,007,661,056 | 3557 | 3557 | ✓ | 1,007,660,072 |
| 41 | 580,624,384 | 2194 | 2194 | ✓ | 580,620,904 |
| 42 | 1,096,032,256 | 420 | 420 | ✓ | 1,096,030,904 |
| 43 | 1,036,374,016 | 380 | 380 | ✓ | 1,036,371,312 |
| 44 | 678,014,976 | 218 | 218 | ✓ | 678,013,624 |
| 45 | 1,022,078,976 | 8937 | 8937 | ✓ | 1,022,074,884 |
| 46 | 1,022,111,744 | 8919 | 8919 | ✓ | 1,022,109,160 |
| 47 | 127,422,464 | 1103 | 1103 | ✓ | 127,418,896 |
| 48 | 1,004,896,256 | 1214 | 1214 | ✓ | 1,004,892,176 |
| 49 | 909,414,400 | 623 | 623 | ✓ | 909,410,320 |
| 50 | 1,008,574,464 | 4063 | 4063 | ✓ | 1,008,574,464 |
| 51 | 313,712,640 | 1429 | 1429 | ✓ | 313,709,936 |
| 52 | 904,417,280 | 9283 | 9283 | ✓ | 904,413,292 |
| 53 | 1,038,438,400 | 19369 | 19369 | ✓ | 1,038,438,400 |
| 54 | 29,224,960 | 1209 | 1209 | ✓ | 29,220,976 |
| 55 | 0 | 0 | 0 | ✓ | — |
| 56 | 0 | 0 | 0 | ✓ | — |
| 57 | 0 | 0 | 0 | ✓ | — |
| 58 | 0 | 0 | 0 | ✓ | — |
| 59 | 0 | 0 | 0 | ✓ | — |
| 60 | 0 | 0 | 0 | ✓ | — |
| 61 | 0 | 0 | 0 | ✓ | — |
| 62 | 0 | 0 | 0 | ✓ | — |
| 63 | 0 | 0 | 0 | ✓ | — |

## 3. Offset tiling — payloads lie back-to-back, 4,096-padded

Method: per container, sort entries by `offset`; for each consecutive pair compare the
stride (`next.offset − offset`) with the slot the current entry must occupy. Because
the key carries an explicit `size` (§1), this is an exact test, not an inference.

- **101,402 of 101,404 consecutive pairs** satisfy
  `next_offset == offset + align4096(size)` **exactly** — each payload starts
  immediately after the previous payload's padded slot. Overlaps: **zero** real
  overlaps anywhere (the one apparent case is §4).
- Global check: Σ over all entries of `align4096(size)` = **51,500,314,624** =
  Σ `ContainerInfo.size` (§8). Raw payload bytes Σ `size` = **51,283,306,666**;
  alignment padding therefore totals **217,007,958 B** across 101,459 entries
  (mean 2,139 B/entry — the expectation for uniform remainders mod 4096 is 2,048).
- So every payload's length is **known exactly** (it is the key's `size` field) —
  the census does not need the successor-inference fallback for anything. What the
  tiling adds is proof that the recorded sizes are the real ones and that there are
  **no unrecorded gaps or hidden payloads** inside the containers: the address space
  of every nonempty container is accounted for from offset 0 to its `size`, slot by
  slot, with nothing left over.

## 4. The one anomaly: the shared slot at container 53, offset 1,038,139,392

The only two non-exact strides in the whole map share one address in container 53:

- Entry `00200000b257958b00c763e5c9a09dca` — **size 8,192**, offset 1,038,139,392.
- Entry `0000000078873de911d1716d965c53fd` — **size 0**, offset 1,038,139,392
  (the *same* offset), followed by the next entry at 1,038,147,584.

Read literally: the zero-length payload's address is the first byte of the 8,192-byte
payload's slot. A zero-length read consumes no bytes, so no byte range is actually
shared — but it breaks the "every entry owns its slot" pattern, and it is the only
place in 101,459 entries that does. The map's tail is otherwise ordinary: container
53's last entry by offset is a 4,096-byte payload at 1,038,434,304, ending exactly at
the container's `size` 1,038,438,400. Why the empty content was recorded at a
neighbour's offset rather than at the end position is not determinable from the
directory; it is flagged here so the `.dat` capture (§9) can look at that address
first.

## 5. Alignment

- **Every one of the 101,459 offsets is a multiple of 4,096** (offset mod 4096 ≡ 0
  for all; offset mod 16 and mod 256 are likewise uniformly 0; gcd of all offsets =
  4,096; gcd of all in-container strides = 4,096). There is no finer or coarser
  pattern to find — 4,096 is *the* content alignment, matching the writer constant
  `CONTENT_ALIGNMENT = 4096` (`container/writer.rs:8`).
- Sizes themselves are unaligned, as expected for real payload lengths: only
  3,480 entries (3.4%) have `size` ≡ 0 (mod 4096); the most common remainders are
  1392 (17,328 entries) and 2744 (10,091) — artefacts of the repeated size families
  in §7, not of the packing.

## 6. Flags census

**One distinct value: `flags = 0`, 101,459 of 101,459 entries.** There is nothing to
correlate — no container, size band, or offset carries a nonzero flag in this build.
Whatever `flags` is for (the writer also only ever writes 0:
`ContentEntry::new(offset, 0, index)` in `write_content`, `container/writer.rs`), the
shipped directory does not use it. Any future build showing nonzero flags would be
new information; none exists here.

## 7. Hash structure, and the payload-size landscape

- The full 16-byte key is **unique for all 101,459 entries** (0 duplicates — the
  parse is a true map), and the 12-byte hash triple is likewise unique for all
  101,459. No two entries share a triple, even inside same-size families.
- Byte-column census of the 16 key bytes: hash bytes 4–15 take all 256 values each
  (uniform, as a hash should). The `size` field's byte 3 takes only 3 distinct
  values (sizes < 2²⁶ ≈ 67 MB) — that is the size showing through, not structure.
- **The hash function is confirmed against a real directory entry.** The census
  contains exactly one size-0 entry: `0000000078873de911d1716d965c53fd`
  (container 53, §4). Computing kfc-base's `ContentHash::from_data(&[])` yields
  size 0, hash0 `e93d8778`, hash1 `6d71d111`, hash2 `fd535c96` — byte-for-byte that
  entry (`harness emptyhash`, `work/emptyhash.log`). So the AES construction in
  `hash/content.rs` is the game's content hash for at least the empty payload, and
  the identification procedure for all other payloads is now mechanical (§9).
- Sizes cluster into content families: 37,721 distinct sizes over 101,459 entries;
  the ten most common sizes alone cover 28.6% of all entries — 1,392 B (6,527),
  2,744 B (4,990), 87,408 B (4,254), 5,592,432 B (2,654), 1,398,128 B (2,400),
  2,796,216 B (2,320), 699,064 B (1,906), 98,832 B (1,564), 24,576 B (1,314),
  349,552 B (1,133). 8,031 sizes are shared by more than one distinct hash triple.
- Extremes: smallest nonzero payload **9 B** (`0900000017d20c46bdd7ea0fd2a6fb74`,
  container 27); the largest is **44,739,280 B**, occurring three times with three
  different hashes (`d0aaaa02…` prefix family; containers 14, 8, 8). 253 entries are
  ≥ 16 MiB, spread across 40 containers. Median payload 98,956 B; mean 505,458 B;
  p95 ≈ 2.80 MB; p99 ≈ 5.59 MB.
- The `keys[]` array order is the static map's bucket order, not any content order:
  adjacent in-file entries of one container have ascending offsets in 50,589 of
  101,404 pairs (≈ 50% — uncorrelated), and container indices change 93,805 times
  along the array. Nothing about placement can be read from file order; placement
  lives entirely in the entries themselves, which §§2–3 show is complete.

## 8. `ContainerInfo.size` — semantics resolved

The field was the one open directory question (A2.5: "fields sum to 51.5 GB, so
`size` is not a payload byte count; semantics unknown"). Tested against everything
the census measured, per nonempty container:

| Candidate meaning of `size` | Test | Result |
|---|---|---|
| Σ of entries' padded slots, `Σ align4096(size)` | equality, per container | **exact for 55/55** (correlation 1.0) |
| End of address span, padded: `align4096(max(offset+size))` | equality, per container | **exact for 55/55** |
| Σ of raw payload sizes | equality | fails: short by the padding (Σ Δ = 217,007,958 B); correlation 0.99949 |
| Entry `count` (or a function of it) | correlation | 0.022 — unrelated |
| A cap or quota | range | no: 54 of 55 values sit *below* 2³⁰, the 55th above |

So **`size` is the container file's byte length** (payloads + their 4,096 alignment
padding, trailing pad included) — i.e. the expected on-disk length of
`enshrouded_{index:03}.dat`. The code agrees on both sides:

- Writer: after writing a payload the writer aligns to 4,096
  (`write_content`, `container/writer.rs:760`), and at finalize each container's
  `ContainerInfo.size` is recorded as the container writer's **final stream
  position** (`container/writer.rs:797`) — the file length by construction.
- The ~1 GiB scale that made the sum look absurd is a *rollover threshold*, not
  something exotic: kfc-base's writer starts a new container once the current
  one's position has reached `KFCWriteOptions::max_container_size` (default
  2³⁰ = 1 GiB; `container/writer.rs:289`, `get_container_writer` at
  `writer.rs:881`). The shipped directory clusters the same way, though around a
  slightly lower line: **all 45 containers ≥ 0.9 GiB are ≥ 1,000,000,000 B
  (10⁹)**, 44 of them land in [10⁹, 1,060,708,352] — at most one maximum-size
  payload (44,739,280 B) past 10⁹ — and the largest, container 42
  (1,096,032,256 B), sits just past even 2³⁰. The remaining ten are spread from
  886,738,944 B down to container 54's 29,224,960 B (smallest: containers 47 at
  127,422,464 B, 51 at 313,712,640 B, 54 at 29,224,960 B), so no single
  fill-to-one-threshold pass explains the whole set — the directory reads as
  accumulated over several writing episodes. Whether the game's threshold is
  exactly 10⁹ cannot be proven from a static directory; the clustering at 10⁹ is
  what the data shows. (The game's own writer is not kfc-base; the packing rule
  matches, the constant is observed, not proven.) All 64 `size` values are
  multiples of 4,096, as file lengths under this packing must be.

Consequence: **the 51,500,314,624-byte total is a prediction, checkable the moment
the game directory is available** — the 55 real `.dat` files' lengths must match
`containers.csv` exactly, individually and in sum. (The A0-era "`.dat` payloads
total 74,410,422 B" figure does not follow from any of this and stays retracted.)

## 9. What this map buys when the `.dat` files are captured — and what stays open

With the addressing proven, a `.dat` capture becomes mechanical:

1. **Verify before trusting**: each captured `enshrouded_{NNN}.dat` must have length
   exactly `containers.csv`'s `size` for its index (55 files, Σ 51,500,314,624 B);
   the nine stubs (55–63) must stay 0. Any mismatch names the file to re-pull.
2. **Slice every payload by address**: payload *i* is the `size` bytes at `offset`
   in file `container_index` — `contents.csv` *is* the slicing table; no parsing of
   the payloads themselves is needed to extract all 101,459.
3. **Verify payload ↔ hash at scale**: recompute `compute_content_guid(payload, 0)`
   per payload and compare the 12-byte triple. The empty-payload match (§7) anchors
   the algorithm; a full pass would prove it for every content family — and any
   mismatch would isolate exactly which family hashes differently.
4. **Look at container 53, offset 1,038,139,392 first** (§4): the directory's only
   irregularity lives there.

Still unknown after this census:

- Whether the shipped `.dat` files actually match the lengths the directory
  predicts (needs the capture; it is a prediction, not an observation).
- Hash behaviour for nonzero seeds or payload families not represented by the empty
  payload — expected to be uniform, provable only against captured payloads.
- `flags` semantics — uniformly 0 here and in the writer; nothing to learn from
  this build.
- Why the empty content shares the 8,192-byte entry's offset (§4).
- Which container the game appends new content to, and whether it ever compacts —
  write-side behaviour a static directory cannot show.

## Deliverables and evidence

| File | Content |
|---|---|
| `contents.csv` | All 101,459 entries: `content_hash` (full 32-hex, on-disk byte order), `size`, `hash0`, `hash1`, `hash2`, `offset`, `flags`, `container_index` — keys in file order. |
| `containers.csv` | All 64 `ContainerInfo` records (`index`, `size`, `count`) beside the measured per-container entry count. |
| `harness/` | The Rust census harness (`dump` / `rawcheck` / `emptyhash`) — build with `cargo build --release` (Rust 1.88; kfc-base path dependency). |
| `work/` | `dump.log`, `rawcheck.log`, `emptyhash.log`, `analyze.py`, `analyze2.py`, `analysis.json`, `analysis2.json`, `partition_table.md`. |

Failure notes: the harness's first build failed on a borrow error
(`ContentEntry` is not `Copy`; fixed by borrowing the value) — nothing else failed.
No GitHub pushes were made; the pristine container, the A2.5 tree, and the
kfc-base trees were only read.
