// KFC4 contents-addressing census harness.
//
// Subcommands:
//   dump <kfc_path> <out_dir>   - parse with kfc-base, write contents.csv +
//                                 containers.csv into <out_dir>
//   rawcheck <kfc_path>         - independently re-read the ContentHash /
//                                 ContentEntry arrays and the ContainerInfo
//                                 records as raw little-endian bytes at the
//                                 spec §2.1 offsets, and verify that
//                                 keys[i] <-> values[i] pairing and every
//                                 container record agree with the kfc-base
//                                 parse (same values, pairing included).
//
// Layout facts used by rawcheck (KFC4-SPEC §2.1, §4.1, §4.5, §4.6):
//   containers:     offset 216,      64 x { u64 size; u64 count; }
//   content_keys:   offset 1,418,760, N x { u32 size; u32 hash0; u32 hash1; u32 hash2; }
//   content_values: offset 3,042,104, N x { u32 offset; u16 flags; u16 container_index; u8 pad[8]; }

use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

use kfc_base::container::KFCFile;

const CONTAINERS_OFF: u64 = 216;
const KEYS_OFF: u64 = 1_418_760;
const VALUES_OFF: u64 = 3_042_104;

/// Full hash as stored on disk: LE bytes of size ‖ hash0 ‖ hash1 ‖ hash2.
fn hash_hex(size: u32, h0: u32, h1: u32, h2: u32) -> String {
    let mut s = String::with_capacity(32);
    for v in [size, h0, h1, h2] {
        for b in v.to_le_bytes() {
            s.push_str(&format!("{b:02x}"));
        }
    }
    s
}

fn dump(kfc_path: &str, out_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file = KFCFile::from_path(Path::new(kfc_path), false)?;
    let contents = file.contents();
    let containers = file.containers();

    println!("version_tag: {}", file.game_version());
    println!("contents_len: {}", contents.len());
    println!("containers_len: {}", containers.len());

    std::fs::create_dir_all(out_dir)?;

    // containers.csv — raw directory records + measured per-container counts
    let mut measured: HashMap<usize, u64> = HashMap::new();
    for (_, entry) in contents.iter() {
        *measured.entry(entry.container_index).or_default() += 1;
    }
    let mut cpath = Path::new(out_dir).join("containers.csv");
    let mut cf = File::create(&mut cpath)?;
    writeln!(cf, "index,size,count,measured_entries")?;
    for (i, c) in containers.iter().enumerate() {
        writeln!(
            cf,
            "{i},{},{},{}",
            c.size,
            c.count,
            measured.get(&i).copied().unwrap_or(0)
        )?;
    }
    println!("containers_csv: {}", cpath.display());

    // contents.csv — one row per entry, in the map's keys[] file order
    let path = Path::new(out_dir).join("contents.csv");
    let mut f = File::create(&path)?;
    writeln!(
        f,
        "content_hash,size,hash0,hash1,hash2,offset,flags,container_index"
    )?;
    let mut dup_check: HashMap<String, u64> = HashMap::new();
    let mut dupes = 0u64;
    for (key, entry) in contents.iter() {
        let hex = hash_hex(key.size(), key.hash0(), key.hash1(), key.hash2());
        let n = dup_check.entry(hex.clone()).or_default();
        *n += 1;
        if *n > 1 {
            dupes += 1;
        }
        writeln!(
            f,
            "{},{},{:08x},{:08x},{:08x},{},{},{}",
            hex,
            key.size(),
            key.hash0(),
            key.hash1(),
            key.hash2(),
            entry.offset,
            entry.flags,
            entry.container_index
        )?;
    }
    println!("contents_csv: {}", path.display());
    println!("duplicate_hash_rows_in_parse: {dupes}");
    Ok(())
}

fn read_at(f: &mut File, off: u64, buf: &mut [u8]) -> std::io::Result<()> {
    f.seek(SeekFrom::Start(off))?;
    f.read_exact(buf)
}

