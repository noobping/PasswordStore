use adw::gio::{self, ResourceLookupFlags};
use std::io::{Error, ErrorKind};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
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
    let apps = data.join("applications");

    let bin_parent_is_writable = bin.parent().map(is_writable).unwrap_or(false);
    let apps_parent_is_writable = apps.parent().map(is_writable).unwrap_or(false);
    // If they exist, they must be writable; if not, the parent must be writable.
    (bin.exists() && bin.is_dir() && is_writable(&bin) || bin_parent_is_writable)
        && (apps.exists() && apps.is_dir() && is_writable(&apps) || apps_parent_is_writable)
}

pub fn is_installed_locally() -> bool {
    let Some(bin) = dirs_next::executable_dir() else {
        return false;
    };
    let Some(data) = dirs_next::data_dir() else {
        return false;
    };
    let bin = bin.join(env!("CARGO_PKG_NAME"));
    let desktop = data
        .join("applications")
        .join(format!("{}.desktop", APP_ID));
    bin.exists() && bin.is_file() && desktop.exists() && desktop.is_file()
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

#[cfg(test)]
mod tests {
    use super::is_writable;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

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
