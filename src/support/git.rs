use std::path::Path;

use crate::support::runtime::git_network_operations_available;

pub(crate) fn has_git_repository(root: &str) -> bool {
    Path::new(root).join(".git").exists()
}

pub(crate) fn password_store_git_state_summary(root: &str) -> String {
    let has_repository = has_git_repository(root);
    let network_available = git_network_operations_available();

    if !has_repository {
        return format!(
            "Password store Git state: {root} -> no Git repository detected, local commits disabled, network operations disabled."
        );
    }

    if network_available {
        #[cfg(feature = "flatpak")]
        {
            return format!(
                "Password store Git state: {root} -> Git repository detected, local commits enabled, network operations enabled through host commands."
            );
        }

        #[cfg(not(feature = "flatpak"))]
        {
            return format!(
                "Password store Git state: {root} -> Git repository detected, local commits enabled, network operations enabled."
            );
        }
    }

    #[cfg(feature = "flatpak")]
    {
        format!(
            "Password store Git state: {root} -> Git repository detected, local commits enabled, network operations disabled because host command execution is unavailable."
        )
    }

    #[cfg(not(feature = "flatpak"))]
    {
        format!(
            "Password store Git state: {root} -> Git repository detected, local commits enabled, network operations disabled."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{has_git_repository, password_store_git_state_summary};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("passwordstore-git-{name}-{nanos}"))
    }

    #[test]
    fn git_repository_detection_checks_for_dot_git_metadata() {
        let git_store = temp_dir_path("git");
        let plain_store = temp_dir_path("plain");
        fs::create_dir_all(git_store.join(".git")).expect("create git metadata");
        fs::create_dir_all(&plain_store).expect("create plain store");

        assert!(has_git_repository(git_store.to_string_lossy().as_ref()));
        assert!(!has_git_repository(plain_store.to_string_lossy().as_ref()));

        let _ = fs::remove_dir_all(&git_store);
        let _ = fs::remove_dir_all(&plain_store);
    }

    #[test]
    fn plain_store_summary_reports_git_disabled() {
        let plain_store = temp_dir_path("plain-summary");
        fs::create_dir_all(&plain_store).expect("create plain store");

        let summary = password_store_git_state_summary(plain_store.to_string_lossy().as_ref());

        assert!(summary.contains("no Git repository detected"));
        assert!(summary.contains("local commits disabled"));

        let _ = fs::remove_dir_all(&plain_store);
    }
}
