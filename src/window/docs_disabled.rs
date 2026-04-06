use adw::gtk::{Box as GtkBox, ListBox, ScrolledWindow, SearchEntry};
use adw::{ApplicationWindow, NavigationPage};
use std::marker::PhantomData;

pub const DOCS_PAGE_TITLE: &str = "Documentation";
pub const DOCS_PAGE_SUBTITLE: &str = "Guides and reference";

pub struct DocumentationPageWidgets<'a>(PhantomData<&'a ()>);

impl<'a> DocumentationPageWidgets<'a> {
    pub fn new(
        _navigation: &'a crate::window::navigation::WindowNavigationState,
        _search_entry: &'a SearchEntry,
        _list: &'a ListBox,
        _detail_page: &'a NavigationPage,
        _detail_scrolled: &'a ScrolledWindow,
        _detail_box: &'a GtkBox,
    ) -> Self {
        Self(PhantomData)
    }
}

#[derive(Clone, Default)]
pub struct DocumentationPageState;

impl DocumentationPageState {
    pub fn new(_widgets: DocumentationPageWidgets<'_>) -> Self {
        Self
    }
}

pub fn register_open_docs_action(_window: &ApplicationWindow, _state: &DocumentationPageState) {}
