//! The location of `ptr.db`: filesystem checks and the move of the file.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// The files of one SQLite database in WAL mode.
pub fn db_files(db: &Path) -> Vec<PathBuf> {
    let s = db.as_os_str().to_string_lossy();
    vec![db.to_path_buf(), PathBuf::from(format!("{s}-wal")), PathBuf::from(format!("{s}-shm"))]
}

/// The total size of the database files that exist.
pub fn db_size(db: &Path) -> u64 {
    db_files(db).iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum()
}

/// The filesystem type and the free bytes of `dir`. Uses `stat -f`.
pub fn fs_info(dir: &Path) -> Option<(String, u64)> {
    let out = std::process::Command::new("stat").args(["-f", "-c", "%T %a %S"]).arg(dir).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let mut it = s.split_whitespace();
    let ty = it.next()?.to_string();
    let avail: u64 = it.next()?.parse().ok()?;
    let bsize: u64 = it.next()?.parse().ok()?;
    Some((ty, avail * bsize))
}

/// The result of the location check.
pub enum FsVerdict {
    Ok,
    /// The location works, but the user must know this.
    Warn(String),
    /// The location must not be used.
    Refuse(String),
}

/// Check the filesystem type for a SQLite database in WAL mode.
pub fn check_fs(fs_type: &str) -> FsVerdict {
    let t = fs_type.to_ascii_lowercase();
    if ["nfs", "smb", "smb2", "cifs", "fuse.sshfs", "9p", "afs", "ceph", "glusterfs"].iter().any(|n| t == *n || t.starts_with("nfs")) {
        return FsVerdict::Refuse(format!(
            "This is a network filesystem ({fs_type}). SQLite WAL mode does not work on it, and the database can become corrupt."
        ));
    }
    if t == "msdos" || t == "vfat" || t == "fat" {
        return FsVerdict::Refuse(format!("This filesystem ({fs_type}) limits a file to 4 GB. The PTR database is much larger."));
    }
    if t.starts_with("fuse") {
        return FsVerdict::Warn(format!("This is a FUSE filesystem ({fs_type}). Make sure that it supports SQLite file locks."));
    }
    FsVerdict::Ok
}

/// True when the two directories are on the same filesystem.
fn same_fs(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.dev() == y.dev(),
        _ => false,
    }
}

/// Move the database files from `src` to `dst`. On the same filesystem
/// this renames the files. Else it copies the files, checks the sizes, and
/// deletes the old files. On an error or a cancel, it deletes the partial
/// copy and keeps the old files. `progress` gets (copied bytes, total bytes).
pub fn move_db(src: &Path, dst: &Path, cancel: &AtomicBool, mut progress: impl FnMut(u64, u64)) -> Result<(), String> {
    let dst_dir = dst.parent().ok_or("the new path has no folder")?;
    std::fs::create_dir_all(dst_dir).map_err(|e| e.to_string())?;
    if dst.exists() {
        return Err(format!("{} exists already", dst.display()));
    }
    let pairs: Vec<(PathBuf, PathBuf)> =
        db_files(src).into_iter().zip(db_files(dst)).filter(|(s, _)| s.exists()).collect();
    let total: u64 = pairs.iter().filter_map(|(s, _)| std::fs::metadata(s).ok()).map(|m| m.len()).sum();

    if src.parent().map(|p| same_fs(p, dst_dir)).unwrap_or(false) {
        for (s, d) in &pairs {
            std::fs::rename(s, d).map_err(|e| e.to_string())?;
        }
        progress(total, total);
        return Ok(());
    }

    if let Some((_, free)) = fs_info(dst_dir) {
        if free < total {
            return Err(format!("not enough free space: {} GB free, {} GB needed", free / 1_000_000_000, total.div_ceil(1_000_000_000)));
        }
    }

    let cleanup = |pairs: &[(PathBuf, PathBuf)]| {
        for (_, d) in pairs {
            let _ = std::fs::remove_file(d);
        }
    };
    let mut done = 0u64;
    let mut buf = vec![0u8; 8 << 20];
    for (s, d) in &pairs {
        let res = (|| -> Result<(), String> {
            let mut r = std::fs::File::open(s).map_err(|e| e.to_string())?;
            let mut w = std::fs::File::create(d).map_err(|e| e.to_string())?;
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err("cancelled".into());
                }
                let n = r.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                w.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                done += n as u64;
                progress(done, total);
            }
            w.sync_all().map_err(|e| e.to_string())?;
            let (a, b) = (std::fs::metadata(s).map_err(|e| e.to_string())?.len(), std::fs::metadata(d).map_err(|e| e.to_string())?.len());
            if a != b {
                return Err(format!("size check failed for {}", d.display()));
            }
            Ok(())
        })();
        if let Err(e) = res {
            cleanup(&pairs);
            return Err(e);
        }
    }
    for (s, _) in &pairs {
        let _ = std::fs::remove_file(s);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdicts() {
        assert!(matches!(check_fs("nfs4"), FsVerdict::Refuse(_)));
        assert!(matches!(check_fs("msdos"), FsVerdict::Refuse(_)));
        assert!(matches!(check_fs("fuseblk"), FsVerdict::Warn(_)));
        assert!(matches!(check_fs("ext2/ext3"), FsVerdict::Ok));
    }

    #[test]
    fn move_renames_on_same_fs() {
        let dir = std::env::temp_dir().join(format!("pichouse-place-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("a.db");
        std::fs::write(&src, b"x").unwrap();
        std::fs::write(dir.join("a.db-wal"), b"yy").unwrap();
        let dst = dir.join("sub/b.db");
        move_db(&src, &dst, &AtomicBool::new(false), |_, _| {}).unwrap();
        assert!(!src.exists());
        assert_eq!(db_size(&dst), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
