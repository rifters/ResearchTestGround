# CORRECTION — Nov 1, 2026, per Eric

Eric confirms the discriminator WAS run on Oct 31: stock 99
clones with `m182_native.json` **all false still crashed** — the
native layer is CLEARED. (An earlier version of this header said
the opposite; it misread him.) The files in that run were written
by the old dialect writer, so the staged pairs below are the
decisive remaining test: canonical files at 82+ load => the
dialect was the cause; still crash => a game-side count limit is
real and the 81 cap is final.

---

# Home load test — proven-writer clone files at the 82 / 99 boundary

Game build 1273043 · Files staged Nov 1, 2026 (VM side of A4)
Hashes: see `SHA256SUMS.txt` in this folder.

## What these files are

| Staged pair | Resources | Forge clones (TemplateResource, GUID segment `4182`) | How it was made |
|---|---|---|---|
| `new82/` | 91,902 | **82** total (81 carried in the baseline + 1 created) | VCOUNT test build capped at 82, run over the baseline |
| `new99/` | 91,919 | **99** total (81 carried in the baseline + 18 created) | Stock spawn_variants, run over the baseline |

- Baseline = the container pulled from your game folder on Nov 1
  (`enshrouded.kfc` SHA-256 `7f6130bb…`, 91,901 resources). It already
  contains the capped-81 clone set from your live sessions, preset GUID
  scheme, 27 enemy templates — the runs above only added the missing
  clones (bases 28–33) on top. That is deliberate: the staged files
  match your actual install's lineage, not a virgin container.
- Both pairs were written by the fork EML built locally from the tree
  carrying **all five writer fixes** (0001 map order, 0002 bundle
  preservation, 0003 no empty chunk, 0004 in-place mode, 0005 24-bit
  stored sizes) — the state of PRs #5 + #6. Directory forensics and a
  byte-identical fixed-point rewrite are in `../A4-FILES-REPORT.md`.
- The clone population is **not** a byte-replica of the Oct 31
  crash-era files: those held 99 copies of a *single* template
  (one GUID stem, counter segment, all size 31,720). These hold
  clones of 33 distinct templates with the current mod's GUID scheme.
  The variable under test is clone **count**, as in the original
  bisect (81 loaded, 82 crashed).

## Backup first (do not skip)

In the game folder (`…\Steam\steamapps\common\Enshrouded`):

1. Create a folder somewhere safe, e.g. `Desktop\kfc-backup-82-99\`.
2. Copy into it: `enshrouded.kfc`, `enshrouded.kfc_resources`, and
   `enshrouded.kfc.bak` if one exists.
3. Verify the copies' sizes match the originals before going on
   (.kfc = 9,396,224 B; _resources ≈ 216.7 MB, size varies).

## Test procedure

Keep the rest of the stack exactly as in the working capped
configuration. The only change is the starting file.

1. **82 first.** Copy `new82\enshrouded.kfc` and
   `new82\enshrouded.kfc_resources` into the game folder, overwriting.
2. Launch, load the world (the actual Oct 31 failure point), play a
   few minutes, note the result.
3. **Then 99.** Restore is *not* needed between tests — overwrite the
   pair with the `new99\` files and repeat.
4. Record per file: loaded / crashed (and if crashed, at world load
   or later), plus anything the EML log shows for
   m182_spawn_variants (it should report its clones already present).

What a pass proves: a file carrying 82 (then 99) clones, written by
the byte-faithful writer, loads — the crash-era failures are then
attributable to the old writer's dialect and/or the native layer,
not to clone count in a correctly written file.

## If it still crashes — the discriminator

The standing confound is the never-run test from Oct 31: the native
layer was ON for every crash-era run. Separate it:

1. Restore the backup pair.
2. Set `m182_native.json` **all false** (pvp, spawner, panel, diag —
   everything off).
3. Drop in the same staged pair that crashed (82 first) and load.

- Loads with native off → the native layer is implicated; the file
  and writer are cleared.
- Still crashes with native off, on a proven-canonical file → the
  cliff is a genuine consumer-side count limit, file-independent.
  The 81 cap is then simply the answer.

## Restore

Delete the dropped-in pair and copy back all files from
`Desktop\kfc-backup-82-99\` (including the `.bak` if you backed one
up). No other cleanup is needed; nothing else in the game folder was
touched.

## Note on equivalence

These staged pairs test the **file question** directly, without
waiting on PR merges. The in-game equivalent — PRs #5 + #6 merged,
DLL rebuilt, then a normal patch run with the uncapped mod — would
produce the same clone additions on the same baseline, but is a
separate, later confirmation, not a prerequisite for this test.
