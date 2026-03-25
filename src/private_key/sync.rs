use crate::backend::{
    armored_ripasso_private_key, list_ripasso_private_keys, remove_ripasso_private_key,
    ripasso_private_key_requires_passphrase, store_ripasso_private_key_bytes,
    ManagedRipassoPrivateKeyProtection,
};

#[cfg(target_os = "linux")]
use crate::backend::{
    armored_host_gpg_private_key, delete_host_gpg_private_key, import_host_gpg_private_key_bytes,
    list_host_gpg_private_keys,
};

use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateKeySyncDirection {
    HostToApp,
    AppToHost,
}

pub fn preflight_host_to_app_private_key_sync() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let app_fingerprints = app_private_key_fingerprints()?;
        for fingerprint in host_private_key_fingerprints()? {
            if app_fingerprints.contains(&normalized_fingerprint(&fingerprint)) {
                continue;
            }

            let armored = armored_host_gpg_private_key(&fingerprint)?;
            if !ripasso_private_key_requires_passphrase(armored.as_bytes())
                .map_err(|err| err.to_string())?
            {
                return Err(
                    "Every synced host key must be password protected before Keycord can store it."
                        .to_string(),
                );
            }
        }

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err("Private-key sync with the host is only available on Linux.".to_string())
    }
}

pub fn sync_private_keys_with_host(direction: PrivateKeySyncDirection) -> Result<(), String> {
    match direction {
        PrivateKeySyncDirection::HostToApp => sync_host_private_keys_to_app(),
        PrivateKeySyncDirection::AppToHost => sync_app_private_keys_to_host(),
    }
}

fn app_private_key_fingerprints() -> Result<HashSet<String>, String> {
    Ok(list_ripasso_private_keys()?
        .into_iter()
        .map(|key| normalized_fingerprint(&key.fingerprint))
        .collect())
}

#[cfg(target_os = "linux")]
fn host_private_key_fingerprints() -> Result<HashSet<String>, String> {
    Ok(list_host_gpg_private_keys()?
        .into_iter()
        .map(|key| normalized_fingerprint(&key.fingerprint))
        .collect())
}

fn normalized_fingerprint(fingerprint: &str) -> String {
    fingerprint.trim().to_ascii_lowercase()
}

fn sync_host_private_keys_to_app() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let host_keys = list_host_gpg_private_keys()?;
        let app_keys = list_ripasso_private_keys()?;
        let host_fingerprints = host_keys
            .iter()
            .map(|key| normalized_fingerprint(&key.fingerprint))
            .collect::<HashSet<_>>();
        let app_fingerprints = app_keys
            .iter()
            .map(|key| normalized_fingerprint(&key.fingerprint))
            .collect::<HashSet<_>>();

        let host_exports = host_keys
            .into_iter()
            .filter(|key| !app_fingerprints.contains(&normalized_fingerprint(&key.fingerprint)))
            .map(|key| {
                let armored = armored_host_gpg_private_key(&key.fingerprint)?;
                if !ripasso_private_key_requires_passphrase(armored.as_bytes())
                    .map_err(|err| err.to_string())?
                {
                    return Err(
                        "Every synced host key must be password protected before Keycord can store it."
                            .to_string(),
                    );
                }
                Ok((key.fingerprint, armored))
            })
            .collect::<Result<Vec<_>, String>>()?;

        for (_, armored) in host_exports {
            store_ripasso_private_key_bytes(armored.as_bytes()).map_err(|err| err.to_string())?;
        }

        for key in app_keys {
            if !matches!(key.protection, ManagedRipassoPrivateKeyProtection::Password) {
                continue;
            }
            if !host_fingerprints.contains(&normalized_fingerprint(&key.fingerprint)) {
                remove_ripasso_private_key(&key.fingerprint)?;
            }
        }

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err("Private-key sync with the host is only available on Linux.".to_string())
    }
}

fn sync_app_private_keys_to_host() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let app_keys = list_ripasso_private_keys()?;
        let host_keys = list_host_gpg_private_keys()?;
        let app_fingerprints = app_keys
            .iter()
            .map(|key| normalized_fingerprint(&key.fingerprint))
            .collect::<HashSet<_>>();
        let host_fingerprints = host_keys
            .iter()
            .map(|key| normalized_fingerprint(&key.fingerprint))
            .collect::<HashSet<_>>();

        let app_exports = app_keys
            .into_iter()
            .filter(|key| matches!(key.protection, ManagedRipassoPrivateKeyProtection::Password))
            .filter(|key| !host_fingerprints.contains(&normalized_fingerprint(&key.fingerprint)))
            .map(|key| {
                armored_ripasso_private_key(&key.fingerprint)
                    .map(|armored| (key.fingerprint, armored))
            })
            .collect::<Result<Vec<_>, String>>()?;

        for (_, armored) in app_exports {
            import_host_gpg_private_key_bytes(armored.as_bytes())?;
        }

        for key in host_keys {
            if !app_fingerprints.contains(&normalized_fingerprint(&key.fingerprint)) {
                delete_host_gpg_private_key(&key.fingerprint)?;
            }
        }

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err("Private-key sync with the host is only available on Linux.".to_string())
    }
}
