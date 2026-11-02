// kfc4-pack-standalone — KFC4 project: rebuild the game container
// byte-exactly from an A2.5 full-fidelity extraction folder ALONE.
//
// Subcommands:
//   pack    <extract-full-dir> <output-dir> [--allow-modified]
//           Standalone rebuild. The ONLY input is the extraction
//           folder; this code path never opens the pristine game
//           files (it takes no reference/pristine argument at all).
//           Fail-closed pre-flight, before the output directory is
//           created or a single output byte is written:
//             - every manifest entry's file exists, its length equals
//               the manifest size, and its FNV fingerprint matches the
//               manifest (a mismatch names the GUID and aborts unless
//               --allow-modified; a length mismatch is always fatal —
//               the recorded stream geometry cannot honor it);
//             - every directory record's bytes hash-match
//               _directory.json (always fatal on mismatch);
//             - every orphan gap's bytes hash-match orphans.json;
//             - chunk geometry is self-consistent and cross-checked
//               against resource_locations.bin;
//             - stream coverage accounting: every uncompressed stream
//               byte is a resource byte, an orphan byte, or a <=15 B
//               zero small gap; overlaps are fatal.
//           Assembly: resources and orphan bytes are placed at their
//           recorded stream offsets in a zero-initialized stream;
//           chunks are re-encoded at their recorded zstd levels and
//           laid into their recorded 4096-byte slots; the .kfc is the
//           verbatim directory records placed at their recorded file
//           offsets in a zero-initialized file. If an --allow-modified
//           edit changes a chunk frame's length, that chunk's
//           compressed_size in the chunk table and the
//           resource_locations compressed aggregate are patched (and
//           reported) so the output stays self-consistent.
//           No .dat files are emitted — their payloads were never
//           captured (dat_files.json: payload_captured: false).
//   fps     <dir> <name> <fp_out>
//           Fingerprint every resource of a .kfc pair (A0 audit
//           format) — parse validation of a built pair, using
//           kfc-base. Comparison tooling only; pack never calls it.
//   bytecmp <fileA> <fileB>
//           Streaming byte comparison: lengths, differing byte count,
//           coalesced differing ranges. Comparison tooling only.
//
// The fingerprint function is the A0 audit's, verbatim: FNV-1a 64-bit
// over [actual_size as LE u64 || decompressed bytes].

use std::{
    collections::HashSet,
    fs::File,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use kfc_base::{
    container::{KFCFile, KFCReader},
    guid::ResourceId,
};
use sha2::{Digest, Sha256};

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

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn fatal(msg: &str) -> ! {
    eprintln!("FATAL: {msg}");
    std::process::exit(1);
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

// ---------------------------------------------------------------- bytecmp

fn read_full(f: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0usize;
    while n < buf.len() {
        match f.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(m) => n += m,
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

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

// ---------------------------------------------------------------- pack (standalone)

struct DirRecord {
    name: String,
    file: String,
    file_offset: u64,
    length: u64,
    sha256: String,
    bytes: Vec<u8>,
}

struct ChunkInfo {
    index: usize,
    offset: u64,            // compressed-stream offset of the slot
    size: u64,              // slot size (4096-aligned)
    compressed_size: u64,   // recorded frame length
    uncompressed_offset: u64,
    uncompressed_size: u64,
    zstd_level: i32,
}

struct OrphanGap {
    index: usize,
    stream_offset: u64,
    length: u64,
    bin_offset: u64,
    sha256: String,
}

struct ManifestEntry {
    guid: String,
    stored_size: u64,
    size: u64,
    offset: u64,
    fingerprint: u64,
    file: String,
}

fn load_json(path: &Path) -> serde_json::Value {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => fatal(&format!("cannot read {}: {e}", path.display())),
    };
    match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => fatal(&format!("cannot parse {}: {e}", path.display())),
    }
}

fn j_u64(v: &serde_json::Value, what: &str) -> u64 {
    v.as_u64().unwrap_or_else(|| fatal(&format!("bad/missing u64: {what}")))
}

fn j_str<'a>(v: &'a serde_json::Value, what: &str) -> &'a str {
    v.as_str().unwrap_or_else(|| fatal(&format!("bad/missing string: {what}")))
}

