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

## Full-fidelity extraction (A2.5, Nov 1, 2026 — all PROVEN)

- The `.kfc` byte budget closes exactly: 9 verbatim directory
  sections (9,394,437 B) + 11 zero alignment bytes (7 after version,
  4 after resource_locations) = header size 9,394,448; the file's
  1,776-byte tail padding is all zero.
- The stream byte budget closes exactly: 483,669,781 resource bytes
  + 16,989,572 orphan + 497,031 zero padding = 501,156,384.
- Final chunk-level pattern: 58 chunks reproduce only at zstd level
  9; chunks 59, 63, 65 reproduce at {1, 3, 9} (recorded as 1);
  chunks 58, 60, 61, 62, 64 reproduce only at level 3. 66/66 frames
  are reproducible by re-encoding.
- **Correction to the A0 audit report:** the `.dat` total of
  74,410,422 B does not follow from `ContainerInfo.size` — those
  fields sum to 51,500,314,624, so `size` is not a payload byte
  count (semantics unknown). The `count` fields do sum exactly to
  the 101,459 contents entries: the containers partition the
  contents set.

## In-place writer (patches 0004+0005, Nov 1, 2026 — PROVEN locally)

- Stored sizes are written truncated to 24 bits (`size & 0xFFFFFF`)
  at the single serialization point; a zero-change round-trip never
  exposed this because the directory carries the loaded stored value
  verbatim — only a rewrite-from-data did.
