use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

type Catalog = BTreeMap<String, CatalogEntry>;

#[derive(Default)]
struct CatalogEntry {
    references: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CompiledDocSource {
    canonical_path: String,
    locale: Option<String>,
    relative_path: String,
}

#[derive(Clone, Debug, Default)]
struct PoEntry {
    msgid: String,
    msgid_plural: Option<String>,
    msgstr: String,
    msgstr_plural: BTreeMap<usize, String>,
}

#[derive(Clone, Copy, Debug)]
enum ActivePoField {
    Id,
    IdPlural,
    Str,
    StrPlural(usize),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ResourceFileEntry {
    source: String,
    alias: Option<String>,
}

impl ResourceFileEntry {
    fn source(source: String) -> Self {
        Self {
            source,
            alias: None,
        }
    }

    fn alias(source: impl Into<String>, alias: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            alias: Some(alias.into()),
        }
    }
}

fn main() {
    let docs_enabled = env::var_os("CARGO_FEATURE_DOCS").is_some();

    println!("cargo:rustc-env=APP_ID={}", app_id());
    println!("cargo:rustc-env=RESOURCE_ID={}", resource_id());
    println!("cargo:rustc-env=GETTEXT_DOMAIN={}", gettext_domain());
    println!(
        "cargo:rustc-env=SEARCH_PROVIDER_BUS_NAME={}",
        search_provider_bus_name()
    );
    println!(
        "cargo:rustc-env=SEARCH_PROVIDER_OBJECT_PATH={}",
        search_provider_object_path()
    );

    export_dependency_versions();
    write_window_ui();
    #[cfg(target_os = "windows")]
    configure_windows_binary_stack_size();

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set for build script"));
    let locale_dir = out_dir.join("locale");
    let data_dir = Path::new("data");
    let docs_dir = Path::new("docs");
    let po_dir = Path::new("po");
    if docs_enabled {
        write_docs_manifest(docs_dir, &out_dir);
    }

    fs::create_dir_all(data_dir).expect("Failed to create data directory");
    fs::create_dir_all(po_dir).expect("Failed to create po directory");

    let mut resource_files = Vec::new();
    collect_icon_assets(data_dir, data_dir, &mut resource_files);
    extend_windows_icon_theme_resources(data_dir, &mut resource_files);
    resource_files.sort();
    write_resources_xml(data_dir, &resource_files);

    glib_build_tools::compile_resources(&[data_dir], "data/resources.xml", "compiled.gresource");

    #[cfg(target_os = "windows")]
    {
        let ico_path = data_dir.join(format!("{}.ico", env!("CARGO_PKG_NAME")));
        println!("cargo:rerun-if-changed={}", ico_path.display());
        let mut res = winresource::WindowsResource::new();
        res.set_icon(ico_path.to_string_lossy().as_ref());
        res.compile().expect("Failed to compile resources");
    }

    write_translation_catalogs(po_dir);
    let locales = compile_translations(po_dir, &locale_dir);
    println!("cargo:rustc-env=LOCALEDIR={}", locale_dir.display());
    println!("cargo:rustc-env=AVAILABLE_LOCALES={}", locales.join(":"));

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=data");
    if docs_enabled {
        println!("cargo:rerun-if-changed=docs");
    }
    println!("cargo:rerun-if-changed=po");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_DOCS");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_SETUP");

    #[cfg(not(feature = "setup"))]
    {
        desktop_file();
        search_provider_files();
    }
}

#[cfg(target_os = "windows")]
fn configure_windows_binary_stack_size() {
    const WINDOWS_MAIN_THREAD_STACK_SIZE_BYTES: usize = 8 * 1024 * 1024;

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    match env::var("CARGO_CFG_TARGET_ENV").as_deref() {
        Ok("gnu") => println!(
            "cargo:rustc-link-arg-bins=-Wl,--stack,{}",
            WINDOWS_MAIN_THREAD_STACK_SIZE_BYTES
        ),
        Ok("msvc") => println!(
            "cargo:rustc-link-arg-bins=/STACK:{}",
            WINDOWS_MAIN_THREAD_STACK_SIZE_BYTES
        ),
        _ => {}
    }
}

