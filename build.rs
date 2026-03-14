use std::{fs, path::Path};

fn main() {
    emit_build_cfgs();
    assert_non_linux_build_has_no_features();
    println!("cargo:rustc-env=APP_ID={}", app_id());
    println!("cargo:rustc-env=RESOURCE_ID={}", resource_id());
    export_dependency_versions();
    write_platform_window_ui();

    // Directories
    let data_dir = Path::new("data");

    // Tell Cargo when to rerun the build script
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=data");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_FLATPAK");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_SETUP");

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

    if should_write_desktop_file() {
        desktop_file();
    }
}

fn write_platform_window_ui() {
    let source = fs::read_to_string("data/window.ui").expect("Failed to read data/window.ui");
    let rendered = if is_non_linux_build() {
        strip_non_linux_ui(&source)
    } else {
        source
    };
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set for build script");
    fs::write(Path::new(&out_dir).join("window.ui"), rendered)
        .expect("Failed to write generated window.ui");
}

fn emit_build_cfgs() {
    for cfg in [
        "keycord_linux",
        "keycord_flatpak",
        "keycord_standard_linux",
        "keycord_restricted",
        "keycord_setup",
    ] {
        println!("cargo:rustc-check-cfg=cfg({cfg})");
    }

    if is_linux_build() {
        println!("cargo:rustc-cfg=keycord_linux");
    } else {
        println!("cargo:rustc-cfg=keycord_restricted");
    }

    if is_flatpak_build() {
        println!("cargo:rustc-cfg=keycord_flatpak");
        println!("cargo:rustc-cfg=keycord_restricted");
    }

    if is_standard_linux_build() {
        println!("cargo:rustc-cfg=keycord_standard_linux");
    }

    if is_setup_build() {
        println!("cargo:rustc-cfg=keycord_setup");
    }
}

fn target_os() -> String {
    std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not set for build script")
}

fn is_linux_build() -> bool {
    target_os() == "linux"
}

fn is_non_linux_build() -> bool {
    !is_linux_build()
}

fn is_flatpak_build() -> bool {
    is_linux_build() && std::env::var_os("CARGO_FEATURE_FLATPAK").is_some()
}

fn is_standard_linux_build() -> bool {
    is_linux_build() && !is_flatpak_build()
}

fn is_setup_build() -> bool {
    is_standard_linux_build() && std::env::var_os("CARGO_FEATURE_SETUP").is_some()
}

fn should_write_desktop_file() -> bool {
    is_linux_build() && !is_setup_build()
}

fn assert_non_linux_build_has_no_features() {
    if !is_non_linux_build() {
        return;
    }

    let mut enabled_features = std::env::vars_os()
        .filter_map(|(key, _)| {
            let key = key.into_string().ok()?;
            key.strip_prefix("CARGO_FEATURE_")
                .map(|feature| feature.to_ascii_lowercase())
        })
        .collect::<Vec<_>>();

    if enabled_features.is_empty() {
        return;
    }

    enabled_features.sort();
    panic!(
        "Non-Linux builds do not allow Cargo features. Enabled feature(s): {}.",
        enabled_features.join(", ")
    );
}

fn strip_non_linux_ui(source: &str) -> String {
    let source = remove_line_containing(source, "name=\"menu-model\">primary_menu");
    let source = remove_menu_block(&source, "primary_menu");
    let source = remove_child_block_containing_id(&source, "backend_preferences");
    let source = remove_child_block_containing_id(&source, "log_page");
    remove_child_block_containing_id(&source, "git_busy_page")
}

fn remove_line_containing(source: &str, pattern: &str) -> String {
    let mut rendered = String::with_capacity(source.len());
    for line in source.lines() {
        if line.contains(pattern) {
            continue;
        }
        rendered.push_str(line);
        rendered.push('\n');
    }
    rendered
}

fn remove_menu_block(source: &str, id: &str) -> String {
    let marker = format!("<menu id=\"{id}\">");
    let Some(start) = source.find(&marker) else {
        return source.to_string();
    };
    let Some(end) = source[start..].find("</menu>") else {
        return source.to_string();
    };
    let end = start + end + "</menu>".len();
    let mut rendered = String::with_capacity(source.len());
    rendered.push_str(&source[..start]);
    rendered.push_str(&source[end..]);
    rendered
}

fn remove_child_block_containing_id(source: &str, id: &str) -> String {
    let marker = format!("id=\"{id}\"");
    let Some(id_index) = source.find(&marker) else {
        return source.to_string();
    };
    let Some(child_start) = source[..id_index].rfind("<child") else {
        return source.to_string();
    };

    let mut depth = 0usize;
    let mut cursor = child_start;
    while cursor < source.len() {
        let next_open = source[cursor..].find("<child").map(|index| cursor + index);
        let next_close = source[cursor..]
            .find("</child>")
            .map(|index| cursor + index);

        match (next_open, next_close) {
            (Some(open), Some(close)) if open < close => {
                depth += 1;
                cursor = open + "<child".len();
            }
            (_, Some(close)) => {
                depth = depth.saturating_sub(1);
                cursor = close + "</child>".len();
                if depth == 0 {
                    let mut rendered = String::with_capacity(source.len());
                    rendered.push_str(&source[..child_start]);
                    rendered.push_str(&source[cursor..]);
                    return rendered;
                }
            }
            _ => break,
        }
    }

    source.to_string()
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
