# KFC4 Properly + The Native Side — Plan

Eric's direction, set Nov 1, 2026: stop depending on bandaged KFC3-era EML
assumptions. Spec the KFC4 format from evidence, build tools that unpack and
repack it completely, and build a version-aware native signature base for
game build 1273043 — the layer his Vein+ShroudDig mod, Auto Delete, and the
controller input bridge all stall on.

## Starting position — what we already have

- Pristine KFC4 pair (Steam-verified, build 1273043) on the VM:
  `enshrouded.kfc` + `enshrouded.kfc_resources` (~215 MB, ~91,800 resources).
- Two writer outputs as test vectors (81-clone and 99-clone files).
- Both codebases cloned: upstream Brabb3l/kfc-parser v0.2.0 (KFC4-only) and
  the M182 fork (EML 0.1.2 snapshot + KFC3/KFC4 magic preservation).
- The build's type registry (`types.json`, 1273043) and resource-type index.
- Rust 1.88 toolchain on the VM; kfc-base builds here.
- Native references already studied: Greeneye function survey (host API +
  signature sets), EnshroudedMixedInput source (Go, signature patching),
  enshwick (typed Rust API + loader), EmberVault (methods; stale builds).
- Confirmed from code: chunk compression is **zstd** (kfc-base uses
  zstd 0.13.3); kfc-resource already has typed content modules (audio,
  image, localization, impact, mesh, math).

## Ground rules (the methodology)

1. **Evidence tags.** Every statement in the spec is PROVEN (byte-level
   test), INFERRED (code reading), or ASSUMED (carried from KFC3). Nothing
   ships as fact on an ASSUMED tag.
2. **Round-trip gates.** No phase advances past a failed equality gate.
   Parse → re-serialize → compare is the recurring test, at byte level
   first, semantic level (same resource set, same decompressed content)
   as the fallback standard — and which standard we met is always stated.
3. **Pristine is sacred.** All work happens on copies. Steam Verify is the
   recovery floor, never the workflow.
4. **One variable at a time.** The M182 nights proved why: bisects and
   discriminators over speculation, every time.
5. **Fail closed.** A signature that doesn't match exactly once, a resource
   that doesn't validate — the tool/mod refuses, loudly, instead of
   writing or hooking anyway.
6. **Per-build versioning.** Spec, registry, and signature DB are stamped
   to a game build. A hotfix triggers a revalidation checklist, not a
   rewrite and not a shrug.

## Part A — KFC4 format

**A0. Round-trip audit (RUNNING).** Can upstream 0.2 parse the pristine
KFC4 and re-serialize it losslessly? Same test against the fork. Output:
verdict + first-difference analysis. This calibrates how much of the
existing stack the rest of the plan can reuse.

**A1. Format spec, from bytes and code together.** Header (magic, version
tag), resource directory (ResourceId = GUID + type hash + part +
reserved, 32 bytes), chunk table, zstd chunk parameters, resource part
semantics, the `.kfc_resources` stream layout, any static-map sections.
Written as a living spec document where each field carries its evidence
tag and the test that proved it.

**A2. Full unpack to a folder.** `kfc4 extract`: the whole container as a
folder tree —
- `_manifest.json`: every resource (GUID, type hash, type name where the
  build registry resolves it, part, sizes, chunk map, content hash).
- One file per resource part, grouped by type, decompressed; typed decode
  where kfc-resource has a module, raw bytes otherwise.
- Gate: extracted set == container directory set, count and hashes.

**A3. Repack.** `kfc4 pack`: folder → container. Gate: repacking an
untouched extraction reproduces the pristine file (byte-identical target;
semantic-identical documented if byte level proves impossible and why).

**A4. Expansion.** Creating genuinely new resources, not just patching:
new GUIDs (the forged-GUID scheme spawn_variants uses, formalized),
directory growth, chunk allocation. Includes re-running the 81-clone
question with a known-correct writer, plus the still-unrun discriminator
(99 clones, native layer fully off) to place the cliff: writer, format
consumer, or engine.

**A5. Type-level decoding, on demand.** Deep field specs per resource
type as projects need them (BuffType, ItemInfo, BalancingTable first —
the ones already half-mapped), rather than boiling the ocean up front.

## Part B — Native side

**B0. Exe inventory.** Pull `enshrouded.exe` (1273043) via the Tailscale
file route (too big for chat upload). Sections, imports, compiler
fingerprints, confirm no symbols, record hashes so every signature is
tied to exact bytes.