fn write_translation_catalogs(po_dir: &Path) {
    let mut catalog = Catalog::new();
    collect_ui_strings(Path::new("data/window.ui"), &mut catalog);
    collect_ui_strings(Path::new("data/shortcuts.ui"), &mut catalog);
    collect_metainfo_strings(Path::new("data/metainfo.xml"), &mut catalog);
    collect_desktop_strings(Path::new("keycord.desktop"), &mut catalog);
    collect_rust_strings(Path::new("src"), &mut catalog);

    let pot_path = po_dir.join(format!("{}.pot", gettext_domain()));
    let en_path = po_dir.join("en.po");
    write_if_changed(&pot_path, render_pot_catalog(&catalog));
    write_if_changed(&en_path, render_po_catalog(&catalog, "en"));
}

fn collect_ui_strings(path: &Path, catalog: &mut Catalog) {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("Failed to read {}: {err}", path.display()));
    let mut search_start = 0usize;

    while let Some(relative_index) = source[search_start..].find("translatable=\"yes\"") {
        let attr_index = search_start + relative_index;
        let text_start = source[attr_index..]
            .find('>')
            .map(|offset| attr_index + offset + 1);
        let Some(text_start) = text_start else {
            break;
        };
        let text_end = source[text_start..]
            .find('<')
            .map(|offset| text_start + offset);
        let Some(text_end) = text_end else {
            break;
        };

        let text = decode_xml_entities(source[text_start..text_end].trim());
        if !text.is_empty() {
            add_catalog_message(
                catalog,
                &text,
                format!("{}:{}", path.display(), line_number(&source, attr_index)),
            );
        }

        search_start = text_end;
    }
}

fn collect_metainfo_strings(path: &Path, catalog: &mut Catalog) {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("Failed to read {}: {err}", path.display()));
    let bytes = source.as_bytes();
    let mut stack = Vec::new();
    let mut index = 0usize;
    let mut text_start = None;

    while index < bytes.len() {
        if bytes[index] != b'<' {
            text_start.get_or_insert(index);
            index += 1;
            continue;
        }

        if let Some(start) = text_start.take() {
            add_metainfo_text(catalog, &source, path, &stack, start, index);
        }

        if source[index..].starts_with("<!--") {
            index = source[index + 4..]
                .find("-->")
                .map(|offset| index + 4 + offset + 3)
                .unwrap_or(bytes.len());
            continue;
        }

        if source[index..].starts_with("<![CDATA[") {
            let data_start = index + 9;
            let data_end = source[data_start..]
                .find("]]>")
                .map(|offset| data_start + offset)
                .unwrap_or(bytes.len());
            add_metainfo_text(catalog, &source, path, &stack, data_start, data_end);
            index = data_end.saturating_add(3).min(bytes.len());
            continue;
        }

        if source[index..].starts_with("<?") {
            index = source[index + 2..]
                .find("?>")
                .map(|offset| index + 2 + offset + 2)
                .unwrap_or(bytes.len());
            continue;
        }

        let tag_end = find_xml_tag_end(bytes, index).unwrap_or(bytes.len().saturating_sub(1));
        let tag = &source[index + 1..tag_end];
        let trimmed = tag.trim();

        if trimmed.starts_with('/') {
            stack.pop();
            index = tag_end + 1;
            continue;
        }

        let tag_name = xml_tag_name(trimmed);
        let inherited_skip = stack.last().copied().unwrap_or(false);
        let skip =
            inherited_skip || tag_has_translate_no(trimmed) || matches!(tag_name, "translation");
        let self_closing = trimmed.ends_with('/');

        if !self_closing {
            stack.push(skip);
        }

        index = tag_end + 1;
    }

    if let Some(start) = text_start {
        add_metainfo_text(catalog, &source, path, &stack, start, bytes.len());
    }
}

fn collect_desktop_strings(path: &Path, catalog: &mut Catalog) {
    if !path.is_file() {
        return;
    }

    let source = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("Failed to read {}: {err}", path.display()));

    for (line_index, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        if key.contains('[') {
            continue;
        }

        if !matches!(key, "Name" | "GenericName" | "Comment" | "Keywords") {
            continue;
        }

        let value = value.trim();
        if value.is_empty() {
            continue;
        }

        add_catalog_message(
            catalog,
            value,
            format!("{}:{}", path.display(), line_index + 1),
        );
    }
}

