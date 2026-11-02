# A4 (VM half) — Clone files regenerated with the proven writer, three-way forensics

Date: Nov 1, 2026 · Game build 1273043 · Writer: fork EML built fresh from
`~/workspace/your_files/kfc4-inplace/src-fork/` (all five patches:
0001 map order, 0002 bundle preservation, 0003 no empty chunk,
0004 in-place mode, 0005 24-bit stored sizes) · `emm` =
mod-manager-cli release build from that tree.
Artifacts: `harness/` (audit harness + `dumpdir`/`readres` additions),
`work/` (dumps, fingerprints, cmpdir/bytecmp logs), `test-files/`
(staged pairs + SHA-256 manifest + TEST-PROTOCOL.md).

## Verdict

**Every dialect feature of the old writer is gone from the regenerated
files, and the files are fixed points of the new writer.** A
zero-change in-place rewrite of both new files reproduces them
byte-for-byte (0 differing bytes in `.kfc` and stream). The directory
deltas against the baseline are *exactly* the added clones and their
mechanical consequences — nothing else moved: all 91,901 pre-existing
resources keep their key order, their directory entries, and their
content fingerprints.

The crash-era files, re-examined at full ResourceId level, hold a
surprise that reframes the home test (see §2): their 99 "clones" are
99 copies of a **single** template on a counter GUID stem — not
33 bases × 3 presets. The staged test files therefore test clone
*count* with a representative 33-template population, which is the
variable the Oct 31 bisect actually isolated.

## 1. What was run

Two `emm run --game-directory <copy> --patch --export` passes over
hash-verified copies of the baseline pair
(`enshrouded.kfc` SHA-256 `7f6130bb…`, stream `1cd249c3…`):

| Run | Mod | Loader result | Output resources | Output stream |
|---|---|---|---|---|
| run99 | stock `m182_spawn_variants` | **Applied 18 patches** | 91,919 | 216,719,360 B (+12,288) |
| run82 | VCOUNT build, `MAX_CLONES = 82` (same cap mechanism as the capped-81 production build) | **Applied 1 patch** | 91,902 | 216,711,168 B (+4,096) |

The mod's own trace line ("variants created=99/82") counts
*p*call successes including GUIDs it skipped as already existing;
the loader's applied-patch count is the ground truth, and it matches
the directory diff exactly (18 / 1 new ResourceIds).

**Correction to the brief's expected totals (92,000 / 91,983):** the
baseline pair is not clone-free. It was pulled from Eric's real game
folder on Nov 1, after live sessions with the capped production
build, and its directory provably contains that build's output:
**81 forge clones over 27 base triples, preset GUID scheme**
(seg3 `4182`, seg4 `0001..0003`), every one with an identifiable base
template in the same file. The stock run therefore only had bases
28–33 left to create (18 clones), and the 82-cap run exactly one
(base 28, preset x3). Final clone totals in the staged files are
**82 and 99** — the bisect's boundary counts — reached as
81 carried + 1 / + 18.

## 2. Crash-era file anatomy (directory-level; their streams were never uploaded)

Both crash files (99: 91,916 resources; 81: 91,898) descend from a
base of 91,817 resources — Eric's Oct 31 stack state, 84 keys short
of the Nov 1 baseline (a second bundle count differs accordingly:
type `88d6b534`, 2,322 vs 2,325 — base lineage, not clones).

- **Clone population:** all 99 clones are type `39768775`
  (TemplateResource), part 0, GUIDs
  `642098d4-fc09-4182-<0001..0063>-782d438bf551` — one stem, a global
  counter in segment 4. Every clone's stored size is **31,720**; the
  stem's base template (`…-48e2-811e-…`, same triple) is 31,712 B
  (+8 = the name suffix). The 81 file holds the first 81 of the same
  series (subset relation exact; difference = 18). Whatever build
  produced these files in-game forged every clone from a single
  template — consistent with the Oct 31 V-MANY-style test
  configuration, not with the 33-base stock loop in the repo source.
  Content-level claims stop here: with no stream on the VM, clone
  *contents* in the crash files cannot be fingerprinted.
- **Key order:** restricted to the 91,817 ResourceIds shared with the
  baseline, **25,550 positions (27.8%) differ** from baseline order,
  first divergence at index 22. The two crash files differ from *each
  other* in 25,835 positions on their 91,895 common keys — the old
  writer did not reproduce its own ordering between sessions.
