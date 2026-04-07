use std::fs;
use std::io;
use std::path::Path;

pub fn ensure_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)
}

pub fn write_atomic_file(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(path, contents)
}

pub fn write_private_file(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    write_atomic_file(path, contents)
}