fn add_metainfo_text(
    catalog: &mut Catalog,
    source: &str,
    path: &Path,
    stack: &[bool],
    start: usize,
    end: usize,
) {
    if stack.last().copied().unwrap_or(false) {
        return;
    }

    let text = normalize_xml_text(&decode_xml_entities(&source[start..end]));
    if text.is_empty() {
        return;
    }

    add_catalog_message(
        catalog,
        &text,
        format!("{}:{}", path.display(), line_number(source, start)),
    );
}

fn normalize_xml_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_xml_tag_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start + 1;
    let mut quote = None;

    while index < bytes.len() {
        match bytes[index] {
            b'\'' | b'"' if quote.is_none() => quote = Some(bytes[index]),
            byte if Some(byte) == quote => quote = None,
            b'>' if quote.is_none() => return Some(index),
            _ => {}
        }
        index += 1;
    }

    None
}

fn xml_tag_name(tag: &str) -> &str {
    let trimmed = tag.trim_start();
    let start = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let end = start
        .find(|ch: char| ch.is_whitespace() || ch == '/')
        .unwrap_or(start.len());
    &start[..end]
}

fn tag_has_translate_no(tag: &str) -> bool {
    tag.contains("translate=\"no\"") || tag.contains("translate='no'")
}

fn collect_rust_strings(dir: &Path, catalog: &mut Catalog) {
    for entry in
        fs::read_dir(dir).unwrap_or_else(|err| panic!("Failed to read {}: {err}", dir.display()))
    {
        let entry = entry.expect("Failed to read source directory entry");
        let path = entry.path();

        if path.is_dir() {
            collect_rust_strings(&path, catalog);
            continue;
        }

        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if file_name.contains("test") {
            continue;
        }

        collect_rust_strings_from_file(&path, catalog);
    }
}

fn collect_rust_strings_from_file(path: &Path, catalog: &mut Catalog) {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("Failed to read {}: {err}", path.display()));
    let bytes = source.as_bytes();
    let mut index = 0usize;
    let mut line = 1usize;

    while index < bytes.len() {
        if bytes[index] == b'\n' {
            line += 1;
            index += 1;
            continue;
        }

        if skip_raw_string(bytes, &mut index, &mut line) {
            continue;
        }

        if skip_cfg_test_item(bytes, &mut index, &mut line) {
            continue;
        }

        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'/') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }

        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len() {
                if bytes[index] == b'\n' {
                    line += 1;
                }
                if bytes[index] == b'*' && bytes[index + 1] == b'/' {
                    index += 2;
                    break;
                }
                index += 1;
            }
            continue;
        }

        if bytes[index] == b'\'' && looks_like_char_literal(bytes, index) {
            index += 1;
            while index < bytes.len() {
                match bytes[index] {
                    b'\\' => index += 2,
                    b'\'' => {
                        index += 1;
                        break;
                    }
                    b'\n' => {
                        line += 1;
                        index += 1;
                    }
                    _ => index += 1,
                }
            }
            continue;
        }

        if bytes[index] != b'"' {
            index += 1;
            continue;
        }

        let literal_line = line;
        index += 1;
        let mut value = String::new();

        while index < bytes.len() {
            match bytes[index] {
                b'\\' => {
                    index += 1;
                    if index >= bytes.len() {
                        break;
                    }
                    push_unescaped_rust_char(bytes, &mut index, &mut value);
                }
                b'"' => {
                    index += 1;
                    break;
                }
                b'\n' => {
                    line += 1;
                    value.push('\n');
                    index += 1;
                }
                byte => {
                    value.push(byte as char);
                    index += 1;
                }
            }
        }

        if looks_translatable_rust_string(&value) {
            add_catalog_message(
                catalog,
                value.trim(),
                format!("{}:{}", path.display(), literal_line),
            );
        }
    }
}

fn push_unescaped_rust_char(bytes: &[u8], index: &mut usize, value: &mut String) {
    match bytes[*index] {
        b'\\' => {
            value.push('\\');
            *index += 1;
        }
        b'"' => {
            value.push('"');
            *index += 1;
        }
        b'n' => {
            value.push('\n');
            *index += 1;
        }
        b'r' => {
            value.push('\r');
            *index += 1;
        }
        b't' => {
            value.push('\t');
            *index += 1;
        }
        b'0' => {
            value.push('\0');
            *index += 1;
        }
        b'u' if bytes.get(*index + 1) == Some(&b'{') => {
            *index += 2;
            let start = *index;
            while *index < bytes.len() && bytes[*index] != b'}' {
                *index += 1;
            }
            let escape = std::str::from_utf8(&bytes[start..*index]).unwrap_or_default();
            if let Ok(codepoint) = u32::from_str_radix(escape, 16) {
                if let Some(ch) = char::from_u32(codepoint) {
                    value.push(ch);
                }
            }
            if *index < bytes.len() {
                *index += 1;
            }
        }
        other => {
            value.push(other as char);
            *index += 1;
        }
    }
}

