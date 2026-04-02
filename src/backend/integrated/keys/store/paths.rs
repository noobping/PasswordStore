use std::path::{Path, PathBuf};

pub(in crate::backend) fn ripasso_keys_dir() -> Result<PathBuf, String> {
    let data_dir = dirs_next::data_local_dir()
        .ok_or_else(|| "Could not determine the data folder.".to_string())?;
    Ok(data_dir.join(env!("CARGO_PKG_NAME")).join("keys"))
}

pub(super) fn ripasso_keys_v2_dir() -> Result<PathBuf, String> {
    let data_dir = dirs_next::data_local_dir()
        .ok_or_else(|| "Could not determine the data folder.".to_string())?;
    Ok(data_dir.join(env!("CARGO_PKG_NAME")).join("keys-v2"))
}

#[cfg(feature = "fidokey")]
pub(super) fn ripasso_fido_keys_dir() -> Result<PathBuf, String> {
    let data_dir = dirs_next::data_local_dir()
        .ok_or_else(|| "Could not determine the data folder.".to_string())?;
    Ok(data_dir.join(env!("CARGO_PKG_NAME")).join("keys-fido"))
}

pub(super) fn hardware_manifest_path(dir: &Path) -> PathBuf {
    dir.join("manifest.toml")
}

pub(super) fn hardware_public_key_path(dir: &Path) -> PathBuf {
    dir.join("public.asc")
}
