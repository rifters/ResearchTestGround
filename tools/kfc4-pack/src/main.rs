// kfc4-pack — KFC4 project phase A3: repack an A2 extraction folder back
// into a container, plus the analysis/verification tooling the A3 gates
// need.
//
// Subcommands:
//   pack    <extract-dir> <reference> <output-dir>
//           [--types <types.json>] [--baseline <fp-pristine.txt>]
//           [--allow-modified]
//           Grade 1 repack: verify every extracted file against the
//           manifest (and, if given, the A0 fingerprint baseline), then
//           drive the A1-patched incremental KFCWriter with every
//           resource's bytes read from the folder, in manifest order,
//           using the reference .kfc as the writer's reference file.
//           Fail-closed: a missing file or a length mismatch aborts
//           naming the GUID; a fingerprint mismatch aborts unless
//           --allow-modified is given. Nothing is written before the
//           whole folder has proven itself.
//   fps     <dir> <name> <fp_out>
//           Fingerprint every resource of a .kfc pair (A0 audit format).
//   cmpdir  <dirA> <dirB> <name>
//           Semantic directory comparison of two .kfc files.
//   bytecmp <fileA> <fileB>
//           Streaming byte comparison: lengths, differing byte count,
//           first difference, coalesced differing ranges.
//   analyze <dir> <name>
//           Grade 2 evidence on a pristine pair: uncompressed-stream
//           layout gaps (sizes, placement, byte content), per-chunk
//           zstd recompression identity, bundle count cross-checks.
//
// The fingerprint function is the A0 audit's, verbatim: FNV-1a 64-bit
// over [actual_size as LE u64 || decompressed bytes].

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    rc::Rc,
};

use kfc_base::{
    container::{KFCFile, KFCReader, KFCWriteOptions, KFCWriter},
    guid::ResourceId,
    reflection::TypeRegistry,
};

