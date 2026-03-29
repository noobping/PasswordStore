use crate::logging::log_error;
use crate::support::actions::register_window_action;
use crate::support::ui::{
    append_info_row, clear_list_box, push_navigation_page_if_needed, visible_navigation_page_is,
};
use crate::support::uri::launch_default_uri;
use crate::window::navigation::{show_docs_page, show_secondary_page_chrome, HasWindowChrome};
use adw::glib::markup_escape_text;
use adw::gtk::{
    Align, Box as GtkBox, Grid, Label, ListBox, Orientation, PolicyType, ScrolledWindow,
    SearchEntry, TextView, Widget, WrapMode,
};
use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, NavigationPage};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

pub const DOCS_PAGE_TITLE: &str = "Documentation";
pub const DOCS_PAGE_SUBTITLE: &str = "Guides and reference";

const DOCS_EMPTY_TITLE: &str = "No matching docs";
const DOCS_EMPTY_SUBTITLE: &str = "Try a different search term.";
const INTERNAL_DOC_URI_SCHEME: &str = "keycord-doc:";

const DOC_PATHS: [&str; 7] = [
    "README.md",
    "getting-started.md",
    "search.md",
    "workflows.md",
    "permissions-and-backends.md",
    "teams-and-organizations.md",
    "use-cases.md",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompiledDocumentSource {
    path: &'static str,
    locale: Option<&'static str>,
    source: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/docs_manifest.rs"));

#[derive(Clone)]
pub struct DocumentationPageState {
    navigation: crate::window::navigation::WindowNavigationState,
    page: NavigationPage,
    search_entry: SearchEntry,
    list: ListBox,
    detail_page: NavigationPage,
    detail_scrolled: ScrolledWindow,
    detail_box: GtkBox,
    documents: Rc<Vec<DocumentationDocument>>,
    current_doc_index: Rc<RefCell<Option<usize>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DocumentationDocument {
    path: String,
    title: String,
    subtitle: String,
    blocks: Vec<DocumentationBlock>,
    anchors: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DocumentationBlock {
    kind: DocumentationBlockKind,
    text: String,
    markup: String,
    search_text: String,
    links: Vec<DocumentationInlineLink>,
    anchor: Option<String>,
    list_marker: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DocumentationBlockKind {
    Heading(u32),
    Paragraph,
    ListItem,
    TableRow,
    CodeBlock,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DocumentationInlineLink {
    label: String,
    target: DocumentationLinkTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DocumentationLinkTarget {
    Internal {
        path: String,
        anchor: Option<String>,
    },
    External(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DocumentationSearchResult {
    doc_index: usize,
    block_index: Option<usize>,
    title: String,
    subtitle: String,
}

#[derive(Default)]
struct InlineParseState {
    text: String,
    markup: String,
    links: Vec<DocumentationInlineLink>,
}

struct ParsedListItem<'a> {
    marker: String,
    text: &'a str,
}

impl DocumentationPageState {
    pub fn new(
        navigation: &crate::window::navigation::WindowNavigationState,
        page: &NavigationPage,
        search_entry: &SearchEntry,
        list: &ListBox,
        detail_page: &NavigationPage,
        detail_scrolled: &ScrolledWindow,
        detail_box: &GtkBox,
    ) -> Self {
        let state = Self {
            navigation: navigation.clone(),
            page: page.clone(),
            search_entry: search_entry.clone(),
            list: list.clone(),
            detail_page: detail_page.clone(),
            detail_scrolled: detail_scrolled.clone(),
            detail_box: detail_box.clone(),
            documents: Rc::new(load_documents()),
            current_doc_index: Rc::new(RefCell::new(None)),
        };
        state.connect_handlers();
        state
    }

    fn connect_handlers(&self) {
        {
            let state = self.clone();
            self.search_entry
                .connect_search_changed(move |_| state.render_search_results());
        }

        {
            let state = self.clone();
            self.navigation
                .nav
                .connect_notify_local(Some("visible-page"), move |_, _| {
                    if visible_navigation_page_is(&state.navigation.nav, &state.page) {
                        state.search_entry.grab_focus();
                    }
                });
        }
    }

    pub fn open(&self) {
        self.render_search_results();
        show_docs_page(&self.navigation);
        self.search_entry.grab_focus();
    }

    fn render_search_results(&self) {
        clear_list_box(&self.list);

        let results = search_documents(&self.documents, self.search_entry.text().as_str());
        if results.is_empty() {
            append_info_row(&self.list, DOCS_EMPTY_TITLE, DOCS_EMPTY_SUBTITLE);
            return;
        }

        for result in results {
            let state = self.clone();
            let row = ActionRow::builder()
                .title(&result.title)
                .subtitle(&result.subtitle)
                .build();
            row.set_activatable(true);
            row.add_suffix(&adw::gtk::Image::from_icon_name("go-next-symbolic"));
            row.connect_activated(move |_| state.open_result(&result));
            self.list.append(&row);
        }
    }

    fn open_result(&self, result: &DocumentationSearchResult) {
        self.open_document(result.doc_index, result.block_index);
    }

    fn open_document(&self, doc_index: usize, block_index: Option<usize>) {
        let Some(document) = self.documents.get(doc_index) else {
            return;
        };

        *self.current_doc_index.borrow_mut() = Some(doc_index);
        self.detail_page.set_title(&document.title);

        clear_box(&self.detail_box);

        let mut target_widget = None::<Widget>;
        let mut index = 0usize;
        while index < document.blocks.len() {
            if matches!(
                document.blocks[index].kind,
                DocumentationBlockKind::TableRow
            ) {
                let table_end = table_run_end(&document.blocks, index);
                let widget = self.render_table(&document.blocks[index..table_end]);
                if block_index.is_some_and(|target| target >= index && target < table_end) {
                    target_widget = Some(widget.clone().upcast::<Widget>());
                }
                self.detail_box.append(&widget);
                index = table_end;
                continue;
            }

            let widget = self.render_block(doc_index, &document.blocks[index]);
            if block_index == Some(index) {
                target_widget = Some(widget.clone().upcast::<Widget>());
            }
            self.detail_box.append(&widget);
            index += 1;
        }

        let chrome = self.navigation.window_chrome();
        show_secondary_page_chrome(&chrome, &document.title, DOCS_PAGE_TITLE, false);
        push_navigation_page_if_needed(&self.navigation.nav, &self.detail_page);

        if let Some(target_widget) = target_widget {
            scroll_to_widget(&self.detail_scrolled, &target_widget);
        } else {
            self.detail_scrolled.vadjustment().set_value(0.0);
        }
    }

    fn render_block(&self, doc_index: usize, block: &DocumentationBlock) -> GtkBox {
        if matches!(block.kind, DocumentationBlockKind::CodeBlock) {
            return self.render_code_block(block);
        }

        if matches!(block.kind, DocumentationBlockKind::ListItem) {
            return self.render_list_item(doc_index, block);
        }

        let container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(6)
            .focusable(true)
            .build();
        container.set_halign(Align::Fill);

        let label = self.build_block_label(doc_index, block);

        match block.kind {
            DocumentationBlockKind::Heading(level) => apply_heading_style(&label, level),
            DocumentationBlockKind::Paragraph
            | DocumentationBlockKind::ListItem
            | DocumentationBlockKind::TableRow => {}
            DocumentationBlockKind::CodeBlock => {}
        }
        container.append(&label);

        container
    }

    fn render_list_item(&self, doc_index: usize, block: &DocumentationBlock) -> GtkBox {
        let container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(0)
            .focusable(true)
            .build();
        container.set_halign(Align::Fill);

        let row = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .build();
        row.set_halign(Align::Fill);

        let marker = Label::new(block.list_marker.as_deref());
        marker.set_xalign(1.0);
        marker.set_yalign(0.0);
        marker.set_selectable(false);
        marker.set_width_chars(3);
        marker.add_css_class("dim-label");

        let content = self.build_block_label(doc_index, block);
        content.set_hexpand(true);

        row.append(&marker);
        row.append(&content);
        container.append(&row);

        container
    }

    fn build_block_label(&self, doc_index: usize, block: &DocumentationBlock) -> Label {
        let label = Label::new(None);
        label.set_xalign(0.0);
        label.set_wrap(true);
        label.set_selectable(true);
        label.set_halign(Align::Fill);

        if !block.links.is_empty() {
            let state = self.clone();
            label.connect_activate_link(move |_, uri| {
                let Some(target) = decode_link_target(uri) else {
                    return adw::glib::Propagation::Proceed;
                };
                state.open_link(doc_index, &target);
                adw::glib::Propagation::Stop
            });
        }

        label.set_use_markup(true);
        label.set_markup(&block_markup(block));
        label
    }

    fn render_code_block(&self, block: &DocumentationBlock) -> GtkBox {
        let container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(0)
            .focusable(true)
            .build();
        container.set_halign(Align::Fill);
        container.add_css_class("card");

        let view = TextView::new();
        view.set_editable(false);
        view.set_cursor_visible(false);
        view.set_monospace(true);
        view.set_wrap_mode(WrapMode::None);
        view.set_left_margin(12);
        view.set_right_margin(12);
        view.set_top_margin(12);
        view.set_bottom_margin(12);
        view.buffer().set_text(&block.text);

        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Automatic)
            .vscrollbar_policy(PolicyType::Never)
            .propagate_natural_height(true)
            .child(&view)
            .build();

        container.append(&scrolled);
        container
    }

    fn render_table(&self, rows: &[DocumentationBlock]) -> GtkBox {
        let container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .focusable(true)
            .build();
        container.set_halign(Align::Fill);

        let grid = Grid::builder()
            .column_spacing(18)
            .row_spacing(8)
            .halign(Align::Fill)
            .hexpand(true)
            .build();

        for (row_index, row) in rows.iter().enumerate() {
            for (column_index, cell) in table_cells(&row.text).iter().enumerate() {
                let label = Label::new(None);
                label.set_xalign(0.0);
                label.set_wrap(true);
                label.set_selectable(true);
                label.set_halign(Align::Fill);
                label.set_hexpand(true);
                label.set_margin_top(3);
                label.set_margin_bottom(3);
                label.set_margin_start(6);
                label.set_margin_end(6);
                if row_index == 0 {
                    label.add_css_class("heading");
                }
                set_label_inline_markup(&label, cell);
                grid.attach(&label, column_index as i32, row_index as i32, 1, 1);
            }
        }

        container.append(&grid);
        container
    }

    fn open_link(&self, current_doc_index: usize, target: &DocumentationLinkTarget) {
        match target {
            DocumentationLinkTarget::Internal { path, anchor } => {
                let target_doc_index = self.resolve_doc_index(current_doc_index, path);
                let block_index = target_doc_index
                    .and_then(|index| self.documents.get(index))
                    .and_then(|document| {
                        anchor
                            .as_ref()
                            .and_then(|slug| document.anchors.get(slug))
                            .copied()
                    });
                if let Some(target_doc_index) = target_doc_index {
                    self.open_document(target_doc_index, block_index);
                }
            }
            DocumentationLinkTarget::External(uri) => open_external_link(uri),
        }
    }

    fn resolve_doc_index(&self, current_doc_index: usize, path: &str) -> Option<usize> {
        if path.is_empty() {
            return Some(current_doc_index);
        }

        let normalized = path
            .rsplit('/')
            .next()
            .map(str::trim)
            .unwrap_or(path)
            .to_lowercase();
        self.documents
            .iter()
            .position(|document| document.path.eq_ignore_ascii_case(&normalized))
    }
}

pub fn register_open_docs_action(window: &ApplicationWindow, state: &DocumentationPageState) {
    let state = state.clone();
    register_window_action(window, "open-docs", move || state.open());
}

fn load_documents() -> Vec<DocumentationDocument> {
    let preferred_locales = preferred_doc_locales();

    DOC_PATHS
        .iter()
        .filter_map(|path| {
            select_document_source(path, &preferred_locales)
                .map(|source| parse_document(path, source))
        })
        .collect()
}

fn select_document_source(path: &str, preferred_locales: &[String]) -> Option<&'static str> {
    select_document_source_from(DOC_SOURCES, path, preferred_locales)
}

fn select_document_source_from(
    sources: &[CompiledDocumentSource],
    path: &str,
    preferred_locales: &[String],
) -> Option<&'static str> {
    let variants = sources
        .iter()
        .filter(|source| source.path == path)
        .copied()
        .collect::<Vec<_>>();

    for locale in preferred_locales {
        if let Some(source) = variants.iter().find(|source| {
            source
                .locale
                .and_then(normalize_locale_tag)
                .is_some_and(|value| value == *locale)
        }) {
            return Some(source.source);
        }
    }

    variants
        .iter()
        .find(|source| source.locale.is_none())
        .map(|source| source.source)
}

fn preferred_doc_locales() -> Vec<String> {
    collect_preferred_doc_locales(
        ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .filter_map(|key| std::env::var(key).ok()),
    )
}

fn collect_preferred_doc_locales(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut locales = Vec::new();

    for value in values {
        for candidate in value.split(':') {
            for locale in expanded_locale_tags(candidate) {
                if !locales.contains(&locale) {
                    locales.push(locale);
                }
            }
        }
    }

    locales
}

fn expanded_locale_tags(value: &str) -> Vec<String> {
    let Some(normalized) = normalize_locale_tag(value) else {
        return Vec::new();
    };

    let mut locales = vec![normalized.clone()];
    if let Some((language, _)) = normalized.split_once('_') {
        if !locales.iter().any(|locale| locale == language) {
            locales.push(language.to_string());
        }
    }

    locales
}

fn normalize_locale_tag(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_codeset = trimmed.split('.').next().unwrap_or(trimmed);
    let without_modifier = without_codeset.split('@').next().unwrap_or(without_codeset);
    if without_modifier.eq_ignore_ascii_case("c") || without_modifier.eq_ignore_ascii_case("posix")
    {
        return None;
    }

    let normalized = without_modifier.replace('-', "_").to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    Some(normalized)
}

fn parse_document(path: &str, source: &str) -> DocumentationDocument {
    let mut blocks = Vec::new();
    let mut lines = source.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim_end();

        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("```") {
            let mut contents = Vec::new();
            for code_line in lines.by_ref() {
                if code_line.trim_start().starts_with("```") {
                    break;
                }
                contents.push(code_line);
            }
            let text = contents.join("\n");
            if !text.is_empty() {
                blocks.push(DocumentationBlock {
                    kind: DocumentationBlockKind::CodeBlock,
                    search_text: text.to_lowercase(),
                    text,
                    markup: String::new(),
                    links: Vec::new(),
                    anchor: None,
                    list_marker: None,
                });
            }
            continue;
        }

        if trimmed.starts_with("![") {
            continue;
        }

        if let Some((level, text)) = parse_heading(trimmed) {
            let inline = parse_inline_markdown(path, text);
            let anchor = slugify_heading(&inline.text);
            blocks.push(DocumentationBlock {
                kind: DocumentationBlockKind::Heading(level),
                search_text: inline.text.to_lowercase(),
                text: inline.text,
                markup: inline.markup,
                links: inline.links,
                anchor: Some(anchor),
                list_marker: None,
            });
            continue;
        }

        if is_table_line(trimmed) {
            let mut table_lines = vec![trimmed.to_string()];
            while let Some(next_line) = lines.peek() {
                if !is_table_line(next_line.trim_end()) {
                    break;
                }
                table_lines.push(lines.next().unwrap_or_default().trim_end().to_string());
            }

            for row in table_lines {
                if is_table_separator_row(&row) {
                    continue;
                }
                let cells = row
                    .trim()
                    .trim_matches('|')
                    .split('|')
                    .map(str::trim)
                    .collect::<Vec<_>>();
                let inline = parse_inline_markdown(path, &cells.join(" | "));
                blocks.push(DocumentationBlock {
                    kind: DocumentationBlockKind::TableRow,
                    search_text: inline.text.to_lowercase(),
                    text: inline.text,
                    markup: inline.markup,
                    links: inline.links,
                    anchor: None,
                    list_marker: None,
                });
            }
            continue;
        }

        if let Some(item) = parse_list_item(trimmed) {
            let inline = parse_inline_markdown(path, item.text);
            blocks.push(DocumentationBlock {
                kind: DocumentationBlockKind::ListItem,
                search_text: inline.text.to_lowercase(),
                text: inline.text,
                markup: inline.markup,
                links: inline.links,
                anchor: None,
                list_marker: Some(item.marker),
            });
            continue;
        }

        let mut paragraph_lines = vec![trimmed.to_string()];
        while let Some(next_line) = lines.peek() {
            let next_trimmed = next_line.trim_end();
            if next_trimmed.is_empty()
                || next_trimmed.starts_with("```")
                || next_trimmed.starts_with("![")
                || parse_heading(next_trimmed).is_some()
                || is_table_line(next_trimmed)
                || parse_list_item(next_trimmed).is_some()
            {
                break;
            }
            paragraph_lines.push(lines.next().unwrap_or_default().trim().to_string());
        }

        let inline = parse_inline_markdown(path, &paragraph_lines.join(" "));
        blocks.push(DocumentationBlock {
            kind: DocumentationBlockKind::Paragraph,
            search_text: inline.text.to_lowercase(),
            text: inline.text,
            markup: inline.markup,
            links: inline.links,
            anchor: None,
            list_marker: None,
        });
    }

    let title = blocks
        .iter()
        .find_map(|block| match block.kind {
            DocumentationBlockKind::Heading(_) => Some(block.text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| path.trim_end_matches(".md").replace('-', " "));
    let subtitle = blocks
        .iter()
        .find_map(|block| match block.kind {
            DocumentationBlockKind::Paragraph => Some(block.text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| title.clone());
    let anchors = blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| block.anchor.as_ref().map(|anchor| (anchor.clone(), index)))
        .collect();

    DocumentationDocument {
        path: path.to_string(),
        title,
        subtitle,
        blocks,
        anchors,
    }
}

fn parse_heading(line: &str) -> Option<(u32, &str)> {
    let level = line.bytes().take_while(|byte| *byte == b'#').count();
    if level == 0 || level > 6 {
        return None;
    }

    let text = line[level..].trim();
    if text.is_empty() {
        return None;
    }

    Some((level as u32, text))
}

fn is_table_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|')
}

fn is_table_separator_row(line: &str) -> bool {
    line.trim()
        .trim_matches('|')
        .split('|')
        .all(|cell| cell.trim().chars().all(|ch| matches!(ch, '-' | ':' | ' ')))
}

fn parse_list_item(line: &str) -> Option<ParsedListItem<'_>> {
    let trimmed = line.trim_start();
    if let Some(text) = trimmed.strip_prefix("- ") {
        return Some(ParsedListItem {
            marker: "•".to_string(),
            text: text.trim(),
        });
    }

    let mut digits = 0usize;
    for ch in trimmed.chars() {
        if ch.is_ascii_digit() {
            digits += 1;
            continue;
        }
        break;
    }
    if digits == 0 {
        return None;
    }

    let numbered = &trimmed[digits..];
    numbered.strip_prefix(". ").map(|text| ParsedListItem {
        marker: format!("{}.", &trimmed[..digits]),
        text: text.trim(),
    })
}

fn parse_inline_markdown(path: &str, source: &str) -> InlineParseState {
    let mut state = InlineParseState::default();
    let chars = source.chars().collect::<Vec<_>>();
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] == '!' && chars.get(index + 1) == Some(&'[') {
            if let Some((next_index, _, _)) = parse_markdown_link(&chars, index + 1) {
                index = next_index;
                continue;
            }
        }

        if chars[index] == '[' {
            if let Some((next_index, label, destination)) = parse_markdown_link(&chars, index) {
                state.text.push_str(&label);
                if let Some(target) = resolve_link_target(path, &destination) {
                    let link = DocumentationInlineLink { label, target };
                    state.markup.push_str(&link_markup(&link));
                    state.links.push(link);
                } else {
                    state.markup.push_str(&inline_markup(&label));
                }
                index = next_index;
                continue;
            }
        }

        if chars[index] == '*' && chars.get(index + 1) == Some(&'*') {
            if let Some((next_index, strong_text)) = parse_strong_span(&chars, index) {
                state.text.push_str(&strong_text);
                state.markup.push_str("<b>");
                state.markup.push_str(&inline_markup(&strong_text));
                state.markup.push_str("</b>");
                index = next_index;
                continue;
            }
        }

        if chars[index] == '`' {
            if let Some(code_end) = chars[index + 1..].iter().position(|ch| *ch == '`') {
                let code = chars[index + 1..index + 1 + code_end]
                    .iter()
                    .collect::<String>();
                state.text.push('`');
                state.text.push_str(&code);
                state.text.push('`');
                state.markup.push_str("<tt>");
                state.markup.push_str(markup_escape_text(&code).as_ref());
                state.markup.push_str("</tt>");
                index += code_end + 2;
                continue;
            }
        }

        state.text.push(chars[index]);
        state
            .markup
            .push_str(markup_escape_text(&chars[index].to_string()).as_ref());
        index += 1;
    }

    state.text = collapse_whitespace(&state.text);
    state
}

fn resolve_link_target(current_path: &str, destination: &str) -> Option<DocumentationLinkTarget> {
    let trimmed = destination.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Some(DocumentationLinkTarget::External(trimmed.to_string()));
    }

    let (path, anchor) = trimmed
        .split_once('#')
        .map_or((trimmed, None), |(path, anchor)| {
            (path, Some(slugify_heading(anchor)))
        });

    if path.ends_with(".md") || path.is_empty() || path.starts_with('#') {
        return Some(DocumentationLinkTarget::Internal {
            path: normalize_doc_path(current_path, path),
            anchor,
        });
    }

    None
}

fn normalize_doc_path(current_path: &str, path: &str) -> String {
    if path.is_empty() {
        return current_path.to_string();
    }

    path.rsplit('/')
        .next()
        .map(str::to_string)
        .unwrap_or_else(|| path.to_string())
}

fn parse_markdown_link(chars: &[char], start: usize) -> Option<(usize, String, String)> {
    let label_end = chars[start + 1..]
        .iter()
        .position(|ch| *ch == ']')
        .map(|offset| start + 1 + offset)?;
    if chars.get(label_end + 1) != Some(&'(') {
        return None;
    }

    let destination_end = chars[label_end + 2..]
        .iter()
        .position(|ch| *ch == ')')
        .map(|offset| label_end + 2 + offset)?;
    let label = chars[start + 1..label_end].iter().collect::<String>();
    let destination = chars[label_end + 2..destination_end]
        .iter()
        .collect::<String>();

    Some((destination_end + 1, label, destination))
}

fn parse_strong_span(chars: &[char], start: usize) -> Option<(usize, String)> {
    if chars.get(start) != Some(&'*') || chars.get(start + 1) != Some(&'*') {
        return None;
    }

    let mut index = start + 2;
    while index + 1 < chars.len() {
        if chars[index] == '*' && chars[index + 1] == '*' {
            if index == start + 2 {
                return None;
            }
            let text = chars[start + 2..index].iter().collect::<String>();
            return Some((index + 2, text));
        }
        index += 1;
    }

    None
}

fn search_documents(
    documents: &[DocumentationDocument],
    query: &str,
) -> Vec<DocumentationSearchResult> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return documents
            .iter()
            .enumerate()
            .map(|(doc_index, document)| DocumentationSearchResult {
                doc_index,
                block_index: None,
                title: document.title.clone(),
                subtitle: summarize_text(&document.subtitle),
            })
            .collect();
    }

    let query = trimmed.to_lowercase();
    documents
        .iter()
        .enumerate()
        .flat_map(|(doc_index, document)| {
            document
                .blocks
                .iter()
                .enumerate()
                .filter(|(_, block)| block.search_text.contains(&query))
                .map(move |(block_index, block)| DocumentationSearchResult {
                    doc_index,
                    block_index: Some(block_index),
                    title: document.title.clone(),
                    subtitle: summarize_text(&block.text),
                })
        })
        .collect()
}

