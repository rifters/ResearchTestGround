# Pristine regen — Clone test pairs regenerated from the TRUE pristine files

Date: Nov 1, 2026 · Game build 1273043 · Writer: fork EML built from
`~/workspace/your_files/kfc4-inplace/src-fork/` (all five patches:
0001 map order, 0002 bundle preservation, 0003 no empty chunk,
0004 in-place mode, 0005 24-bit stored sizes); `emm` = mod-manager-cli
release build from that tree, cargo-verified fresh against the sources.
Verification harness: the A4 audit harness
(`~/workspace/your_files/kfc4-a4/harness/`, subcommands audit / dumpdir /
cmpdir / bytecmp / inplace-roundtrip / readres), rebuilt fresh.
Artifacts: `work/` (logs, dumps, fingerprints), `test-files/`
(staged pairs + SHA256SUMS.txt).

Base files (Steam-fresh, never modded; hashes verified before and
after all work):

| File | Bytes | SHA-256 |
|---|---|---|
| `enshrouded.kfc` | 9,392,128 | `54a8cb76b3229c486e…` |
| `enshrouded.kfc_resources` | 214,536,192 | `53786c45797d275661…` |

Pristine profile (audit, PROVEN): 91,812 resources, 101,459 contents,
146 bundles, **58 chunks**, **55 containers (all nonzero)**, exactly
one 24-bit size correction (`509feadb-4c60-425f-9c7c-deeefd9b6920`,
stored 11,105,420 → actual 44,659,852). A fresh audit reproduces the
delivered `fp-true-pristine.txt` fingerprint file exactly.

## Verdict

The two decisive test pairs are regenerated from a provably clean
stock base: **new82 = pristine + exactly 82 clones (91,894
resources)**, **new99 = pristine + exactly 99 clones (91,911
resources)** — the full 33-base production population created in a
single run each, with no pre-carried clones and no other mod's
patches anywhere in the ancestry. Every A4-grade content gate
passes: the fingerprint delta against pristine is exactly the clone
lines and nothing else, pre-existing resources keep key order,
directory entries, and contents; the stream is the pristine stream
plus one appended chunk; both pairs are fixed points of the writer.

One gate result differs from the A4-era expectation and is reported
plainly in §2: a zero-change in-place rewrite of the **game's own**
pristine `.kfc` is **not** byte-identical (the stream is). The
writer normalizes the shipped directory into its canonical form —
semantically lossless, proven below — which is also the form both
test pairs are delivered in.

## 1. What was run

Two `emm run --game-directory <copy> --patch --export
--export-directory <copy>/export` passes over hash-verified copies
of the pristine pair, replicating the A4 invocation exactly (same
`.cache/types.json` registry from `~/workspace/repro-gamedir/`, same
`export/` folder requirement, mod under `mods/m182_spawn_variants/`):

| Run | Mod | Loader result | Output resources | Output stream |
|---|---|---|---|---|
| run99 | stock `m182_spawn_variants` (repo source, unmodified) | **Applied 99 patches** | 91,911 | 214,581,248 B (+45,056) |
| run82 | VCOUNT build, `MAX_CLONES = 82` (same cap mechanism as the capped-81 production build; the file differs from stock only by the cap block) | **Applied 82 patches** | 91,894 | 214,573,056 B (+36,864) |

The loader's applied-patch count is the ground truth and matches
the directory diff exactly (99 / 82 new ResourceIds). The mod's own
trace agrees ("variants created=99/82, failed=0"). Clean-base
signatures vs A4's runs: the template scan finds **592** enemy
templates (A4's modded baseline: 673) and 11,675 objects (A4:
11,676) — the stock game only; the 33 clone bases are an explicit
list in the mod and are unaffected.

## 2. Gate G0 — zero-change in-place rewrite of the pristine pair

Result: **stream byte-identical (0 differing bytes); `.kfc` NOT
byte-identical** — 5,892,161 of 9,392,128 bytes differ positionally,
lengths identical, first difference at offset 4 (inside the
header's own location fields). This is a property of the *source*,
not a writer malfunction, proven three ways:

- **Semantic identity is total** (cmpdir, pristine vs G0 output):
  version equal; resource key sequence, key set, and every
  (offset, size) entry equal — 0 value diffs; contents key sequence
  and values equal (101,459); bundle keys, internal hashes,
  indices, and counts all equal; all 58 chunk records
  field-identical.
- **Content identity is total**: the full-read fingerprint sets of
  pristine and G0 output are identical (91,812 / 91,812 lines,
  multiset compare).
- **The one structural delta**: the shipped file declares **55**
  containers (exactly the nonzero ones); the writer's canonical
  form writes **64** slots, materializing the 9 empty containers
  (55 nonzero in both). (INFERRED: this normalization, plus the
  header relayout it forces, is the visible part of why the bytes
  move; the diff is diffuse — ~1.03M small ranges — consistent
  with a full directory re-serialization rather than a localized
  change.)

Contrast with A1/A4: there the zero-change gate passed byte-exact,
because those baselines were files the writer lineage had already
produced. The game's shipped `.kfc` was never in writer-canonical
form. Consequence for the home test: none — the game reads the
directory semantically, and both staged pairs are delivered in the
same canonical form A4's pairs were (and are fixed points of it,
§3). It does retire one A1-era phrasing: "byte-faithful" holds
for writer-produced files; for the shipped file the correct claim
is *semantically lossless, content-identical normalization*.