**B1. Signature harvest from known-good sources.** Greeneye's sets
(teleport, physics, raycast, glider, gravity — source available in Eric's
forks), MixedInput's mode-determination patterns, emmveepee's salvage
signatures, the M182 native layer's hooked functions. Each candidate gets
re-validated against the current exe: exactly one hit or it's rejected.

**B2. Target list, driven by his projects.**
- Mining hit/dig functions — Vein + ShroudDig merge (the big one).
- Trash/Transfer event raisers — Auto Delete actuators
  (TrashItemEvent hash 0x248b58ac is the xref seed).
- Input-mode determination — controller bridge (MixedInput is the worked
  example; ClientPlayerInput layout already mapped from types).
- Item-action dispatch — salvage/delete family.
- UI settings-row construction — the StatSliders route, for the XHL
  panel question.
- XHL's config protocol surface — if the panel project revives.

**B3. Per-function RE loop.** Static xref in Ghidra from data anchors
(event/component hashes, strings) → candidate signature → uniqueness
proof across the whole exe → dynamic confirmation (log/breakpoint in a
test build on Eric's PC) → entry written into the DB with build stamp,
evidence, and the consumer that proved it.

**B4. Host library.** One small shared base, patterned on GreeneyeHost /
enshwick: AOB scan, inline hook + trampoline, call redirection, guarded
reads, hot-reload-safe storage. Signatures live in data files per build —
a game update means revalidating the DB, not patching every mod.

**B5. Proof by port.** One real consumer end-to-end before generalizing:
Auto Delete's actuators are the smallest complete test (find raisers,
fire event, verify in game).

## Tools

- **KFC side:** Rust 1.88 + the kfc-parser crates as the starting code
  base; Python for byte forensics and manifest work (the split_kfc.py
  pattern); zstd CLI/library for independent chunk checks.
- **Native side:** Ghidra (static RE, headless for batch signature
  validation), x64dbg on Eric's PC (dynamic confirmation), objdump
  (already on the VM) + a PE viewer for inventory; a small Rust/C++
  pattern-scan harness for exactly-once validation.
- **Second eyes:** Hermes/Glimmer on Eric's PC for bulk local analysis
  (stateless calls — every brief self-contained, full paths, forward
  slashes); Copilot (back Nov 1) for code volume; Remy for audit,
  spec-keeping, and verification.
- **Bookkeeping:** this plan + the spec + the signature DB under git in
  the goal workspace, so every claim has a commit and a build stamp.

## Expected problems

- **KFC4 may differ from KFC3 in more than the magic** — chunk ordering,
  directory canonicalization, part semantics. The round-trip may fail on
  ordering rather than content; that's still a finding, and A1 exists to
  pin it down.
- **Repack canonicalization.** Keen's writer may sort/pad in ways the
  reverse-engineered writer doesn't; byte-identical may be unreachable
  and semantic-identical becomes the honest standard.
- **The type registry is generated from the exe per build.** Data-side
  "expansion" almost certainly means new instances of existing types —
  inventing new types likely needs native help. Expectation set in A4.
- **The 81-clone cliff may be engine-side** (a cache/array limit), not a
  writer defect. A correct writer might not move it; the discriminator
  test decides where it lives.
- **Signatures rot every hotfix** and some functions inline or duplicate,
  producing multi-hit patterns that need call-site-relative signatures.
- **The shipped exe may differ from the running image** (Steam DRM
  wrapping); dynamic confirmation on the PC is the arbiter when static
  analysis disagrees with runtime.
- **Closed-source neighbors.** XHL's internals are observable only
  through logs/behavior; Greeneye and MixedInput are open in Eric's
  forks, XHL is not.
- **Scope creep is the main risk.** The spec could eat months. The gates
  keep it honest: only what extract/pack and the named native targets
  need gets spec'd deeply; everything else stays at inventory level
  until a project pulls it in.

## Sequence

1. A0 audit verdict (running now).
2. A1 spec skeleton + A2 extract v1 — VM-side, no PC time needed.
3. A3 repack gate. A4 expansion + clone-cliff discriminator.
4. B0 exe pull + inventory (needs one PC file-serve session).
5. B1–B3 signature DB for the named targets; B4 host; B5 proof port.

Each step lands as a file in this goal's workspace with its evidence
attached, so the work survives context resets — ours and the tools'.
