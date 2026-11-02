# KFC4 A2 — Full Unpack to a Folder: Report

Date: Nov 1, 2026 · Game build 1273043 · Tool: `kfc4-extract` (this folder)
Inputs: Eric's pristine pair `~/workspace/repro-gamedir/enshrouded.kfc` +
`enshrouded.kfc_resources` (read-only), type registry
`~/workspace/repro-gamedir/.cache/types.json` (12,646 types), reader =
A1-patched kfc-base (reader behavior-identical to upstream v0.2.0).

## Verdict

**Gate PASSED — at the byte level.** The full container is unpacked to a
folder tree, and the extraction verifies exactly against the A0 baseline:
91,901 files, every per-resource fingerprint identical to
`fp-pristine.txt` (91,901 / 91,901, zero mismatches), and the extracted
byte total equals A0's decompressed total to the byte.

## The tool

```
kfc4-extract <container.kfc> <output-dir> [--types <types.json>]
```

- Source: `kfc4-extract/` (binary `kfc4-extract`); build with
  `cargo build --release` (Rust 1.88). Depends on the A1 kfc-base by path.
- Iterates the directory in stream-offset order (chunk decompression
  stays sequential), reads every resource through `KFCCursor`, writes
  its decompressed raw bytes, and records a manifest entry.
- Fail-closed: a missing resource, a read error, or a length mismatch
  against the directory's (24-bit-recovered) size aborts the run naming
  the GUID. No silent skips.
- Type names resolve through the registry's **qualified** hash:
  `ResourceId.type_hash` is FNV-1a-32 of the qualified type name —
  verified independently: FNV-1a-32(qualified name) == type_hash for
  all 146 types (e.g. `keen::GameKnowledgeQueryResourceDb` →
  `e613a80a`).
- Fingerprints are the A0 audit's exact function, copied verbatim:
  **FNV-1a 64-bit** (offset basis `0xcbf29ce484222325`, prime
  `0x100000001b3`) over `[actual_size as LE u64 ‖ decompressed bytes]`.
