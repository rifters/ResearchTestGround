// KFC4 round-trip audit harness.
// Subcommands:
//   audit     <dir> <name> <fp_out>   - parse + full content read of every resource
//   roundtrip <srcdir> <workdir> <name> <registry_json> - zero-mod incremental reserialize on copies
//   cmpdir    <dirA> <dirB> <name>    - semantic directory comparison of two .kfc files
use std::{
    collections::HashSet,
    fs::File,
    io::Write,
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

fn audit(dir: &str, name: &str, fp_out: &str) -> Result<(), Box<dyn std::error::Error>> {
    let kfc_path = Path::new(dir).join(format!("{name}.kfc"));
    let file = KFCFile::from_path(&kfc_path, false)?;
    println!("version_tag: {}", file.game_version());
    println!("containers: {}", file.containers().len());
    let nonzero_containers = file.containers().iter().filter(|c| c.size > 0).count();
    println!("containers_nonzero_size: {nonzero_containers}");
    println!("resources: {}", file.resources().len());
    println!("contents: {}", file.contents().len());
    println!("bundles: {}", file.resource_bundles().len());
    println!("chunks: {}", file.resource_chunks().len());

    let mut types = HashSet::new();
    for k in file.resources().keys() {
        types.insert(k.type_hash());
    }
    println!("distinct_type_hashes_in_keys: {}", types.len());

    // chunk table consistency
    let res_path = Path::new(dir).join(format!("{name}.kfc_resources"));
    let res_len = std::fs::metadata(&res_path).map(|m| m.len()).unwrap_or(0);
    println!("resources_file_len: {res_len}");
    let mut prev_uend = 0u64;
    let mut contiguous = true;
    let mut within = true;
    let mut sum_size = 0u64;
    let mut sum_comp = 0u64;
    let mut sum_uncomp = 0u64;
    for (i, ch) in file.resource_chunks().iter().enumerate() {
        if ch.uncompressed_offset != prev_uend {
            contiguous = false;
            println!("chunk {i}: uncompressed gap: expected start {prev_uend}, got {}", ch.uncompressed_offset);
        }
        prev_uend = ch.uncompressed_offset + ch.uncompressed_size;
        if ch.offset + ch.size > res_len {
            within = false;
            println!("chunk {i}: compressed range exceeds resources file");
        }
        sum_size += ch.size;
        sum_comp += ch.compressed_size;
        sum_uncomp += ch.uncompressed_size;
    }
    println!("chunks_uncompressed_contiguous: {contiguous}");
    println!("chunks_within_file: {within}");
    println!("chunks_sum_size: {sum_size}");
    println!("chunks_sum_compressed: {sum_comp}");
    println!("chunks_sum_uncompressed: {sum_uncomp}");

    // full content pass
    let reader = KFCReader::new(dir, name)?;
    let mut cursor = reader.new_cursor()?;
    let ids: Vec<ResourceId> = file.resources().keys().to_vec();
    let mut ok = 0usize;
    let mut missing = 0usize;
    let mut errors = 0usize;
    let mut err_msgs: Vec<String> = Vec::new();
    let mut total_bytes = 0u64;
    let mut size_corrections = 0usize;
    let mut correction_examples: Vec<String> = Vec::new();
    let mut len_mismatch = 0usize;
    let mut fps: Vec<String> = Vec::with_capacity(ids.len());
    let mut buf: Vec<u8> = Vec::new();

    for id in &ids {
        let entry = file.resources().get(id).unwrap();
        let actual = reader.resource_size(id).unwrap_or(entry.size);
        if actual != entry.size {
            size_corrections += 1;
            if correction_examples.len() < 10 {
                correction_examples.push(format!("{id} stored={} actual={actual}", entry.size));
            }
        }
        buf.clear();
        match cursor.read_resource_into(id, &mut buf) {
            Ok(true) => {
                ok += 1;
                total_bytes += buf.len() as u64;
                if buf.len() as u64 != actual {
                    len_mismatch += 1;
                }
                let fp = fnv1a64(&[&actual.to_le_bytes(), &buf]);
                fps.push(format!("{id} {:08x} {} {} {:016x}", id.type_hash(), entry.size, actual, fp));
            }
            Ok(false) => {
                missing += 1;
                if err_msgs.len() < 5 {
                    err_msgs.push(format!("{id}: read returned None"));
                }
            }
            Err(e) => {
                errors += 1;
                if err_msgs.len() < 5 {
                    err_msgs.push(format!("{id}: {e}"));
                }
            }
        }
    }
    println!("read_ok: {ok}");
    println!("read_missing: {missing}");
    println!("read_errors: {errors}");
    for m in &err_msgs {
        println!("read_issue: {m}");
    }
    println!("read_total_bytes: {total_bytes}");
    println!("read_len_mismatch_vs_actual: {len_mismatch}");
    println!("size_corrections_24bit: {size_corrections}");
    for c in &correction_examples {
        println!("correction: {c}");
    }

    fps.sort();
    let mut out = File::create(fp_out)?;
    for line in &fps {
        writeln!(out, "{line}")?;
    }
    println!("fingerprints_written: {} -> {fp_out}", fps.len());
    Ok(())
}

fn roundtrip(
    srcdir: &str,
    workdir: &str,
    name: &str,
    registry_json: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let src = Path::new(srcdir);
    let work = Path::new(workdir);
    std::fs::create_dir_all(work)?;

    let mut copied: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let fname = entry.file_name().to_string_lossy().to_string();
        if fname == format!("{name}.kfc")
            || fname == format!("{name}.kfc_resources")
            || (fname.starts_with(&format!("{name}_")) && fname.ends_with(".dat"))
        {
            let dst = work.join(&fname);
            std::fs::copy(entry.path(), &dst)?;
            copied.push(dst);
        }
    }
    println!("copied {} files to {}", copied.len(), workdir);
    let kfc_copy = work.join(format!("{name}.kfc"));
    let res_copy = work.join(format!("{name}.kfc_resources"));
    println!("pre_kfc_len: {}", std::fs::metadata(&kfc_copy)?.len());
    println!("pre_resources_len: {}", std::fs::metadata(&res_copy)?.len());

    let registry_file = File::open(registry_json)?;
    let registry: TypeRegistry = serde_json::from_reader(std::io::BufReader::new(registry_file))?;
    println!("registry_types: {}", registry.len());
    let registry = Rc::new(registry);

    let ref_file = Rc::new(KFCFile::from_path(&kfc_copy, false)?);
    println!("ref_resources: {}", ref_file.resources().len());

    // Exactly the mod-loader's construction (mod-loader-lua/src/load.rs create_writer).
    let writer = KFCWriter::new_incremental_with_options(
        work,
        name,
        ref_file,
        registry,
        KFCWriteOptions {
            overwrite_containers: true,
            ..Default::default()
        },
    )?;
    // ZERO modifications: no write_resource / write_content calls.
    match writer.finalize() {
        Ok(()) => println!("finalize: OK"),
        Err(e) => println!("finalize: ERROR: {e}"),
    }
    println!("post_kfc_len: {}", std::fs::metadata(&kfc_copy)?.len());
    println!("post_resources_len: {}", std::fs::metadata(&res_copy)?.len());
    Ok(())
}

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
    let mut first_chunk_diff: Option<String> = None;
    for (i, (x, y)) in chunks_a.iter().zip(chunks_b.iter()).enumerate() {
        if x.offset != y.offset || x.size != y.size || x.compressed_size != y.compressed_size
            || x.uncompressed_offset != y.uncompressed_offset || x.uncompressed_size != y.uncompressed_size
        {
            chunk_field_diffs += 1;
            if first_chunk_diff.is_none() {
                first_chunk_diff = Some(format!(
                    "chunk {i}: A(off={},size={},cs={},uo={},us={}) B(off={},size={},cs={},uo={},us={})",
                    x.offset, x.size, x.compressed_size, x.uncompressed_offset, x.uncompressed_size,
                    y.offset, y.size, y.compressed_size, y.uncompressed_offset, y.uncompressed_size
                ));
            }
        }
    }
    println!("chunk_field_diffs_in_common_range: {chunk_field_diffs}");
    if let Some(d) = first_chunk_diff {
        println!("first_chunk_diff: {d}");
    }

    // resources map
    let (ra, rb) = (a.resources(), b.resources());
    println!("resources_len: {} vs {}", ra.len(), rb.len());
    let keys_a: Vec<_> = ra.keys().to_vec();
    let keys_b: Vec<_> = rb.keys().to_vec();
    println!("resource_key_sequence_eq: {}", keys_a == keys_b);
    let set_a: HashSet<_> = keys_a.iter().collect();
    let set_b: HashSet<_> = keys_b.iter().collect();
    println!("resource_key_set_eq: {}", set_a == set_b);
    let mut val_diffs = 0usize;
    let mut keys_only_in_a = 0usize;
    let mut first_val_diff: Option<String> = None;
    for k in keys_a.iter() {
        let (x, y) = match (ra.get(k), rb.get(k)) {
            (Some(x), Some(y)) => (x, y),
            _ => {
                keys_only_in_a += 1;
                continue;
            }
        };
        if x.offset != y.offset || x.size != y.size {
            val_diffs += 1;
            if first_val_diff.is_none() {
                first_val_diff = Some(format!("{k}: A(off={},size={}) B(off={},size={})", x.offset, x.size, y.offset, y.size));
            }
        }
    }
    println!("resource_keys_only_in_a: {keys_only_in_a}");
    println!("resource_value_diffs: {val_diffs}");
    if let Some(d) = first_val_diff {
        println!("first_resource_value_diff: {d}");
    }
    // order comparison restricted to common prefix
    let mut first_order_diff: Option<usize> = None;
    for (i, (x, y)) in keys_a.iter().zip(keys_b.iter()).enumerate() {
        if x != y {
            first_order_diff = Some(i);
            break;
        }
    }
    println!("resource_first_key_order_diff_index: {:?}", first_order_diff);

    // contents map
    let (ca, cb) = (a.contents(), b.contents());
    println!("contents_len: {} vs {}", ca.len(), cb.len());
    let ckeys_a: Vec<_> = ca.keys().to_vec();
    let ckeys_b: Vec<_> = cb.keys().to_vec();
    println!("content_key_sequence_eq: {}", ckeys_a == ckeys_b);
    let cset_a: HashSet<_> = ckeys_a.iter().collect();
    let cset_b: HashSet<_> = ckeys_b.iter().collect();
    println!("content_key_set_eq: {}", cset_a == cset_b);
    let mut cval_diffs = 0usize;
    for k in ckeys_a.iter() {
        let (x, y) = match (ca.get(k), cb.get(k)) {
            (Some(x), Some(y)) => (x, y),
            _ => continue,
        };
        if x.offset != y.offset || x.flags != y.flags || x.container_index != y.container_index {
            cval_diffs += 1;
        }
    }
    println!("content_value_diffs: {cval_diffs}");

    // bundles map
    let (ba, bb) = (a.resource_bundles(), b.resource_bundles());
    println!("bundles_len: {} vs {}", ba.len(), bb.len());
    let bkeys_a: Vec<_> = ba.keys().to_vec();
    let bkeys_b: Vec<_> = bb.keys().to_vec();
    println!("bundle_key_sequence_eq: {}", bkeys_a == bkeys_b);
    let mut bval_diffs = 0usize;
    let mut b_hash_diffs = 0usize;
    let mut b_index_diffs = 0usize;
    let mut b_count_diffs = 0usize;
    for k in bkeys_a.iter() {
        let (x, y) = match (ba.get(k), bb.get(k)) {
            (Some(x), Some(y)) => (x, y),
            _ => continue,
        };
        if x.internal_hash != y.internal_hash { b_hash_diffs += 1; }
        if x.index != y.index { b_index_diffs += 1; }
        if x.count != y.count { b_count_diffs += 1; }
        if x.internal_hash != y.internal_hash || x.index != y.index || x.count != y.count {
            bval_diffs += 1;
        }
    }
    println!("bundle_value_diffs: {bval_diffs}");
    println!("bundle_internal_hash_diffs: {b_hash_diffs}");
    println!("bundle_index_diffs: {b_index_diffs}");
    println!("bundle_count_diffs: {b_count_diffs}");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("audit") => audit(&args[2], &args[3], &args[4]),
        Some("roundtrip") => roundtrip(&args[2], &args[3], &args[4], &args[5]),
        Some("cmpdir") => cmpdir(&args[2], &args[3], &args[4]),
        _ => {
            eprintln!("usage: audit|roundtrip|cmpdir ...");
            std::process::exit(2);
        }
    }
}
