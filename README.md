# ResearchTestGround

Eric's playground repo for the Enshrouded KFC4 + native-side research:
findings, knowns, and test tools, kept with their evidence attached so the
work survives context resets — ours and the tools'.

Game build under study: **1273043** (Hotfix 1.0.0.3). Everything here is
stamped to that build unless a file says otherwise.

## Ground rules

- Every format claim carries an evidence tag: **PROVEN** (byte-verified on
  a real container), **INFERRED** (code reading, cited), or **ASSUMED**
  (carried from KFC3, untested). Nothing graduates silently.
- Round-trip gates: parse → re-serialize → compare. Byte level first,
  semantic level as the documented fallback.
- Pristine files are sacred; tools work on copies.
- Signatures must match exactly once or the tool refuses (fail closed).

## Index

- `plan/kfc4-and-native-plan.md` — the master plan: KFC4 spec/unpack/
  repack/expansion phases, native signature phases, tools, expected
  problems.
- `kfc4/knowns.md` — distilled proven/inferred/assumed facts about the
  KFC4 container.
- `kfc4/round-trip-audit.md` — the A0 audit (Nov 1, 2026): full parse of a
  pristine build-1273043 container, zero-change round-trip, verdict
  *compatible but lossy*.
- `kfc4/KFC4-SPEC.md` — the A1 format spec: every field evidence-tagged
  with the build-1273043 worked example.
- `kfc4/a1-byte-faithful-writer.md` — the A1 report: three writer fixes
  make the zero-change round-trip **byte-identical** (0 differing bytes,
  SHA-256 verified).
- `kfc4/a2-full-unpack.md` — the A2 report: the whole container unpacked
  to a folder tree, verified fingerprint-for-fingerprint against A0.
- `kfc4/a1-port-to-m182-fork.md` — the three writer fixes ported to the
  M182 fork EML: byte-identical round-trip there too, magic
  preservation intact.
- `tools/kfc4-writer-fixes-fork/` — the fork versions of the patches,
  in apply order.
- `kfc4/a3-repack.md` — the A3 report: the folder is a proven-complete
  payload representation; byte-identical repack is blocked by the
  writer's append-only mechanics, and the folder-only rebuild gaps are
  measured (≈25 MB of closing records defined as "A2.5").
- `tools/kfc4-pack/` — the A3 repack tool (folder → container, with
  fail-closed pre-flight verification).
- `kfc4/a2-5-full-fidelity-extraction.md` — the A2.5 report: the
  extraction now captures the complete directory side verbatim; both
  game files' byte budgets close exactly. Remaining gap: `.dat`
  payloads (PC-side).
- `tools/kfc4-extract-full/` — the A2.5 extractor.
- `kfc4/inplace-writer.md` — patches 0004 (in-place writer mode) and
  0005 (24-bit stored-size truncation): a same-size edit now changes
  exactly 2 bytes in the `.kfc`; ported to the fork as PR #6
  (stacked on PR #5).
- `kfc4/standalone-pack.md` — the standalone pack report: the
  container rebuilt from the extraction folder alone, bit-identical
  to the game's files.
- `tools/kfc4-pack-standalone/` — the standalone packer.
- `kfc4/contents-addressing-map.md` — the contents addressing census:
  all 101,459 entries mapped; payloads tile their containers
  back-to-back at 4,096 alignment; the content hash construction is
  identified; `ContainerInfo.size` is resolved (the padded `.dat`
  file length, predicted exactly for all 55 files).
- `data/contents-1.csv` + `data/contents-2.csv` (concatenate to
  restore the full 101,459-row dump; header is in part 1) and
  `data/containers.csv` — the contents dump and container table
  behind that census.
- `kfc4/a4-clone-cliff-files.md` — the A4 file work: 82/99-clone
  files regenerated with the proven writer; every old dialect
  feature is gone, and test pairs are staged for the load test.
- `kfc4/a4-test-protocol.md` — the home load-test protocol for
  those pairs (82 first, then 99, native-off discriminator).
- `tools/kfc4-audit/` — the Rust audit harness (upstream + fork builds)
  used to produce the audit numbers.
- `tools/kfc4-writer-fixes/` — the three patches against upstream
  v0.2.0, in apply order.
- `tools/kfc4-extract/` — the A2 extraction tool (container → folder
  tree + manifest).
- `tools/split_kfc.py` — splits a large game file into 50 MB parts for
  transfer, with progress output.

Native-side findings will land under `native/` when that phase starts.
