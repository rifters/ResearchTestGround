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
- `tools/kfc4-audit/` — the Rust audit harness (upstream + fork builds)
  used to produce the audit numbers.
- `tools/split_kfc.py` — splits a large game file into 50 MB parts for
  transfer, with progress output.

Native-side findings will land under `native/` when that phase starts.