- **Bundle indices:** 146/146 differ from the baseline's, and 146/146
  differ between the two crash files (recomputed arbitrarily per
  run). Internal hashes: 0 diffs everywhere. One count diff
  (TemplateResource 12,349 → 12,367 in the 99 file).
- **Entries:** on common keys, 4,429 offset diffs and 1,270 size
  diffs vs the baseline (relocation churn + re-derived sizes across
  those sessions' write histories).
- **Contents map:** key sequence shuffled vs baseline; values
  identical (0 diffs).
- **Chunk geometry:** 60 chunks, stream 215,687,168 B /
  495,871,952 B uncompressed — a compacted, history-dependent layout
  unrelated to the game's 66-chunk geometry (A0's dialect marker).

## 3. Three-way comparison

### (a) NEW-99 vs CRASH-99 — dialect features, quantified

| Old dialect feature | In CRASH-99 | In NEW-99 |
|---|---|---|
| Static-map key order destroyed (HashMap rebuild) | 25,550 / 91,817 common-key positions differ from baseline order (27.8%) | **0 / 91,901 positions differ** — baseline order preserved exactly (patch 0001) |
| Bundle indices recomputed arbitrarily | 146/146 differ from baseline; also 146/146 vs the sibling crash file | Indices are *segment arithmetic*: every shift is exactly 0 or +18 (the TemplateResource segment growth), verified exhaustively per bundle; internal hashes 0 diffs (patch 0002) |
| Spurious empty chunk appended | n/a (different lineage) — but see fossil note | **None.** Chunks 66 → 67, and the appended chunk carries real data (581,024 B uncompressed). The only zero-length chunk is the fossil empty chunk the *baseline itself* carries from a pre-A1 session |
| Stored sizes re-derived in full | 1,270 size diffs on common entries vs baseline | **0 entry diffs** (offset+size) on all 91,901 common entries; the 24-bit resource stores 11,105,420 in both files; reader recovery still fires exactly once per file (patch 0005) |
| Relocation of untouched resources | 4,429 offset diffs on common entries | **None** — and the stream's first 216,707,072 bytes are hash-identical to the baseline stream (pure append) |
| Contents map shuffled | sequence differs, values equal | sequence **and** values identical (101,459 entries) |

Raw positional byte diff, CRASH-99 `.kfc` vs NEW-99 `.kfc`:
6,173,574 of 9,396,224 bytes — two files that both claim "99 clones"
share almost nothing at the byte level (different base lineage +
dialect vs canonical). Positional diffs *within* the new lineage
(pristine→NEW-99: 5,315,266 bytes) are dominated by section shifts
after insertions and are **not** semantic diffs; the semantic deltas
are the enumerated ones above and in §3(b).

One residual byte-level non-preservation, stated honestly: in the
*changed-sequence* path the bundle **map's key order** is rebuilt
deterministically but not in loaded order — NEW files differ from
the baseline in 32/146 bundle-key positions (NEW-82 and NEW-99 agree
with each other exactly). All bundle *values* are correct; map order
is not load-bearing for lookups. This is the one dialect surface the
five patches do not close, and it only appears when resources are
added or removed.

### (b) NEW-99 vs baseline (pristine pair)

Complete delta, nothing else:

- **+18 ResourceIds**, all TemplateResource clones (6 base triples ×
  presets x3/x10/x40 — the bases the capped build excludes:
  Crocodile + Ancient Constructs; names in §4). 0 removed.
- **+1 chunk record** (the appended chunk above); the 66 inherited
  chunk records are field-identical.
- **Bundle count +18** for TemplateResource (12,349 → 12,367);
  bundle indices shifted per §3(a); resource-location aggregates
  updated (+581,024 uncompressed / +9,588 compressed).
- **Contents, containers, version: identical.**
- **Content fingerprints:** the new file's fingerprint set is the
  A0 baseline set **plus exactly the 18 clone lines** — 0 baseline
  lines missing or altered (multiset compare).

NEW-82 vs baseline is the same shape with +1 clone / +32,512 B /
+3,493 B and bundle count 12,350.

### (c) NEW-82 vs NEW-99

- Key sets differ by **exactly 17** ResourceIds (the remaining
  clones); NEW-82 ⊂ NEW-99.
