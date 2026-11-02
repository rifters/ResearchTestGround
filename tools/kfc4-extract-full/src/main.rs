// kfc4-extract-full — KFC4 project phase A2.5: full-fidelity unpack of
// a KFC container to ONE self-contained folder.
//
// Usage:
//   kfc4-extract-full <container.kfc> <output-dir>
//                     [--types <types.json>] [--baseline <fp-pristine.txt>]
//
// Output layout:
//   <output-dir>/resources/<TypeName-or-type_HASH>/<guid>__p<part>.bin
//   <output-dir>/_manifest.json    one entry per resource part, now with
//                                  the absolute uncompressed stream offset
//   <output-dir>/_summary.json     totals + per-type stats + A2.5 stats
//   <output-dir>/directory/        everything the A2 folder lacked:
//     header.bin, version.bin, containers.bin, resource_locations.bin,
//     chunk_table.bin (incl. its u32 count prefix), resource_indices.bin,
//     contents_map.bin, resource_map.bin, bundle_map.bin
//                                  verbatim section slices of the .kfc
//     resource_keys.json           resource key sequence in FILE order
//     chunks.json                  per-chunk geometry + zstd level,
//                                  determined by re-encoding tests
//     chunks_frames/chunk_NNN.zst  original frames for chunks whose
//                                  level no stock level reproduces
//                                  (only written if such a chunk exists)
//     orphans/orphans.bin          data-bearing stream gaps, verbatim
//     orphans/orphans.json         gap index (offset, length, SHA-256)
//     dat_files.json               .dat inventory — PAYLOADS NOT CAPTURED
//     _directory.json              index of the above: offsets, lengths,
//                                  SHA-256s, closing-data totals
//
// Fail-closed throughout: any structural inconsistency, read error, or
// length/fingerprint mismatch aborts with a message naming the resource
// or section. A built-in verification phase re-reads everything from
// disk (fresh header parse, fresh decompression) and re-checks every
// claim before exiting 0.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs::File,
    io::{BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use kfc_base::{
    container::{KFCFile, KFCReader},
    guid::{Guid, ResourceId},
    reflection::{LookupKey, TypeRegistry},
};
use sha2::{Digest, Sha256};

/// Fingerprint: copied verbatim from the A0 audit harness (and A2) so
/// verification against `fp-pristine.txt` is apples-to-apples.
///
/// FNV-1a 64-bit (offset basis 0xcbf29ce484222325, prime 0x100000001b3)
/// over [actual_size as LE u64 || decompressed bytes].
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

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    let digest = h.finalize();
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Make a type name safe as a single directory name (from A2):
/// `keen::VfxMeshResource` -> `keen__VfxMeshResource`.
fn sanitize_dir_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ':' && chars.peek() == Some(&':') {
            chars.next();
            out.push_str("__");
        } else if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            || c.is_control()
        {
            out.push('_');
        } else {
            out.push(c);
        }
    }
    out
}

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn u64le(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

/// One header location, parsed from the raw bytes (the header module of
/// kfc-base is not public; the layout is spec §2: field position 16+8i,
/// stored offset is self-relative).
struct Loc {
    name: &'static str,
    elem: u64, // bytes per element (0 for unused)
    offset: u64,
    count: u64,
}

const LOC_DEFS: [(&str, u64); 16] = [
    ("version", 1),
    ("containers", 16),
    ("unused0", 0),
    ("unused1", 0),
    ("resource_locations", 12),
    ("resource_indices", 4),
    ("content_buckets", 8),
    ("content_keys", 16),
    ("content_values", 16),
    ("resource_buckets", 8),
    ("resource_keys", 32),
    ("resource_values", 8),
    ("resource_bundle_buckets", 8),
    ("resource_bundle_keys", 4),
    ("resource_bundle_values", 12),
    ("resource_chunks", 20),
];

fn parse_header(kfc: &[u8]) -> Result<(u32, Vec<Loc>), String> {
    if kfc.len() < 144 {
        return Err(format!("file too short for a KFC header: {} bytes", kfc.len()));
    }
    let magic = u32le(kfc, 0);
    if magic != 0x3443_464b {
        return Err(format!("bad magic {magic:08x} (expected 0x3443464b 'KFC4')"));
    }
    let size = u32le(kfc, 4);
    let mut locs = Vec::with_capacity(16);
    for (i, &(name, elem)) in LOC_DEFS.iter().enumerate() {
        let pos = 16 + 8 * i;
        let stored = u32le(kfc, pos) as u64;
        let count = u32le(kfc, pos + 4) as u64;
        locs.push(Loc {
            name,
            elem,
            offset: pos as u64 + stored,
            count,
        });
    }
    Ok((size, locs))
}

/// Byte span of a location's section, bounds-checked (fail closed).
fn span<'a>(kfc: &'a [u8], loc: &Loc) -> Result<&'a [u8], String> {
    let len = loc.count * loc.elem;
    let end = loc.offset + len;
    if end > kfc.len() as u64 {
        return Err(format!(
            "section {} out of bounds: offset {} + {} bytes > file length {}",
            loc.name,
            loc.offset,
            len,
            kfc.len()
        ));
    }
    Ok(&kfc[loc.offset as usize..end as usize])
}


fn span_or_fatal<'a>(kfc: &'a [u8], loc: &Loc) -> &'a [u8] {
    match span(kfc, loc) {
        Ok(s) => s,
        Err(e) => fatal(&e),
    }
}

fn fatal(msg: &str) -> ! {
    eprintln!("EXTRACT FAILED: {msg}");
    std::process::exit(1);
}

struct TypeStats {
    type_name: Option<String>,
    count: u64,
    bytes: u64,
}

struct ManifestEntry {
    guid: String,
    type_hash: u32,
    type_name: Option<String>,
    part: u32,
    stored_size: u64,
    size: u64,
    offset: u64,
    fingerprint: u64,
    chunks: Vec<usize>,
    file: String,
}

struct Work {
    id: ResourceId,
    stored_size: u64,
    size: u64,
    offset: u64,
}

struct DirRecord {
    name: String,
    file: String, // path relative to output dir
    section: String,
    file_offset: Option<u64>, // offset within the .kfc (None: stream-derived)
    length: u64,
    sha256: String,
}

/// Test whether re-encoding `raw` with the stock zstd stream encoder at
/// `level` reproduces `frame` byte-for-byte (the method A3's probe used:
/// `zstd::stream::copy_encode`).
fn encode_matches(raw: &[u8], frame: &[u8], level: i32) -> bool {
    let mut out = Vec::with_capacity(frame.len() + 64);
    if zstd::stream::copy_encode(&mut &raw[..], &mut out, level).is_err() {
        return false;
    }
    out == frame
}

