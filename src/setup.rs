use adw::gio::{self, ResourceLookupFlags};
use std::io::{Error, ErrorKind};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::{env, fs};
use std::{
    process,
    time::{SystemTime, UNIX_EPOCH},
};

const APP_ID: &str = env!("APP_ID");
const GETTEXT_DOMAIN: &str = env!("GETTEXT_DOMAIN");
const LOCALEDIR: &str = env!("LOCALEDIR");
const RESOURCE_ID: &str = env!("RESOURCE_ID");
const AVAILABLE_LOCALES: &str = env!("AVAILABLE_LOCALES");
const SEARCH_PROVIDER_BUS_NAME: &str = env!("SEARCH_PROVIDER_BUS_NAME");
const SEARCH_PROVIDER_OBJECT_PATH: &str = env!("SEARCH_PROVIDER_OBJECT_PATH");

pub fn local_menu_action_label(installed: bool) -> &'static str {
    if installed {
        "Remove from app menu"
    } else {
        "Add to app menu"
    }
}

pub fn can_install_locally() -> bool {
    let Some(bin) = dirs_next::executable_dir() else {
        return false;
    };
    let Some(data) = dirs_next::data_dir() else {
        return false;
    };

    can_install_into(&bin, &data)
}

pub fn is_installed_locally() -> bool {
    let Some(bin) = installed_local_binary_path() else {
        return false;
    };
    let Some(data) = dirs_next::data_dir() else {
        return false;
    };
    let desktop = data
        .join("applications")
        .join(format!("{}.desktop", APP_ID));
    bin.exists() && bin.is_file() && desktop.exists() && desktop.is_file()
}

pub fn is_current_executable_installed_locally() -> bool {
    let Ok(current_exe) = std::env::current_exe() else {
        return false;
    };
    let Some(installed_exe) = installed_local_binary_path() else {
        return false;
    };

    same_file_path(&current_exe, &installed_exe)
}

fn installed_local_binary_path() -> Option<PathBuf> {
    let Some(bin) = dirs_next::executable_dir() else {
        return None;
    };

    Some(bin.join(env!("CARGO_PKG_NAME")))
}

pub fn install_locally() -> std::io::Result<()> {
    let project = env!("CARGO_PKG_NAME");
    let exe_path = std::env::current_exe()?;
    let Some(bin) = dirs_next::executable_dir() else {
        return Err(Error::new(
            ErrorKind::NotFound,
            "No executable directory found",
        ));
    };
    let Some(data) = dirs_next::data_dir() else {
        return Err(Error::new(ErrorKind::NotFound, "No data directory found"));
    };
    let apps = data.join("applications");
    let dbus_services = data.join("dbus-1").join("services");
    let search_providers = data.join("gnome-shell").join("search-providers");
    let icons = data
        .join("icons")
        .join("hicolor")
        .join("scalable")
        .join("apps");
    let locale_root = data.join("locale");
    let dest = bin.join(project);

    if !can_install_into(&bin, &data) {
        return Err(Error::new(
            ErrorKind::PermissionDenied,
            "One or more local install directories are not writable.",
        ));
    }

    std::fs::create_dir_all(&bin)?;
    std::fs::create_dir_all(&apps)?;
    std::fs::create_dir_all(&dbus_services)?;
    std::fs::create_dir_all(&search_providers)?;
    std::fs::create_dir_all(&icons)?;
    std::fs::copy(&exe_path, &dest)?;

    let mut perms = std::fs::metadata(&dest)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&dest, perms)?;

    write_desktop_file(&apps, &dest)?;
    write_search_provider_file(&search_providers)?;
    write_search_provider_service_file(&dbus_services, &dest)?;
    extract_icon(&icons)?;
    install_locales(&locale_root)?;

    Ok(())
}