fn block_markup(block: &DocumentationBlock) -> String {
    match block.kind {
        DocumentationBlockKind::ListItem => block.markup.clone(),
        DocumentationBlockKind::Paragraph
        | DocumentationBlockKind::Heading(_)
        | DocumentationBlockKind::CodeBlock => block.markup.clone(),
        DocumentationBlockKind::TableRow => block.markup.clone(),
    }
}

fn summarize_text(text: &str) -> String {
    const LIMIT: usize = 120;

    if text.chars().count() <= LIMIT {
        return text.to_string();
    }

    let trimmed = text.chars().take(LIMIT).collect::<String>();
    format!("{}...", trimmed.trim_end())
}

fn slugify_heading(text: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_was_dash = false;
            continue;
        }

        if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }

    slug.trim_matches('-').to_string()
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clear_box(container: &GtkBox) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn apply_heading_style(label: &Label, level: u32) {
    match level {
        1 => label.add_css_class("title-1"),
        2 => label.add_css_class("title-2"),
        3 => label.add_css_class("title-3"),
        _ => label.add_css_class("heading"),
    }
}

fn scroll_to_widget(scrolled: &ScrolledWindow, widget: &Widget) {
    let scrolled = scrolled.clone();
    let widget = widget.clone();
    adw::glib::idle_add_local_once(move || {
        widget.grab_focus();
        let adjustment = scrolled.vadjustment();
        let max_value = (adjustment.upper() - adjustment.page_size()).max(0.0);
        let target = f64::from(widget.allocation().y()).clamp(0.0, max_value);
        adjustment.set_value(target);
    });
}