fn looks_translatable_rust_string(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }

    if trimmed.ends_with(".md") && !trimmed.chars().any(char::is_whitespace) {
        return false;
    }

    if trimmed.starts_with("[Desktop Entry]")
        || trimmed.starts_with("[Shell Search Provider]")
        || trimmed.starts_with("[D-BUS Service]")
    {
        return false;
    }

    if trimmed.starts_with('#') || trimmed.chars().any(|ch| ch == '\u{1b}') {
        return false;
    }

    if trimmed.starts_with('/') || trimmed.starts_with("./") {
        return false;
    }

    if !trimmed.chars().any(char::is_whitespace)
        && (trimmed.starts_with('.')
            || trimmed.starts_with("../")
            || trimmed.contains('/')
            || trimmed.contains('@'))
    {
        return false;
    }

    if trimmed.contains("example.com") {
        return false;
    }

    if trimmed.starts_with("io.github.")
        || trimmed.starts_with("org.")
        || trimmed.starts_with("app.")
        || trimmed.starts_with("win.")
        || trimmed.starts_with("edit-")
        || trimmed.starts_with("document-")
        || trimmed.starts_with("folder-")
        || trimmed.starts_with("go-")
        || trimmed.starts_with("list-")
        || trimmed.starts_with("open-")
        || trimmed.starts_with("view-")
    {
        return false;
    }

    if trimmed.contains("::") {
        return false;
    }

    if !trimmed.chars().any(char::is_alphabetic) {
        return false;
    }

    if trimmed
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-' | '.'))
    {
        return false;
    }

    if trimmed.chars().any(char::is_whitespace) {
        return true;
    }

    if trimmed
        .chars()
        .any(|ch| matches!(ch, '.' | '!' | '?' | ':'))
    {
        return true;
    }

    trimmed
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
        && trimmed.chars().any(|ch| ch.is_ascii_lowercase())
}

fn looks_like_char_literal(bytes: &[u8], index: usize) -> bool {
    let mut cursor = index + 1;
    while cursor < bytes.len() && cursor <= index + 6 {
        match bytes[cursor] {
            b'\\' => cursor += 2,
            b'\'' => return true,
            b'\n' => return false,
            _ => cursor += 1,
        }
    }

    false
}

fn skip_raw_string(bytes: &[u8], index: &mut usize, line: &mut usize) -> bool {
    if bytes[*index] != b'r' {
        return false;
    }

    let mut cursor = *index + 1;
    let mut hashes = 0usize;
    while cursor < bytes.len() && bytes[cursor] == b'#' {
        hashes += 1;
        cursor += 1;
    }

    if cursor >= bytes.len() || bytes[cursor] != b'"' {
        return false;
    }

    *index = cursor + 1;
    while *index < bytes.len() {
        if bytes[*index] == b'\n' {
            *line += 1;
            *index += 1;
            continue;
        }

        if bytes[*index] == b'"'
            && bytes
                .get(*index + 1..*index + 1 + hashes)
                .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
        {
            *index += 1 + hashes;
            return true;
        }

        *index += 1;
    }

    true
}

fn skip_cfg_test_item(bytes: &[u8], index: &mut usize, line: &mut usize) -> bool {
    const ATTR: &[u8] = b"#[cfg(test)]";
    if !bytes[*index..].starts_with(ATTR) {
        return false;
    }

    *index += ATTR.len();
    while *index < bytes.len() {
        match bytes[*index] {
            b'\n' => {
                *line += 1;
                *index += 1;
            }
            b' ' | b'\t' | b'\r' => *index += 1,
            _ => break,
        }
    }

    while *index < bytes.len() && bytes[*index] != b'{' && bytes[*index] != b';' {
        if bytes[*index] == b'\n' {
            *line += 1;
        }
        *index += 1;
    }

    if *index >= bytes.len() {
        return true;
    }

    if bytes[*index] == b';' {
        *index += 1;
        return true;
    }

    let mut depth = 0usize;
    while *index < bytes.len() {
        match bytes[*index] {
            b'{' => {
                depth += 1;
                *index += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                *index += 1;
                if depth == 0 {
                    break;
                }
            }
            b'\n' => {
                *line += 1;
                *index += 1;
            }
            _ => *index += 1,
        }
    }

    true
}