/// Determine a chunk's zstd level by test. Search order: 9, then 3 (the
/// two levels A3 found in the game's frames), then ascending 1..=19
/// (skipping tested) until the first ascending match or until levels
/// above the best-known match would be reached. This guarantees the
/// returned level is the SMALLEST matching level in 1..=19: every level
/// below it has been tested and has failed. Returns (smallest, all
/// matches observed during the search).
fn determine_level(raw: &[u8], frame: &[u8]) -> (Option<i32>, Vec<i32>) {
    let mut matching: Vec<i32> = Vec::new();
    let mut best: Option<i32> = None;
    for lvl in [9, 3] {
        if encode_matches(raw, frame, lvl) {
            matching.push(lvl);
            best = Some(best.map_or(lvl, |b: i32| b.min(lvl)));
        }
    }
    for lvl in 1..=19i32 {
        if lvl == 9 || lvl == 3 {
            continue;
        }
        if let Some(b) = best {
            if lvl > b {
                break;
            }
        }
        if encode_matches(raw, frame, lvl) {
            matching.push(lvl);
            best = Some(lvl);
            break;
        }
    }
    matching.sort_unstable();
    (best, matching)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "usage: kfc4-extract-full <container.kfc> <output-dir> [--types <types.json>] [--baseline <fp-file>]"
        );
        std::process::exit(2);
    }
    let kfc_path = PathBuf::from(&args[1]);
    let out_dir = PathBuf::from(&args[2]);
    let mut types_path: Option<PathBuf> = None;
    let mut baseline_path: Option<PathBuf> = None;
    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--types" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("--types requires a path argument");
                    std::process::exit(2);
                }
                types_path = Some(PathBuf::from(&args[i]));
            }
            "--baseline" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("--baseline requires a path argument");
                    std::process::exit(2);
                }
                baseline_path = Some(PathBuf::from(&args[i]));
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    if types_path.is_none() {
        if let Some(parent) = kfc_path.parent() {
            let candidate = parent.join(".cache").join("types.json");
            if candidate.is_file() {
                types_path = Some(candidate);
            }
        }
    }

    let dir = kfc_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let name = kfc_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or("container path has no file stem")?;

    let started = Instant::now();
    let mut verify_failures: Vec<String> = Vec::new();

    // ================= raw header parse =================
    let kfc_bytes = std::fs::read(&kfc_path)?;
    println!("kfc_file_bytes: {}", kfc_bytes.len());
    let (size_field, locs) = match parse_header(&kfc_bytes) {
        Ok(v) => v,
        Err(e) => fatal(&e),
    };
    println!("header_size_field: {size_field}");
    let loc = |n: &str| locs.iter().find(|l| l.name == n).unwrap();

    // ================= parsed directory (cross-check) =================
    let file = KFCFile::from_path(&kfc_path, false)?;
    let version_tag = file.game_version().to_string();
    println!("version_tag: {version_tag}");
    println!("resources_in_directory: {}", file.resources().len());
    println!("chunks_in_table: {}", file.resource_chunks().len());

    // The raw parse and kfc-base's parse must agree on every count, and
    // the version string must be the version section's bytes verbatim.
    let cross = [
        ("resource_keys", file.resources().len() as u64),
        ("content_keys", file.contents().len() as u64),
        ("resource_bundle_keys", file.resource_bundles().len() as u64),
        ("containers", file.containers().len() as u64),
        ("resource_chunks", file.resource_chunks().len() as u64),
    ];
    for (lname, parsed) in cross {
        let raw = loc(lname).count;
        if raw != parsed {
            fatal(&format!(
                "count mismatch for {lname}: raw header says {raw}, kfc-base parsed {parsed}"
            ));
        }
    }
    match span(&kfc_bytes, loc("version")) {
        Ok(sec) => {
            if sec != version_tag.as_bytes() {
                fatal("version section bytes differ from kfc-base's game_version()");
            }
        }
        Err(e) => fatal(&e),
    }

    // ================= type registry =================
    let registry: Option<TypeRegistry> = match &types_path {
        Some(p) => {
            let f = File::open(p)?;
            let reg: TypeRegistry = serde_json::from_reader(std::io::BufReader::new(f))?;
            println!("registry_types: {} (from {})", reg.len(), p.display());
            Some(reg)
        }
        None => {
            println!("registry_types: none (type names will be null)");
            None
        }
    };

    let mut type_name_by_hash: HashMap<u32, Option<String>> = HashMap::new();
    {
        let mut distinct = HashSet::new();
        for k in file.resources().keys() {
            distinct.insert(k.type_hash());
        }
        println!("distinct_type_hashes: {}", distinct.len());
        for h in distinct {
            let resolved = registry.as_ref().and_then(|reg| {
                reg.get_by_hash(LookupKey::Qualified(h))
                    .map(|meta| meta.qualified_name.clone())
            });
            type_name_by_hash.insert(h, resolved);
        }
    }
    let mut dir_name_by_hash: HashMap<u32, String> = HashMap::new();
    {
        let mut used: HashMap<String, u32> = HashMap::new();
        for (&h, name) in &type_name_by_hash {
            let base = match name {
                Some(n) => sanitize_dir_name(n),
                None => format!("type_{h:08x}"),
            };
            let dir_name = match used.get(&base) {
                Some(&other) if other != h => format!("{base}__{h:08x}"),
                _ => base.clone(),
            };
            used.insert(dir_name.clone(), h);
            dir_name_by_hash.insert(h, dir_name);
        }
    }

    // ================= work list =================
    let reader = KFCReader::new(&dir, &name)?;
    let mut cursor = reader.new_cursor()?;
    let mut work: Vec<Work> = Vec::with_capacity(file.resources().len());
    for id in file.resources().keys() {
        let entry = file.resources().get(id).unwrap();
        let size = reader.resource_size(id).unwrap_or(entry.size);
        work.push(Work {
            id: *id,
            stored_size: entry.size,
            size,
            offset: entry.offset,
        });
    }
    work.sort_by_key(|w| (w.offset, w.id.guid(), w.id.type_hash(), w.id.part_index()));

    let chunks = file.resource_chunks();
    let chunk_indices_for = |offset: u64, size: u64| -> Vec<usize> {
        if size == 0 {
            return Vec::new();
        }
        let end = offset + size;
        let mut out = Vec::new();
        for (idx, ch) in chunks.iter().enumerate() {
            let c_start = ch.uncompressed_offset;
            let c_end = ch.uncompressed_offset + ch.uncompressed_size;
            if c_start < end && offset < c_end {
                out.push(idx);
            }
        }
        out
    };

    // ================= payload extraction pass (fail-closed) =================
    let resources_root = out_dir.join("resources");
    std::fs::create_dir_all(&resources_root)?;
    let mut created_dirs: HashSet<u32> = HashSet::new();
    let mut entries: Vec<ManifestEntry> = Vec::with_capacity(work.len());
    let mut per_type: BTreeMap<u32, TypeStats> = BTreeMap::new();
    let mut total_bytes: u64 = 0;
    let mut size_corrections = 0usize;
    let mut zero_size = 0usize;
    let mut buf: Vec<u8> = Vec::new();

    for (n, w) in work.iter().enumerate() {
        let guid_str = format!("{}", w.id.guid());
        buf.clear();
        match cursor.read_resource_into(&w.id, &mut buf) {
            Ok(true) => {}
            Ok(false) => fatal(&format!(
                "resource {guid_str} (type {:08x}, part {}) not found by reader",
                w.id.type_hash(),
                w.id.part_index()
            )),
            Err(e) => fatal(&format!(
                "resource {guid_str} (type {:08x}, part {}) read error: {e}",
                w.id.type_hash(),
                w.id.part_index()
            )),
        }
        if buf.len() as u64 != w.size {
            fatal(&format!(
                "resource {guid_str} (type {:08x}, part {}) length mismatch: directory says {}, read {}",
                w.id.type_hash(),
                w.id.part_index(),
                w.size,
                buf.len()
            ));
        }

        let type_hash = w.id.type_hash();
        let type_name = type_name_by_hash.get(&type_hash).cloned().unwrap_or(None);
        let dir_name = dir_name_by_hash.get(&type_hash).unwrap().clone();
        if created_dirs.insert(type_hash) {
            std::fs::create_dir_all(resources_root.join(&dir_name))?;
        }
        let file_name = format!("{guid_str}__p{}.bin", w.id.part_index());
        let rel_file = format!("resources/{dir_name}/{file_name}");
        {
            let target = out_dir.join(&rel_file);
            let mut attempt = 0;
            loop {
                match std::fs::write(&target, &buf) {
                    Ok(()) => break,
                    Err(e) if attempt < 10 => {
                        attempt += 1;
                        eprintln!("write retry {attempt}/10 for {rel_file} after error: {e}");
                        std::thread::sleep(std::time::Duration::from_millis(50 * attempt));
                    }
                    Err(e) => fatal(&format!(
                        "resource {guid_str} write to {} failed: {e}",
                        target.display()
                    )),
                }
            }
        }

        let fingerprint = fnv1a64(&[&w.size.to_le_bytes(), &buf]);
        let chunk_list = chunk_indices_for(w.offset, w.size);
        if w.stored_size != w.size {
            size_corrections += 1;
        }
        if w.size == 0 {
            zero_size += 1;
        }
        total_bytes += w.size;
        let stats = per_type.entry(type_hash).or_insert(TypeStats {
            type_name: type_name.clone(),
            count: 0,
            bytes: 0,
        });
        stats.count += 1;
        stats.bytes += w.size;
        entries.push(ManifestEntry {
            guid: guid_str,
            type_hash,
            type_name,
            part: w.id.part_index(),
            stored_size: w.stored_size,
            size: w.size,
            offset: w.offset,
            fingerprint,
            chunks: chunk_list,
            file: rel_file,
        });
        if (n + 1) % 10000 == 0 {
            println!("extracted {}/{}", n + 1, work.len());
        }
    }
    println!("extracted {}/{}", work.len(), work.len());

    // Manifest sorted by (guid, type hash, part) as in A2, plus `offset`.
    entries.sort_by(|a, b| {
        a.guid
            .cmp(&b.guid)
            .then(a.type_hash.cmp(&b.type_hash))
            .then(a.part.cmp(&b.part))
    });
    {
        let mf = File::create(out_dir.join("_manifest.json"))?;
        let mut out = BufWriter::new(mf);
        writeln!(out, "[")?;
        let total = entries.len();
        for (idx, e) in entries.iter().enumerate() {
            let v = serde_json::json!({
                "guid": e.guid,
                "type_hash": format!("{:08x}", e.type_hash),
                "type_name": e.type_name,
                "part": e.part,
                "stored_size": e.stored_size,
                "size": e.size,
                "offset": e.offset,
                "fingerprint": format!("{:016x}", e.fingerprint),
                "chunks": e.chunks,
                "file": e.file,
            });
            let comma = if idx + 1 == total { "" } else { "," };
            writeln!(out, "{}{comma}", serde_json::to_string(&v)?)?;
        }
        writeln!(out, "]")?;
        out.flush()?;
    }

    // ================= directory dumps =================
    let dir_root = out_dir.join("directory");
    std::fs::create_dir_all(&dir_root)?;
    let mut dir_records: Vec<DirRecord> = Vec::new();
    let mut dump = |rec_name: &str,
                    file_name: &str,
                    section: &str,
                    bytes: &[u8],
                    file_offset: Option<u64>,
                    records: &mut Vec<DirRecord>|
     -> Result<(), Box<dyn std::error::Error>> {
        std::fs::write(dir_root.join(file_name), bytes)?;
        records.push(DirRecord {
            name: rec_name.to_string(),
            file: format!("directory/{file_name}"),
            section: section.to_string(),
            file_offset,
            length: bytes.len() as u64,
            sha256: sha256_hex(bytes),
        });
        println!(
            "directory/{file_name}: {} bytes (sha256 {})",
            bytes.len(),
            sha256_hex(bytes)
        );
        Ok(())
    };

    // header.bin — the full 144-byte header verbatim.
    dump(
        "header",
        "header.bin",
        "header (144 B: magic, size, unk0, 16 locations)",
        &kfc_bytes[..144],
        Some(0),
        &mut dir_records,
    )?;
    // version.bin
    {
        let l = loc("version");
        let sec = span_or_fatal(&kfc_bytes, l);
        dump("version", "version.bin", "version", sec, Some(l.offset), &mut dir_records)?;
    }
    // containers.bin
    {
        let l = loc("containers");
        let sec = span_or_fatal(&kfc_bytes, l);
        dump(
            "containers",
            "containers.bin",
            "containers (ContainerInfo[count])",
            sec,
            Some(l.offset),
            &mut dir_records,
        )?;
    }
    // resource_locations.bin
    {
        let l = loc("resource_locations");
        let sec = span_or_fatal(&kfc_bytes, l);
        dump(
            "resource_locations",
            "resource_locations.bin",
            "resource_locations (ResourceLocation[count])",
            sec,
            Some(l.offset),
            &mut dir_records,
        )?;
    }
    // resource_indices.bin
    {
        let l = loc("resource_indices");
        let sec = span_or_fatal(&kfc_bytes, l);
        dump(
            "resource_indices",
            "resource_indices.bin",
            "resource_indices (u32[count])",
            sec,
            Some(l.offset),
            &mut dir_records,
        )?;
    }
    // chunk_table.bin — INCLUDING the u32 count prefix 4 bytes before
    // the header-pointed location (spec §5 quirk).
    {
        let l = loc("resource_chunks");
        if l.offset < 4 {
            fatal("resource_chunks offset < 4: no room for the count prefix");
        }
        let start = l.offset - 4;
        let prefix = u32le(&kfc_bytes, start as usize) as u64;
        if prefix != l.count {
            fatal(&format!(
                "chunk-count prefix at {} is {prefix}, header location count is {}",
                start, l.count
            ));
        }
        let end = l.offset + l.count * 20;
        if end > kfc_bytes.len() as u64 {
            fatal("chunk table out of bounds");
        }
        let bytes = &kfc_bytes[start as usize..end as usize];
        println!(
            "chunk_table span: [{start}, {end}) = 4 (count prefix = {prefix}) + {} x 20 B records",
            l.count
        );
        dump(
            "chunk_table",
            "chunk_table.bin",
            "resource_chunks, captured from the u32 count prefix (offset-4) through the last ResourceChunkInfo",
            bytes,
            Some(start),
            &mut dir_records,
        )?;
    }
    // The three static maps: full on-disk span from buckets start to
    // values end (buckets/keys/values as stored, including any
    // inter-array alignment bytes inside the span).
    let dump_map = |rec_name: &str,
                    file_name: &str,
                    buckets: &Loc,
                    keys: &Loc,
                    values: &Loc,
                    records: &mut Vec<DirRecord>|
     -> Result<(), Box<dyn std::error::Error>> {
        let b_end = buckets.offset + buckets.count * buckets.elem;
        let k_end = keys.offset + keys.count * keys.elem;
        let v_end = values.offset + values.count * values.elem;
        let contiguous = b_end == keys.offset && k_end == values.offset;
        let gap_bytes = (keys.offset - b_end) + (values.offset - k_end);
        let bytes = &kfc_bytes[buckets.offset as usize..v_end as usize];
        println!(
            "directory/{file_name}: span [{}, {v_end}) {} bytes, contiguous={contiguous}, inter-array gap bytes={gap_bytes}",
            buckets.offset,
            bytes.len()
        );
        std::fs::write(dir_root.join(file_name), bytes)?;
        records.push(DirRecord {
            name: rec_name.to_string(),
            file: format!("directory/{file_name}"),
            section: format!(
                "static map: {} buckets + {} keys + {} values (span buckets..values, contiguous={contiguous})",
                buckets.name, keys.name, values.name
            ),
            file_offset: Some(buckets.offset),
            length: bytes.len() as u64,
            sha256: sha256_hex(bytes),
        });
        Ok(())
    };
    dump_map(
        "contents_map",
        "contents_map.bin",
        loc("content_buckets"),
        loc("content_keys"),
        loc("content_values"),
        &mut dir_records,
    )?;
    dump_map(
        "resource_map",
        "resource_map.bin",
        loc("resource_buckets"),
        loc("resource_keys"),
        loc("resource_values"),
        &mut dir_records,
    )?;
    dump_map(
        "bundle_map",
        "bundle_map.bin",
        loc("resource_bundle_buckets"),
        loc("resource_bundle_keys"),
        loc("resource_bundle_values"),
        &mut dir_records,
    )?;

    // ================= resource key sequence (file order) =================
    // Parsed from the raw resource_keys section; cross-checked against
    // kfc-base's parsed map key sequence, entry by entry.
    let key_seq: Vec<(Guid, u32, u32)> = {
        let l = loc("resource_keys");
        let sec = span_or_fatal(&kfc_bytes, l);
        let mut out = Vec::with_capacity(l.count as usize);
        let mut nonzero_reserved = 0u64;
        for i in 0..l.count as usize {
            let r = &sec[i * 32..i * 32 + 32];
            let mut g = [0u8; 16];
            g.copy_from_slice(&r[..16]);
            if u32le(r, 24) != 0 || u32le(r, 28) != 0 {
                nonzero_reserved += 1;
            }
            out.push((Guid::new(g), u32le(r, 16), u32le(r, 20)));
        }
        if nonzero_reserved > 0 {
            fatal(&format!(
                "{nonzero_reserved} resource keys have nonzero reserved fields"
            ));
        }
        let parsed = file.resources().keys();
        if parsed.len() != out.len() {
            fatal("resource key sequence length differs from kfc-base's map");
        }
        for (i, (id, (g, th, part))) in parsed.iter().zip(out.iter()).enumerate() {
            if id.guid() != *g || id.type_hash() != *th || id.part_index() != *part {
                fatal(&format!(
                    "resource key sequence diverges from kfc-base at index {i}"
                ));
            }
        }
        println!(
            "resource key sequence: {} keys, identical to kfc-base's parsed order",
            out.len()
        );
        out
    };
    {
        let arr: Vec<serde_json::Value> = key_seq
            .iter()
            .enumerate()
            .map(|(i, (g, th, part))| {
                serde_json::json!({
                    "index": i,
                    "guid": format!("{g}"),
                    "type_hash": format!("{th:08x}"),
                    "type_name": type_name_by_hash.get(th).cloned().unwrap_or(None),
                    "part": part,
                })
            })
            .collect();
        std::fs::write(
            dir_root.join("resource_keys.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "count": arr.len(),
                "order": "file order (resource map keys array order)",
                "keys": arr,
            }))?,
        )?;
    }

    // ================= .dat inventory (payloads NOT captured) =================
    {
        let l = loc("containers");
        let sec = span_or_fatal(&kfc_bytes, l);
        let mut inv = Vec::with_capacity(l.count as usize);
        for i in 0..l.count as usize {
            let size = u64le(sec, i * 16);
            let count = u64le(sec, i * 16 + 8);
            let parsed = &file.containers()[i];
            if parsed.size != size || parsed.count as u64 != count {
                fatal(&format!("container {i}: raw directory and kfc-base disagree"));
            }
            let fname = format!("{name}_{i:03}.dat");
            let local = dir.join(&fname);
            let (present, local_size) = match std::fs::metadata(&local) {
                Ok(m) => (true, Some(m.len())),
                Err(_) => (false, None),
            };
            inv.push(serde_json::json!({
                "index": i,
                "file": fname,
                "directory_size": size,
                "directory_count": count,
                "local_file_present": present,
                "local_file_size": local_size,
                "payload_captured": false,
            }));
        }
        std::fs::write(
            dir_root.join("dat_files.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "note": "PAYLOADS NOT CAPTURED. This is the directory-side inventory only. On this VM the game directory holds zero-byte .dat stubs (indices 55-63); the real payload files exist only on Eric's PC. A full-fidelity folder must additionally copy the .dat files wholesale.",
                "containers": inv,
            }))?,
        )?;
        println!(
            "dat inventory: {} containers recorded, payloads NOT captured",
            inv.len()
        );
    }

    // ================= stream pass: chunks, levels, orphans =================
    let stream_path = dir.join(format!("{name}.kfc_resources"));
    let stream_len = std::fs::metadata(&stream_path)?.len();
    println!("stream_file: {} ({} bytes)", stream_path.display(), stream_len);
    let mut stream_file = File::open(&stream_path)?;

    // Cross-check the raw chunk table (from chunk_table.bin's span)
    // against kfc-base's parsed records.
    {
        let l = loc("resource_chunks");
        let sec = span_or_fatal(&kfc_bytes, l);
        for (i, ch) in chunks.iter().enumerate() {
            let r = &sec[i * 20..i * 20 + 20];
            let vals = [
                u32le(r, 0) as u64,
                u32le(r, 4) as u64,
                u32le(r, 8) as u64,
                u32le(r, 12) as u64,
                u32le(r, 16) as u64,
            ];
            let parsed = [
                ch.offset,
                ch.size,
                ch.compressed_size,
                ch.uncompressed_offset,
                ch.uncompressed_size,
            ];
            if vals != parsed {
                fatal(&format!("chunk {i}: raw table {vals:?} != kfc-base {parsed:?}"));
            }
        }
        println!("chunk table: raw parse identical to kfc-base for {} chunks", chunks.len());
    }
    // ResourceLocation aggregates cross-check.
    {
        let l = loc("resource_locations");
        let sec = span_or_fatal(&kfc_bytes, l);
        let unc = u32le(sec, 0) as u64;
        let comp = u32le(sec, 4) as u64;
        let cnt = u32le(sec, 8) as u64;
        let sum_unc: u64 = chunks.iter().map(|c| c.uncompressed_size).sum();
        let sum_comp: u64 = chunks.iter().map(|c| c.compressed_size).sum();
        let sum_slot: u64 = chunks.iter().map(|c| c.size).sum();
        println!(
            "resource_location: uncompressed={unc} (chunk sum {sum_unc}), compressed={comp} (frame sum {sum_comp}), count={cnt}; slot sum={sum_slot}, stream file={stream_len}"
        );
        if unc != sum_unc || comp != sum_comp || cnt != work.len() as u64 {
            fatal("ResourceLocation aggregates disagree with the chunk table");
        }
        if sum_slot != stream_len {
            fatal("chunk slots do not tile the stream file exactly");
        }
    }

    let total_uncompressed: u64 = chunks.iter().map(|c| c.uncompressed_size).sum();
    let mut uncompressed: Vec<u8> = Vec::with_capacity(total_uncompressed as usize);
    let mut chunk_json: Vec<serde_json::Value> = Vec::with_capacity(chunks.len());
    let mut level_hist: BTreeMap<String, u64> = BTreeMap::new();
    let mut frames_dir_created = false;
    let mut frame_buf: Vec<u8> = Vec::new();
    for (idx, ch) in chunks.iter().enumerate() {
        frame_buf.clear();
        frame_buf.resize(ch.compressed_size as usize, 0);
        stream_file.seek(SeekFrom::Start(ch.offset))?;
        stream_file.read_exact(&mut frame_buf)?;
        let raw = match zstd::bulk::decompress(&frame_buf, ch.uncompressed_size as usize) {
            Ok(r) => r,
            Err(e) => fatal(&format!("chunk {idx}: zstd decompress failed: {e}")),
        };
        if raw.len() as u64 != ch.uncompressed_size {
            fatal(&format!(
                "chunk {idx}: decompressed {} bytes, table says {}",
                raw.len(),
                ch.uncompressed_size
            ));
        }
        if ch.uncompressed_offset != uncompressed.len() as u64 {
            fatal(&format!(
                "chunk {idx}: uncompressed_offset {} != running total {}",
                ch.uncompressed_offset,
                uncompressed.len()
            ));
        }
        let (level, matching) = determine_level(&raw, &frame_buf);
        let level_key = match level {
            Some(l) => format!("{l}"),
            None => "null".to_string(),
        };
        *level_hist.entry(level_key).or_insert(0) += 1;
        let mut frame_file: Option<String> = None;
        if level.is_none() {
            if !frames_dir_created {
                std::fs::create_dir_all(dir_root.join("chunks_frames"))?;
                frames_dir_created = true;
            }
            let fname = format!("chunk_{idx:03}.zst");
            std::fs::write(dir_root.join("chunks_frames").join(&fname), &frame_buf)?;
            frame_file = Some(format!("directory/chunks_frames/{fname}"));
            println!("chunk {idx}: NO stock zstd level reproduces the frame; original frame saved");
        } else {
            println!(
                "chunk {idx}: zstd level {} (also matching: {:?})",
                level.unwrap(),
                matching
            );
        }
        uncompressed.extend_from_slice(&raw);
        chunk_json.push(serde_json::json!({
            "index": idx,
            "offset": ch.offset,
            "size": ch.size,
            "compressed_size": ch.compressed_size,
            "uncompressed_offset": ch.uncompressed_offset,
            "uncompressed_size": ch.uncompressed_size,
            "zstd_level": level,
            "zstd_levels_matching": matching,
            "frame_file": frame_file,
        }));
    }
    println!(
        "uncompressed stream rebuilt: {} bytes; level histogram: {:?}",
        uncompressed.len(),
        level_hist
    );

    // ---- orphan gaps: coverage of [offset, offset+size) vs stream ----
    let mut gaps_large: Vec<(u64, u64)> = Vec::new(); // (stream offset, length)
    let mut small_count = 0u64;
    let mut small_bytes = 0u64;
    let mut overlap_count = 0u64;
    {
        let mut cursor_pos = 0u64;
        let mut classify = |a: u64,
                            b: u64,
                            uncompressed: &[u8],
                            large: &mut Vec<(u64, u64)>,
                            sc: &mut u64,
                            sb: &mut u64| {
            if b <= a {
                return;
            }
            let len = b - a;
            if len <= 15 && uncompressed[a as usize..b as usize].iter().all(|&x| x == 0) {
                *sc += 1;
                *sb += len;
            } else {
                if len <= 15 {
                    println!(
                        "WARNING: gap [{a}, {b}) is <=15 B but NOT all zero; dumping as a large gap"
                    );
                }
                large.push((a, len));
            }
        };
        for w in &work {
            if w.offset > cursor_pos {
                classify(
                    cursor_pos,
                    w.offset,
                    &uncompressed,
                    &mut gaps_large,
                    &mut small_count,
                    &mut small_bytes,
                );
            }
            if w.offset < cursor_pos {
                overlap_count += 1;
            }
            cursor_pos = cursor_pos.max(w.offset + w.size);
        }
        if cursor_pos < total_uncompressed {
            classify(
                cursor_pos,
                total_uncompressed,
                &uncompressed,
                &mut gaps_large,
                &mut small_count,
                &mut small_bytes,
            );
        }
    }
    println!(
        "gaps: {} large ({} bytes), {} small ({} bytes), overlaps: {}",
        gaps_large.len(),
        gaps_large.iter().map(|g| g.1).sum::<u64>(),
        small_count,
        small_bytes,
        overlap_count
    );
    if overlap_count > 0 {
        fatal(&format!("{overlap_count} resources overlap a previous resource's range"));
    }
    // A3 comparison targets (measured on this same pristine pair).
    const A3_LARGE_N: u64 = 3_960;
    const A3_LARGE_B: u64 = 16_989_572;
    const A3_SMALL_N: u64 = 64_139;
    const A3_SMALL_B: u64 = 497_031;
    let large_bytes: u64 = gaps_large.iter().map(|g| g.1).sum();
    if gaps_large.len() as u64 != A3_LARGE_N
        || large_bytes != A3_LARGE_B
        || small_count != A3_SMALL_N
        || small_bytes != A3_SMALL_B
    {
        verify_failures.push(format!(
            "orphan census differs from A3: large {}/{} (A3 {A3_LARGE_N}/{A3_LARGE_B}), small {small_count}/{small_bytes} (A3 {A3_SMALL_N}/{A3_SMALL_B})",
            gaps_large.len(),
            large_bytes
        ));
    }

    // ---- orphans dump ----
    let orphans_dir = dir_root.join("orphans");
    std::fs::create_dir_all(&orphans_dir)?;
    let mut orphans_bin: Vec<u8> = Vec::with_capacity(large_bytes as usize);
    let mut orphan_entries: Vec<serde_json::Value> = Vec::with_capacity(gaps_large.len());
    for (i, (off, len)) in gaps_large.iter().enumerate() {
        let slice = &uncompressed[*off as usize..(*off + *len) as usize];
        orphan_entries.push(serde_json::json!({
            "index": i,
            "stream_offset": off,
            "length": len,
            "orphans_bin_offset": orphans_bin.len() as u64,
            "sha256": sha256_hex(slice),
        }));
        orphans_bin.extend_from_slice(slice);
    }
    std::fs::write(orphans_dir.join("orphans.bin"), &orphans_bin)?;
    std::fs::write(
        orphans_dir.join("orphans.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "total_uncompressed_stream_bytes": total_uncompressed,
            "large_gaps": {
                "count": gaps_large.len(),
                "total_bytes": large_bytes,
                "storage": "orphans.bin, concatenated in stream-offset order; per-gap offsets in this index",
                "gaps": orphan_entries,
            },
            "small_gaps": {
                "count": small_count,
                "total_bytes": small_bytes,
                "storage": "NOT dumped",
                "rule": "A gap of <=15 bytes between consecutive resources is 16-byte alignment padding and is 100% zero; reproduce by zero-filling up to the next resource's recorded offset. The trailing gap after the last resource follows the same rule.",
            },
        }))?,
    )?;
    dir_records.push(DirRecord {
        name: "orphans".to_string(),
        file: "directory/orphans/orphans.bin".to_string(),
        section: "resources stream (uncompressed) — data-bearing gaps referenced by no resource".to_string(),
        file_offset: None,
        length: orphans_bin.len() as u64,
        sha256: sha256_hex(&orphans_bin),
    });
    println!(
        "orphans.bin: {} bytes ({} large gaps)",
        orphans_bin.len(),
        gaps_large.len()
    );

    // ================= chunks.json =================
    std::fs::write(
        dir_root.join("chunks.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "chunk_count": chunk_json.len(),
            "stream_file_bytes": stream_len,
            "total_uncompressed_bytes": total_uncompressed,
            "level_search": "Per chunk, re-encode the decompressed data with zstd::stream::copy_encode (the encoder kfc-base uses) and compare to the original frame byte-for-byte. Order: 9, then 3, then ascending 1..=19 (skipping tested) until the first ascending match or until levels above the best match are passed. zstd_level is therefore the SMALLEST level in 1..=19 that reproduces the frame; zstd_levels_matching lists every match observed during that search. null = no level in 1..=19 reproduces it; the original frame is then preserved under chunks_frames/.",
            "level_histogram": &level_hist,
            "chunks": &chunk_json,
        }))?,
    )?;

    // ================= _directory.json =================
    let verbatim_bytes: u64 = dir_records
        .iter()
        .filter(|r| r.name != "orphans")
        .map(|r| r.length)
        .sum();
    let orphans_bin_bytes: u64 = dir_records
        .iter()
        .filter(|r| r.name == "orphans")
        .map(|r| r.length)
        .sum();
    let mut json_artifacts: Vec<serde_json::Value> = Vec::new();
    let mut json_bytes: u64 = 0;
    for jf in [
        "resource_keys.json",
        "chunks.json",
        "orphans/orphans.json",
        "dat_files.json",
    ] {
        let p = dir_root.join(jf);
        let bytes = std::fs::read(&p)?;
        json_bytes += bytes.len() as u64;
        json_artifacts.push(serde_json::json!({
            "name": jf,
            "file": format!("directory/{jf}"),
            "kind": "derived index (JSON)",
            "length": bytes.len(),
            "sha256": sha256_hex(&bytes),
        }));
    }
    let closing_records_total = verbatim_bytes + orphans_bin_bytes;
    let directory_total = closing_records_total + json_bytes;
    println!(
        "closing data: verbatim records {verbatim_bytes} B + orphans {orphans_bin_bytes} B = {closing_records_total} B; JSON artifacts {json_bytes} B; directory/ total {directory_total} B"
    );
    {
        let records_json: Vec<serde_json::Value> = dir_records
            .iter()
            .map(|r| {
                serde_json::json!({
                    "name": r.name,
                    "file": r.file,
                    "section": r.section,
                    "file_offset": r.file_offset,
                    "length": r.length,
                    "sha256": r.sha256,
                })
            })
            .collect();
        std::fs::write(
            dir_root.join("_directory.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "container": kfc_path.display().to_string(),
                "container_sha256": sha256_hex(&kfc_bytes),
                "version_tag": version_tag,
                "header_size_field": size_field,
                "records": records_json,
                "json_artifacts": json_artifacts,
                "small_gap_rule": "Gaps of <=15 bytes between consecutive resources in the uncompressed stream are 16-byte alignment padding, 100% zero (verified); reproduce by zero-filling up to the next resource's recorded offset. The trailing gap after the last resource follows the same rule. Small gaps are NOT dumped.",
                "orphan_totals": {
                    "large_count": gaps_large.len(),
                    "large_bytes": large_bytes,
                    "small_count": small_count,
                    "small_bytes": small_bytes,
                },
                "closing_data_bytes": {
                    "verbatim_records": verbatim_bytes,
                    "orphans_bin": orphans_bin_bytes,
                    "closing_records_total": closing_records_total,
                    "json_artifacts": json_bytes,
                    "directory_total": directory_total,
                },
            }))?,
        )?;
    }

    // ================= summary =================
    let distinct_guids: HashSet<&str> = entries.iter().map(|e| e.guid.as_str()).collect();
    let multi_part = entries.iter().filter(|e| e.part > 0).count();
    let max_part = entries.iter().map(|e| e.part).max().unwrap_or(0);
    let unresolved: Vec<String> = type_name_by_hash
        .iter()
        .filter(|(_, v)| v.is_none())
        .map(|(h, _)| format!("{h:08x}"))
        .collect();
    let resolved_count = type_name_by_hash.len() - unresolved.len();
    let per_type_json: Vec<serde_json::Value> = {
        let mut v: Vec<(&u32, &TypeStats)> = per_type.iter().collect();
        v.sort_by(|a, b| b.1.count.cmp(&a.1.count).then(a.0.cmp(b.0)));
        v.iter()
            .map(|(h, s)| {
                serde_json::json!({
                    "type_hash": format!("{h:08x}"),
                    "type_name": s.type_name,
                    "count": s.count,
                    "bytes": s.bytes,
                })
            })
            .collect()
    };
    let summary = serde_json::json!({
        "container": kfc_path.display().to_string(),
        "version_tag": version_tag,
        "registry_types": registry.as_ref().map(|r| r.len()),
        "resources": entries.len(),
        "distinct_guids": distinct_guids.len(),
        "parts_total": entries.len(),
        "entries_with_part_gt_0": multi_part,
        "max_part_index": max_part,
        "total_bytes": total_bytes,
        "distinct_type_hashes": type_name_by_hash.len(),
        "resolved_type_hashes": resolved_count,
        "unresolved_type_hashes": unresolved,
        "size_corrections_24bit": size_corrections,
        "zero_size_resources": zero_size,
        "chunks_in_table": chunks.len(),
        "per_type": per_type_json,
        "manifest_offsets": "absolute uncompressed stream offset per entry (new in A2.5)",
        "a25": {
            "directory_records_bytes": verbatim_bytes,
            "orphans_bin_bytes": orphans_bin_bytes,
            "closing_records_total_bytes": closing_records_total,
            "json_artifacts_bytes": json_bytes,
            "directory_total_bytes": directory_total,
            "zstd_level_histogram": level_hist,
            "orphan_large_gaps": gaps_large.len(),
            "orphan_large_bytes": large_bytes,
            "orphan_small_gaps": small_count,
            "orphan_small_bytes": small_bytes,
            "dat_payloads_captured": false,
        },
        "elapsed_seconds": started.elapsed().as_secs_f64(),
    });
    std::fs::write(
        out_dir.join("_summary.json"),
        serde_json::to_string_pretty(&summary)?,
    )?;

    println!("total_resources: {}", entries.len());
    println!("total_bytes: {total_bytes}");
    println!("resolved_type_hashes: {resolved_count}/{}", type_name_by_hash.len());

    // ================= VERIFICATION (fresh reads from disk) =================
    println!("--- verification phase (fresh reads) ---");
    // V1: directory/*.bin vs fresh header parse + fresh slices.
    let kfc2 = std::fs::read(&kfc_path)?;
    let (_, locs2) = match parse_header(&kfc2) {
        Ok(v) => v,
        Err(e) => fatal(&e),
    };
    let loc2 = |n: &str| locs2.iter().find(|l| l.name == n).unwrap();
    let expected_span = |name: &str| -> (u64, u64) {
        // (offset, length) of the record's expected bytes in the .kfc
        match name {
            "header" => (0, 144),
            "version" | "containers" | "resource_locations" | "resource_indices" => {
                let l = loc2(name);
                (l.offset, l.count * l.elem)
            }
            "chunk_table" => {
                let l = loc2("resource_chunks");
                (l.offset - 4, 4 + l.count * 20)
            }
            "contents_map" => {
                let b = loc2("content_buckets");
                let v = loc2("content_values");
                (b.offset, v.offset + v.count * v.elem - b.offset)
            }
            "resource_map" => {
                let b = loc2("resource_buckets");
                let v = loc2("resource_values");
                (b.offset, v.offset + v.count * v.elem - b.offset)
            }
            "bundle_map" => {
                let b = loc2("resource_bundle_buckets");
                let v = loc2("resource_bundle_values");
                (b.offset, v.offset + v.count * v.elem - b.offset)
            }
            _ => (0, 0),
        }
    };
    for rec in &dir_records {
        if rec.name == "orphans" {
            continue; // verified against the stream below
        }
        let (off, len) = expected_span(&rec.name);
        let expected = &kfc2[off as usize..(off + len) as usize];
        let on_disk = std::fs::read(out_dir.join(&rec.file))?;
        if on_disk != expected {
            verify_failures.push(format!(
                "directory record {} differs from fresh slice [{off}, {})",
                rec.name,
                off + len
            ));
        }
        if sha256_hex(&on_disk) != rec.sha256 {
            verify_failures.push(format!("directory record {} sha256 mismatch", rec.name));
        }
        println!("verify directory/{}: {} bytes OK", rec.file, on_disk.len());
    }
    // V2: resource_keys.json vs fresh raw parse.
    {
        let text = std::fs::read_to_string(dir_root.join("resource_keys.json"))?;
        let parsed: serde_json::Value = serde_json::from_str(&text)?;
        let keys = parsed["keys"].as_array().unwrap();
        let l = loc2("resource_keys");
        let sec = span_or_fatal(&kfc2, l);
        if keys.len() as u64 != l.count {
            verify_failures.push("resource_keys.json count mismatch".to_string());
        }
        let mut bad = 0u64;
        for (i, kv) in keys.iter().enumerate() {
            let r = &sec[i * 32..i * 32 + 32];
            let mut g = [0u8; 16];
            g.copy_from_slice(&r[..16]);
            let guid = Guid::new(g).to_string();
            let th = format!("{:08x}", u32le(r, 16));
            if kv["guid"].as_str() != Some(guid.as_str())
                || kv["type_hash"].as_str() != Some(th.as_str())
                || kv["part"].as_u64() != Some(u32le(r, 20) as u64)
            {
                bad += 1;
            }
        }
        println!(
            "verify resource_keys.json: {} entries, {} mismatches",
            keys.len(),
            bad
        );
        if bad > 0 {
            verify_failures.push(format!("resource_keys.json has {bad} mismatched entries"));
        }
    }
    // V3: fresh decompression of the whole stream; frame/level re-check.
    let mut uncompressed2: Vec<u8> = Vec::with_capacity(total_uncompressed as usize);
    {
        let mut sf = File::open(&stream_path)?;
        let mut fbuf: Vec<u8> = Vec::new();
        for (idx, ch) in chunks.iter().enumerate() {
            fbuf.clear();
            fbuf.resize(ch.compressed_size as usize, 0);
            sf.seek(SeekFrom::Start(ch.offset))?;
            sf.read_exact(&mut fbuf)?;
            let raw = match zstd::bulk::decompress(&fbuf, ch.uncompressed_size as usize) {
                Ok(r) => r,
                Err(e) => fatal(&format!("verify: chunk {idx} decompress failed: {e}")),
            };
            // level re-check from the written chunks.json values
            let cj = &chunk_json[idx];
            match cj["zstd_level"].as_i64() {
                Some(l) => {
                    if !encode_matches(&raw, &fbuf, l as i32) {
                        verify_failures.push(format!(
                            "chunk {idx}: recorded level {l} does not reproduce the frame on re-check"
                        ));
                    }
                }
                None => {
                    let saved = std::fs::read(
                        dir_root.join("chunks_frames").join(format!("chunk_{idx:03}.zst")),
                    )?;
                    if saved != fbuf {
                        verify_failures.push(format!(
                            "chunk {idx}: saved fallback frame differs from the stream"
                        ));
                    }
                }
            }
            uncompressed2.extend_from_slice(&raw);
        }
    }
    if uncompressed2 != uncompressed {
        verify_failures.push("fresh decompression differs from extraction-pass stream".to_string());
    } else {
        println!("verify stream: fresh decompression identical ({} bytes)", uncompressed2.len());
    }
    drop(uncompressed);
    // V4: orphans.bin vs fresh stream, per gap.
    {
        let bin = std::fs::read(orphans_dir.join("orphans.bin"))?;
        if bin.len() as u64 != large_bytes {
            verify_failures.push("orphans.bin length mismatch".to_string());
        }
        let mut bin_off = 0u64;
        let mut bad = 0u64;
        for (off, len) in &gaps_large {
            let slice = &uncompressed2[*off as usize..(*off + *len) as usize];
            let stored = &bin[bin_off as usize..(bin_off + *len) as usize];
            if slice != stored || sha256_hex(slice) != sha256_hex(stored) {
                bad += 1;
            }
            bin_off += len;
        }
        println!(
            "verify orphans: {} large gaps re-hashed against fresh stream, {} mismatches",
            gaps_large.len(),
            bad
        );
        if bad > 0 {
            verify_failures.push(format!("orphans: {bad} gaps differ from the fresh stream"));
        }
    }
    // V5: baseline fingerprints (A0 line format, sorted-set equality).
    if let Some(bp) = &baseline_path {
        let text = std::fs::read_to_string(bp)?;
        let base_set: BTreeSet<String> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.trim_end().to_string())
            .collect();
        let our_set: BTreeSet<String> = entries
            .iter()
            .map(|e| {
                format!(
                    "{} {:08x} {} {} {:016x}",
                    e.guid, e.type_hash, e.stored_size, e.size, e.fingerprint
                )
            })
            .collect();
        println!(
            "verify baseline: ours {} lines, baseline {} lines, identical: {}",
            our_set.len(),
            base_set.len(),
            our_set == base_set
        );
        if our_set != base_set {
            let only_ours: Vec<_> = our_set.difference(&base_set).take(5).collect();
            let only_base: Vec<_> = base_set.difference(&our_set).take(5).collect();
            verify_failures.push(format!(
                "baseline fingerprint mismatch; examples only-ours: {only_ours:?}, only-baseline: {only_base:?}"
            ));
        }
    }
    // V6: every manifest entry's bytes == fresh stream slice at its
    // recorded offset (validates the new offset field end-to-end).
    {
        let mut bad = 0u64;
        for e in &entries {
            let slice = &uncompressed2[e.offset as usize..(e.offset + e.size) as usize];
            let disk = std::fs::read(out_dir.join(&e.file))?;
            if disk != slice {
                bad += 1;
                if bad <= 5 {
                    eprintln!("offset check failed for {} at {}", e.guid, e.offset);
                }
            }
        }
        println!(
            "verify offsets: {}/{} entries match the fresh stream at their recorded offset, {} mismatches",
            entries.len() - bad as usize,
            entries.len(),
            bad
        );
        if bad > 0 {
            verify_failures.push(format!("{bad} manifest offset checks failed"));
        }
        // Named spot cases (also covered by the full check above).
        let by_end: Vec<&ManifestEntry> = {
            let mut v: Vec<&ManifestEntry> = entries.iter().collect();
            v.sort_by_key(|e| e.offset + e.size);
            v
        };
        let first = entries.iter().min_by_key(|e| e.offset).unwrap();
        let last = by_end.last().unwrap();
        let big = entries.iter().find(|e| e.size == 44_659_852);
        let gap_ends: HashSet<u64> = gaps_large.iter().map(|(o, l)| o + l).collect();
        let adjacent = entries
            .iter()
            .filter(|e| gap_ends.contains(&e.offset))
            .min_by_key(|e| e.offset);
        println!("spot case first-by-offset: {} {} @ {}", first.guid, first.file, first.offset);
        println!(
            "spot case last-by-end: {} {} ends @ {}",
            last.guid,
            last.file,
            last.offset + last.size
        );
        match big {
            Some(b) => println!(
                "spot case 44,659,852-byte resource: {} {} @ {} (chunks {:?})",
                b.guid, b.file, b.offset, b.chunks
            ),
            None => verify_failures.push("44,659,852-byte resource not found".to_string()),
        }
        match adjacent {
            Some(a) => println!(
                "spot case orphan-adjacent: {} {} @ {} (starts exactly at a large gap's end)",
                a.guid, a.file, a.offset
            ),
            None => verify_failures.push("no orphan-adjacent resource found".to_string()),
        }
    }

    println!("elapsed_seconds: {:.2}", started.elapsed().as_secs_f64());
    if verify_failures.is_empty() {
        println!("VERIFY: ALL OK");
        Ok(())
    } else {
        for f in &verify_failures {
            eprintln!("VERIFY FAILED: {f}");
        }
        std::process::exit(1);
    }
}