fn open_external_link(uri: &str) {
    if let Err(error) = launch_default_uri(uri) {
        log_error(format!(
            "Failed to open documentation link.\nURL: {uri}\nerror: {error}"
        ));
    }
}

fn encode_link_target(target: &DocumentationLinkTarget) -> String {
    match target {
        DocumentationLinkTarget::Internal { path, anchor } => match anchor {
            Some(anchor) => format!("{INTERNAL_DOC_URI_SCHEME}{path}#{anchor}"),
            None => format!("{INTERNAL_DOC_URI_SCHEME}{path}"),
        },
        DocumentationLinkTarget::External(uri) => uri.clone(),
    }
}

fn decode_link_target(uri: &str) -> Option<DocumentationLinkTarget> {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        return Some(DocumentationLinkTarget::External(uri.to_string()));
    }

    let payload = uri.strip_prefix(INTERNAL_DOC_URI_SCHEME)?;
    let (path, anchor) = payload
        .split_once('#')
        .map_or((payload, None), |(path, anchor)| {
            (path, Some(anchor.to_string()))
        });

    Some(DocumentationLinkTarget::Internal {
        path: path.to_string(),
        anchor,
    })
}

fn link_markup(link: &DocumentationInlineLink) -> String {
    let href = encode_link_target(&link.target);
    format!(
        "<a href=\"{}\">{}</a>",
        markup_escape_text(&href),
        inline_markup(&link.label)
    )
}

