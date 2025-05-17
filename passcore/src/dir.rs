use anyhow::{Result, anyhow};
use directories::BaseDirs;
use std::env;
use std::path::PathBuf;

/// Default sub‑directory used by *pass* when PASSWORD_STORE_DIR is not set.
const DEFAULT_STORE_DIR: &str = ".password-store";

/// Determine the password‑store directory.
pub fn discover_store_dir() -> Result<PathBuf> {
    if let Ok(dir) = env::var("PASSWORD_STORE_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let base = BaseDirs::new().ok_or_else(|| anyhow!("Could not determine home directory"))?;
    Ok(base.home_dir().join(DEFAULT_STORE_DIR))
}

pub fn exists_store_dir() -> bool {
    let store_dir = match discover_store_dir() {
        Ok(dir) => dir,
        Err(_) => return false,
    };
    if store_dir.exists() {
        return true;
    }
    return false;
}
