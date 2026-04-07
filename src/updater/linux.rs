use super::common::DownloadedUpdate;
use super::logic::{select_update_release_by, ReleaseCandidate, SelectedRelease};
use crate::i18n::gettext;
use crate::logging::log_error;
use crate::setup::{install_locally, is_current_executable_installed_locally};
use adw::glib;
use adw::prelude::*;
use adw::AlertDialog;
use rand::random;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process;

const AUTO_INSTALL_ARG: &str = "--auto-install";
const UPDATE_STAGING_DIR_MODE: u32 = 0o700;
const UPDATE_EXECUTABLE_MODE: u32 = 0o700;
const UPDATE_DIR_ATTEMPTS: usize = 32;

pub fn supports_updater() -> bool {
    is_current_executable_installed_locally()
}

pub fn update_check_body() -> &'static str {
    "Looking for a newer Linux standalone build on GitHub Releases."
}

pub fn update_available_description() -> &'static str {
    "A newer Linux standalone build is available."
}

pub fn ready_status() -> &'static str {
    "The update is ready to install."
}

pub fn install_failed_toast() -> &'static str {
    "Couldn't start the installer."
}

pub fn select_update_release(
    current_version: &str,
    releases: &[ReleaseCandidate],
) -> Result<Option<SelectedRelease>, String> {
    let Some(arch) = release_arch() else {
        return Err(format!(
            "Unsupported Linux update architecture: {}",
            std::env::consts::ARCH
        ));
    };

    Ok(select_update_release_by(
        current_version,
        releases,
        |release, asset| asset.name == linux_release_asset_name(&release.tag_name, arch),
    ))
}

pub fn download_target(release: &SelectedRelease) -> Result<DownloadedUpdate, String> {
    let dir = create_update_dir()?;
    Ok(DownloadedUpdate {
        path: dir.join(&release.asset.name),
        size: release.asset.size,
        cleanup_dir: Some(dir),
    })
}

pub fn cleanup_download(download: &DownloadedUpdate) {
    if let Some(dir) = &download.cleanup_dir {
        let _ = fs::remove_dir_all(dir);
        return;
    }

    let _ = fs::remove_file(&download.path);
}

pub fn launch_update(download: &DownloadedUpdate) -> Result<(), String> {
    let mut perms = fs::metadata(&download.path)
        .map_err(|error| format!("Failed to read update file metadata: {error}"))?
        .permissions();
    perms.set_mode(UPDATE_EXECUTABLE_MODE);
    fs::set_permissions(&download.path, perms)
        .map_err(|error| format!("Failed to make the downloaded update executable: {error}"))?;

    let Some(cleanup_dir) = download.cleanup_dir.as_ref() else {
        return Err("Linux update is missing its cleanup directory.".to_string());
    };

    process::Command::new(&download.path)
        .arg(AUTO_INSTALL_ARG)
        .arg(cleanup_dir)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to start Linux update install helper: {error}"))
}

pub fn handle_special_command(args: &[OsString]) -> Option<glib::ExitCode> {
    let cleanup_dir = auto_install_cleanup_dir(args)?;
    Some(run_auto_install(&cleanup_dir))
}

fn auto_install_cleanup_dir(args: &[OsString]) -> Option<PathBuf> {
    if args.get(1).is_none_or(|arg| arg != AUTO_INSTALL_ARG) {
        return None;
    }

    args.get(2).map(PathBuf::from)
}

fn run_auto_install(cleanup_dir: &Path) -> glib::ExitCode {
    crate::i18n::init();

    let result = auto_install_update(cleanup_dir);
    match result {
        Ok(()) => 0.into(),
        Err(error) => {
            log_error(format!("Linux auto-install failed: {error}"));
            eprintln!("Keycord update install failed: {error}");
            show_auto_install_error_dialog(&error);
            1.into()
        }
    }
}

fn auto_install_update(cleanup_dir: &Path) -> Result<(), String> {
    let current_exe = std::env::current_exe()
        .map_err(|error| format!("Failed to resolve the update helper executable: {error}"))?;
    let Some(parent) = current_exe.parent() else {
        return Err("The update helper executable has no parent directory.".to_string());
    };
    if parent != cleanup_dir {
        return Err(
            "The update helper cleanup directory does not match the downloaded update location."
                .to_string(),
        );
    }

    install_locally()
        .map_err(|error| format!("Failed to install the downloaded update: {error}"))?;
    if let Err(error) = fs::remove_dir_all(cleanup_dir) {
        eprintln!(
            "Keycord update cleanup failed for '{}': {error}",
            cleanup_dir.display()
        );
    }
    Ok(())
}

fn release_arch() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Some("x86_64"),
        "aarch64" => Some("aarch64"),
        _ => None,
    }
}

