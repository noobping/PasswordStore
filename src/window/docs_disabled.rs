use adw::gtk::{Box as GtkBox, ListBox, ScrolledWindow, SearchEntry};
use adw::{ApplicationWindow, NavigationPage};

pub const DOCS_PAGE_TITLE: &str = "Documentation";
pub const DOCS_PAGE_SUBTITLE: &str = "Guides and reference";

pub struct DocumentationPageWidgets<'a> {
    pub navigation: &'a crate::window::navigation::WindowNavigationState,
    pub page: &'a NavigationPage,
    pub search_entry: &'a SearchEntry,
    pub list: &'a ListBox,
    pub detail_page: &'a NavigationPage,
    pub detail_scrolled: &'a ScrolledWindow,
    pub detail_box: &'a GtkBox,
}

#[derive(Clone, Default)]
pub struct DocumentationPageState;

impl DocumentationPageState {
    pub fn new(_widgets: DocumentationPageWidgets<'_>) -> Self {
        Self
    }
}

pub fn register_open_docs_action(_window: &ApplicationWindow, _state: &DocumentationPageState) {}