fn set_label_inline_markup(label: &Label, text: &str) {
    label.set_use_markup(true);
    label.set_markup(&inline_markup(text));
}

fn inline_markup(text: &str) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let mut output = String::new();
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] == '*' && chars.get(index + 1) == Some(&'*') {
            if let Some((next_index, strong_text)) = parse_strong_span(&chars, index) {
                output.push_str("<b>");
                output.push_str(&inline_markup(&strong_text));
                output.push_str("</b>");
                index = next_index;
                continue;
            }
        }

        if chars[index] == '`' {
            if let Some(code_end) = chars[index + 1..].iter().position(|ch| *ch == '`') {
                let code = chars[index + 1..index + 1 + code_end]
                    .iter()
                    .collect::<String>();
                output.push_str("<tt>");
                output.push_str(markup_escape_text(&code).as_ref());
                output.push_str("</tt>");
                index += code_end + 2;
                continue;
            }
        }

        let mut plain = String::new();
        plain.push(chars[index]);
        index += 1;
        while index < chars.len()
            && chars[index] != '`'
            && !(chars[index] == '*' && chars.get(index + 1) == Some(&'*'))
        {
            plain.push(chars[index]);
            index += 1;
        }
        output.push_str(markup_escape_text(&plain).as_ref());
    }

    output
}