- Transient-write retry: file writes retry up to 10 times with backoff
  (see "Run notes" — the VM's filesystem returned EPERM once mid-run);
  a failure that survives all retries is fatal and names the resource.

## Folder layout

```
extracted/
  _manifest.json     91,901 entries (26 MB JSON array)
  _summary.json      totals, per-type counts/bytes, unresolved list
  resources/
    <TypeName>/      146 dirs; `::` -> `__` (e.g. keen__ecs__TemplateResource);
                     unresolved types would be type_<hash> (none occurred)
      <guid>__p<part>.bin   decompressed raw bytes of one resource part
```

Manifest entry: `guid`, `type_hash` (hex), `type_name` (qualified, or
null), `part`, `stored_size`, `size` (actual, post 24-bit recovery),
`fingerprint` (hex), `chunks` (indices of the chunks the resource's
bytes span in the uncompressed stream), `file` (path relative to the
output dir). Example — the container's largest resource spans 6 chunks:
VoxelTemperatureResource, part 3, 44,659,852 bytes, chunks [3..8].

## Run stats

| Measure | Value |
|---|---|
| Resources extracted | 91,901 files |
| Total decompressed bytes | 483,669,781 |
| Wall time (tool-reported) | 13.4 s |
| Distinct type hashes | 146 — **all 146 resolved to names** |
| Registry types loaded | 12,646 |
| 24-bit size corrections | 1 (the same resource A0 found) |
| Zero-size resources | 0 |
| On-disk footprint of `extracted/` | 767 MB (483 MB content + 26 MB manifest + per-file block overhead) |

Top types by count: SoundResource 16,472 · TemplateResource 12,349 ·
RenderModel 10,470 · SoundContainerResource 6,940 · UiTextureResource
5,699 · JointAnimation 5,459 (largest by bytes: 168,652,464) ·
ItemInfo 4,262 · RenderMaterialResource 3,458. Full table in
`_summary.json`.

## Verification

| Check | Result |
|---|---|
| File count on disk == manifest entries == directory entries | **91,901 == 91,901 == 91,901**, 0 missing |
| Every fingerprint == A0 baseline `fp-pristine.txt` | **Identical as sorted line sets, 91,901 / 91,901, 0 mismatches** (lines compared in the audit's own format: guid, type hash, stored size, actual size, fingerprint) |
| Sum of extracted file sizes == A0 decompressed total | **483,669,781 == 483,669,781** (the 501,156,384-byte uncompressed stream total exceeds this by 17,486,603 bytes of inter-resource alignment/unused space, as A0 documented) |
| Type-name resolution | **146 / 146 resolved**, 0 unresolved. Cross-checks: the name set is identical to the game CSV export's 146 names; FNV-1a-32(name) == type_hash for all 146 |

## Counts reconciliation (parts and GUIDs)

The audit's "91,901 resources" are **ResourceIds** — (GUID, type, part)
triples — and the extraction is 1:1 with them, so no count drift. But
the GUID census is new information:

- 91,901 ResourceIds cover only **74,542 distinct GUIDs**. 7,624 GUIDs
  own more than one ResourceId (17,359 extra entries), **always
  spanning multiple types**: a GUID names a logical group (a scene, a
  world, an atlas), and its ResourceIds are the group's constituent
  typed resources.
- `part_index` enumerates repeated constituents of one type inside a
  group: 299 (GUID, type) pairs repeat, and in 298 of them the parts
  are exactly `0..n-1`. 8,691 entries have part > 0; max part is 1600.
- The extreme case is the main world, GUID
  `509feadb-4c60-425f-9c7c-deeefd9b6920`: **8,109 ResourceIds** across
  15 types — RenderModelChunkModelResource ×1601, SceneEntityChunkResource
  ×1601, SceneWaterEntityResource ×1601, WaterChunkResource ×1600,
  WaterDisplacingResource ×1600, VolumetricFog3ModelResource ×96,
  VoxelWorldResource ×2, plus 8 singletons — part = chunk index
  (0..1600). Its VoxelTemperatureResource part 3 is the 44,659,852-byte
  resource whose size A0's 24-bit recovery restored.
- Next largest groups: a UI atlas GUID (UiTextureResource ×207, parts
  0..206, + 1 UiMapResource) and several 35-entry scene groups
  (15 types, parts to 16).
- One anomaly, observed and unexplained: GUID
  `4ae2414d-c834-49b5-9321-0d45ae5a9b83`'s RenderTextureResource parts
  are 1..9 — no part 0. The fingerprint gate proves the bytes are the
  container's own; the missing part 0 is the game's data, not an
  extraction loss.

## CSV cross-check discrepancies (in the export, not the extraction)

Cross-checking against `0_resource_types_export_1273043.csv`:

- Name sets identical (146/146); per-type counts joined **by name**
  agree for **143/146** types.
- The CSV's `FirstSampleGUID` column is unreliable: for 40 of 146 rows
  the sample GUID belongs to a different type than the row's name (the
  samples cluster on WaterChunkResource / RenderTextureResource /
  DetailScatterResource entries). In all 40 cases the row's own name
  and count check out against the container via FNV — the defect is in
  the export's sample join. (A naive sample-based cross-check reports
  these as 40 name + 26 count "mismatches"; joined correctly, they
  vanish.)
- Three counts genuinely differ, mechanism unknown:
  NotifierResource CSV 2,319 vs container 2,325 (+6);
  TemplateResource CSV 12,267 vs 12,349 (+82); ImpactProgram CSV 570
  vs 571 (+1) — 89 entries total (0.097%). Not explained by part
  filtering or GUID dedup (all entries of these types are part 0 with
  distinct GUIDs). The container side is fingerprint-proven, so this
  is an exporter-side undercount; flagged here for the record, it does
  not touch the gate.

## Run notes

- The first full run died after 61,052 files with an OS error
  (`EPERM, "Operation not permitted"`) returned from a file write —
  while the same paths were immediately writable from the shell and a
  clean re-run (with the retry logic added) completed with **zero**
  retries triggered. Treated as a transient VM filesystem behavior at
  ~8,000 file creations/sec, not a format or tool defect; the partial
  output was deleted and the successful run is a single clean pass.
- Inputs were only ever opened read-only. Verified after the run:
  SHA-256 of the pristine pair still equals A1's recorded values
  (`7f6130bb…e544f79e9` / `1cd249c3…0290afe341`) — untouched.

## Deliverables

- Tool: `~/workspace/your_files/kfc4-a2/kfc4-extract/` (source + release
  binary under `target/release/`).
- Extraction: `~/workspace/your_files/kfc4-a2/extracted/` (91,901
  resource files + `_manifest.json` + `_summary.json`).
- Run log: `~/workspace/your_files/kfc4-a2/extract-run.log`.
- This report: `~/workspace/your_files/kfc4-a2/A2-REPORT.md`.

Next per the plan: **A3 — repack** (`kfc4 pack`: folder → container;
gate: an untouched extraction repacks to the pristine bytes, which A1
says is now attainable), and the `.dat` content half remains untested
(payloads absent on this VM).
