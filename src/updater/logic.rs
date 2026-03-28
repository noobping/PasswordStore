use semver::Version;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseCandidate {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedRelease {
    pub tag_name: String,
    pub version: Version,
    pub asset: ReleaseAsset,
}

pub fn parse_release_version(tag_name: &str) -> Option<Version> {
    let trimmed = tag_name.trim();
    let normalized = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    Version::parse(normalized).ok()
}

pub fn select_update_release(
    current_version: &str,
    releases: &[ReleaseCandidate],
) -> Option<SelectedRelease> {
    let current = Version::parse(current_version).ok()?;

    releases
        .iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter_map(|release| {
            let version = parse_release_version(&release.tag_name)?;
            if version <= current {
                return None;
            }

            let asset = release
                .assets
                .iter()
                .find(|asset| asset.name.to_ascii_lowercase().ends_with(".msi"))?
                .clone();

            Some(SelectedRelease {
                tag_name: release.tag_name.clone(),
                version,
                asset,
            })
        })
        .max_by(|left, right| left.version.cmp(&right.version))
}

pub fn cached_installer_matches(path: &Path, expected_size: u64) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() == expected_size)
}

pub fn any_dirty(flags: impl IntoIterator<Item = bool>) -> bool {
    flags.into_iter().any(|flag| flag)
}

#[cfg(test)]
mod tests {
    use super::{
        any_dirty, cached_installer_matches, parse_release_version, select_update_release,
        ReleaseAsset, ReleaseCandidate,
    };
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn asset(name: &str, size: u64) -> ReleaseAsset {
        ReleaseAsset {
            name: name.to_string(),
            browser_download_url: format!("https://example.com/{name}"),
            size,
        }
    }

    fn release(
        tag_name: &str,
        draft: bool,
        prerelease: bool,
        assets: Vec<ReleaseAsset>,
    ) -> ReleaseCandidate {
        ReleaseCandidate {
            tag_name: tag_name.to_string(),
            draft,
            prerelease,
            assets,
        }
    }

    #[test]
    fn parses_semver_tags_with_or_without_v_prefix() {
        assert_eq!(
            parse_release_version("v1.2.3").map(|version| version.to_string()),
            Some("1.2.3".to_string())
        );
        assert_eq!(
            parse_release_version("1.2.3").map(|version| version.to_string()),
            Some("1.2.3".to_string())
        );
        assert!(parse_release_version("release-1.2.3").is_none());
    }

    #[test]
    fn selects_the_highest_newer_stable_release_with_an_msi_asset() {
        let releases = vec![
            release("v1.0.1", false, false, vec![asset("keycord.zip", 10)]),
            release("v1.1.0", false, true, vec![asset("keycord.msi", 20)]),
            release("v1.2.0", true, false, vec![asset("keycord.msi", 30)]),
            release("v1.3.0", false, false, vec![asset("keycord.msi", 40)]),
            release("v1.4.0", false, false, vec![asset("keycord.exe", 50)]),
        ];

        let selected = select_update_release("1.0.0", &releases).expect("expected release");
        assert_eq!(selected.version.to_string(), "1.3.0");
        assert_eq!(selected.asset.name, "keycord.msi");
    }

    #[test]
    fn ignores_equal_or_older_versions() {
        let releases = vec![
            release("v0.9.9", false, false, vec![asset("keycord.msi", 10)]),
            release("v1.0.0", false, false, vec![asset("keycord.msi", 20)]),
        ];

        assert!(select_update_release("1.0.0", &releases).is_none());
    }

    #[test]
    fn cached_installer_check_requires_an_exact_file_size_match() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("keycord-updater-{nanos}.msi"));
        fs::write(&path, b"12345").expect("write cached installer");

        assert!(cached_installer_matches(&path, 5));
        assert!(!cached_installer_matches(&path, 4));

        fs::remove_file(&path).expect("remove cached installer");
    }

    #[test]
    fn dirty_state_helper_reports_when_any_window_has_unsaved_work() {
        assert!(any_dirty([false, true, false]));
        assert!(!any_dirty([false, false, false]));
    }
}