pub fn uninstall_locally() -> std::io::Result<()> {
    let Some(bin) = dirs_next::executable_dir() else {
        return Err(Error::new(
            ErrorKind::NotFound,
            "No executable directory found",
        ));
    };
    let Some(data) = dirs_next::data_dir() else {
        return Err(Error::new(ErrorKind::NotFound, "No data directory found"));
    };
    let bin = bin.join(env!("CARGO_PKG_NAME"));
    let icon = data
        .join("icons")
        .join("hicolor")
        .join("scalable")
        .join("apps")
        .join(format!("{}.svg", APP_ID));
    let desktop = data
        .join("applications")
        .join(format!("{}.desktop", APP_ID));
    let search_provider = data
        .join("gnome-shell")
        .join("search-providers")
        .join(format!("{}.search-provider.ini", APP_ID));
    let service = data
        .join("dbus-1")
        .join("services")
        .join(format!("{SEARCH_PROVIDER_BUS_NAME}.service"));
    if bin.exists() {
        fs::remove_file(bin)?;
    }
    if desktop.exists() {
        fs::remove_file(desktop)?;
    }
    if search_provider.exists() {
        fs::remove_file(search_provider)?;
    }
    if service.exists() {
        fs::remove_file(service)?;
    }
    if icon.exists() {
        fs::remove_file(icon)?;
    }
    remove_installed_locales(&data.join("locale"))?;
    Ok(())
}

fn can_install_into(bin: &Path, data: &Path) -> bool {
    let mut targets = vec![
        bin.to_path_buf(),
        data.join("applications"),
        data.join("dbus-1").join("services"),
        data.join("gnome-shell").join("search-providers"),
        data.join("icons")
            .join("hicolor")
            .join("scalable")
            .join("apps"),
    ];
    if locale_install_required() {
        targets.push(data.join("locale"));
    }

    targets
        .iter()
        .all(|target| install_target_dir_is_eligible(target))
}

fn locale_install_required() -> bool {
    available_locales().any(|locale| {
        Path::new(LOCALEDIR)
            .join(locale)
            .join("LC_MESSAGES")
            .join(format!("{GETTEXT_DOMAIN}.mo"))
            .exists()
    })
}

fn is_writable(dir: &Path) -> bool {
    for attempt in 0..8u32 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let test_path = dir.join(format!(
            ".perm_test.{}.{}.{}",
            process::id(),
            nanos,
            attempt
        ));
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&test_path)
        {
            Ok(_) => {
                let _ = std::fs::remove_file(test_path);
                return true;
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return false,
        }
    }

    false
}

fn install_target_dir_is_eligible(path: &Path) -> bool {
    let mut candidate = Some(path);
    while let Some(dir) = candidate {
        if dir.exists() {
            return dir.is_dir() && is_writable(dir);
        }
        candidate = dir.parent();
    }

    false
}

fn same_file_path(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn write_desktop_file(apps_path: &Path, bin_path: &Path) -> std::io::Result<()> {
    let project = env!("CARGO_PKG_NAME");
    let comment = option_env!("CARGO_PKG_DESCRIPTION").unwrap_or("Password manager");
    let exec = bin_path.display(); // absolute path to the installed binary
    let contents = format!(
        "[Desktop Entry]
Type=Application
Version=1.0
Name={project}
Comment={comment}
Exec={exec}
Icon={APP_ID}
Terminal=false
Categories=System;Security;
StartupNotify=true
",
    );

    let file = apps_path.join(format!("{}.desktop", APP_ID));
    fs::write(&file, contents)?;

    // Make sure it's readable by the user
    let mut perms = fs::metadata(&file)?.permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&file, perms)?;

    Ok(())
}

fn write_search_provider_file(search_providers_path: &Path) -> std::io::Result<()> {
    let contents = format!(
        "[Shell Search Provider]
DesktopId={APP_ID}.desktop
BusName={SEARCH_PROVIDER_BUS_NAME}
ObjectPath={SEARCH_PROVIDER_OBJECT_PATH}
Version=2
"
    );
    let file = search_providers_path.join(format!("{}.search-provider.ini", APP_ID));
    fs::write(&file, contents)?;

    let mut perms = fs::metadata(&file)?.permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&file, perms)?;

    Ok(())
}

fn write_search_provider_service_file(
    services_path: &Path,
    bin_path: &Path,
) -> std::io::Result<()> {
    let contents = format!(
        "[D-BUS Service]
Name={SEARCH_PROVIDER_BUS_NAME}
Exec={} --search-provider
",
        bin_path.display()
    );
    let file = services_path.join(format!("{SEARCH_PROVIDER_BUS_NAME}.service"));
    fs::write(&file, contents)?;

    let mut perms = fs::metadata(&file)?.permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&file, perms)?;

    Ok(())
}