fn decode_xml_entities(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn add_catalog_message(catalog: &mut Catalog, message: &str, reference: String) {
    let entry = catalog.entry(message.to_string()).or_default();
    entry.references.insert(reference);
}

fn render_pot_catalog(catalog: &Catalog) -> String {
    render_catalog(catalog, None)
}

fn render_po_catalog(catalog: &Catalog, language: &str) -> String {
    render_catalog(catalog, Some(language))
}

fn render_catalog(catalog: &Catalog, language: Option<&str>) -> String {
    let mut output = String::new();
    write_po_header(&mut output, language);

    for (message, entry) in catalog {
        for reference in &entry.references {
            writeln!(output, "#: {reference}").expect("Failed to format po reference");
        }
        write_po_string_field(&mut output, "msgid", message);
        if let Some(language) = language {
            let _ = language;
            write_po_string_field(&mut output, "msgstr", message);
        } else {
            write_po_string_field(&mut output, "msgstr", "");
        }
        output.push('\n');
    }

    output
}

fn write_po_header(output: &mut String, language: Option<&str>) {
    let language = language.unwrap_or("");
    output.push_str("msgid \"\"\n");
    output.push_str("msgstr \"\"\n");
    output.push_str(&po_wrapped_line(&format!(
        "Project-Id-Version: {} {}\n",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    )));
    output.push_str(&po_wrapped_line("MIME-Version: 1.0\n"));
    output.push_str(&po_wrapped_line(
        "Content-Type: text/plain; charset=UTF-8\n",
    ));
    output.push_str(&po_wrapped_line("Content-Transfer-Encoding: 8bit\n"));
    output.push_str(&po_wrapped_line(&format!("Language: {language}\n")));
    output.push_str(&po_wrapped_line(
        "Plural-Forms: nplurals=2; plural=(n != 1);\n",
    ));
    output.push('\n');
}

fn write_po_string_field(output: &mut String, field: &str, value: &str) {
    if value.is_empty() {
        let _ = writeln!(output, "{field} \"\"");
        return;
    }

    if value.contains('\n') {
        let _ = writeln!(output, "{field} \"\"");
        output.push_str(&po_wrapped_line(value));
        return;
    }

    let _ = writeln!(output, "{field} \"{}\"", escape_po_string(value));
}

fn po_wrapped_line(value: &str) -> String {
    format!("\"{}\"\n", escape_po_string(value))
}

fn escape_po_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn compile_translations(po_dir: &Path, locale_dir: &Path) -> Vec<String> {
    let locales = discover_po_locales(po_dir);
    for locale in &locales {
        let po_path = po_dir.join(format!("{locale}.po"));
        let mo_path = locale_dir
            .join(locale)
            .join("LC_MESSAGES")
            .join(format!("{}.mo", gettext_domain()));
        let bytes = compile_mo_file(&po_path);
        if let Some(parent) = mo_path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|err| panic!("Failed to create {}: {err}", parent.display()));
        }
        write_if_changed_binary(&mo_path, &bytes);
    }
    locales
}