fn fnv1a64(parts: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for part in parts {
        for &b in part.iter() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

// ---------------------------------------------------------------- fps

fn fps(dir: &str, name: &str, fp_out: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file = KFCFile::from_path(Path::new(dir).join(format!("{name}.kfc")), false)?;
    let reader = KFCReader::new(dir, name)?;
    let mut cursor = reader.new_cursor()?;
    let ids: Vec<ResourceId> = file.resources().keys().to_vec();
    let mut ok = 0usize;
    let mut missing = 0usize;
    let mut errors = 0usize;
    let mut total_bytes = 0u64;
    let mut len_mismatch = 0usize;
    let mut lines: Vec<String> = Vec::with_capacity(ids.len());
    let mut buf: Vec<u8> = Vec::new();
    for id in &ids {
        let entry = file.resources().get(id).unwrap();
        let actual = reader.resource_size(id).unwrap_or(entry.size);
        buf.clear();
        match cursor.read_resource_into(id, &mut buf) {
            Ok(true) => {
                ok += 1;
                total_bytes += buf.len() as u64;
                if buf.len() as u64 != actual {
                    len_mismatch += 1;
                }
                let fp = fnv1a64(&[&actual.to_le_bytes(), &buf]);
                lines.push(format!("{id} {:08x} {} {} {:016x}", id.type_hash(), entry.size, actual, fp));
            }
            Ok(false) => missing += 1,
            Err(_) => errors += 1,
        }
    }
    println!("read_ok: {ok}");
    println!("read_missing: {missing}");
    println!("read_errors: {errors}");
    println!("read_total_bytes: {total_bytes}");
    println!("read_len_mismatch_vs_actual: {len_mismatch}");
    lines.sort();
    let mut out = File::create(fp_out)?;
    for line in &lines {
        writeln!(out, "{line}")?;
    }
    println!("fingerprints_written: {} -> {fp_out}", lines.len());
    Ok(())
}

// ---------------------------------------------------------------- cmpdir

fn cmpdir(dir_a: &str, dir_b: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let a = KFCFile::from_path(Path::new(dir_a).join(format!("{name}.kfc")), false)?;
    let b = KFCFile::from_path(Path::new(dir_b).join(format!("{name}.kfc")), false)?;
    println!("version_eq: {}", a.game_version() == b.game_version());

    let cont_eq = a.containers().len() == b.containers().len()
        && a.containers().iter().zip(b.containers()).all(|(x, y)| x.size == y.size && x.count == y.count);
    println!("containers_eq: {cont_eq} ({} vs {})", a.containers().len(), b.containers().len());

    let chunks_a = a.resource_chunks();
    let chunks_b = b.resource_chunks();
    println!("chunks_len: {} vs {}", chunks_a.len(), chunks_b.len());
    let mut chunk_field_diffs = 0usize;
    for (x, y) in chunks_a.iter().zip(chunks_b.iter()) {
        if x.offset != y.offset || x.size != y.size || x.compressed_size != y.compressed_size
            || x.uncompressed_offset != y.uncompressed_offset || x.uncompressed_size != y.uncompressed_size
        {
            chunk_field_diffs += 1;
        }
    }
    println!("chunk_field_diffs_in_common_range: {chunk_field_diffs}");

    let (ra, rb) = (a.resources(), b.resources());
    println!("resources_len: {} vs {}", ra.len(), rb.len());
    let keys_a: Vec<_> = ra.keys().to_vec();
    let keys_b: Vec<_> = rb.keys().to_vec();
    println!("resource_key_sequence_eq: {}", keys_a == keys_b);
    let mut offset_diffs = 0usize;
    let mut size_diffs = 0usize;
    let mut first_size_diff: Option<String> = None;
    for k in keys_a.iter() {
        let (x, y) = match (ra.get(k), rb.get(k)) {
            (Some(x), Some(y)) => (x, y),
            _ => continue,
        };
        if x.offset != y.offset {
            offset_diffs += 1;
        }
        if x.size != y.size {
            size_diffs += 1;
            if first_size_diff.is_none() {
                first_size_diff = Some(format!("{k}: A(size={}) B(size={})", x.size, y.size));
            }
        }
    }
    println!("resource_offset_diffs: {offset_diffs}");
    println!("resource_size_diffs: {size_diffs}");
    if let Some(d) = first_size_diff {
        println!("first_resource_size_diff: {d}");
    }

    let (ca, cb) = (a.contents(), b.contents());
    println!("contents_len: {} vs {}", ca.len(), cb.len());
    let ckeys_a: Vec<_> = ca.keys().to_vec();
    let ckeys_b: Vec<_> = cb.keys().to_vec();
    println!("content_key_sequence_eq: {}", ckeys_a == ckeys_b);
    let mut cval_diffs = 0usize;
    for k in ckeys_a.iter() {
        if let (Some(x), Some(y)) = (ca.get(k), cb.get(k)) {
            if x.offset != y.offset || x.flags != y.flags || x.container_index != y.container_index {
                cval_diffs += 1;
            }
        }
    }
    println!("content_value_diffs: {cval_diffs}");

    let (ba, bb) = (a.resource_bundles(), b.resource_bundles());
    println!("bundles_len: {} vs {}", ba.len(), bb.len());
    let bkeys_a: Vec<_> = ba.keys().to_vec();
    let bkeys_b: Vec<_> = bb.keys().to_vec();
    println!("bundle_key_sequence_eq: {}", bkeys_a == bkeys_b);
    let mut b_hash_diffs = 0usize;
    let mut b_index_diffs = 0usize;
    let mut b_count_diffs = 0usize;
    for k in bkeys_a.iter() {
        if let (Some(x), Some(y)) = (ba.get(k), bb.get(k)) {
            if x.internal_hash != y.internal_hash { b_hash_diffs += 1; }
            if x.index != y.index { b_index_diffs += 1; }
            if x.count != y.count { b_count_diffs += 1; }
        }
    }
    println!("bundle_internal_hash_diffs: {b_hash_diffs}");
    println!("bundle_index_diffs: {b_index_diffs}");
    println!("bundle_count_diffs: {b_count_diffs}");
    Ok(())
}

// ---------------------------------------------------------------- bytecmp

fn bytecmp(path_a: &str, path_b: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut fa = File::open(path_a)?;
    let mut fb = File::open(path_b)?;
    let len_a = fa.metadata()?.len();
    let len_b = fb.metadata()?.len();
    println!("len_a: {len_a}");
    println!("len_b: {len_b}");
    const BUF: usize = 8 * 1024 * 1024;
    let mut ba = vec![0u8; BUF];
    let mut bb = vec![0u8; BUF];
    let mut pos = 0u64;
    let mut diff_bytes = 0u64;
    let mut first_diff: Option<u64> = None;
    let mut ranges: Vec<(u64, u64)> = Vec::new();
    let mut open_range: Option<u64> = None;
    loop {
        let na = read_full(&mut fa, &mut ba)?;
        let nb = read_full(&mut fb, &mut bb)?;
        if na == 0 && nb == 0 {
            break;
        }
        let n = na.max(nb);
        for i in 0..n {
            let x = if i < na { ba[i] } else { 0 };
            let y = if i < nb { bb[i] } else { 0 };
            let present_both = i < na && i < nb;
            if !present_both || x != y {
                diff_bytes += 1;
                let at = pos + i as u64;
                if first_diff.is_none() {
                    first_diff = Some(at);
                }
                if open_range.is_none() {
                    open_range = Some(at);
                }
            } else if let Some(start) = open_range.take() {
                ranges.push((start, pos + i as u64));
            }
        }
        pos += n as u64;
        if na == 0 || nb == 0 {
            // one side exhausted; the remainder is entirely "differing"
            // and has been accounted byte-by-byte above only for the
            // overlapping window; account the tail now.
            let remaining = len_a.max(len_b) - pos;
            if remaining > 0 {
                diff_bytes += remaining;
                if first_diff.is_none() {
                    first_diff = Some(pos);
                }
                if open_range.is_none() {
                    open_range = Some(pos);
                }
                ranges.push((open_range.take().unwrap(), len_a.max(len_b)));
            }
            break;
        }
    }
    if let Some(start) = open_range.take() {
        ranges.push((start, pos));
    }
    println!("differing_bytes: {diff_bytes}");
    println!("first_diff_offset: {:?}", first_diff);
    println!("diff_ranges: {}", ranges.len());
    for (i, (s, e)) in ranges.iter().enumerate() {
        if i >= 25 {
            println!("  ... ({} more ranges)", ranges.len() - 25);
            break;
        }
        println!("  range[{i}]: {s}..{e} ({} bytes)", e - s);
    }
    Ok(())
}

fn read_full(f: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0usize;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            m => n += m,
        }
    }
    Ok(n)
}