fn rawcheck(kfc_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file = KFCFile::from_path(Path::new(kfc_path), false)?;
    let contents = file.contents();
    let n = contents.len();
    println!("parsed_contents: {n}");

    let mut raw = File::open(kfc_path)?;

    // Pairing check: raw keys[i] must equal parsed keys()[i] field-for-field,
    // and raw values[i] must equal parsed values()[i] field-for-field.
    let mut key_mismatch = 0u64;
    let mut val_mismatch = 0u64;
    let mut first_key_mm: Option<usize> = None;
    let mut first_val_mm: Option<usize> = None;
    let mut buf16 = [0u8; 16];
    for i in 0..n {
        read_at(&mut raw, KEYS_OFF + 16 * i as u64, &mut buf16)?;
        let size = u32::from_le_bytes(buf16[0..4].try_into().unwrap());
        let h0 = u32::from_le_bytes(buf16[4..8].try_into().unwrap());
        let h1 = u32::from_le_bytes(buf16[8..12].try_into().unwrap());
        let h2 = u32::from_le_bytes(buf16[12..16].try_into().unwrap());
        let k = contents.keys()[i];
        if size != k.size() || h0 != k.hash0() || h1 != k.hash1() || h2 != k.hash2() {
            key_mismatch += 1;
            if first_key_mm.is_none() {
                first_key_mm = Some(i);
            }
        }

        read_at(&mut raw, VALUES_OFF + 16 * i as u64, &mut buf16)?;
        let off = u32::from_le_bytes(buf16[0..4].try_into().unwrap()) as u64;
        let flags = u16::from_le_bytes(buf16[4..6].try_into().unwrap());
        let ci = u16::from_le_bytes(buf16[6..8].try_into().unwrap()) as usize;
        let pad_nonzero = buf16[8..16].iter().any(|&b| b != 0);
        let v = &contents.values()[i];
        if off != v.offset || flags != v.flags || ci != v.container_index || pad_nonzero {
            val_mismatch += 1;
            if first_val_mm.is_none() {
                first_val_mm = Some(i);
            }
        }
    }
    println!("rawcheck_key_mismatches: {key_mismatch} (first: {first_key_mm:?})");
    println!("rawcheck_value_mismatches: {val_mismatch} (first: {first_val_mm:?})");

    // Container records: raw {u64 size; u64 count} vs parsed.
    let mut cont_mismatch = 0u64;
    for (i, c) in file.containers().iter().enumerate() {
        read_at(&mut raw, CONTAINERS_OFF + 16 * i as u64, &mut buf16)?;
        let size = u64::from_le_bytes(buf16[0..8].try_into().unwrap());
        let count = u64::from_le_bytes(buf16[8..16].try_into().unwrap());
        if size != c.size || count as usize != c.count {
            cont_mismatch += 1;
        }
    }
    println!("rawcheck_container_mismatches: {cont_mismatch}");
    Ok(())
}

/// Compute the content GUID of an empty payload via kfc-base's
/// `ContentHash::from_data` (AES-based `compute_content_guid`, seed 0)
/// and print it — used to identify the size-0 entry in the census.
fn emptyhash() -> Result<(), Box<dyn std::error::Error>> {
    use kfc_base::guid::ContentHash;
    let h = ContentHash::from_data(&[]);
    println!(
        "empty_payload_hash: size={} hash0={:08x} hash1={:08x} hash2={:08x}",
        h.size(),
        h.hash0(),
        h.hash1(),
        h.hash2()
    );
    println!(
        "empty_payload_hash_hex: {}",
        hash_hex(h.size(), h.hash0(), h.hash1(), h.hash2())
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("dump") => dump(&args[2], &args[3]),
        Some("rawcheck") => rawcheck(&args[2]),
        Some("emptyhash") => emptyhash(),
        _ => {
            eprintln!("usage: dump <kfc> <out_dir> | rawcheck <kfc> | emptyhash");
            std::process::exit(2);
        }
    }
}
