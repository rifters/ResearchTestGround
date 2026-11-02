# KFC4 A1 — Byte-Faithful Writer: Report

Date: Nov 1, 2026 · Game build 1273043 · Base: Brabb3l/kfc-parser **v0.2.0**
(commit `1670859`) · Builds on the A0 audit
(`~/workspace/your_files/kfc4-audit/REPORT.md`).

## Verdict

**Byte-identical round-trip: ACHIEVED.** With the three A1 fixes, a
zero-change re-serialization of Eric's pristine KFC4 pair reproduces
both files exactly — `.kfc` 0 differing bytes (identical SHA-256), the
216 MB resources stream byte-identical, chunk count unchanged at 66,
and all 91,901 per-resource fingerprints identical to the pristine
parse. The A0 baseline for the same procedure was 6,545,161 differing
`.kfc` bytes, a +4,096-byte stream, and 66→67 chunks.

The writer also still works as a writer: a real patch run (replace one
resource, add one new resource) produces a valid pair in which both
writes read back byte-exact and untouched resources are unchanged
(§4).

## 1. The fixes

Patched tree: `~/workspace/your_files/kfc4-a1/src/` (a copy of upstream
v0.2.0; the clones under `~/workspace/repos/` were not modified).
Each fix is a separate commit and a separate patch in
`~/workspace/your_files/kfc4-a1/patches/`, applied in order:

| Patch | Commit | Files | Change |
|---|---|---|---|
| `0001-static-map-order.patch` | `ef3fe27` | `container/static_map.rs` | `StaticMapBuilder` now stores entries in an `IndexMap` (kfc-base already depended on indexmap 2.9) instead of a `HashMap`. `build()` already sorted with a stable sort, so within-bucket order is now the insertion order — and a builder seeded from a loaded map inserts in the loaded key order, so unchanged maps serialize exactly as loaded. Includes a unit test pinning within-bucket order preservation. |
| `0002-bundle-preservation.patch` | `7bc743c` | `container/file.rs`, `container/writer.rs` | New `KFCFile::set_resources_preserving_bundles`, used by `KFCWriter::finalize` for incremental writes. Unchanged resource key sequence ⇒ the reference file's `resource_indices` and bundle map are carried over verbatim. Changed ⇒ groups are rebuilt with existing types in their previous bundle-segment order and new types appended ascending by type hash, indices reassigned as running offsets. The order derivation the game itself uses could not be found (spec §4.7), so preservation is the strategy. |
| `0003-no-empty-chunk.patch` | `21972fc` | `container/writer.rs` | `finalize()` submits the trailing chunk only when the chunk buffer is non-empty. A zero-change write no longer appends the empty zstd chunk. |

Gating: the patches apply independently in series order (verified:
applying 0001→0002→0003 to a pristine v0.2.0 worktree reproduces the
patched tree exactly). Patch 0003 also applies standalone to pristine
v0.2.0 — that is how the fix-3-only attribution build in §3 was made.
`cargo test -p kfc-base`: 21/21 pass (20 upstream + 1 new).
`cargo check -p mod-loader-lua` (the KFCWriter consumer) is clean; the
2 warnings it and kfc-base show are pre-existing in pristine v0.2.0
(unused `HashMap`/`Hash32` imports in `writer.rs` — present upstream,
left untouched).

## 2. Main proof — zero-change round-trip, all fixes

Procedure identical to A0 (the loader's own path:
`KFCWriter::new_incremental_with_options` + `finalize()`, zero
`write_resource`/`write_content` calls, on copies; registry =
`.cache/types.json`, 12,646 types). Harness:
`~/workspace/your_files/kfc4-a1/harness/` (`roundtrip` subcommand).

| Measure | Pristine | A0 (unpatched) | A1 (patched) |
|---|---|---|---|
| `.kfc` length | 9,396,224 | 9,396,224 | 9,396,224 |
| `.kfc` header.size | 9,394,448 | 9,394,472 (+24) | **9,394,448** |
| `.kfc` differing bytes | — | 6,545,161 | **0** |
| `.kfc` SHA-256 | `7f6130bb…e544f79e9` | — | `7f6130bb…e544f79e9` (identical) |
| Resources stream length | 216,707,072 | 216,711,168 (+4,096) | **216,707,072** |
| Resources stream bytes | — | original prefix + empty-chunk tail | **byte-identical** (`cmp` clean; SHA-256 `1cd249c3…0290afe341` both) |
| Chunk count | 66 | 67 | **66** |
| Re-read of output | — | 91,901 ok | **91,901 ok / 0 missing / 0 errors / 0 length mismatches** |
| Per-resource fingerprints | — | identical | **identical** (`work/fp-roundtrip-full.txt` == A0 `fp-pristine.txt`, file diff clean) |

Logs: `work/roundtrip-full.log`, `work/audit-roundtrip-full.log`.