fn table_run_end(blocks: &[DocumentationBlock], start: usize) -> usize {
    let mut index = start;
    while index < blocks.len() && matches!(blocks[index].kind, DocumentationBlockKind::TableRow) {
        index += 1;
    }
    index
}

fn table_cells(row: &str) -> Vec<String> {
    row.split(" | ").map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        collect_preferred_doc_locales, inline_markup, parse_document, parse_inline_markdown,
        search_documents, select_document_source_from, table_cells, table_run_end,
        CompiledDocumentSource, DocumentationBlockKind, DocumentationDocument,
        DocumentationInlineLink, DocumentationLinkTarget,
    };
    use std::collections::BTreeMap;

    fn fake_document(
        title: &str,
        subtitle: &str,
        blocks: Vec<super::DocumentationBlock>,
    ) -> DocumentationDocument {
        DocumentationDocument {
            path: format!("{title}.md"),
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            blocks,
            anchors: BTreeMap::new(),
        }
    }

    #[test]
    fn parse_document_creates_structured_blocks() {
        let document = parse_document(
            "guide.md",
            "# Guide\n\nParagraph with [link](README.md).\n\n- Item one\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n```text\ncode\n```\n\n![Ignored](../screenshots/file.png)\n",
        );

        assert_eq!(document.title, "Guide");
        assert_eq!(
            document
                .blocks
                .iter()
                .map(|block| block.kind)
                .collect::<Vec<_>>(),
            vec![
                DocumentationBlockKind::Heading(1),
                DocumentationBlockKind::Paragraph,
                DocumentationBlockKind::ListItem,
                DocumentationBlockKind::TableRow,
                DocumentationBlockKind::TableRow,
                DocumentationBlockKind::CodeBlock,
            ]
        );
        assert_eq!(document.blocks[1].links.len(), 1);
        assert_eq!(
            document.blocks[1].markup,
            "Paragraph with <a href=\"keycord-doc:README.md\">link</a>."
        );
        assert_eq!(document.blocks[2].list_marker.as_deref(), Some("•"));
        assert_eq!(
            document.blocks[1].links[0],
            DocumentationInlineLink {
                label: "link".to_string(),
                target: DocumentationLinkTarget::Internal {
                    path: "README.md".to_string(),
                    anchor: None,
                },
            }
        );
    }

    #[test]
    fn parse_inline_markdown_preserves_code_and_links() {
        let inline = parse_inline_markdown(
            "search.md",
            "Use [Search Guide](search.md#Quick Reference), **Documentation**, and `find otp`.",
        );

        assert_eq!(
            inline.text,
            "Use Search Guide, Documentation, and `find otp`."
        );
        assert_eq!(
            inline.markup,
            "Use <a href=\"keycord-doc:search.md#quick-reference\">Search Guide</a>, <b>Documentation</b>, and <tt>find otp</tt>."
        );
        assert_eq!(
            inline.links,
            vec![DocumentationInlineLink {
                label: "Search Guide".to_string(),
                target: DocumentationLinkTarget::Internal {
                    path: "search.md".to_string(),
                    anchor: Some("quick-reference".to_string()),
                },
            }]
        );
    }

    #[test]
    fn search_documents_returns_docs_for_empty_query_and_matches_in_order() {
        let documents = vec![
            fake_document(
                "Getting Started",
                "Start here.",
                vec![super::DocumentationBlock {
                    kind: DocumentationBlockKind::Paragraph,
                    text: "Start here.".to_string(),
                    markup: "Start here.".to_string(),
                    search_text: "start here.".to_string(),
                    links: Vec::new(),
                    anchor: None,
                    list_marker: None,
                }],
            ),
            fake_document(
                "Search Guide",
                "Learn search.",
                vec![
                    super::DocumentationBlock {
                        kind: DocumentationBlockKind::Heading(2),
                        text: "OTP".to_string(),
                        markup: "OTP".to_string(),
                        search_text: "otp".to_string(),
                        links: Vec::new(),
                        anchor: Some("otp".to_string()),
                        list_marker: None,
                    },
                    super::DocumentationBlock {
                        kind: DocumentationBlockKind::Paragraph,
                        text: "Use find otp.".to_string(),
                        markup: "Use find otp.".to_string(),
                        search_text: "use find otp.".to_string(),
                        links: Vec::new(),
                        anchor: None,
                        list_marker: None,
                    },
                ],
            ),
        ];

        let empty = search_documents(&documents, "");
        assert_eq!(empty.len(), 2);
        assert_eq!(empty[0].title, "Getting Started");
        assert_eq!(empty[1].title, "Search Guide");

        let matches = search_documents(&documents, "otp");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].title, "Search Guide");
        assert_eq!(matches[0].block_index, Some(0));
        assert_eq!(matches[1].block_index, Some(1));
    }

    #[test]
    fn table_helpers_group_runs_and_split_cells() {
        let blocks = vec![
            super::DocumentationBlock {
                kind: DocumentationBlockKind::Paragraph,
                text: "before".to_string(),
                markup: "before".to_string(),
                search_text: "before".to_string(),
                links: Vec::new(),
                anchor: None,
                list_marker: None,
            },
            super::DocumentationBlock {
                kind: DocumentationBlockKind::TableRow,
                text: "A | B".to_string(),
                markup: "A | B".to_string(),
                search_text: "a | b".to_string(),
                links: Vec::new(),
                anchor: None,
                list_marker: None,
            },
            super::DocumentationBlock {
                kind: DocumentationBlockKind::TableRow,
                text: "1 | 2".to_string(),
                markup: "1 | 2".to_string(),
                search_text: "1 | 2".to_string(),
                links: Vec::new(),
                anchor: None,
                list_marker: None,
            },
            super::DocumentationBlock {
                kind: DocumentationBlockKind::Paragraph,
                text: "after".to_string(),
                markup: "after".to_string(),
                search_text: "after".to_string(),
                links: Vec::new(),
                anchor: None,
                list_marker: None,
            },
        ];

        assert_eq!(table_run_end(&blocks, 1), 3);
        assert_eq!(table_cells("A | B | C"), vec!["A", "B", "C"]);
    }

    #[test]
    fn preferred_doc_locales_normalize_locale_variants() {
        let locales = collect_preferred_doc_locales([
            "nl_NL.UTF-8:fr".to_string(),
            "POSIX".to_string(),
            "nl".to_string(),
        ]);

        assert_eq!(locales, vec!["nl_nl", "nl", "fr"]);
    }

    #[test]
    fn document_sources_prefer_locale_specific_variants() {
        let sources = [
            CompiledDocumentSource {
                path: "guide.md",
                locale: None,
                source: "english",
            },
            CompiledDocumentSource {
                path: "guide.md",
                locale: Some("nl"),
                source: "dutch",
            },
            CompiledDocumentSource {
                path: "guide.md",
                locale: Some("pt_BR"),
                source: "brazilian-portuguese",
            },
        ];

        assert_eq!(
            select_document_source_from(&sources, "guide.md", &["nl".to_string()]),
            Some("dutch")
        );
        assert_eq!(
            select_document_source_from(&sources, "guide.md", &["pt_br".to_string()]),
            Some("brazilian-portuguese")
        );
        assert_eq!(
            select_document_source_from(&sources, "guide.md", &["de".to_string()]),
            Some("english")
        );
    }

    #[test]
    fn inline_markup_formats_code_spans() {
        assert_eq!(
            inline_markup("Use `pass` and `gpg`."),
            "Use <tt>pass</tt> and <tt>gpg</tt>."
        );
        assert_eq!(
            inline_markup("Open **Documentation** from the menu."),
            "Open <b>Documentation</b> from the menu."
        );
        assert_eq!(
            inline_markup("Keep **`pass`** visible."),
            "Keep <b><tt>pass</tt></b> visible."
        );
        assert_eq!(
            inline_markup("Keep `unterminated as text"),
            "Keep `unterminated as text"
        );
    }
}
