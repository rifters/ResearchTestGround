# KFC4 A1 → M182 Fork Port — Byte-Faithful Writer Fixes

Date: Nov 1, 2026 · Game build 1273043 · Ports the three A1 fixes
(`~/workspace/your_files/kfc4-a1/A1-REPORT.md`, patches against upstream
kfc-parser v0.2.0) to Eric's M182 fork EML (EML 0.1.2 pre-0.2.0 snapshot).

## Verdict

**Port complete — all proof gates pass.** With the three ported fixes,
the fork's writer reproduces Eric's pristine KFC4 pair **byte-identically**
on a zero-change write (`.kfc` and resources stream SHA-256 equal to the
pristine files), the output magic stays **KFC4** through Carlos's
preservation path (and a synthetic KFC3-tagged input round-trips
byte-identically with its **KFC3** tag preserved), and a real patch run
(replace + clone-add) matches the A1 upstream profile number for number.
No fix needed structural adaptation beyond context: the fork's container
write code proved byte-identical to upstream pre-patch code in every
region the A1 patches touch — the differences are exactly the magic
plumbing, which the port leaves untouched.

## Baseline identification

Fork baseline = shipped code at **`d6fbba2b8c306f9b75597bec2dc876d3e73e9cf9`**
("Add new marker check for dbghelp.dll"), taken from the clean local clone
in `base/` (checked out at that commit; the source clone's working tree was
never touched — its pre-existing `ports-work` modifications are byte-for-byte
as found).

One ref subtlety, recorded for honesty: the local clone carries two heads —
`refs/heads/m182-native-layer` = `c82e57a` (an older ancestor: "Remove the
market…") and `refs/remotes/origin/m182-native-layer` = `d6fbba2`. This port
uses **`d6fbba2`, the origin state** (it is also the commit the `ports-work`
tree is based on). Between the two, `eml-fork/crates/kfc-base` differs by
exactly one commit — the PR #2 reader bounds check (8 lines in `reader.rs`);
all container *write* paths are identical either way, so the choice does not
affect the port.

## The fixes, per fix

All patches live in `patches/`, apply in order at the **`eml-fork/`**
directory level (paths are `crates/kfc-base/...`), and were verified to
reproduce `src/` exactly when applied to a pristine baseline copy
(0 diff lines).

### 0001-static-map-order.patch — `crates/kfc-base/src/container/static_map.rs`

Fork state: `StaticMapBuilder` held `entries: HashMap<K, V>` with the same
stable `sort_by_key` in `build()` as upstream pre-patch — the region is
byte-identical to upstream's. `indexmap` was already a kfc-base dependency
(workspace), exactly as upstream.

Adaptation: verbatim in substance. `entries` becomes `IndexMap<K, V>`, the
`From<HashMap>` conversion becomes `map.into_iter().collect()`, plus
upstream's doc comments and the within-bucket-order unit test (inserted at
the end of the fork's smaller test module — the fork lacks upstream 0.2's
extra `from_parts` tests; its `build()` routes through
`StaticMap::from_parts(...).expect(...)`, which the change does not touch).

### 0002-bundle-preservation.patch — `container/file.rs`, `container/writer.rs`

Fork state: `set_resources` / `rebuild_resource_bundles` byte-identical to
upstream pre-patch. `finalize()` identical except the fork's own magic block
(finalize reads the existing output file's 4-byte tag and calls
`file.set_magic(magic)` immediately before `set_resources`).

Adaptation: upstream's change applied as-is — new
`KFCFile::set_resources_preserving_bundles`, `rebuild_resource_bundles`
gains the `previous: Option<&KFCFile>` parameter (verbatim carry-over fast
path when the resource key sequence is unchanged; otherwise previous
bundle-segment order for existing types, new types appended ascending by
type hash). In `writer.rs` the `set_resources(...)` call becomes the same
`match` on `incremental_data` as upstream. **The magic block two statements
above it is deliberately untouched**, as are `set_magic` in `file.rs`,
`KFCFile.magic`, and the header's dual-magic read/write support.

### 0003-no-empty-chunk.patch — `container/writer.rs`

Fork state: `finalize()` opened with the same unconditional
`self.submit_resource_data()?;`.

Adaptation: identical one-hunk guard — submit the trailing chunk only when
`!self.chunk_writer.get_ref().is_empty()`. The fork's `chunk_writer` field
and `submit_resource_data` are structurally the same as upstream's.

## Proof gates

### Gate 1 — unit tests (`cargo test -p kfc-base`, Rust 1.88.0)

| Tree | Result |
|---|---|
| Fork baseline (shipped, unpatched) | **13 passed / 0 failed** |
| Ported (`src/`) | **14 passed / 0 failed** (13 + the new within-bucket-order test) |

No pre-existing failures on the baseline, so the delta is fully
attributable. Note: the fork's env-gated `magic_tests` case self-skips
without `M182_KFC_SAMPLE` set, in both trees (Carlos's own test, left
as-is). `cargo check -p mod-loader-lua` (the `KFCWriter` consumer) is
clean on the ported tree.

### Gate 2 — zero-change round-trip of the pristine pair (harness `roundtrip`, copies only)

