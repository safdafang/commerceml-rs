//! Unpacking uploaded archives with limits against zip-slip and zip bombs.

use std::io::Read;

use crate::storage::{FileStorage, sanitize_name};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    /// Most bytes to unpack in total.
    pub(crate) max_total: u64,
    /// Most entries per archive.
    pub(crate) max_entries: usize,
}

/// Unpacks `archive` next to it and deletes it. Returns the unpacked names
/// and their total size. Sizes are counted while decompressing, so lying
/// headers do not help a zip bomb.
pub(crate) fn extract(
    storage: &dyn FileStorage,
    session: &str,
    archive: &str,
    limits: &Limits,
) -> Result<(Vec<String>, u64), String> {
    let file = storage.open(session, archive).map_err(|e| e.to_string())?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| format!("{archive}: not a valid zip archive: {e}"))?;
    if zip.len() > limits.max_entries {
        return Err(format!("{archive}: more than {} entries", limits.max_entries));
    }
    let mut names = Vec::new();
    let mut total = 0u64;
    let mut buf = vec![0u8; 64 * 1024];
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("{archive}: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .enclosed_name()
            .and_then(|p| sanitize_name(&p.to_string_lossy()))
            .ok_or_else(|| format!("{archive}: unsafe path {:?}", entry.name().unwrap_or_default()))?;
        storage.remove(session, &name).map_err(|e| e.to_string())?;
        loop {
            let n = entry
                .read(&mut buf)
                .map_err(|e| format!("{archive}: {name}: {e}"))?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > limits.max_total {
                return Err(format!(
                    "{archive}: unpacked size exceeds {} bytes",
                    limits.max_total
                ));
            }
            storage
                .append(session, &name, &buf[..n])
                .map_err(|e| e.to_string())?;
        }
        names.push(name);
    }
    drop(zip);
    storage.remove(session, archive).map_err(|e| e.to_string())?;
    Ok((names, total))
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::write::SimpleFileOptions;

    use super::*;
    use crate::storage::MemoryStorage;

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in entries {
            w.start_file(*name, SimpleFileOptions::default()).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    const LIMITS: Limits = Limits {
        max_total: 1024,
        max_entries: 10,
    };

    #[test]
    fn extracts_and_removes_archive() {
        let s = MemoryStorage::new();
        s.append(
            "s",
            "pack.zip",
            &archive(&[("import.xml", b"<a/>"), ("import_files/ab/x.jpg", b"jpg")]),
        )
        .unwrap();
        let (names, total) = extract(&s, "s", "pack.zip", &LIMITS).unwrap();
        assert_eq!(names, ["import.xml", "import_files/ab/x.jpg"]);
        assert_eq!(total, 7);
        assert_eq!(s.list("s").unwrap(), ["import.xml", "import_files/ab/x.jpg"]);
    }

    #[test]
    fn rejects_zip_slip_and_bombs() {
        let s = MemoryStorage::new();
        s.append("s", "slip.zip", &archive(&[("../../evil.xml", b"x")]))
            .unwrap();
        assert!(
            extract(&s, "s", "slip.zip", &LIMITS)
                .unwrap_err()
                .contains("unsafe path")
        );

        let big = vec![0u8; 4096];
        s.append("s", "bomb.zip", &archive(&[("big.xml", &big)])).unwrap();
        assert!(
            extract(&s, "s", "bomb.zip", &LIMITS)
                .unwrap_err()
                .contains("exceeds")
        );

        let many: Vec<(String, &[u8])> = (0..11).map(|i| (format!("f{i}.xml"), &b"x"[..])).collect();
        let refs: Vec<(&str, &[u8])> = many.iter().map(|(n, d)| (n.as_str(), *d)).collect();
        s.append("s", "many.zip", &archive(&refs)).unwrap();
        assert!(
            extract(&s, "s", "many.zip", &LIMITS)
                .unwrap_err()
                .contains("entries")
        );

        s.append("s", "junk.zip", b"not a zip").unwrap();
        assert!(
            extract(&s, "s", "junk.zip", &LIMITS)
                .unwrap_err()
                .contains("not a valid zip")
        );
    }
}
