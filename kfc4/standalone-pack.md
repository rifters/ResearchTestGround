# Standalone pack — byte-exact rebuild from the A2.5 folder alone

- **When:** 2026-11-01 (VM, local; game build 1273043 container)
- **Construction input (the only one):** A2.5 extraction
  `~/workspace/your_files/kfc4-a25/extracted-full/` (resources/ +
  _manifest.json with stream offsets, directory/ records, chunks.json,
  orphans/).
- **Comparison only (never opened by the pack code path):** pristine pair
  `~/workspace/repro-gamedir/enshrouded.kfc` +
  `enshrouded.kfc_resources`; baseline
  `~/workspace/your_files/kfc4-audit/work/fp-pristine.txt`.
- **Tool:** `kfc4-pack-standalone`
  (`~/workspace/your_files/kfc4-standalone/kfc4-pack-standalone/`),
  zstd pinned `=0.13.3` (the version A2.5's reproduction proof used);
  kfc-base from the in-place tree, used **only** by the `fps`
  verification subcommand.

## Verdict

**PASS — byte-exact.** The standalone pack rebuilds both game files from
the extraction folder alone and the outputs are **bit-identical to the
pristine pair** (SHA-256 below). The A2.5 folder is therefore not just a
faithful representation (A3's verdict) but a *sufficient* one: every byte
of `enshrouded.kfc` and `enshrouded.kfc_resources` is either stored in the
folder or reproduced by the folder's recorded rules (per-chunk zstd
levels, 4096-byte slot zero padding, ≤15 B zero small gaps, the two zero
alignment gaps, the zero tail). A3's Grade 2 — impossible from the A2
folder — is closed by the A2.5 records, exactly as A3 §3 itemized.

Structural guarantee: `pack` takes no reference/pristine argument at all.
Its inputs are the extraction folder and an output directory; the
pristine hashes were compared afterwards by a separate shell step
(`work/sha-main-vs-pristine.log`), and the parse check ran through the
separate `fps` subcommand.

## 1. How the pack works

```
kfc4-pack-standalone pack <extract-full-dir> <output-dir> [--allow-modified]
kfc4-pack-standalone fps <dir> <name> <fp-out>     (verification only)
kfc4-pack-standalone bytecmp <fileA> <fileB>       (verification only)
```

**Pre-flight (fail-closed; the output directory is not created until
every check has passed):**

1. **Directory records** — all 9 `.bin` records re-read and verified:
   length and SHA-256 against `directory/_directory.json`. Always fatal
   on mismatch.
2. **Orphan bytes** — `orphans.bin` verified against its `_directory.json`
   hash; each of the 3,960 gaps in `orphans.json` slice-hashed against
   its recorded SHA-256; gap total must equal the bin length.
3. **Chunk geometry** — chunk uncompressed ranges must tile
   `[0, stream_len)` and compressed slots must tile
   `[0, compressed_len)`, indices sequential, every frame ≤ its slot;
   cross-checked against the `resource_locations` record
   (uncompressed 501,156,384 / compressed-frames 216,562,036 / count
   91,901) and against `orphans.json`'s recorded stream length. The 9
   records must tile `[0, header_size)` without overlap.
4. **Coverage accounting** — all resource spans (manifest offsets) and
   orphan spans sorted and walked: overlaps are fatal; any uncovered
   region must be a ≤15 B small gap (a larger one would be stream data
   the folder does not hold). The computed census must match
   `orphans.json`'s recorded small-gap census.
5. **Manifest files** — all 91,901 files read: length must equal the
   manifest size (always fatal — the recorded geometry cannot honor a
   length change); FNV-1a-64 fingerprint (A0 function) must match the
   manifest. A fingerprint mismatch names the GUID and aborts unless
   `--allow-modified`.

**Assembly:** a zero-initialized 501,156,384-byte stream takes each
resource's bytes at its manifest offset and each orphan gap's bytes at
its stream offset (the small gaps stay zero). The stream is split per
`chunks.json` geometry and each chunk zstd-encoded at its recorded level
and laid into its recorded slot (frame at the slot start, remainder
zero). The `.kfc` is a zero-initialized file of
`align_up(header_size, 4096)` bytes with the 9 records placed verbatim at
their recorded offsets; the uncovered regions are verified zero and
reported. If an allowed edit changes a chunk frame's length, that
chunk's `compressed_size` in the chunk table and the
`resource_locations` compressed aggregate are patched to the actual
values (and reported) so the output stays self-consistent; with no
modified content, any frame-length divergence is fatal instead
(encoder/folder inconsistency). No `.dat` files are emitted (§4).

## 2. Main run — accounting and gates

Run: `pack extracted-full work/out-main` — exit 0, 67 s.
Log: `work/pack-main.log`.

**Stream accounting (computed from the folder, matches A2.5/A3):**

| Class | Bytes |
|---|---|
| Resources (91,901 files) | 483,669,781 |
| Orphan gaps (3,960) | 16,989,572 |
| Small zero gaps (64,139) | 497,031 |
| **Uncompressed stream** | **501,156,384** |

**Compressed stream:** 66 frames sum to 216,562,036; slots sum to
216,707,072 (= output length); **every frame reproduced at its recorded
level with exactly its recorded length — zero frame patches.**

**`.kfc` accounting:** records cover 9,394,437 of header size 9,394,448;
uncovered regions are exactly the recorded ones — `[209, 216)` (7 B,
after version), `[1252, 1256)` (4 B, after resource_locations), and the
tail `[9,394,448, 9,396,224)` (1,776 B) — all zero.

**Main gate — SHA-256, computed after construction** (`work/sha-main-vs-pristine.log`):

| File | Pristine | Standalone output | Match |
|---|---|---|---|
| `enshrouded.kfc` | `7f6130bb3d800fd5b0f91b2644792827045acc43ccbf666255dcfa8e544f79e9` | `7f6130bb3d800fd5b0f91b2644792827045acc43ccbf666255dcfa8e544f79e9` | ✓ |
| `enshrouded.kfc_resources` | `1cd249c30ff51a6b0d4126da3db4809bd0860c49ff859b3a1939f10290afe341` | `1cd249c30ff
...[truncated 5533 chars]