## 3. Per-pair verification (both pairs, all PROVEN by the harness runs in `work/`)

| Check | new82 | new99 |
|---|---|---|
| Resource count | 91,894 = 91,812 + 82 ✓ | 91,911 = 91,812 + 99 ✓ |
| Fingerprint set delta vs pristine | +82 lines, **0** pristine lines lost/altered; all 82 are type `39768775` (TemplateResource) | +99 lines, **0** lost/altered; all 99 type `39768775` |
| Pre-existing key order | subsequence of new key sequence == pristine sequence exactly (0 diffs) | same |
| Pre-existing directory entries | 0 (offset, size) value diffs (cmpdir) | 0 |
| Contents map | sequence and values identical (101,459) | identical |
| Chunk records | 58 inherited records field-identical; +1 appended chunk at stream offset 214,536,192 (34,526 B compressed / 2,592,856 B uncompressed) | same shape; appended chunk 43,385 B / 3,141,384 B |
| Stream prefix | first 214,536,192 bytes byte-identical to pristine stream (pure append, `cmp` verified) | identical |
| 24-bit resource | still stores 11,105,420; reader recovery fires exactly once | same |
| Bundle deltas | TemplateResource count +82; every bundle index shift ∈ {0, +82} (segment arithmetic); internal hashes 0 diffs | count +99; shifts ∈ {0, +99}; hashes 0 diffs |
| Fixed point | zero-change in-place rewrite of the staged pair: **0 differing bytes** in `.kfc` and stream | **0 / 0** |
| Full read-back | 91,894 read OK, 0 missing, 0 errors | 91,911 read OK, 0 / 0 |

Clone population census (every clone read back individually,
`readres`, name field extracted): new99 — 99/99 named, suffixes
@x3 ×33, @x10 ×33, @x40 ×33, 33 distinct bases (the full production
population, including Crocodile, Scorpion_Fire, SpiderAnt, and the
three Ancient Constructs). new82 — 82/82 named, @x3 ×28, @x10 ×27,
@x40 ×27, 28 distinct bases; the 5 bases absent vs new99 are
Scorpion_Fire, SpiderAnt, and the three Ancient Constructs — i.e.
the first 28 bases of the mod's list, the 82nd clone being
Crocodile's @x3, exactly the cap mechanism's arithmetic.

Known residual, unchanged from A4: on additions the bundle **map's
key order** is rebuilt deterministically but not in loaded order —
36/146 positions differ from pristine (A4: 32/146), both new files
agree with each other; all bundle values are correct. This is the
open patch-0006 candidate, and it is a writer-canonical-form
property, present in A4's staged pairs as well.

## 4. Staged home test files

`test-files/` (hashes in `SHA256SUMS.txt`):

| Pair | `.kfc` SHA-256 | Stream SHA-256 |
|---|---|---|
| new82 | `10a549a1a3a327b64a…` | `f428ff66f061904dfc…` |
| new99 | `3c6ecc498c2754cd03…` | `fe59dd60a5b13ace40…` |

**What the home load test will prove:** whether a canonical file
carrying 82 (then 99) clones — built this time from a provably
stock base with zero other-mod contamination — loads in the real
stack. Combined with Eric's Oct 31 discriminator result (native
layer all-false, still crashed, on dialect-written files), a crash
here on canonical files closes the question as a genuine
consumer-side count cliff and the 81 cap is final; a load clears
the file/writer at these counts outright.

## 5. How these pairs differ from A4's

- **Base**: A4's baseline was Eric's live Nov 1 game file — it
  carried the capped-81 production build's output (81 pre-existing
  clones over 27 bases) plus thousands of other mods' patches
  (baseline 91,901 resources, 66 chunks, 64 containers). These
  pairs descend from the Steam-shipped file only: 91,812
  resources, 58 chunks, 55 containers, zero clones, zero mod
  patches. Every clone in them was created by the run that
  produced the file (A4's run99 created only 18 of its 99; its
  run82 only 1 of its 82).
- **Stream size**: 214.5 MB vs A4's 216.7 MB — the pristine stream
  has no accumulated append history (no fossil empty chunk; the
  appended clone chunk is chunk 59 here, 67 in A4).
- **Verification strength**: A4 spot-checked 4 clones' stat edits;
  here the name/preset census covers 100% of clones in both
  files, and the fixed-point and fingerprint gates are identical
  in kind and result.
- **G0 knowledge**: A4 never ran the zero-change gate against a
  never-written game file; §2's normalization finding is new and
  applies retroactively to how A1's byte-faithfulness should be
  read.

## Method notes / caveats

- The pristine pair was only ever read or copied; both SHA-256
  hashes were re-verified after all runs completed (see table in
  the header — final check logged in `work/`).
- The VCOUNT build used for run82 is
  `~/workspace/your_files/m182-variants-fix/mod-VCOUNT.lua` with
  only the `MAX_CLONES` constant changed 33 → 82; a diff against
  the repo stock `mod.lua` confirms the cap block is the sole
  difference (same as A4).
- Stat-edit float verification (A4 §4 style) was not repeated per
  clone; it is subsumed here by the loader's patch count, the
  fingerprint delta, and the full-population name census — the
  clone bytes are whatever the production mod + five-fix writer
  produce, read back byte-stable across the fixed-point pass.
- Intermediates: run/fixed-point work copies (~2.1 GB) were
  deleted after staging; all logs, directory dumps, and
  fingerprint files are kept in `work/`.