fn linux_release_asset_name(tag_name: &str, arch: &str) -> String {
    format!("{}-{tag_name}.{arch}", env!("CARGO_PKG_NAME"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UpdateStagingRoot {
    path: PathBuf,
    needs_creation: bool,
}

fn create_update_dir() -> Result<PathBuf, String> {
    create_update_dir_in(&update_staging_root())
}

fn update_staging_root() -> UpdateStagingRoot {
    if let Some(base) = dirs_next::cache_dir().or_else(dirs_next::data_local_dir) {
        UpdateStagingRoot {
            path: base.join(env!("CARGO_PKG_NAME")).join("updates"),
            needs_creation: true,
        }
    } else {
        UpdateStagingRoot {
            path: std::env::temp_dir(),
            needs_creation: false,
        }
    }
}

fn create_update_dir_in(root: &UpdateStagingRoot) -> Result<PathBuf, String> {
    ensure_update_staging_root(root)?;

    for _ in 0..UPDATE_DIR_ATTEMPTS {
        let candidate = root.path.join(format!("update-{:032x}", random::<u128>()));
        match create_private_update_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to create Linux update staging directory '{}': {error}",
                    candidate.display()
                ));
            }
        }
    }

    Err("Failed to allocate a private Linux update staging directory.".to_string())
}

fn ensure_update_staging_root(root: &UpdateStagingRoot) -> Result<(), String> {
    if !root.needs_creation {
        return Ok(());
    }

    fs::create_dir_all(&root.path).map_err(|error| {
        format!(
            "Failed to create Linux update staging root '{}': {error}",
            root.path.display()
        )
    })?;
    let mut perms = fs::metadata(&root.path)
        .map_err(|error| {
            format!(
                "Failed to read Linux update staging root metadata '{}': {error}",
                root.path.display()
            )
        })?
        .permissions();
    perms.set_mode(UPDATE_STAGING_DIR_MODE);
    fs::set_permissions(&root.path, perms).map_err(|error| {
        format!(
            "Failed to secure Linux update staging root '{}': {error}",
            root.path.display()
        )
    })
}

fn create_private_update_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.mode(UPDATE_STAGING_DIR_MODE);
    builder.create(path)
}

fn show_auto_install_error_dialog(error: &str) {
    if adw::init().is_err() {
        return;
    }

    let body = format!(
        "{}\n\n{}",
        gettext("Keycord couldn't finish installing the downloaded Linux update."),
        error
    );
    let dialog = AlertDialog::new(Some(&gettext("Couldn't install the update.")), Some(&body));
    dialog.add_response("close", &gettext("Close"));
    dialog.set_close_response("close");
    dialog.set_default_response(Some("close"));

    let loop_ = glib::MainLoop::new(None, false);
    let loop_for_response = loop_.clone();
    dialog.connect_response(None, move |dialog, _| {
        dialog.close();
        loop_for_response.quit();
    });

    dialog.present(None::<&adw::gtk::Widget>);
    loop_.run();
}

#[cfg(test)]
mod tests {
    use super::{
        auto_install_cleanup_dir, create_update_dir_in, linux_release_asset_name, release_arch,
        UpdateStagingRoot, UPDATE_STAGING_DIR_MODE,
    };
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::process;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_test_root() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);

        let root = std::env::temp_dir().join(format!(
            "keycord-updater-test-{}-{}",
            process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("create updater test root");
        root
    }

    #[test]
    fn auto_install_command_extracts_cleanup_directory() {
        let args = vec![
            OsString::from("keycord"),
            OsString::from("--auto-install"),
            OsString::from("/tmp/keycord-update"),
        ];

        assert_eq!(
            auto_install_cleanup_dir(&args)
                .expect("expected cleanup dir")
                .to_string_lossy(),
            "/tmp/keycord-update"
        );
    }

    #[test]
    fn non_auto_install_arguments_are_ignored() {
        let args = vec![OsString::from("keycord"), OsString::from("query")];
        assert!(auto_install_cleanup_dir(&args).is_none());
    }

    #[test]
    fn linux_release_asset_name_matches_publish_format() {
        assert_eq!(
            linux_release_asset_name("v1.2.3", "x86_64"),
            "keycord-v1.2.3.x86_64".to_string()
        );
    }

    #[test]
    fn supported_release_arch_is_publishable_when_present() {
        if let Some(arch) = release_arch() {
            assert!(matches!(arch, "x86_64" | "aarch64"));
        }
    }

    #[test]
    fn update_dirs_are_private_and_unpredictable() {
        let root = temp_test_root();
        let staging_root = UpdateStagingRoot {
            path: root.join("updates"),
            needs_creation: true,
        };

        let first = create_update_dir_in(&staging_root).expect("create first update dir");
        let second = create_update_dir_in(&staging_root).expect("create second update dir");

        assert_ne!(first, second);
        assert!(first.is_dir());
        assert!(second.is_dir());
        assert_eq!(
            fs::metadata(&first)
                .expect("first dir metadata")
                .permissions()
                .mode()
                & 0o777,
            UPDATE_STAGING_DIR_MODE
        );
        assert_eq!(
            fs::metadata(&second)
                .expect("second dir metadata")
                .permissions()
                .mode()
                & 0o777,
            UPDATE_STAGING_DIR_MODE
        );

        let _ = fs::remove_dir_all(root);
    }
}
