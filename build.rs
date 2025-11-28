use std::{fs, path::Path};

#[cfg(not(feature = "setup"))]
pub const APP_ID: &str = concat!("dev.noobping.", env!("CARGO_PKG_NAME"));
const RESOURCE_ID: &str = "/dev/noobping/passwordstore";

fn main() {
    // Directories
    let data_dir = Path::new("data");
    let icons_dir = data_dir.join("icons");

    glib_build_tools::compile_resources(
        &["data"],
        "data/resources.xml",
        "compiled.gresource",
    );

    // Tell Cargo when to rerun the build script
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=data");

    // Ensure data/ exists
    fs::create_dir_all(&data_dir).unwrap();
    fs::create_dir_all(&icons_dir).unwrap();

    // Collect all .svg icon files in data/icons/
    let mut icons: Vec<String> = fs::read_dir(&icons_dir)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? == "svg" {
                Some(path.file_name()?.to_string_lossy().into_owned())
            } else {
                None
            }
        })
        .collect();

    icons.sort();

    // Generate resources.xml content
    let mut xml = String::from("<gresources>\n");
    xml.push_str(&format!("\t<gresource prefix=\"{RESOURCE_ID}\">\n"));
    // xml.push_str(&format!("\t\t<file>{}.svg</file>\n", APP_ID));
    // xml.push_str(&format!("\t\t<file>{}.png</file>\n", APP_ID));

    // Add files
    for f in &icons {
        xml.push_str(&format!("\t\t<file>icons/{}</file>\n", f));
    }

    xml.push_str("\t</gresource>\n</gresources>\n");

    // Write resources.xml there
    fs::write(data_dir.join("resources.xml"), xml).unwrap();

    // Compile GResources from data/resources.xml into resources.gresource
    glib_build_tools::compile_resources(
        &["data"],              // root directory for resources.xml and files
        "data/resources.xml",   // path to resources.xml
        "resources.gresource",  // output file name (embedded into the binary)
    );

    #[cfg(not(feature = "setup"))]
    desktop_file();
}

#[cfg(not(feature = "setup"))]
fn desktop_file() {
    use std::{fs, path::Path};
    let project = env!("CARGO_PKG_NAME");
    let dir = Path::new(".");
    let version = env!("CARGO_PKG_VERSION");
    let comment = option_env!("CARGO_PKG_DESCRIPTION").unwrap_or("Password manager");
    let contents = format!(
        "[Desktop Entry]
Type=Application
Version={version}
Name={project}
Comment={comment}
Exec={project} %u
Icon={APP_ID}
Terminal=false
Categories=Utility;
"
    );
    fs::write(&dir.join(format!("{project}.desktop")), contents)
        .expect("Can not build desktop file")
}