## 3. Attribution — each fix isolated

Two extra builds of the same harness: **fix-3-only** (pristine +
patch 0003) and **fix-1+2** (commits `ef3fe27`+`7bc743c`, no patch
0003). Same round-trip procedure; directories compared semantically
with the harness `cmpdir` subcommand.

**Fix-3-only** — the stream is cured, the directory is not:
- Resources stream byte-identical, chunks 66 = 66, all 66 chunk records
  field-identical.
- `.kfc` positional byte diff: 1,393,150 (first diff at offset 2580,
  the `resource_indices` section). Semantic compare: resource/content
  key sequences differ, bundle `index` diffs **146/146**, all values
  otherwise equal. Exactly the A0 disease minus the chunk.
- Note on numbers: positional diff counts for the HashMap-ordered
  configurations vary run to run (std `HashMap` iteration order is
  process-randomized), and A0's 6,545,161 additionally included the
  +24-byte chunk-table insertion shifting every later positional
  comparison. The semantic compare is the stable measure.

**Fix-1+2 only** — the directory is cured, the chunk is not:
- Semantic compare vs pristine: **everything equal** — resource,
  content, and bundle key sequences all equal; 0 resource value diffs;
  0 bundle `index`/`count`/`internal_hash` diffs; containers and
  version equal; the 66 original chunk records field-identical.
- Sole residual: chunks 66→67 (the spurious empty chunk), stream
  +4,096 bytes appended after a byte-identical original prefix,
  `header.size` +24. The positional `.kfc` diff (6,451,798 bytes) is
  the 24-byte section shift caused by the extra chunk record, as the
  semantic compare proves — not content loss.

Conclusion: fixes 1+2 fully account for the directory dialect, fix 3
fully accounts for the stream growth, and only the combination is
byte-faithful.

## 4. Real patch run — the writer still writes

Harness `writetest` subcommand (full patch set), on copies: replace
resource `36234b22-85f2-4001-ac56-002b379d0d88` (16 B) with its first
byte flipped, and add a new resource `a1a1a1a1-a1a1-a1a1-a1a1-a1a1a1a1a1a1`
(100-byte payload) under the same (existing) type — the
spawn_variants-style operation. Results (`work/writetest-full.log`,
`work/cmpdir-writetest.log`):

- `finalize: OK`; resources 91,901 → 91,902; chunks 66 → 67, last chunk
  uncompressed size **128** (16 replaced + 112 aligned new bytes — a
  real chunk, not the empty-chunk pathology).
- All 66 original chunk records unchanged.
- New resource reads back byte-exact; replaced resource reads back
  byte-exact (with the flip).
- 201 untouched resources spot-checked across the key sequence:
  0 mismatches.
- Directory diff vs pristine is exactly the explainable set: 1 resource
  value diff (the replaced resource, relocated to uncompressed offset
  501,156,384 — the incremental writer's append semantics, unchanged
  from upstream behavior); bundle `count` diffs 1 (the grown type);
  bundle `index` diffs 41 (segments after the grown one shift by one,
  per the documented rule, spec §9.2); `internal_hash` diffs 0;
  contents map identical in sequence and values.

## 5. Caveats

- The fixes are against **upstream** v0.2.0. The M182 fork was not
  re-run in A1; per the A0 fork↔upstream diff its container read/write
  code is identical except magic handling (it would additionally need
  its `set_magic` preservation kept). Porting the three patches to the
  fork is mechanical but untested.
- `.dat` content payloads remain untested (only zero-byte stubs exist
  locally) — the byte-faithful result covers the directory + resources
  stream pair, and directory-level content entries only.
- No in-game acceptance test: that the game loads an A1-written file
  exactly as it loads an upstream-written one is highly likely (the
  zero-change output is literally the game's own file) but only Eric
  can run the game.
- One proven instance still underlies the 24-bit size recovery
  (spec §7.2); the writer changes do not interact with it (written
  entry sizes are the byte counts handed to `write_resource`).

## 6. Artifacts

- `src/` — patched upstream v0.2.0 copy (git history: base `1670859`,
  fix commits `ef3fe27`, `7bc743c`, `21972fc`).
- `patches/0001-static-map-order.patch`, `0002-bundle-preservation.patch`,
  `0003-no-empty-chunk.patch` — apply in order to pristine v0.2.0.
- `harness/`, `harness-fix3/`, `harness-fix12/` — proof harnesses
  (`audit` / `roundtrip` / `cmpdir` / `writetest`).
- `work/` — run logs, `fp-roundtrip-full.txt` fingerprints, and the
  round-tripped/writetest `.kfc` outputs. The 216 MB resources-stream
  outputs were deleted after comparison (byte-identity is recorded
  above with hashes; they are reproducible by re-running the harness).
- Spec: `~/workspace/goals/kfc4-format-audit-and-native-signature-base/files/KFC4-SPEC.md`.