- Directory: 1 entry offset differs (the shared clone sits at a
  different position inside the appended chunk), 1 chunk record
  differs (the appended chunk), bundle count differs for
  TemplateResource only (12,350 vs 12,367) with the same
  segment-arithmetic index shifts ({0,+1} vs {0,+18}).
- The shared clone's **content is byte-identical** in both files.

## 4. Clone content spot-check (NEW files)

Four created clones read back and compared against their base
templates byte-for-byte (offsets are within-resource):

| Clone | Preset | Name in clone data | EnemyScaling healthScaleFactor | DamageMod dataStorage[2] |
|---|---|---|---|---|
| `a75e62f9-…-0001` (AncientConstruct_Modular10_Slicer) | x3 | `…Slicer@x3` ✓ | 1.3 → 3.9 (×3) @21476 | 1.0 → 1.5 (×1.5) @18604 |
| `168f1d49-…-0002` (AncientConstruct_Modular04_Bull) | x10 | `…Bull@x10` ✓ | 1.0 → 10.0 (×10) @20436 | 32.0 → 80.0 (×2.5) @21560 |
| `246c430f-…-0003` (AncientConstruct_Modular02_FireRoller) | x40 | `…FireRoller@x40` ✓ | 1.2 → 48.0 (×40) @21564 | 1.0 → 5.0 (×5.0) @18500 |
| `77866ca5-…-0001` (Wildbeast_Crocodile_Amphibious) — the single clone in NEW-82 | x3 | `…Amphibious@x3` ✓ | 1.5 → 4.5 (×3) @21292 | 0.7 → 1.05 (×1.5) @18420 |

Both documented edits are present with the exact preset multipliers
in every sampled clone. (Clone bytes are an EML reflection
re-serialization of the template, so the diff is not a minimal
two-float diff — the name field and both edited floats are exactly
as specified, and the resources parse and fingerprint stably.)

## 5. Fixed-point check

Each new pair was copied and rewritten with **zero changes** through
the in-place writer (patch 0004 mode), then byte-compared:

- NEW-82: `.kfc` 0 differing bytes; stream 0 differing bytes.
- NEW-99: `.kfc` 0 differing bytes; stream 0 differing bytes.

The regenerated files are already in the writer's canonical form —
a second write is a true no-op, the property the old writer lacked
(A0: its zero-change write moved ~6.5M bytes and appended a chunk).

## 6. Staged home test files

`test-files/` (hashes in `SHA256SUMS.txt`, procedure in
`TEST-PROTOCOL.md`):

| Pair | `.kfc` SHA-256 | Stream SHA-256 |
|---|---|---|
| new82 | `4984253bda77d9ad…` | `bfd26e2d34319c1b…` |
| new99 | `79fa2509fc182174…` | `7bad29f2545b9058…` |

**What the home load test will prove:** whether a byte-canonical
file carrying 82 (then 99) clones loads in the real stack. A pass
clears the file/writer at those counts outright. A crash, combined
with the protocol's native-off discriminator run
(`m182_native.json` all false, same staged file), separates the
remaining suspects: loads-with-native-off ⇒ native layer; still
crashes ⇒ a genuine consumer-side count cliff, file-independent, and
the 81 cap is simply final.

**What it won't prove:** it does not by itself separate writer from
native layer if it crashes (that's the discriminator's job); it does
not validate the merged-DLL write path in game (PRs #5 + #6 merged +
rebuilt is the equivalent, later confirmation); and it tests clone
*count* with the current mod's population — not the crash-era
single-template population, which cannot be reproduced from the
repo source and is, per §2, what actually crashed in October.

## Method notes / caveats

- Crash-side content claims are directory-only by necessity: the
  crash-era `_resources` streams were never uploaded, so clone
  contents there are characterized by stored sizes alone.
- `dumpdir` in the local harness initially keyed dumps by GUID only;
  caught and fixed mid-run (ResourceId = GUID + type + part; the
  container holds 74,542 distinct GUIDs for 91,901 resources). All
  numbers in this report are from the corrected dumps.
- The `emm` CLI required an `export/` folder in the game directory
  (known fork CLI AppState quirk); created empty in both workdirs.
- Intermediates deleted: the two run workdirs and both fixed-point
  copies (~930 MB). Kept: all dumps, fingerprints, and logs in
  `work/`; the staged pairs in `test-files/`.