fn discover_po_locales(po_dir: &Path) -> Vec<String> {
    let mut locales = fs::read_dir(po_dir)
        .unwrap_or_else(|err| panic!("Failed to read {}: {err}", po_dir.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("po"))
        .filter_map(|path| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    locales.sort();
    locales
}

fn compile_mo_file(path: &Path) -> Vec<u8> {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("Failed to read {}: {err}", path.display()));
    let mut entries = parse_po_entries(&source);
    entries.sort_by(|left, right| {
        let left_key = mo_original_key(left);
        let right_key = mo_original_key(right);
        left_key.cmp(&right_key)
    });

    let count = entries.len() as u32;
    let originals_offset = 28u32;
    let translations_offset = originals_offset + count * 8;
    let originals_data_offset = translations_offset + count * 8;

    let original_keys = entries
        .iter()
        .map(mo_original_key)
        .collect::<Vec<Vec<u8>>>();
    let translated_values = entries
        .iter()
        .map(mo_translation_value)
        .collect::<Vec<Vec<u8>>>();

    let mut original_table = Vec::with_capacity(entries.len());
    let mut translation_table = Vec::with_capacity(entries.len());
    let mut data = Vec::new();
    let mut offset = originals_data_offset;

    for key in &original_keys {
        original_table.push((key.len() as u32, offset));
        data.extend_from_slice(key);
        data.push(0);
        offset += key.len() as u32 + 1;
    }

    for value in &translated_values {
        translation_table.push((value.len() as u32, offset));
        data.extend_from_slice(value);
        data.push(0);
        offset += value.len() as u32 + 1;
    }

    let mut output = Vec::new();
    push_u32_le(&mut output, 0x9504_12de);
    push_u32_le(&mut output, 0);
    push_u32_le(&mut output, count);
    push_u32_le(&mut output, originals_offset);
    push_u32_le(&mut output, translations_offset);
    push_u32_le(&mut output, 0);
    push_u32_le(&mut output, 0);

    for (length, offset) in &original_table {
        push_u32_le(&mut output, *length);
        push_u32_le(&mut output, *offset);
    }
    for (length, offset) in &translation_table {
        push_u32_le(&mut output, *length);
        push_u32_le(&mut output, *offset);
    }

    output.extend_from_slice(&data);
    output
}

fn mo_original_key(entry: &PoEntry) -> Vec<u8> {
    match &entry.msgid_plural {
        Some(msgid_plural) => {
            let mut bytes = entry.msgid.as_bytes().to_vec();
            bytes.push(0);
            bytes.extend_from_slice(msgid_plural.as_bytes());
            bytes
        }
        None => entry.msgid.as_bytes().to_vec(),
    }
}

fn mo_translation_value(entry: &PoEntry) -> Vec<u8> {
    if entry.msgstr_plural.is_empty() {
        return entry.msgstr.as_bytes().to_vec();
    }

    let max_index = entry.msgstr_plural.keys().copied().max().unwrap_or(0);
    let mut bytes = Vec::new();
    for index in 0..=max_index {
        if index > 0 {
            bytes.push(0);
        }
        if let Some(value) = entry.msgstr_plural.get(&index) {
            bytes.extend_from_slice(value.as_bytes());
        }
    }
    bytes
}

fn parse_po_entries(source: &str) -> Vec<PoEntry> {
    let mut entries = Vec::new();
    let mut current = PoEntry::default();
    let mut active_field = None;

    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            finalize_po_entry(&mut entries, &mut current);
            active_field = None;
            continue;
        }

        if trimmed.starts_with('#') {
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("msgid_plural ") {
            current.msgid_plural = Some(parse_po_quoted(value));
            active_field = Some(ActivePoField::IdPlural);
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("msgid ") {
            if !current.msgid.is_empty()
                || !current.msgstr.is_empty()
                || !current.msgstr_plural.is_empty()
            {
                finalize_po_entry(&mut entries, &mut current);
            }
            current.msgid = parse_po_quoted(value);
            active_field = Some(ActivePoField::Id);
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("msgstr ") {
            current.msgstr = parse_po_quoted(value);
            active_field = Some(ActivePoField::Str);
            continue;
        }

        if let Some((index, value)) = parse_po_plural_msgstr(trimmed) {
            current.msgstr_plural.insert(index, value);
            active_field = Some(ActivePoField::StrPlural(index));
            continue;
        }

        if trimmed.starts_with('"') {
            let value = parse_po_quoted(trimmed);
            match active_field {
                Some(ActivePoField::Id) => current.msgid.push_str(&value),
                Some(ActivePoField::IdPlural) => current
                    .msgid_plural
                    .get_or_insert_with(String::new)
                    .push_str(&value),
                Some(ActivePoField::Str) => current.msgstr.push_str(&value),
                Some(ActivePoField::StrPlural(index)) => current
                    .msgstr_plural
                    .entry(index)
                    .or_default()
                    .push_str(&value),
                None => {}
            }
        }
    }

    finalize_po_entry(&mut entries, &mut current);
    entries
}

fn parse_po_plural_msgstr(line: &str) -> Option<(usize, String)> {
    let remainder = line.strip_prefix("msgstr[")?;
    let closing = remainder.find(']')?;
    let index = remainder[..closing].parse().ok()?;
    let value = remainder[closing + 1..].trim_start();
    Some((index, parse_po_quoted(value)))
}

fn parse_po_quoted(value: &str) -> String {
    let trimmed = value.trim();
    let stripped = trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or("");
    unescape_po_string(stripped)
}

fn unescape_po_string(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        let Some(escaped) = chars.next() else {
            break;
        };

        match escaped {
            '\\' => output.push('\\'),
            '"' => output.push('"'),
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            other => output.push(other),
        }
    }

    output
}

fn finalize_po_entry(entries: &mut Vec<PoEntry>, current: &mut PoEntry) {
    if current.msgid.is_empty() && current.msgstr.is_empty() && current.msgstr_plural.is_empty() {
        return;
    }

    entries.push(std::mem::take(current));
}

fn push_u32_le(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn line_number(source: &str, index: usize) -> usize {
    source[..index]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn write_if_changed(path: &Path, contents: String) {
    if fs::read_to_string(path).ok().as_deref() == Some(contents.as_str()) {
        return;
    }

    fs::write(path, contents)
        .unwrap_or_else(|err| panic!("Failed to write {}: {err}", path.display()));
}

fn write_if_changed_binary(path: &Path, contents: &[u8]) {
    if fs::read(path).ok().as_deref() == Some(contents) {
        return;
    }

    fs::write(path, contents)
        .unwrap_or_else(|err| panic!("Failed to write {}: {err}", path.display()));
}

fn write_resources_xml(data_dir: &Path, resource_files: &[ResourceFileEntry]) {
    let mut xml = String::from("<gresources>\n");
    writeln!(xml, "\t<gresource prefix=\"{}\">", resource_id())
        .expect("Failed to format resource prefix");
    for file in resource_files {
        if let Some(alias) = file.alias.as_deref() {
            writeln!(xml, "\t\t<file alias=\"{alias}\">{}</file>", file.source)
                .expect("Failed to format aliased resource entry");
        } else {
            writeln!(xml, "\t\t<file>{}</file>", file.source)
                .expect("Failed to format resource entry");
        }
    }
    xml.push_str("\t</gresource>\n</gresources>\n");
    let path = data_dir.join("resources.xml");
    write_if_changed(&path, xml);
}

fn write_window_ui() {
    let rendered = with_translation_domain(rendered_window_ui());
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set for build script");
    fs::write(Path::new(&out_dir).join("window.ui"), rendered)
        .expect("Failed to write generated window.ui");
}

fn write_docs_manifest(docs_dir: &Path, out_dir: &Path) {
    let mut sources = collect_doc_sources(docs_dir);
    sources.sort();

    let mut output = String::from("const DOC_SOURCES: &[CompiledDocumentSource] = &[\n");
    for source in sources {
        let locale = match source.locale {
            Some(locale) => format!("Some({locale:?})"),
            None => "None".to_string(),
        };
        writeln!(
            output,
            "    CompiledDocumentSource {{ path: {:?}, locale: {}, source: include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{}\")) }},",
            source.canonical_path,
            locale,
            source.relative_path
        )
        .expect("Failed to format docs manifest entry");
    }
    output.push_str("];\n");

    write_if_changed(&out_dir.join("docs_manifest.rs"), output);
}

fn collect_doc_sources(docs_dir: &Path) -> Vec<CompiledDocSource> {
    let mut sources = Vec::new();

    for entry in fs::read_dir(docs_dir)
        .unwrap_or_else(|err| panic!("Failed to read {}: {err}", docs_dir.display()))
    {
        let entry = entry.expect("Failed to read docs directory entry");
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some((canonical_path, locale)) = parse_doc_source_file_name(file_name) else {
            continue;
        };

        sources.push(CompiledDocSource {
            canonical_path,
            locale,
            relative_path: format!("docs/{file_name}"),
        });
    }

    sources
}

fn parse_doc_source_file_name(file_name: &str) -> Option<(String, Option<String>)> {
    let stem = file_name.strip_suffix(".md")?;

    if let Some((base, locale)) = stem.rsplit_once('.') {
        if looks_like_locale_tag(locale) {
            return Some((format!("{base}.md"), Some(locale.to_string())));
        }
    }

    Some((file_name.to_string(), None))
}

fn looks_like_locale_tag(value: &str) -> bool {
    value.len() >= 2
        && value.chars().any(|ch| ch.is_ascii_alphabetic())
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}

fn with_translation_domain(source: String) -> String {
    if source.contains("<interface domain=") {
        return source;
    }

    source.replacen(
        "<interface>",
        &format!("<interface domain=\"{}\">", gettext_domain()),
        1,
    )
}

fn rendered_window_ui() -> String {
    fs::read_to_string("data/window.ui").expect("Failed to read data/window.ui")
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

fn collect_icon_assets(dir: &Path, data_dir: &Path, resource_files: &mut Vec<ResourceFileEntry>) {
    for entry in fs::read_dir(dir).expect("Failed to read resource directory") {
        let entry = entry.expect("Failed to read resource directory entry");
        let path = entry.path();

        if path.is_dir() {
            collect_icon_assets(&path, data_dir, resource_files);
        } else if matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("png" | "svg")
        ) && path
            .components()
            .any(|component| component.as_os_str() == "apps")
        {
            let rel = path
                .strip_prefix(data_dir)
                .expect("Resource path should stay within data/");
            resource_files.push(ResourceFileEntry::source(
                rel.to_string_lossy().into_owned(),
            ));
        }
    }
}

fn extend_windows_icon_theme_resources(
    data_dir: &Path,
    resource_files: &mut Vec<ResourceFileEntry>,
) {
    let theme_index = data_dir.join("windows/share/icons/hicolor/index.theme");
    if theme_index.is_file() {
        resource_files.push(ResourceFileEntry::source(
            "windows/share/icons/hicolor/index.theme".to_string(),
        ));
    }

    let sources = resource_files
        .iter()
        .map(|entry| entry.source.clone())
        .collect::<Vec<_>>();
    for source in sources {
        for alias in windows_icon_theme_aliases(&source) {
            resource_files.push(ResourceFileEntry::alias(source.clone(), alias));
        }
    }
}

fn windows_icon_theme_aliases(source: &str) -> Vec<String> {
    if source.starts_with("256x256/apps/") || source.starts_with("scalable/apps/") {
        return vec![format!("windows/share/icons/hicolor/{source}")];
    }

    if let Some(file_name) = source.strip_prefix("symbolic/apps/") {
        let mut aliases = vec![format!(
            "windows/share/icons/hicolor/symbolic/actions/{file_name}"
        )];
        if file_name.starts_with("io.github.noobping.") {
            aliases.push(format!(
                "windows/share/icons/hicolor/symbolic/apps/{file_name}"
            ));
        }
        return aliases;
    }

    Vec::new()
}

#[cfg(not(feature = "setup"))]
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

#[cfg(not(feature = "setup"))]
fn search_provider_files() {
    let project = env!("CARGO_PKG_NAME");
    let dir = Path::new(".");
    let desktop_id = format!("{}.desktop", app_id());
    let bus_name = search_provider_bus_name();
    let object_path = search_provider_object_path();
    let search_provider_contents = format!(
        "[Shell Search Provider]
DesktopId={desktop_id}
BusName={bus_name}
ObjectPath={object_path}
Version=2
"
    );
    fs::write(
        dir.join(format!("{project}-search-provider.ini")),
        search_provider_contents,
    )
    .expect("Can not build search provider file");

    let service_contents = format!(
        "[D-BUS Service]
Name={bus_name}
Exec={project} --search-provider
"
    );
    fs::write(
        dir.join(format!("{project}-search-provider.service")),
        service_contents,
    )
    .expect("Can not build search provider D-Bus service file");
}

// Flatpak builds must use the manifest ID so GtkApplication can own the
// matching D-Bus name inside the sandbox.
#[cfg(all(debug_assertions, not(feature = "flatpak")))]
const fn app_id() -> &'static str {
    concat!("io.github.noobping.", env!("CARGO_PKG_NAME"), "-beta")
}

#[cfg(any(not(debug_assertions), feature = "flatpak"))]
const fn app_id() -> &'static str {
    concat!("io.github.noobping.", env!("CARGO_PKG_NAME"))
}

const fn resource_id() -> &'static str {
    concat!("/io/github/noobping/", env!("CARGO_PKG_NAME"))
}

const fn gettext_domain() -> &'static str {
    env!("CARGO_PKG_NAME")
}

fn search_provider_bus_name() -> String {
    format!("{}.SearchProvider", app_id().replace('-', "_"))
}

fn search_provider_object_path() -> String {
    format!("/{}", search_provider_bus_name().replace('.', "/"))
}