| Measure | Pristine | Fork baseline (measured here) | A0 fork run (ports-work tree) | **Ported fork** |
|---|---|---|---|---|
| `.kfc` differing bytes | — | 6,544,151 | 6,538,916 | **0** |
| `.kfc` SHA-256 | `7f6130bb…e544f79e9` | — | — | **`7f6130bb…e544f79e9` (identical)** |
| `.kfc` header.size | 9,394,448 | 9,394,472 (+24) | +24 | **9,394,448 (unchanged)** |
| Resources stream length | 216,707,072 | 216,711,168 (+4,096) | +4,096 | **216,707,072 (unchanged)** |
| Resources stream SHA-256 | `1cd249c3…0290afe341` | prefix identical + tail | same | **`1cd249c3…0290afe341` (identical)** |
| Chunk count | 66 | 67 | 67 | **66** |
| Chunk record field diffs | — | — | — | **0** |
| Resource/content/bundle key sequences | — | all differ | differ | **all equal** |
| Bundle `index` diffs | — | 146/146 | 146/146 | **0** |
| Resource/content value diffs | — | 0 | 0 | **0** |
| Output magic | KFC4 | KFC4 ✓ | KFC4 ✓ | **KFC4 ✓** |

(The positional differing-byte counts of the unpatched configurations are
process-randomized — std `HashMap` iteration order — so 6,544,151 vs A0's
6,538,916 is the same profile, not a discrepancy; the semantic measures
agree exactly.)

Re-read of the ported output (`audit`): **91,901 read ok / 0 missing /
0 errors**; fingerprints written by the ported harness are **identical to
A0's `fp-pristine.txt`** (file diff clean — 91,901/91,901).

### Gate 3 — writer still writes (harness `writetest`: replace + clone-add)

Same scenario as A1 §4, and the fork lands on the same numbers: replace
target `36234b22-85f2-4001-ac56-002b379d0d88` (16 B, first byte flipped),
new resource `a1a1a1a1-a1a1-a1a1-a1a1-a1a1a1a1a1a1` (100 B payload, existing
type). `finalize: OK`; resources 91,901 → 91,902; chunks 66 → 67 with last
chunk uncompressed size **128** (a real chunk); all 66 original chunk
records unchanged; both writes read back **byte-exact**; 201 untouched
resources spot-checked, **0 mismatches**. Directory diff vs pristine is
exactly the explainable set: 1 resource value diff (the replacement),
bundle `count` diffs 1, bundle `index` diffs 41, `internal_hash` diffs 0.

### Magic preservation — extra checks

- KFC4: every run above wrote `KFC4` (the pristine input's tag) via the
  fork's `set_magic` path — Carlos's behavior preserved.
- KFC3 (synthetic): a copy of the pristine `.kfc` with its tag flipped to
  `KFC3` (layout identical) round-tripped through the ported writer
  **byte-identically, output magic `KFC3`**. This proves the
  tag-preservation path end-to-end; it is *not* a genuine KFC3-era file
  (none exists on this VM), so real-KFC3 compatibility remains as
  (un)tested as it was before the port — unchanged, not regressed.

## Caveats

- **KFC3 write path with real KFC3 data: untested** (no sample on the VM;
  see above). The A0 finding stands: the fork accepts both magics with one
  layout, and the port does not alter that surface.
- `.dat` content payloads remain untested — locally only zero-byte stubs
  exist; the byte-faithful result covers the directory + resources-stream
  pair and directory-level content entries (101,459, carried exactly in
  semantic compare).
- The fork reader's missing 0.2 chunk-cache restructure is a *read*-path
  structural difference; it is out of scope for this port and A0 already
  showed the fork's reads are exact (fingerprints identical to upstream's).
- `dbghelp-proxy` is Windows-only and was not built here (same constraint
  as A0/A1); the Linux-verifiable consumer, `mod-loader-lua`, checks clean.
- No in-game acceptance test: only Eric can run the game. The zero-change
  output being literally the game's own file makes this a formality for
  the round-trip case; mod-written files carry the same profile as A1's
  upstream-written ones.

## Artifacts

- `PORT-REPORT.md` — this file.
- `patches/0001-static-map-order.patch`, `0002-bundle-preservation.patch`,
  `0003-no-empty-chunk.patch` — apply in order at the `eml-fork/` level
  to the fork baseline (`d6fbba2`); verified to reproduce `src/` exactly.
- `src/` — the ported `eml-fork/` tree (baseline + the three fixes).
- `base/` — clean clone of the fork checked out at `d6fbba2` (pristine).
- `harness/` — proof harness (A0 fork harness + A1's `writetest`
  subcommand), path-depending on `src/crates/kfc-base`. `harness-base/` —
  the same harness against the unpatched baseline copy, for reproduction
  of the baseline numbers.
- `work/` — run logs (`roundtrip-base.log`, `roundtrip-port.log`,
  `roundtrip-kfc3.log`, `cmpdir-*.log`, `writetest-port.log`,
  `audit-roundtrip-port.log`), `fp-roundtrip-port.txt` fingerprints, and
  the round-tripped/writetest `.kfc` outputs. The 216 MB resources-stream
  outputs were deleted after comparison (byte-identity recorded above
  with hashes; reproducible by re-running the harness). `work/portgit/`
  holds the git history the patches were cut from (baseline + one commit
  per fix).
