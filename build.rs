fn main() {
    // Compile GResources from data/resources.xml into resources.gresource
    glib_build_tools::compile_resources(
        &["data"],                 // root directory for resources.xml
        "data/resources.xml",   // path to your resources.xml
        "resources.gresource",  // output file name (embedded into the binary)
    );
}
