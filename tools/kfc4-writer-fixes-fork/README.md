# kfc4 writer fixes — M182 fork port

The three byte-faithful writer fixes (see `../kfc4-writer-fixes/` for
the upstream v0.2.0 versions) ported to Eric's M182 fork EML
(0.1.2 pre-0.2.0 snapshot, baseline commit `d6fbba2`). Full evidence:
`../../kfc4/a1-port-to-m182-fork.md`.

Apply in order at the fork's `eml-fork/` level:

1. `0001-static-map-order.patch` — IndexMap in `StaticMapBuilder`.
2. `0002-bundle-preservation.patch` — bundle/index preservation in
   incremental finalize. Carlos's magic-preservation block sits two
   statements above the replaced call and is untouched.
3. `0003-no-empty-chunk.patch` — no empty trailing chunk on finalize.

No structural adaptation was needed: in every region these patches
touch, the fork's write code is byte-identical to upstream pre-patch
code; the differences are exactly the magic plumbing. Result on the
fork: zero-change round-trip byte-identical, output magic preserved
(KFC4 in, KFC4 out; a synthetic KFC3-tagged input also round-trips
with its tag preserved). kfc-base tests 14/14.
