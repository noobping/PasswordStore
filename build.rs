use std::{env, fmt::Write as _, fs, path::Path};

#[cfg(not(target_os = "linux"))]
const NON_LINUX_WINDOW_UI_IDS: &[&str] = &[
    "backend_preferences",
    "log_page",
    "store_import_page",
    "tools_page",
    "git_busy_page",
];

fn main() {
    println!("cargo:rustc-env=APP_ID={}", app_id());
    println!("cargo:rustc-env=RESOURCE_ID={}", resource_id());
    export_dependency_versions();
    write_window_ui();

    // Directories
    let data_dir = Path::new("data");

    // Tell Cargo when to rerun the build script
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=data");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_SETUP");

    // Ensure data/ exists
    fs::create_dir_all(data_dir).unwrap();

    // Collect all .svg icon files in data/icons/
    let mut icons = Vec::new();
    collect_svg_icons(data_dir, data_dir, &mut icons);
    icons.sort();

    write_resources_xml(data_dir, &icons);

    // Compile GResources from data/resources.xml into resources.gresource
    glib_build_tools::compile_resources(&["data"], "data/resources.xml", "compiled.gresource");

    #[cfg(all(target_os = "linux", not(feature = "setup")))]
    desktop_file();
}

fn write_resources_xml(data_dir: &Path, icons: &[String]) {
    let mut xml = String::from("<gresources>\n");
    writeln!(xml, "\t<gresource prefix=\"{}\">", resource_id())
        .expect("Failed to format resource prefix");
    for icon in icons {
        writeln!(xml, "\t\t<file>{icon}</file>").expect("Failed to format resource entry");
    }
    xml.push_str("\t</gresource>\n</gresources>\n");
    fs::write(data_dir.join("resources.xml"), xml).expect("Failed to write data/resources.xml");
}

fn write_window_ui() {
    let rendered = rendered_window_ui();
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set for build script");
    fs::write(Path::new(&out_dir).join("window.ui"), rendered)
        .expect("Failed to write generated window.ui");
}

#[cfg(target_os = "linux")]
fn rendered_window_ui() -> String {
    fs::read_to_string("data/window.ui").expect("Failed to read data/window.ui")
}

#[cfg(not(target_os = "linux"))]
fn rendered_window_ui() -> String {
    let rendered = fs::read_to_string("data/window.ui").expect("Failed to read data/window.ui");
    strip_non_linux_ui(rendered)
}

#[cfg(not(target_os = "linux"))]
fn strip_non_linux_ui(mut source: String) -> String {
    for id in NON_LINUX_WINDOW_UI_IDS {
        source = remove_child_block_containing_id(&source, id);
    }
    source
}

#[cfg(not(target_os = "linux"))]
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
    for entry in fs::read_dir(dir).expect("Failed to read resource directory") {
        let entry = entry.expect("Failed to read resource directory entry");
        let path = entry.path();

        if path.is_dir() {
            collect_svg_icons(&path, data_dir, icons);
        } else if path.extension().and_then(|e| e.to_str()) == Some("svg") {
            // Strip "data/" so we end up with e.g. "icons/foo/bar.svg"
            let rel = path
                .strip_prefix(data_dir)
                .expect("Resource path should stay within data/");
            icons.push(rel.to_string_lossy().into_owned());
        }
    }
}

#[cfg(all(target_os = "linux", not(feature = "setup")))]
fn desktop_file() {
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
    fs::write(dir.join(format!("{project}.desktop")), contents)
        .expect("Can not build desktop file");
}

#[cfg(debug_assertions)]
const fn app_id() -> &'static str {
    concat!("io.github.noobping.", env!("CARGO_PKG_NAME"), "-beta")
}

#[cfg(not(debug_assertions))]
const fn app_id() -> &'static str {
    concat!("io.github.noobping.", env!("CARGO_PKG_NAME"))
}

const fn resource_id() -> &'static str {
    concat!("/io/github/noobping/", env!("CARGO_PKG_NAME"))
}