// ---------------------------------------------------------------- analyze

fn analyze(dir: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file = KFCFile::from_path(Path::new(dir).join(format!("{name}.kfc")), false)?;
    let reader = KFCReader::new(dir, name)?;
    let chunks = file.resource_chunks().to_vec();
    let total_uncomp: u64 = chunks.iter().map(|c| c.uncompressed_size).sum();
    let total_comp: u64 = chunks.iter().map(|c| c.compressed_size).sum();
    println!("version_tag: {}", file.game_version());
    println!("chunks: {}", chunks.len());
    println!("chunks_sum_uncompressed: {total_uncomp}");
    println!("chunks_sum_compressed_frames: {total_comp}");
    println!("resources: {}", file.resources().len());

    // --- layout: resources in stream-offset order, gap census
    let mut placed: Vec<(u64, u64, ResourceId)> = Vec::with_capacity(file.resources().len());
    for (id, entry) in file.resources().iter() {
        let actual = reader.resource_size(id).unwrap_or(entry.size);
        placed.push((entry.offset, actual, *id));
    }
    placed.sort_by_key(|(off, _, _)| *off);
    let mut gaps: Vec<(u64, u64)> = Vec::new(); // (start, len) in uncompressed stream
    let mut expected = 0u64;
    let mut overlaps = 0usize;
    for (off, size, _) in &placed {
        if *off < expected {
            overlaps += 1;
        } else if *off > expected {
            gaps.push((expected, off - expected));
        }
        expected = expected.max(off + size);
    }
    if expected < total_uncomp {
        gaps.push((expected, total_uncomp - expected));
    }
    let gap_total: u64 = gaps.iter().map(|(_, l)| l).sum();
    println!("layout_overlaps: {overlaps}");
    println!("layout_gap_regions: {}", gaps.len());
    println!("layout_gap_bytes_total: {gap_total}");
    println!("layout_covered_end: {expected}");
    let mut hist: BTreeMap<u64, usize> = BTreeMap::new();
    for (_, l) in &gaps {
        *hist.entry(*l).or_default() += 1;
    }
    let mut hist_sorted: Vec<(u64, usize)> = hist.into_iter().collect();
    hist_sorted.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (len, count) in hist_sorted.iter().take(12) {
        println!("gap_len {len}: x{count}");
    }
    if let Some(max) = gaps.iter().map(|(_, l)| l).max() {
        println!("gap_len_max: {max}");
    }
    // gap placement relative to chunk boundaries
    let mut gap_in_chunk = 0u64;
    let mut gap_crossing = 0u64;
    for (start, len) in &gaps {
        let end = start + len;
        let crossing = chunks.iter().any(|c| {
            let cs = c.uncompressed_offset;
            let ce = cs + c.uncompressed_size;
            *start < ce && end > cs && !(*start >= cs && end <= ce)
        });
        if crossing {
            gap_crossing += len;
        } else {
            gap_in_chunk += len;
        }
    }
    println!("gap_bytes_inside_one_chunk: {gap_in_chunk}");
    println!("gap_bytes_crossing_chunk_boundary: {gap_crossing}");

    // --- per-chunk: decompress, check gap bytes are zero, recompress
    let res_path = Path::new(dir).join(format!("{name}.kfc_resources"));
    let stream = std::fs::read(&res_path)?;
    println!("stream_len: {}", stream.len());
    // resource-covered intervals for zero-check masking
    let mut covered: Vec<(u64, u64)> = placed.iter().map(|(o, s, _)| (*o, o + s)).collect();
    covered.sort();
    let mut ident = 0usize;
    let mut diff = 0usize;
    let mut ident9 = 0usize;
    let mut diff9 = 0usize;
    let mut gap_nonzero_bytes = 0u64;
    let mut first_gap_nonzero: Option<u64> = None;
    for (i, ch) in chunks.iter().enumerate() {
        let frame = &stream[ch.offset as usize..(ch.offset + ch.compressed_size) as usize];
        let raw = zstd::bulk::decompress(frame, ch.uncompressed_size as usize)?;
        assert_eq!(raw.len() as u64, ch.uncompressed_size);
        // zero-check: bytes of this chunk not covered by any resource
        let base = ch.uncompressed_offset;
        let cend = base + ch.uncompressed_size;
        let mut cursor = base;
        for (s, e) in covered.iter().filter(|(s, e)| *e > base && *s < cend) {
            let s = (*s).max(base);
            let e = (*e).min(cend);
            if s > cursor {
                for (j, &b) in raw[(cursor - base) as usize..(s - base) as usize].iter().enumerate() {
                    if b != 0 {
                        gap_nonzero_bytes += 1;
                        if first_gap_nonzero.is_none() {
                            first_gap_nonzero = Some(cursor + j as u64);
                        }
                    }
                }
            }
            cursor = cursor.max(e);
        }
        if cursor < cend {
            for (j, &b) in raw[(cursor - base) as usize..].iter().enumerate() {
                if b != 0 {
                    gap_nonzero_bytes += 1;
                    if first_gap_nonzero.is_none() {
                        first_gap_nonzero = Some(cursor + j as u64);
                    }
                }
            }
        }
        // recompress identity at level 0, the writer's own call shape
        let mut out: Vec<u8> = Vec::new();
        zstd::stream::copy_encode(&mut &raw[..], &mut out, 0)?;
        if out == frame {
            ident += 1;
        } else {
            diff += 1;
            if diff <= 3 {
                println!("chunk {i}: recompressed differs ({} vs {} bytes)", out.len(), frame.len());
            }
        }
        // and at level 9, the level the probe found in the game's frames
        let mut out9: Vec<u8> = Vec::new();
        zstd::stream::copy_encode(&mut &raw[..], &mut out9, 9)?;
        if out9 == frame {
            ident9 += 1;
        } else {
            diff9 += 1;
            if diff9 <= 3 {
                println!("chunk {i}: level-9 recompressed differs ({} vs {} bytes)", out9.len(), frame.len());
            }
        }
    }
    println!("gap_nonzero_bytes: {gap_nonzero_bytes}");
    println!("gap_first_nonzero_offset: {:?}", first_gap_nonzero);
    println!("zstd_recompress_identical_chunks: {ident}");
    println!("zstd_recompress_differing_chunks: {diff}");
    println!("zstd_level9_identical_chunks: {ident9}");
    println!("zstd_level9_differing_chunks: {diff9}");

    // --- bundle cross-check: bundle count vs per-type resource count
    let mut per_type: HashMap<u32, u64> = HashMap::new();
    for id in file.resources().keys() {
        *per_type.entry(id.type_hash()).or_default() += 1;
    }
    let mut count_mismatch = 0usize;
    for (th, bundle) in file.resource_bundles().iter() {
        let actual = per_type.get(th).copied().unwrap_or(0);
        if actual != bundle.count as u64 {
            count_mismatch += 1;
            println!("bundle {th:08x}: count {} vs per-type {}", bundle.count, actual);
        }
    }
    println!("bundle_count_mismatches: {count_mismatch}");
    println!("bundles: {}", file.resource_bundles().len());
    // resource_indices sanity: length equals resource count
    println!("contents: {}", file.contents().len());
    println!("containers_records: {}", file.containers().len());
    Ok(())
}

