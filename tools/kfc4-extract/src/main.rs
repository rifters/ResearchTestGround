// kfc4-extract — KFC4 project phase A2: full unpack of a KFC container
// to a folder tree, with a manifest and verification-grade fingerprints.
//
// Usage:
//   kfc4-extract <container.kfc> <output-dir> [--types <types.json>]
//
// Output layout:
//   <output-dir>/resources/<TypeName-or-type_HASH>/<guid>__p<part>.bin
//   <output-dir>/_manifest.json   one entry per resource part
//   <output-dir>/_summary.json    totals + per-type stats
//
// Fail-closed: any resource that fails to read, or whose decompressed
// length disagrees with the directory's (24-bit-recovered) size, aborts
// the run with a message naming the GUID. No silent skips.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    time::Instant,
};

use kfc_base::{
    container::{KFCFile, KFCReader},
    guid::ResourceId,
    reflection::{LookupKey, TypeRegistry},
};

/// Fingerprint: copied verbatim from the A0 audit harness
/// (`~/workspace/your_files/kfc4-audit/harness/src/main.rs`) so A2
/// verification against `fp-pristine.txt` is apples-to-apples.
///
/// FNV-1a 64-bit (offset basis 0xcbf29ce484222325, prime 0x100000001b3)
/// computed over the concatenation of:
///   1. the resource's actual size as 8 little-endian bytes, then
///   2. the resource's decompressed content bytes.
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

/// Make a type name safe as a single directory name on any filesystem:
/// `keen::VfxMeshResource` -> `keen__VfxMeshResource`; any remaining
/// path-hostile characters become `_`.
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
    fingerprint: u64,
    chunks: Vec<usize>,
    file: String, // path relative to output dir
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: kfc4-extract <container.kfc> <output-dir> [--types <types.json>]");
        std::process::exit(2);
    }
    let kfc_path = PathBuf::from(&args[1]);
    let out_dir = PathBuf::from(&args[2]);
    let mut types_path: Option<PathBuf> = None;
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
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    // Default registry location: <container dir>/.cache/types.json
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

    // ---- directory ----
    let file = KFCFile::from_path(&kfc_path, false)?;
    let version_tag = file.game_version().to_string();
    println!("version_tag: {version_tag}");
    println!("resources_in_directory: {}", file.resources().len());
    println!("chunks_in_table: {}", file.resource_chunks().len());

    // ---- type registry (optional) ----
    let registry: Option<TypeRegistry> = match &types_path {
        Some(p) => {
            let f = File::open(p)?;
            let reg: TypeRegistry =
                serde_json::from_reader(std::io::BufReader::new(f))?;
            println!("registry_types: {} (from {})", reg.len(), p.display());
            Some(reg)
        }
        None => {
            println!("registry_types: none (type names will be null)");
            None
        }
    };

    // Resolve every distinct type hash once.
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

    // Directory name per type hash (collision-safe).
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

    // ---- reader ----
    let reader = KFCReader::new(&dir, &name)?;
    let mut cursor = reader.new_cursor()?;

    // Work list sorted by stream offset: chunk decompression stays
    // sequential instead of thrashing the chunk cache.
    struct Work {
        id: ResourceId,
        stored_size: u64,
        size: u64,
        offset: u64,
    }
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

    // ---- extraction pass (fail-closed) ----
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
            Ok(false) => {
                eprintln!(
                    "EXTRACT FAILED: resource {} (type {:08x}, part {}) not found by reader",
                    guid_str,
                    w.id.type_hash(),
                    w.id.part_index()
                );
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!(
                    "EXTRACT FAILED: resource {} (type {:08x}, part {}) read error: {e}",
                    guid_str,
                    w.id.type_hash(),
                    w.id.part_index()
                );
                std::process::exit(1);
            }
        }
        if buf.len() as u64 != w.size {
            eprintln!(
                "EXTRACT FAILED: resource {} (type {:08x}, part {}) length mismatch: directory says {}, read {}",
                guid_str,
                w.id.type_hash(),
                w.id.part_index(),
                w.size,
                buf.len()
            );
            std::process::exit(1);
        }

        let type_hash = w.id.type_hash();
        let type_name = type_name_by_hash.get(&type_hash).cloned().unwrap_or(None);
        let dir_name = dir_name_by_hash.get(&type_hash).unwrap().clone();
        if created_dirs.insert(type_hash) {
            std::fs::create_dir_all(resources_root.join(&dir_name))?;
        }
        let file_name = format!("{guid_str}__p{}.bin", w.id.part_index());
        let rel_file = format!("resources/{dir_name}/{file_name}");
        // Retry transient filesystem errors (observed once on this VM:
        // EPERM after ~61k rapid file creations, immediately
        // reproducible as writable from the shell). A failure that
        // survives every retry is fatal and names the resource.
        {
            let target = out_dir.join(&rel_file);
            let mut attempt = 0;
            loop {
                match std::fs::write(&target, &buf) {
                    Ok(()) => break,
                    Err(e) if attempt < 10 => {
                        attempt += 1;
                        eprintln!(
                            "write retry {attempt}/10 for {rel_file} after error: {e}"
                        );
                        std::thread::sleep(std::time::Duration::from_millis(
                            50 * attempt,
                        ));
                    }
                    Err(e) => {
                        eprintln!(
                            "EXTRACT FAILED: resource {} (type {:08x}, part {}) write to {} failed: {e}",
                            guid_str,
                            w.id.type_hash(),
                            w.id.part_index(),
                            target.display()
                        );
                        std::process::exit(1);
                    }
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
            fingerprint,
            chunks: chunk_list,
            file: rel_file,
        });

        if (n + 1) % 10000 == 0 {
            println!("extracted {}/{}", n + 1, work.len());
        }
    }
    println!("extracted {}/{}", work.len(), work.len());

    // ---- manifest (sorted by guid, type hash, part for determinism) ----
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

    // ---- summary ----
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
        "elapsed_seconds": started.elapsed().as_secs_f64(),
    });
    std::fs::write(
        out_dir.join("_summary.json"),
        serde_json::to_string_pretty(&summary)?,
    )?;

    println!("total_resources: {}", entries.len());
    println!("distinct_guids: {}", distinct_guids.len());
    println!("total_bytes: {total_bytes}");
    println!("resolved_type_hashes: {resolved_count}/{}", type_name_by_hash.len());
    println!("unresolved_type_hashes: {}", unresolved.len());
    println!("size_corrections_24bit: {size_corrections}");
    println!("zero_size_resources: {zero_size}");
    println!("elapsed_seconds: {:.2}", started.elapsed().as_secs_f64());
    println!("output_dir: {}", out_dir.display());
    Ok(())
}