fn extract_icon(apps_dir: &Path) -> std::io::Result<()> {
    let resource_path = format!("{}/scalable/apps/{}.svg", RESOURCE_ID, APP_ID);
    println!("Looking up resource: {resource_path}");
    let bytes = gio::resources_lookup_data(&resource_path, ResourceLookupFlags::NONE)
        .map_err(|e| Error::new(ErrorKind::NotFound, format!("Resource not found: {e}")))?;
    let out_path = apps_dir.join(format!("{}.svg", APP_ID));
    std::fs::write(&out_path, bytes.as_ref())?;
    Ok(())
}

fn install_locales(locale_root: &Path) -> std::io::Result<()> {
    for locale in available_locales() {
        let source = Path::new(LOCALEDIR)
            .join(locale)
            .join("LC_MESSAGES")
            .join(format!("{GETTEXT_DOMAIN}.mo"));
        if !source.exists() {
            continue;
        }

        let destination = locale_root
            .join(locale)
            .join("LC_MESSAGES")
            .join(format!("{GETTEXT_DOMAIN}.mo"));
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, destination)?;
    }

    Ok(())
}

fn remove_installed_locales(locale_root: &Path) -> std::io::Result<()> {
    for locale in available_locales() {
        let destination = locale_root
            .join(locale)
            .join("LC_MESSAGES")
            .join(format!("{GETTEXT_DOMAIN}.mo"));
        if destination.exists() {
            fs::remove_file(destination)?;
        }
    }

    Ok(())
}

fn available_locales() -> impl Iterator<Item = &'static str> {
    AVAILABLE_LOCALES
        .split(':')
        .filter(|locale| !locale.is_empty())
}

#[test]
fn writability_probe_does_not_truncate_existing_perm_test_files() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("keycord-setup-writable-{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    let existing = dir.join(".perm_test");
    fs::write(&existing, "keep").expect("write marker");

    assert!(is_writable(&dir));
    assert_eq!(
        fs::read_to_string(&existing).expect("read marker"),
        "keep".to_string()
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn install_target_dir_rejects_existing_non_writable_directories() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("keycord-setup-existing-dir-{unique}"));
    let target = root.join("applications");
    fs::create_dir_all(&target).expect("create target dir");
    let mut permissions = fs::metadata(&target)
        .expect("read target metadata")
        .permissions();
    permissions.set_mode(0o500);
    fs::set_permissions(&target, permissions).expect("make target non-writable");

    assert!(!install_target_dir_is_eligible(&target));

    let mut cleanup_permissions = fs::metadata(&target)
        .expect("read target metadata for cleanup")
        .permissions();
    cleanup_permissions.set_mode(0o700);
    fs::set_permissions(&target, cleanup_permissions).expect("restore target permissions");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn install_target_dir_accepts_missing_nested_directories_under_writable_ancestor() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("keycord-setup-missing-dir-{unique}"));
    fs::create_dir_all(&root).expect("create root dir");

    assert!(install_target_dir_is_eligible(
        &root
            .join("icons")
            .join("hicolor")
            .join("scalable")
            .join("apps")
    ));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn install_eligibility_checks_all_written_target_directories() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("keycord-setup-eligibility-{unique}"));
    let bin = root.join("bin");
    let data = root.join("data");
    let search_providers = data.join("gnome-shell").join("search-providers");
    fs::create_dir_all(&bin).expect("create bin dir");
    fs::create_dir_all(&search_providers).expect("create search provider dir");
    let mut permissions = fs::metadata(&search_providers)
        .expect("read search provider metadata")
        .permissions();
    permissions.set_mode(0o500);
    fs::set_permissions(&search_providers, permissions).expect("make search provider non-writable");

    assert!(!can_install_into(&bin, &data));

    let mut cleanup_permissions = fs::metadata(&search_providers)
        .expect("read search provider metadata for cleanup")
        .permissions();
    cleanup_permissions.set_mode(0o700);
    fs::set_permissions(&search_providers, cleanup_permissions)
        .expect("restore search provider permissions");
    let _ = fs::remove_dir_all(root);
}