- In-place mode (`KFCWriteOptions::in_place`, opt-in): chunk zstd
  levels are detectable from the reference file itself by
  re-encode-and-compare (66/66 detected on 1273043, matching A2.5),
  and frames sit in **4096-byte slots** — a re-encoded frame that
  still fits its original slot keeps it, so a same-size in-place
  edit changes exactly 2 bytes in the `.kfc` (aggregate + one chunk
  record's compressed_size) and confines stream diffs to the edited
  chunk's frame. Overflow and new resources append at the reference
  stream's end.

## Standalone rebuild (Nov 1, 2026 — PROVEN)

- A folder-only rebuild is **byte-exact**: `kfc4-pack-standalone`,
  fed only the A2.5 extraction folder, reproduces both game files
  bit-for-bit (SHA-256 match), and an edited folder rebuilds with
  diffs confined to the edited chunk's frame plus the two directory
  fields that describe it. With A1–A2.5, the format's read/write
  model is closed on this build; the only uncaptured part of a full
  game directory is the `.dat` payload set (PC-side).

## Contents addressing (census, Nov 1, 2026 — PROVEN from the directory)

- A content's payload length is stored in its key:
  `ContentHash { size: u32, hash: [u32; 3] }`; the entry is
  `{ offset: u32, flags: u16, container_index: u16 }`. All 101,459
  offsets are multiples of 4,096, and payloads tile each container
  back-to-back: `next_offset == align4096(offset + size)` for
  101,402 of 101,404 consecutive pairs, zero overlaps. (One
  irregularity: the map's single size-0 entry shares its offset in
  container 53 with an 8,192-byte entry.) All flags are 0.
- **The content hash is identified**: the size-0 entry's key equals
  `ContentHash::from_data(&[])` through kfc-base's AES
  `compute_content_guid` — that construction is the game's content
  hash (proven on the empty payload; general verification becomes
  mechanical once payloads are captured).
- **`ContainerInfo.size` is resolved** (supersedes the A2.5 note):
  it is the `.dat` file's length — Σ `align4096(size)` over the
  container's payloads, exact for 55/55 nonempty containers
  (Σ 51,500,314,624 B; raw payloads Σ 51,283,306,666 B). The
  directory therefore predicts every `.dat` file's length exactly,
  and `data/contents.csv` is the slicing table for a capture.

## Reference-pair correction + clone-cliff files (A4, Nov 1, 2026 — PROVEN)

- The reference pair used by every phase is Eric's live game file
  as pulled Oct 31, 2026 — **not** a clone-free container: it
  carries the capped spawn_variants output (81 clones, 27 bases ×
  3 presets) inside its 91,901 resources. All phase results are
  relative to that snapshot and remain exact; "pristine" in the
  reports means this pair.
- The crash-era uploaded pair is the *single-template* test
  population (99 copies of template `642098d4…` on one GUID stem),
  not the 33-base production set; the 81 file is its first-81
  subset.
- With the five writer fixes, regenerated 82- and 99-clone files
  show **zero** dialect features (key order 0 diffs, no entry
  diffs, contents identical, no spurious chunk, pure-append
  stream) and are fixed points of the in-place writer. One
  residual: when resources are *added*, the bundle map's own key
  order is rebuilt deterministically rather than preserved
  (32/146 positions; all values correct) — a patch-0006 candidate.

## Clone-cliff attribution — CORRECTED (Nov 1, 2026, per Eric)

- The discriminator **was run** on Oct 31: stock 99 clones with
  `m182_native.json` all false **still crashed** — the native
  layer is **cleared**. (An earlier version of this section said
  the opposite; it misread Eric's report.) The files in that run
  were old-writer dialect, so the remaining suspects are the
  dialect itself and a game-side count consumer; the staged
  canonical 82/99 pairs are the decisive test.

## True pristine base + regenerated pairs (Nov 1, 2026)

- Steam-fresh pair secured (Eric deleted + Verified): 91,812
  resources, contents 101,459 (identical to the modded file —
  the census stands), 58 chunks, one 24-bit correction. His old
  live file carried 89 extra resources and 4,279 modified ones.
- G0 finding: on a *shipped* file the five-fix writer is
  semantically lossless but not byte-identical in the `.kfc` —
  the shipped directory declares 55 containers (the nonzero
  ones); the writer canonicalizes to 64 slots. The stream is
  byte-identical. Preserving the shipped layout is a patch-0007
  candidate. A1's byte-exact result was relative to an already
  writer-produced baseline.
- Regenerated pairs from pristine (A4 gates all pass; full
  report `kfc4/regen-report.md`): new82 = 91,894 resources
  (clone #82 is Crocodile @x3), new99 = 91,911. These supersede
  the A4 pairs for the home load test.

## Load test result — the cliff is the game's own (Nov 2, 2026)

- Eric loaded the canonical new82 pair at home: **crashed**.
  Combined with the Oct 31 native-off run, every file-side and
  native-side suspect is now eliminated: the writer's dialect
  (this file was writer-canonical, fixed-point stable), the
  native layer (off, still crashed), and clone content (single-
  template and reversed-order runs from Oct 31). What remains
  is a count consumer inside the game's own template handling.
- Working configuration stands: **MAX_CLONES = 81**. The new99
  test is moot. Reopening the question would require finding
  the consumer in `enshrouded.exe` (Part B territory).

## Confound — retest planned (Nov 2, 2026)

- Eric flagged it: the new82 load test ran with his mod stack
  live, and EML re-exports the `.kfc` on launch — the bytes the
  game loaded were the installed writer's re-serialization, not
  the staged pair. The crash above is therefore suggestive, not
  decisive. Retest protocol: restore the clean pair, re-apply
  the 82 set, rename `dinput8.dll` and `dbghelp.dll` off (no
  loaders, no patch phase), launch, load a world.

## Clean retest — the file is innocent (Nov 2, 2026)

- Eric re-ran with **no mods at all** (dinput8.dll and
  dbghelp.dll renamed off): the staged new82 pair **loaded with
  no crashes**. The game accepts 82 clones in a canonical file.
  The cliff therefore lives in the live-stack path, not the
  file and not a game count limit.
- Reassessment of the Oct 31 native-off datapoint: all-false
  `m182_native.json` still had dbghelp.dll loaded and the EML
  patch phase running over the file — the EML/native presence
  was never actually removed in any crashing run until now.
- Prime suspects: the patched EML's patch/export phase over a
  file already containing the clones (create-call collisions,
  enlarged-container export), or an EML mod walking the clone
  templates. Next rung: dbghelp restored alone (dinput8 stays
  off) — crash convicts the EML side; a clean load points at
  the MLP/XHL/Greeneye chain or an interaction.

## Empty-mods rung — trigger is a mod's patch output (Nov 2, 2026)

- dbghelp live, mods folder empty, staged new82 pair: EML log
  shows "Applied 0 patches" and the game **loaded fully**. The
  post-run `.kfc` hashes byte-identical to the staged file —
  the new writer's zero-patch export is a true fixed point.
  The native layer was live and active the whole session.
  Runtime, native layer, and writer are all cleared.
- The Benchmark error is **benign**: it appears in the
  successful log too (`[ecsc]` pass; the game continues and
  initializes 12,349 template infos = stock + 82 clones). The
  fatal element in modded runs is the `[X]` allocator assert
  following it in the server pass.
- Remaining suspect space: the EML mods' patch output
  (4,433 patches) interacting with the 82nd clone. Bisect
  starts with `m182_spawn_variants` out (the only
  clone-count-dependent mod), then `zz_m182_item_stats`.

## Patch-pass diff — no content difference at 82 (Nov 2, 2026)

- Local five-fix `emm` passes (full mod set, capped-81 variants)
  over staged-81 and staged-82 files, plus a same-input control
  run: identical patch counts (4,429); EXPORT82 vs EXPORT81
  content differs only by the Crocodile clone itself
  (byte-identical pass-through). Remaining diffs are proven
  noise: 27 `*_Factory` templates flip ±1/−7 bytes between any
  two runs; QSR notifier GUIDs are random per run. Full report:
  `patchpass-diff` work (VM-side).
- The +5 creates are solved: Auto Loot Plus's fixed-GUID smash
  ImpactProgram + aura template (`a5c0de5a…a5000/a5002`), and
  Quick Stack Range's 3 random-GUID NotifierResource copies —
  identical over the 81 file and in the in-game export.
- The Benchmark template (`7d243d9e…`, 96 B) is byte-identical
  in pristine, staged, and both exports. No pass touches it.
- Conclusion: the trigger is consumption-side — the live
  process dies on a **relocated re-export** at 82 templates,
  not on patch content. Candidate fix under test: enable the
  in-place writer mode (patch 0004) in the fork EML's runtime
  export path, keeping the post-patch layout as staged.

## In-place runtime failure + fallback fix (Nov 2, 2026)

- PR #8 wired `in_place: true` into the runtime's `create_writer`.
  In-game: pass 1 exported in-place successfully, but EML's
  second pass builds its reference from the `.bak` (pre-patch
  directory) while the stream on disk is pass 1's export.
  `InPlaceData::load`'s strict snapshot dies with
  `IO error: incomplete frame`; AppState fails; EML restores
  only the `.kfc`, leaving a two-generation pair (game:
  "Could not find core resource package").
- Reproduced locally by forcing a second CLI pass over an
  in-place export through the backup-reference path; a
  consistent in-place export pair itself decodes cleanly
  (59/59 chunks) — the exports are well-formed; the snapshot
  just requires directory/stream generation match.
- Fix (PR #9): a failed snapshot degrades that run to the
  incremental writer instead of failing writer creation.
  Verified: kfc-base tests 25/25; forced second pass succeeds.

## Pass-2 panics — base reads pair the wrong stream (Nov 2, 2026 — PROVEN)

- The 99 launch on the PR #8 build: pass 1 clean (99 created,
  4,532 patches), pass 2 died mid-run with the cannot-unwind
  abort — first reported on bufftweaks, later on Auto Loot Plus.
  Mechanism, reproduced in the CLI: the loader's "original file"
  view pairs the `.bak` DIRECTORY with the LIVE stream. That was
  safe only while exports were append-only (original bytes never
  move — the fork's own guard.rs doc states this assumption).
  In-place exports rewrite slots, so backup-directed reads return
  rewritten bytes; the first mod to read unwrapped dies.
- PR #10 (pair backup): `create_backup` also copies the stream to
  `.kfc_resources.bak`; restores restore both halves.
- PR #11 (pair reader): `create_reader` sets
  `resource_extension: "kfc_resources.bak"` so the backup
  directory reads the backup stream. Verified locally: the forced
  second pass that exited 1 with the incomplete-frame cascade now
  exits 0, zero frame errors.
- First 99 launch on the #11 build: both passes clean, no panic.

## Template arithmetic — a ceiling that fit every point (Nov 2, 2026 — counts PROVEN, interpretation SUPERSEDED below)

- Pristine file: 12,267 TemplateResources. Eric's daily-driver
  baseline: exactly 12,349 (81 clones + the Auto Loot Plus aura
  template). Every crashing in-process launch was ≥12,350
  template infos; every loading one ≤12,349. The byte-budget
  variant was killed (bare-82 vs modded-82 differ by the aura's
  736 bytes but exactly 1 in count). Reading at the time: a
  game-side registration ceiling at 12,349 — see the cold test
  below, which kills this interpretation.

## Re-patch corruption — the backup is the eternal base (Nov 2, 2026 — PROVEN)

- Loader model (code-verified in app_state.rs/lib.rs): the
  working registry and all "original" reads come from the `.bak`
  pair; the live file is a derived artifact; baked resources
  persist across launches only because unchanged sets skip
  patching (files.json cache). Any set change re-patches from the
  backup.
- Failure: bake (in-place export), remove a mod, restart → the
  re-patch wrote the backup generation's directory over the baked
  generation's stream. Game log at 2s: `[resource] Could not load
  resource bundle #0. error:eof reached` → Steam_CoreResourcePackageNotFound.
- PR #12 (restore-before-patch): in AppState::new, when a patch
  pass will run and the live `.kfc` differs from the backup,
  restore the pair first. A/B verified in the CLI on the exact
  sequence: the pre-fix pair is unparseable on read-back (chunk
  EOFs; version field reads as garbage 0x6d783f9b); the fixed
  build's pair audits clean — 91,817 resources, 0 size mismatches.

## Bake-and-remove cannot test baked files (Nov 2, 2026 — PROVEN by measurement)

- Eric's sequence on the #12 build (bake 99, exit, remove mod,
  restart) loaded the world — but the pulled live `.kfc` measures
  91,817 resources, 12,268 TemplateResources, and 0 of the 99
  clone GUIDs: the restart's restore+re-patch (PR #12 working as
  designed) had rebuilt the file without the clones. Baked-file
  tests require a staged pair with the loaders out of the process.
- Tooling note: TemplateResource in ResourceId terms is
  type_hash 964069237; the A0 fingerprint files' type column
  (39768775) is a different representation.

## COLD TEST — there is no file-side ceiling (Nov 2, 2026 — PROVEN)

- Staged pristine+99 pair placed, BOTH loaders renamed off, cold
  launch: game log shows `[ecss] Initialized 12,366 template
  infos` and the same for [ecsc], zero fatal lines, session
  established, world played. The identical file content crashes
  when a patch pass runs in the same process.
- Verdict: the "ceiling" is a process-state interaction, not a
  count limit. Eric's runtime-strain theory is the standing
  model: EML's patch chain (Lua execution + export, ~4,500
  patches) leaves the process in a state where the [ecss]
  template array's allocator is never set at high counts —
  consistent with the assert text ('No allocator set') and with
  the exhausted static hunt (no 12,349 constant exists in the
  exe). Every crash on record is a patch-pass launch; the daily
  12,349 file loads on skip launches; an empty-mods patch pass at
  12,349 loaded.
- Practical: the 99 clones are playable today via bake-once-
  then-skip (the bake launch still crashes at world load; the
  next, unchanged launch skips patching and loads).
- Candidate proper fix (design only): run the patch phase out of
  process — a thin proxy that spawns the standalone patcher and
  waits. In-process, the loader DLL never exits, and freed
  patch-phase heap remains as fragmentation.
- Open: the true cold file limit above 12,366 (untested); what
  exactly the patch pass leaves behind (Ghidra trace of template
  registration + allocator assignment, running); whether the
  dedicated server binary (fewer passes, 16-player headroom)
  tolerates the same chain — Eric's hypothesis, untested.

## .dat payloads — first container verified (Nov 2, 2026 — PROVEN)

- `enshrouded_054.dat` pulled from the game folder: 29,224,960
  bytes, exactly the census-predicted length. All 1,209 payloads
  slice at their recorded offsets and `ContentHash::from_data`
  reproduces every directory key; 0 out-of-bounds, 0 unaligned.
  Payloads are raw slices addressed by the contents map; no
  per-container transform. Remaining containers unverified
  (51.5 GB total; pull per purpose).

## Ghidra trace — registration mapped, template array exonerated (Nov 2, 2026 — PROVEN from the binary)

- Full report: `kfc4-partb/GHIDRA-TRACE.md` in the project files
  (Ghidra DB built from the exe's `.pdata` — 72,747 functions —
  after full auto-analysis OOM-failed 4× on a 7 GB VM).
- Registration driver `0x1409c0210` iterates the template
  records and calls register-one-template `0x1409bf800` (FUN_A),
  then logs 'Initialized %,u template infos'. Correction to an
  earlier note: the ECS log strings DO have direct `lea` xrefs at
  their true start addresses — the original probe had hit
  mid-string fragment offsets.
- The TemplateInfo slice (`world+0x280`) grows through a change
  function at `world+0x298` whose null-guard asserts a DIFFERENT
  message ('Cannot increase slice capacity… no change function
  registered!'). The template array is therefore NOT the firing
  container.
- The crash assert's exact condition (from the decompiled
  setSize utility family): a GROW request (newSize > capacity)
  against a BaseDynamicArray whose +0x20 tagged-allocator field
  is 0. Shrink/reset calls can never reach it. Container layout:
  +0x00 data, +0x08 count, +0x10 capacity, +0x18 changeFn,
  +0x20 tagged allocator.
- FUN_A's own builder containers are provably safe (allocator
  field set unconditionally per call from a TLS context object).
  The firing grow is most likely a NEIGHBOURING container in the
  same ~1 ms window after the benign Benchmark miss — which is
  why the crash tracks template count and patch-chain presence
  rather than the file.
- Decisive next step (one run): x64dbg conditional breakpoint at
  the shared assert reporter `0x14002f590` with
  `rdx == 0x1413b55b0` under the crashing configuration; the
  return address names the firing instantiation. Full 7-BP plan
  in the report §6.

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
- **PARTLY PROVEN** — `.dat` content *payloads*: container 54 fully
  verified against the directory (1,209/1,209 hash matches, see the
  .dat section above); the other 54 non-empty containers are
  unverified.
- **NOTE** — Upstream 0.2 now *rejects* KFC3 files outright
  (`InvalidMagic`); the fork accepts both magics and preserves the
  existing file's magic on write.