// ---------------------------------------------------------------- probe

fn probe(dir: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file = KFCFile::from_path(Path::new(dir).join(format!("{name}.kfc")), false)?;
    let reader = KFCReader::new(dir, name)?;
    let chunks = file.resource_chunks().to_vec();
    let total_uncomp: u64 = chunks.iter().map(|c| c.uncompressed_size).sum();

    let mut placed: Vec<(u64, u64, ResourceId)> = Vec::with_capacity(file.resources().len());
    for (id, entry) in file.resources().iter() {
        let actual = reader.resource_size(id).unwrap_or(entry.size);
        placed.push((entry.offset, actual, *id));
    }
    placed.sort_by_key(|(off, _, _)| *off);
    // gap regions with neighbor context
    struct Gap {
        start: u64,
        len: u64,
        prev: Option<usize>,
        nonzero: u64,
    }
    let mut gaps: Vec<Gap> = Vec::new();
    let mut expected = 0u64;
    for (idx, (off, size, _)) in placed.iter().enumerate() {
        if *off > expected {
            gaps.push(Gap {
                start: expected,
                len: off - expected,
                prev: if idx == 0 { None } else { Some(idx - 1) },
                nonzero: 0,
            });
        }
        expected = expected.max(off + size);
    }
    if expected < total_uncomp {
        gaps.push(Gap { start: expected, len: total_uncomp - expected, prev: Some(placed.len() - 1), nonzero: 0 });
    }
    let small_gap_bytes: u64 = gaps.iter().filter(|g| g.len <= 15).map(|g| g.len).sum();
    let big_gap_bytes: u64 = gaps.iter().filter(|g| g.len > 15).map(|g| g.len).sum();
    println!("gap_regions_le15: {} bytes {small_gap_bytes}", gaps.iter().filter(|g| g.len <= 15).count());
    println!("gap_regions_gt15: {} bytes {big_gap_bytes}", gaps.iter().filter(|g| g.len > 15).count());

    let res_path = Path::new(dir).join(format!("{name}.kfc_resources"));
    let stream = std::fs::read(&res_path)?;
    for ch in chunks.iter() {
        let frame = &stream[ch.offset as usize..(ch.offset + ch.compressed_size) as usize];
        let raw = zstd::bulk::decompress(frame, ch.uncompressed_size as usize)?;
        let base = ch.uncompressed_offset;
        let cend = base + ch.uncompressed_size;
        for g in gaps.iter_mut() {
            let gs = g.start.max(base);
            let ge = (g.start + g.len).min(cend);
            if gs < ge {
                g.nonzero += raw[(gs - base) as usize..(ge - base) as usize]
                    .iter()
                    .filter(|&&b| b != 0)
                    .count() as u64;
            }
        }
    }
    let mut by_nonzero: Vec<&Gap> = gaps.iter().filter(|g| g.nonzero > 0).collect();
    by_nonzero.sort_by_key(|g| std::cmp::Reverse(g.nonzero));
    println!("gap_regions_with_nonzero: {}", by_nonzero.len());
    for g in by_nonzero.iter().take(15) {
        let (prev_desc, next_desc) = match g.prev {
            Some(pi) => {
                let (po, ps, pid) = &placed[pi];
                let next = placed.get(pi + 1).map(|(no, ns, nid)| format!("next {nid} off={no} size={ns}"));
                (format!("prev {pid} off={po} size={ps}"), next.unwrap_or_else(|| "next <end>".to_string()))
            }
            None => ("prev <start>".to_string(), String::new()),
        };
        println!(
            "gap start={} len={} nonzero={} | {prev_desc} | {next_desc}",
            g.start, g.len, g.nonzero
        );
    }

    // orphan near-duplicate check: the two largest gap regions are both
    // ~927.5 KB; if they are two versions of one payload, the
    // superseded-copy theory of the orphan regions gains support.
    let mut big: Vec<&Gap> = gaps.iter().filter(|g| g.len > 100_000).collect();
    big.sort_by_key(|g| std::cmp::Reverse(g.len));
    if big.len() >= 2 {
        // decompress the whole stream once, chunk by chunk, into a buffer
        let mut full: Vec<u8> = Vec::with_capacity(total_uncomp as usize);
        for ch in chunks.iter() {
            let frame = &stream[ch.offset as usize..(ch.offset + ch.compressed_size) as usize];
            let raw = zstd::bulk::decompress(frame, ch.uncompressed_size as usize)?;
            full.extend_from_slice(&raw);
        }
        let (a, b) = (big[0], big[1]);
        let ra = &full[a.start as usize..(a.start + a.len) as usize];
        let rb = &full[b.start as usize..(b.start + b.len) as usize];
        let n = ra.len().min(rb.len());
        let d = ra[..n].iter().zip(&rb[..n]).filter(|(x, y)| x != y).count();
        println!(
            "orphan_pair_largest: starts {} ({}) vs {} ({}): compared {n} bytes, differing {d}",
            a.start, a.len, b.start, b.len
        );
        // shared-prefix length as a second similarity signal
        let prefix = ra.iter().zip(rb.iter()).take_while(|(x, y)| x == y).count();
        println!("orphan_pair_shared_prefix: {prefix}");
    }

    // which chunks does level 9 fail on, and does anything else fit them?
    for (ci, ch) in chunks.iter().enumerate() {
        let frame = &stream[ch.offset as usize..(ch.offset + ch.compressed_size) as usize];
        let raw = zstd::bulk::decompress(frame, ch.uncompressed_size as usize)?;
        let mut out9: Vec<u8> = Vec::new();
        zstd::stream::copy_encode(&mut &raw[..], &mut out9, 9)?;
        if out9 != frame {
            println!(
                "level9_diff_chunk {ci}: uncompressed_size={} game_frame={} level9={} frame_head={:02x?}",
                ch.uncompressed_size,
                frame.len(),
                out9.len(),
                &frame[..8.min(frame.len())]
            );
            for level in 1..=19 {
                if level == 9 {
                    continue;
                }
                let mut out: Vec<u8> = Vec::new();
                zstd::stream::copy_encode(&mut &raw[..], &mut out, level)?;
                if out == frame {
                    println!("level9_diff_chunk {ci}: level {level} reproduces it EXACTLY");
                }
            }
            println!("level9_diff_chunk {ci}: sweep 1..=19 done");
            let bulk = zstd::bulk::compress(&raw, 9)?;
            if bulk == frame {
                println!("level9_diff_chunk {ci}: bulk one-shot level 9 reproduces it EXACTLY");
            }
        }
    }

    // zstd level sweep on chunks 0 and 1 + frame header comparison
    for ci in [0usize, 1] {
        let ch = &chunks[ci];
        let frame = &stream[ch.offset as usize..(ch.offset + ch.compressed_size) as usize];
        let raw = zstd::bulk::decompress(frame, ch.uncompressed_size as usize)?;
        print!("chunk {ci} game_frame_head:");
        for b in &frame[..16.min(frame.len())] {
            print!(" {b:02x}");
        }
        println!(" ({} bytes)", frame.len());
        for level in [1, 2, 3, 4, 5, 7, 9, 12, 15, 19] {
            let mut out: Vec<u8> = Vec::new();
            zstd::stream::copy_encode(&mut &raw[..], &mut out, level)?;
            if out == frame {
                println!("chunk {ci}: level {level} reproduces the game frame EXACTLY");
            } else {
                println!("chunk {ci}: level {level} -> {} bytes (game {})", out.len(), frame.len());
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- pack

struct ManifestEntry {
    guid: String,
    id: ResourceId,
    stored_size: u64,
    size: u64,
    fingerprint: u64,
    file: String,
}

fn load_manifest(extract_dir: &Path) -> Result<Vec<ManifestEntry>, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(extract_dir.join("_manifest.json"))?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let arr = value.as_array().ok_or("manifest is not a JSON array")?;
    let mut entries = Vec::with_capacity(arr.len());
    for item in arr {
        let guid = item["guid"].as_str().ok_or("manifest entry missing guid")?.to_string();
        let type_hash = u32::from_str_radix(item["type_hash"].as_str().ok_or("bad type_hash")?, 16)?;
        let part = item["part"].as_u64().ok_or("bad part")? as u32;
        let id = ResourceId::parse(&guid, type_hash, part).ok_or("unparseable guid in manifest")?;
        entries.push(ManifestEntry {
            guid,
            id,
            stored_size: item["stored_size"].as_u64().ok_or("bad stored_size")?,
            size: item["size"].as_u64().ok_or("bad size")?,
            fingerprint: u64::from_str_radix(item["fingerprint"].as_str().ok_or("bad fingerprint")?, 16)?,
            file: item["file"].as_str().ok_or("bad file")?.to_string(),
        });
    }
    Ok(entries)
}

fn pack(
    extract_dir: &Path,
    reference: &Path,
    output_dir: &Path,
    types: Option<&Path>,
    baseline: Option<&Path>,
    allow_modified: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // resolve reference (dir or .kfc file) to (dir, name)
    let (ref_dir, name): (PathBuf, String) = if reference.is_file() {
        (
            reference.parent().ok_or("reference file has no parent")?.to_path_buf(),
            reference.file_stem().ok_or("reference file has no stem")?.to_string_lossy().to_string(),
        )
    } else {
        let candidate = reference.join("enshrouded.kfc");
        if candidate.exists() {
            (reference.to_path_buf(), "enshrouded".to_string())
        } else {
            let mut found = None;
            for e in std::fs::read_dir(reference)? {
                let e = e?;
                let p = e.path();
                if p.extension().map(|x| x == "kfc").unwrap_or(false) {
                    found = Some((reference.to_path_buf(), p.file_stem().unwrap().to_string_lossy().to_string()));
                    break;
                }
            }
            found.ok_or("no .kfc found in reference dir")?
        }
    };
    println!("reference: {} (name={name})", ref_dir.display());

    let entries = load_manifest(extract_dir)?;
    println!("manifest_entries: {}", entries.len());

    // A0 baseline as a multiset of audit-format lines
    let mut baseline_set: HashMap<String, u32> = HashMap::new();
    if let Some(bp) = baseline {
        let text = std::fs::read_to_string(bp)?;
        for line in text.lines() {
            if !line.is_empty() {
                *baseline_set.entry(line.to_string()).or_default() += 1;
            }
        }
        println!("baseline_lines: {}", baseline_set.values().sum::<u32>());
    }

    // ---- pass 1: the folder proves itself, before a byte is written
    let mut modified: Vec<String> = Vec::new();
    let mut total_bytes = 0u64;
    for (i, e) in entries.iter().enumerate() {
        let path = extract_dir.join(&e.file);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(err) => {
                eprintln!("FATAL: missing/unreadable file for {} ({}): {err}", e.guid, e.file);
                std::process::exit(1);
            }
        };
        if bytes.len() as u64 != e.size {
            eprintln!(
                "FATAL: length mismatch for {}: file has {} bytes, manifest size {}",
                e.guid,
                bytes.len(),
                e.size
            );
            std::process::exit(1);
        }
        total_bytes += bytes.len() as u64;
        let fp = fnv1a64(&[&e.size.to_le_bytes(), &bytes]);
        let mut bad = false;
        if fp != e.fingerprint {
            bad = true;
        }
        if baseline.is_some() {
            let line = format!(
                "{} {:08x} {} {} {:016x}",
                e.guid,
                e.id.type_hash(),
                e.stored_size,
                e.size,
                fp
            );
            match baseline_set.get_mut(&line) {
                Some(c) if *c > 0 => {
                    *c -= 1;
                }
                _ => {
                    bad = true;
                }
            }
        }
        if bad {
            modified.push(format!("{} ({})", e.guid, e.file));
        }
        if (i + 1) % 20000 == 0 {
            println!("verify: {}/{}", i + 1, entries.len());
        }
    }
    println!("verified_bytes: {total_bytes}");
    println!("modified_entries: {}", modified.len());
    for m in &modified {
        println!("modified: {m}");
    }
    if baseline.is_some() {
        // A content edit leaves its original baseline line unconsumed, so
        // each flagged modification explains one leftover. Leftovers
        // beyond the flagged modifications mean baseline entries with no
        // counterpart in the folder at all: the folder is incomplete,
        // which is fatal even with --allow-modified.
        let leftover: u32 = baseline_set.values().sum();
        if leftover as usize > modified.len() {
            eprintln!(
                "FATAL: {leftover} baseline entries have no counterpart in the folder, only {} explained by flagged modifications (folder is incomplete)",
                modified.len()
            );
            std::process::exit(1);
        }
        println!("baseline_leftover_explained_by_modifications: {leftover}");
    }
    if !modified.is_empty() && !allow_modified {
        eprintln!("ABORT: {} fingerprint mismatches; refusing to pack (use --allow-modified to accept content edits)", modified.len());
        std::process::exit(1);
    }

    // ---- pass 2: copy the reference pair, then write from the folder
    std::fs::create_dir_all(output_dir)?;
    for entry in std::fs::read_dir(&ref_dir)? {
        let entry = entry?;
        let fname = entry.file_name().to_string_lossy().to_string();
        if fname == format!("{name}.kfc")
            || fname == format!("{name}.kfc_resources")
            || (fname.starts_with(&format!("{name}_")) && fname.ends_with(".dat"))
        {
            std::fs::copy(entry.path(), output_dir.join(&fname))?;
            println!("copied reference file: {fname}");
        }
    }

    let registry_path: PathBuf = match types {
        Some(p) => p.to_path_buf(),
        None => {
            let p = ref_dir.join(".cache").join("types.json");
            if p.exists() {
                p
            } else {
                eprintln!("no --types given and {} not found", p.display());
                std::process::exit(1);
            }
        }
    };
    let registry_file = File::open(&registry_path)?;
    let registry: TypeRegistry = serde_json::from_reader(std::io::BufReader::new(registry_file))?;
    println!("registry_types: {}", registry.len());
    let registry = Rc::new(registry);

    let ref_file = Rc::new(KFCFile::from_path(output_dir.join(format!("{name}.kfc")), false)?);
    println!("ref_resources: {}", ref_file.resources().len());

    let mut writer = KFCWriter::new_incremental_with_options(
        output_dir,
        &name,
        ref_file,
        registry,
        KFCWriteOptions {
            overwrite_containers: true,
            ..Default::default()
        },
    )?;

    for (i, e) in entries.iter().enumerate() {
        let bytes = std::fs::read(extract_dir.join(&e.file))?;
        writer.write_resource(&e.id, &bytes)?;
        if (i + 1) % 20000 == 0 {
            println!("write: {}/{}", i + 1, entries.len());
        }
    }
    writer.finalize()?;
    println!("finalize: OK");
    println!("out_kfc_len: {}", std::fs::metadata(output_dir.join(format!("{name}.kfc")))?.len());
    println!("out_resources_len: {}", std::fs::metadata(output_dir.join(format!("{name}.kfc_resources")))?.len());
    Ok(())
}

// ---------------------------------------------------------------- main

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("fps") => fps(&args[2], &args[3], &args[4]),
        Some("cmpdir") => cmpdir(&args[2], &args[3], &args[4]),
        Some("bytecmp") => bytecmp(&args[2], &args[3]),
        Some("analyze") => analyze(&args[2], &args[3]),
        Some("probe") => probe(&args[2], &args[3]),
        Some("pack") => {
            let mut types: Option<PathBuf> = None;
            let mut baseline: Option<PathBuf> = None;
            let mut allow_modified = false;
            let mut i = 5;
            while i < args.len() {
                match args[i].as_str() {
                    "--types" => {
                        types = Some(PathBuf::from(&args[i + 1]));
                        i += 2;
                    }
                    "--baseline" => {
                        baseline = Some(PathBuf::from(&args[i + 1]));
                        i += 2;
                    }
                    "--allow-modified" => {
                        allow_modified = true;
                        i += 1;
                    }
                    other => {
                        eprintln!("unknown pack argument: {other}");
                        std::process::exit(2);
                    }
                }
            }
            pack(
                Path::new(&args[2]),
                Path::new(&args[3]),
                Path::new(&args[4]),
                types.as_deref(),
                baseline.as_deref(),
                allow_modified,
            )
        }
        _ => {
            eprintln!("usage: fps|cmpdir|bytecmp|analyze|pack ...");
            std::process::exit(2);
        }
    }
}
