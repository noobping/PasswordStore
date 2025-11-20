use std::{fs, path::Path};

fn main() {
    // Directories
    let data_dir = Path::new("data");
    let icons_dir = data_dir.join("icons");

    // Tell Cargo when to rerun the build script
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=data");

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
    let mut xml = String::from(
        r#"<gresources>
  <gresource prefix="/dev/noobping/passadw">
"#,
    );

    // Add icons as files under icons/
    for f in &icons {
        xml.push_str(&format!("    <file>icons/{}</file>\n", f));
    }

    xml.push_str("  </gresource>\n</gresources>\n");

    // Ensure data/ exists and write resources.xml there
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("resources.xml"), xml).unwrap();

    // Compile GResources from data/resources.xml into resources.gresource
    glib_build_tools::compile_resources(
        &["data"],              // root directory for resources.xml and files
        "data/resources.xml",   // path to your resources.xml
        "resources.gresource",  // output file name (embedded into the binary)
    );
}
