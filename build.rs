use std::{fs, path::Path};

fn main() {
    println!("cargo:rustc-env=APP_ID={}", app_id());
    println!("cargo:rustc-env=RESOURCE_ID={}", resource_id());
    export_dependency_versions();

    // Directories
    let data_dir = Path::new("data");

    // Tell Cargo when to rerun the build script
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=data");

    // Ensure data/ exists
    fs::create_dir_all(data_dir).unwrap();

    // Collect all .svg icon files in data/icons/
    let mut icons = Vec::new();
    collect_svg_icons(data_dir, data_dir, &mut icons);
    icons.sort();

    // Generate resources.xml content
    let mut xml = String::from("<gresources>\n");
    xml.push_str(&format!("\t<gresource prefix=\"{}\">\n", resource_id()));
    for f in &icons {
        xml.push_str(&format!("\t\t<file>{}</file>\n", f));
    }
    xml.push_str("\t</gresource>\n</gresources>\n");

    // Write resources.xml there
    fs::write(data_dir.join("resources.xml"), xml).unwrap();

    // Compile GResources from data/resources.xml into resources.gresource
    glib_build_tools::compile_resources(&["data"], "data/resources.xml", "compiled.gresource");

    #[cfg(not(feature = "setup"))]
    desktop_file();
}

fn export_dependency_versions() {
    let lockfile =
        fs::read_to_string("Cargo.lock").expect("Failed to read Cargo.lock for version metadata");
    let ripasso = find_locked_package_version(&lockfile, "ripasso")
        .expect("ripasso version not found in Cargo.lock");
    let sequoia = find_locked_package_version(&lockfile, "sequoia-openpgp")
        .expect("sequoia-openpgp version not found in Cargo.lock");

    println!("cargo:rustc-env=RIPASSO_VERSION={ripasso}");
    println!("cargo:rustc-env=SEQUOIA_OPENPGP_VERSION={sequoia}");
}

fn find_locked_package_version(lockfile: &str, package: &str) -> Option<String> {
    let mut current_package = None;

    for line in lockfile.lines() {
        let trimmed = line.trim();

        if trimmed == "[[package]]" {
            current_package = None;
            continue;
        }

        if let Some(name) = trimmed
            .strip_prefix("name = \"")
            .and_then(|value| value.strip_suffix('"'))
        {
            current_package = Some(name);
            continue;
        }

        if current_package == Some(package) {
            if let Some(version) = trimmed
                .strip_prefix("version = \"")
                .and_then(|value| value.strip_suffix('"'))
            {
                return Some(version.to_string());
            }
        }
    }

    None
}

/// Recursively collect all `.svg` files under `dir`,
/// and push their path *relative to `data_dir`* into `icons`.
fn collect_svg_icons(dir: &Path, data_dir: &Path, icons: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.is_dir() {
            collect_svg_icons(&path, data_dir, icons);
        } else if path.extension().and_then(|e| e.to_str()) == Some("svg") {
            // Strip "data/" so we end up with e.g. "icons/foo/bar.svg"
            let rel = path.strip_prefix(data_dir).unwrap();
            icons.push(rel.to_string_lossy().into_owned());
        }
    }
}

#[cfg(not(feature = "setup"))]
fn desktop_file() {
    use std::{fs, path::Path};
    let app_id = app_id();
    let project = env!("CARGO_PKG_NAME");
    let dir = Path::new(".");
    let comment = option_env!("CARGO_PKG_DESCRIPTION").unwrap_or("Password manager");
    let contents = format!(
        "[Desktop Entry]
Type=Application
Version=1.0
Name=Keycord
Comment={comment}
Exec={project}
Icon={app_id}
Terminal=false
Categories=System;Security;
StartupNotify=true
"
    );
    fs::write(dir.join(format!("{project}.desktop")), contents).expect("Can not build desktop file")
}

#[cfg(debug_assertions)]
fn app_id() -> &'static str {
    concat!("io.github.noobping.", env!("CARGO_PKG_NAME"), "-beta")
}

#[cfg(not(debug_assertions))]
fn app_id() -> &'static str {
    concat!("io.github.noobping.", env!("CARGO_PKG_NAME"))
}

fn resource_id() -> &'static str {
    concat!("/io/github/noobping/", env!("CARGO_PKG_NAME"))
}