fn pack(extract_dir: &Path, output_dir: &Path, allow_modified: bool) {
    let dir_dir = extract_dir.join("directory");

    // ---------- load folder metadata
    let directory = load_json(&dir_dir.join("_directory.json"));
    let header_size_field = j_u64(&directory["header_size_field"], "header_size_field");
    let mut records: Vec<DirRecord> = Vec::new();
    let mut orphans_record: Option<(u64, String)> = None;
    for r in directory["records"].as_array().unwrap_or_else(|| fatal("records not an array")) {
        let name = j_str(&r["name"], "record name").to_string();
        let file = j_str(&r["file"], "record file").to_string();
        let length = j_u64(&r["length"], "record length");
        let sha256 = j_str(&r["sha256"], "record sha256").to_string();
        if r["file_offset"].is_null() {
            orphans_record = Some((length, sha256));
        } else {
            records.push(DirRecord {
                name,
                file,
                file_offset: j_u64(&r["file_offset"], "record file_offset"),
                length,
                sha256,
                bytes: Vec::new(),
            });
        }
    }
    records.sort_by_key(|r| r.file_offset);
    println!("directory_records: {}", records.len());
    println!("header_size_field: {header_size_field}");

    let chunks_v = load_json(&dir_dir.join("chunks.json"));
    let chunk_count = j_u64(&chunks_v["chunk_count"], "chunk_count") as usize;
    let mut chunks: Vec<ChunkInfo> = Vec::with_capacity(chunk_count);
    for c in chunks_v["chunks"].as_array().unwrap_or_else(|| fatal("chunks not an array")) {
        chunks.push(ChunkInfo {
            index: j_u64(&c["index"], "chunk index") as usize,
            offset: j_u64(&c["offset"], "chunk offset"),
            size: j_u64(&c["size"], "chunk size"),
            compressed_size: j_u64(&c["compressed_size"], "chunk compressed_size"),
            uncompressed_offset: j_u64(&c["uncompressed_offset"], "chunk uncompressed_offset"),
            uncompressed_size: j_u64(&c["uncompressed_size"], "chunk uncompressed_size"),
            zstd_level: c["zstd_level"].as_i64().unwrap_or_else(|| fatal("bad zstd_level")) as i32,
        });
    }

    let orphans_v = load_json(&dir_dir.join("orphans").join("orphans.json"));
    let recorded_stream_len = j_u64(&orphans_v["total_uncompressed_stream_bytes"], "total_uncompressed_stream_bytes");
    let mut gaps: Vec<OrphanGap> = Vec::new();
    for g in orphans_v["large_gaps"]["gaps"].as_array().unwrap_or_else(|| fatal("large_gaps.gaps not an array")) {
        gaps.push(OrphanGap {
            index: j_u64(&g["index"], "gap index") as usize,
            stream_offset: j_u64(&g["stream_offset"], "gap stream_offset"),
            length: j_u64(&g["length"], "gap length"),
            bin_offset: j_u64(&g["orphans_bin_offset"], "gap orphans_bin_offset"),
            sha256: j_str(&g["sha256"], "gap sha256").to_string(),
        });
    }
    let small_gap_count_recorded = j_u64(&orphans_v["small_gaps"]["count"], "small_gaps.count");
    let small_gap_bytes_recorded = j_u64(&orphans_v["small_gaps"]["total_bytes"], "small_gaps.total_bytes");

    let manifest_v = load_json(&extract_dir.join("_manifest.json"));
    let mut entries: Vec<ManifestEntry> = Vec::new();
    for item in manifest_v.as_array().unwrap_or_else(|| fatal("manifest is not a JSON array")) {
        entries.push(ManifestEntry {
            guid: j_str(&item["guid"], "manifest guid").to_string(),
            stored_size: j_u64(&item["stored_size"], "manifest stored_size"),
            size: j_u64(&item["size"], "manifest size"),
            offset: j_u64(&item["offset"], "manifest offset"),
            fingerprint: u64::from_str_radix(j_str(&item["fingerprint"], "manifest fingerprint"), 16)
                .unwrap_or_else(|_| fatal("bad fingerprint hex in manifest")),
            file: j_str(&item["file"], "manifest file").to_string(),
        });
    }
    println!("manifest_entries: {}", entries.len());

    // ---------- pre-flight 1: directory records (hash-verified, always fatal)
    for r in records.iter_mut() {
        let path = extract_dir.join(&r.file);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => fatal(&format!("directory record {} unreadable ({}): {e}", r.name, path.display())),
        };
        if bytes.len() as u64 != r.length {
            fatal(&format!(
                "directory record {} length mismatch: file has {} bytes, _directory.json records {}",
                r.name, bytes.len(), r.length
            ));
        }
        let digest = sha256_hex(&bytes);
        if digest != r.sha256 {
            fatal(&format!(
                "directory record {} sha256 mismatch: computed {digest}, _directory.json records {}",
                r.name, r.sha256
            ));
        }
        r.bytes = bytes;
    }
    println!("preflight_directory_records: {} verified", records.len());

    // ---------- pre-flight 2: orphan bytes (hash-verified, always fatal)
    let (orph_len, orph_sha) = orphans_record.unwrap_or_else(|| fatal("no orphans record in _directory.json"));
    let orphans_bin = match std::fs::read(dir_dir.join("orphans").join("orphans.bin")) {
        Ok(b) => b,
        Err(e) => fatal(&format!("orphans.bin unreadable: {e}")),
    };
    if orphans_bin.len() as u64 != orph_len {
        fatal(&format!("orphans.bin length {} != recorded {orph_len}", orphans_bin.len()));
    }
    let digest = sha256_hex(&orphans_bin);
    if digest != orph_sha {
        fatal(&format!("orphans.bin sha256 mismatch: computed {digest}, recorded {orph_sha}"));
    }
    let mut gap_bytes_total = 0u64;
    for g in &gaps {
        let end = (g.bin_offset + g.length) as usize;
        if end > orphans_bin.len() {
            fatal(&format!("orphan gap {} overruns orphans.bin", g.index));
        }
        let slice = &orphans_bin[g.bin_offset as usize..end];
        let d = sha256_hex(slice);
        if d != g.sha256 {
            fatal(&format!(
                "orphan gap {} (stream offset {}) sha256 mismatch: computed {d}, recorded {}",
                g.index, g.stream_offset, g.sha256
            ));
        }
        gap_bytes_total += g.length;
    }
    if gap_bytes_total != orphans_bin.len() as u64 {
        fatal(&format!(
            "orphan gaps total {gap_bytes_total} != orphans.bin length {}",
            orphans_bin.len()
        ));
    }
    println!("preflight_orphans: {} gaps, {} bytes verified", gaps.len(), gap_bytes_total);

    // ---------- pre-flight 3: chunk geometry + cross-checks
    if chunks.len() != chunk_count {
        fatal(&format!("chunks.json lists {} chunks, chunk_count says {chunk_count}", chunks.len()));
    }
    let mut stream_len = 0u64;
    let mut compressed_len = 0u64;
    let mut frames_sum = 0u64;
    for (i, c) in chunks.iter().enumerate() {
        if c.index != i {
            fatal(&format!("chunk at position {i} has index {}", c.index));
        }
        if c.uncompressed_offset != stream_len {
            fatal(&format!(
                "chunk {i} uncompressed_offset {} != running total {stream_len} (uncompressed ranges do not tile)",
                c.uncompressed_offset
            ));
        }
        if c.offset != compressed_len {
            fatal(&format!(
                "chunk {i} compressed offset {} != running total {compressed_len} (slots do not tile)",
                c.offset
            ));
        }
        if c.compressed_size > c.size {
            fatal(&format!("chunk {i} frame {} exceeds slot {}", c.compressed_size, c.size));
        }
        stream_len += c.uncompressed_size;
        compressed_len += c.size;
        frames_sum += c.compressed_size;
    }
    if stream_len != recorded_stream_len {
        fatal(&format!(
            "chunk uncompressed total {stream_len} != orphans.json total_uncompressed_stream_bytes {recorded_stream_len}"
        ));
    }
    // cross-check against the resource_locations record (u32 x3 LE:
    // uncompressed_size, compressed_size, count)
    let resloc = records
        .iter()
        .find(|r| r.name == "resource_locations")
        .unwrap_or_else(|| fatal("resource_locations record missing"));
    if resloc.bytes.len() != 12 {
        fatal(&format!("resource_locations is {} bytes, expected 12", resloc.bytes.len()));
    }
    let rl_unc = u32::from_le_bytes(resloc.bytes[0..4].try_into().unwrap()) as u64;
    let rl_comp = u32::from_le_bytes(resloc.bytes[4..8].try_into().unwrap()) as u64;
    let rl_count = u32::from_le_bytes(resloc.bytes[8..12].try_into().unwrap()) as u64;
    if rl_unc != stream_len {
        fatal(&format!("resource_locations uncompressed_size {rl_unc} != chunk total {stream_len}"));
    }
    if rl_comp != frames_sum {
        fatal(&format!("resource_locations compressed_size {rl_comp} != frame sum {frames_sum}"));
    }
    if rl_count != entries.len() as u64 {
        fatal(&format!("resource_locations count {rl_count} != manifest entries {}", entries.len()));
    }
    println!("preflight_geometry: stream_len={stream_len} compressed_len={compressed_len} frames_sum={frames_sum}");

    // records must tile [0, header_size_field) without overlap, leaving only gaps
    let mut covered = 0u64;
    let mut prev_end = 0u64;
    for r in &records {
        if r.file_offset < prev_end {
            fatal(&format!("directory record {} overlaps the previous record", r.name));
        }
        covered += r.length;
        prev_end = r.file_offset + r.length;
    }
    if prev_end != header_size_field {
        fatal(&format!("directory records end at {prev_end}, header_size_field is {header_size_field}"));
    }
    let kfc_len = (header_size_field + 4095) / 4096 * 4096;
    let uncovered_in_size = header_size_field - covered;
    println!(
        "kfc_geometry: covered={covered} uncovered_within_size={uncovered_in_size} tail={} kfc_len={kfc_len}",
        kfc_len - header_size_field
    );

    // ---------- pre-flight 4: stream coverage accounting
    // spans: (start, end, kind) kind 0 = resource, 1 = orphan
    let mut spans: Vec<(u64, u64, u8)> = Vec::with_capacity(entries.len() + gaps.len());
    let mut resource_bytes = 0u64;
    for e in &entries {
        spans.push((e.offset, e.offset + e.size, 0));
        resource_bytes += e.size;
    }
    for g in &gaps {
        spans.push((g.stream_offset, g.stream_offset + g.length, 1));
    }
    spans.sort();
    let mut cursor = 0u64;
    let mut small_count = 0u64;
    let mut small_bytes = 0u64;
    for (s, e, _kind) in &spans {
        let (s, e) = (*s, *e);
        if s < cursor {
            fatal(&format!("stream span overlap at offset {s} (coverage cursor at {cursor})"));
        }
        if e > stream_len {
            fatal(&format!("stream span [{s}, {e}) overruns stream length {stream_len}"));
        }
        if s > cursor {
            let gap = s - cursor;
            if gap > 15 {
                fatal(&format!(
                    "unaccounted stream region [{cursor}, {s}) of {gap} bytes: not a resource, not a dumped orphan, too large for a zero small gap"
                ));
            }
            small_count += 1;
            small_bytes += gap;
        }
        cursor = e;
    }
    if cursor < stream_len {
        let gap = stream_len - cursor;
        if gap > 15 {
            fatal(&format!("unaccounted trailing stream region [{cursor}, {stream_len}) of {gap} bytes"));
        }
        small_count += 1;
        small_bytes += gap;
    }
    if resource_bytes + gap_bytes_total + small_bytes != stream_len {
        fatal(&format!(
            "coverage does not close: resources {resource_bytes} + orphans {gap_bytes_total} + small {small_bytes} != stream {stream_len}"
        ));
    }
    if small_count != small_gap_count_recorded || small_bytes != small_gap_bytes_recorded {
        fatal(&format!(
            "small-gap census mismatch: computed {small_count} gaps / {small_bytes} bytes, orphans.json records {small_gap_count_recorded} / {small_gap_bytes_recorded}"
        ));
    }
    println!("coverage: resources={resource_bytes} orphans={gap_bytes_total} small_gaps={small_bytes} ({} gaps) total={stream_len}", small_count);

    // ---------- pre-flight 5: manifest files prove themselves
    let mut modified: Vec<String> = Vec::new();
    for e in &entries {
        let path = extract_dir.join(&e.file);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(err) => fatal(&format!("missing/unreadable file for {} ({}): {err}", e.guid, e.file)),
        };
        if bytes.len() as u64 != e.size {
            fatal(&format!(
                "length mismatch for {}: file has {} bytes, manifest size {} (recorded geometry cannot honor a length change; always fatal)",
                e.guid, bytes.len(), e.size
            ));
        }
        let fp = fnv1a64(&[&e.size.to_le_bytes(), &bytes]);
        if fp != e.fingerprint {
            modified.push(e.guid.clone());
        }
    }
    if !modified.is_empty() {
        for g in &modified {
            eprintln!("modified: {g}");
        }
        if !allow_modified {
            eprintln!(
                "FATAL: {} manifest entr{} fail fingerprint verification; aborting (no output written). Re-run with --allow-modified to build anyway.",
                modified.len(),
                if modified.len() == 1 { "y" } else { "ies" }
            );
            std::process::exit(1);
        }
        println!("preflight_manifest: {} entries verified, {} MODIFIED (allowed by --allow-modified)", entries.len(), modified.len());
    } else {
        println!("preflight_manifest: {} entries verified, 0 modified", entries.len());
    }
    let modified_set: HashSet<&str> = modified.iter().map(|s| s.as_str()).collect();

    // ---------- assembly 1: uncompressed stream
    let mut stream = vec![0u8; stream_len as usize];
    let mut written_resources = 0u64;
    for e in &entries {
        let path = extract_dir.join(&e.file);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(err) => fatal(&format!("missing/unreadable file for {} ({}): {err}", e.guid, e.file)),
        };
        if bytes.len() as u64 != e.size {
            fatal(&format!("length mismatch for {} during assembly", e.guid));
        }
        stream[e.offset as usize..(e.offset + e.size) as usize].copy_from_slice(&bytes);
        written_resources += e.size;
    }
    let mut written_orphans = 0u64;
    for g in &gaps {
        let slice = &orphans_bin[g.bin_offset as usize..(g.bin_offset + g.length) as usize];
        stream[g.stream_offset as usize..(g.stream_offset + g.length) as usize].copy_from_slice(slice);
        written_orphans += g.length;
    }
    if written_resources != resource_bytes || written_orphans != gap_bytes_total {
        fatal("assembly byte totals diverged from coverage accounting");
    }
    println!("assembly_stream: resources={written_resources} orphans={written_orphans} zeros={small_bytes}");

    // ---------- assembly 2: compressed stream
    let mut compressed = vec![0u8; compressed_len as usize];
    let mut frame_patches: Vec<(usize, u64, u64)> = Vec::new(); // (chunk index, recorded, actual)
    let mut frames_written_sum = 0u64;
    for c in &chunks {
        let data = &stream[c.uncompressed_offset as usize..(c.uncompressed_offset + c.uncompressed_size) as usize];
        let mut frame: Vec<u8> = Vec::new();
        if let Err(e) = zstd::stream::copy_encode(Cursor::new(data), &mut frame, c.zstd_level) {
            fatal(&format!("chunk {} zstd encode failed: {e}", c.index));
        }
        if frame.len() as u64 > c.size {
            fatal(&format!(
                "chunk {} re-encoded frame is {} bytes, exceeds its recorded slot {}",
                c.index, frame.len(), c.size
            ));
        }
        if frame.len() as u64 != c.compressed_size {
            frame_patches.push((c.index, c.compressed_size, frame.len() as u64));
        }
        compressed[c.offset as usize..c.offset as usize + frame.len()].copy_from_slice(&frame);
        frames_written_sum += frame.len() as u64;
    }
    if !frame_patches.is_empty() {
        for (idx, rec, act) in &frame_patches {
            eprintln!("frame length change: chunk {idx}: recorded compressed_size {rec} -> actual {act}");
        }
        if !allow_modified {
            fatal("frame lengths diverged from chunks.json with no modified content allowed; encoder/folder inconsistency");
        }
        println!("frame_patches: {} chunk(s) re-sized by the edit; chunk table + aggregate will be patched", frame_patches.len());
    }
    println!("assembly_compressed: {} bytes, frames_sum={frames_written_sum}", compressed.len());

    // ---------- assembly 3: .kfc
    let mut kfc = vec![0u8; kfc_len as usize];
    for r in &records {
        kfc[r.file_offset as usize..(r.file_offset + r.length) as usize].copy_from_slice(&r.bytes);
    }
    // uncovered regions (alignment gaps + tail) — zero by construction; verify + report
    let mut uncovered: Vec<(u64, u64)> = Vec::new();
    {
        let mut cur = 0u64;
        for r in &records {
            if r.file_offset > cur {
                uncovered.push((cur, r.file_offset));
            }
            cur = r.file_offset + r.length;
        }
        if cur < kfc_len {
            uncovered.push((cur, kfc_len));
        }
    }
    let uncovered_total: u64 = uncovered.iter().map(|(s, e)| e - s).sum();
    let expected_uncovered = uncovered_in_size + (kfc_len - header_size_field);
    if uncovered_total != expected_uncovered {
        fatal(&format!("uncovered .kfc bytes {uncovered_total} != expected {expected_uncovered}"));
    }
    for (s, e) in &uncovered {
        if kfc[*s as usize..*e as usize].iter().any(|&b| b != 0) {
            fatal(&format!("uncovered .kfc region [{s}, {e}) is not zero"));
        }
        println!("kfc_uncovered: [{s}, {e}) ({} bytes, zero)", e - s);
    }
    // patch chunk-table compressed_size + aggregate for re-sized frames (edit case)
    if !frame_patches.is_empty() {
        let ct = records.iter().find(|r| r.name == "chunk_table").unwrap();
        let rl = records.iter().find(|r| r.name == "resource_locations").unwrap();
        for (idx, _rec, act) in &frame_patches {
            // chunk_table.bin: u32 count prefix, then 20-byte records;
            // compressed_size is the 3rd u32 of each record.
            let pos = (ct.file_offset + 4 + (*idx as u64) * 20 + 8) as usize;
            kfc[pos..pos + 4].copy_from_slice(&(*act as u32).to_le_bytes());
        }
        let pos = (rl.file_offset + 4) as usize;
        kfc[pos..pos + 4].copy_from_slice(&(frames_written_sum as u32).to_le_bytes());
        println!("kfc_patched: chunk_table compressed_size x{} + resource_locations aggregate -> {frames_written_sum}", frame_patches.len());
    }
    println!("assembly_kfc: {} bytes", kfc.len());

    // ---------- write outputs (only now is anything written)
    if let Err(e) = std::fs::create_dir_all(output_dir) {
        fatal(&format!("cannot create output dir {}: {e}", output_dir.display()));
    }
    let res_path = output_dir.join("enshrouded.kfc_resources");
    let kfc_path = output_dir.join("enshrouded.kfc");
    if let Err(e) = std::fs::write(&res_path, &compressed) {
        fatal(&format!("cannot write {}: {e}", res_path.display()));
    }
    if let Err(e) = std::fs::write(&kfc_path, &kfc) {
        fatal(&format!("cannot write {}: {e}", kfc_path.display()));
    }
    println!("wrote: {} ({} bytes, sha256 {})", res_path.display(), compressed.len(), sha256_hex(&compressed));
    println!("wrote: {} ({} bytes, sha256 {})", kfc_path.display(), kfc.len(), sha256_hex(&kfc));
    if !modified_set.is_empty() {
        println!("note: built from a folder with {} modified resource(s) allowed by --allow-modified", modified_set.len());
    }

    // ---------- the standing caveat: .dat payloads
    let dat_v = load_json(&dir_dir.join("dat_files.json"));
    let containers = dat_v["containers"].as_array().map(|a| a.len()).unwrap_or(0);
    let nonzero = dat_v["containers"]
        .as_array()
        .map(|a| a.iter().filter(|c| c["directory_count"].as_u64().unwrap_or(0) > 0).count())
        .unwrap_or(0);
    println!(
        "dat_caveat: dat_files.json inventories {containers} containers ({nonzero} nonzero), every one payload_captured: false; NO .dat files emitted (payloads were never captured on this VM)"
    );
}

// ---------------------------------------------------------------- main

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("pack") => {
            let extract = PathBuf::from(args.get(2).ok_or("usage: pack <extract-full-dir> <output-dir> [--allow-modified]")?);
            let output = PathBuf::from(args.get(3).ok_or("usage: pack <extract-full-dir> <output-dir> [--allow-modified]")?);
            let allow = args.iter().any(|a| a == "--allow-modified");
            pack(&extract, &output, allow);
            Ok(())
        }
        Some("fps") => fps(
            args.get(2).ok_or("usage: fps <dir> <name> <fp-out>")?,
            args.get(3).ok_or("usage: fps <dir> <name> <fp-out>")?,
            args.get(4).ok_or("usage: fps <dir> <name> <fp-out>")?,
        ),
        Some("bytecmp") => bytecmp(
            args.get(2).ok_or("usage: bytecmp <fileA> <fileB>")?,
            args.get(3).ok_or("usage: bytecmp <fileA> <fileB>")?,
        ),
        _ => {
            eprintln!("usage: pack|fps|bytecmp ...");
            std::process::exit(2);
        }
    }
}
