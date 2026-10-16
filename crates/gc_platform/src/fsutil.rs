//! File helpers that survive a crash or a full disk.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

/// Write `bytes` to `path` so that a reader (or a crash halfway through)
/// sees either the old file or the new one, never half of the new one:
/// write a temporary file beside it, then rename it over the target. The
/// folder is made if it is missing. There is no fsync (it stalls the frame
/// loop for milliseconds), so this guards against a crash of the game, not
/// against power loss; see [`write_durable`] for that.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write(path, bytes, false)
}

/// [`write_atomic`] that also flushes the data to disk before the rename,
/// so a power cut cannot leave an empty file. For small files saved rarely
/// (settings); it can take many milliseconds.
pub fn write_durable(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write(path, bytes, true)
}

fn write(path: &Path, bytes: &[u8], sync: bool) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    // The process id keeps two running copies of the game from sharing a
    // temporary file.
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp{}", std::process::id()));
    let tmp = Path::new(&tmp);
    let result = (|| {
        let mut file = File::create(tmp)?;
        file.write_all(bytes)?;
        if sync {
            file.sync_all()?;
        }
        // Replaces an existing file on Unix and on Windows.
        fs::rename(tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("gc_fsutil_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn writes_and_replaces_without_leaving_a_temp_file() {
        let dir = temp("replace");
        let path = dir.join("sub").join("a.txt");
        write_atomic(&path, b"first").unwrap();
        write_durable(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        let names: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["a.txt"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_write_keeps_the_old_file() {
        let dir = temp("fail");
        let path = dir.join("keep.txt");
        write_atomic(&path, b"old").unwrap();
        // A directory where the temp file should go makes the write fail.
        fs::create_dir(dir.join(format!("keep.txt.tmp{}", std::process::id()))).unwrap();
        assert!(write_atomic(&path, b"new").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"old");
        let _ = fs::remove_dir_all(&dir);
    }
}
