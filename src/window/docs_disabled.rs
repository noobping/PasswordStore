use adw::gtk::{Box as GtkBox, ListBox, ScrolledWindow, SearchEntry};
use adw::{ApplicationWindow, NavigationPage};

pub const DOCS_PAGE_TITLE: &str = "Documentation";
pub const DOCS_PAGE_SUBTITLE: &str = "Guides and reference";

#[derive(Clone, Default)]
pub struct DocumentationPageState;

impl DocumentationPageState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        _navigation: &crate::window::navigation::WindowNavigationState,
        _page: &NavigationPage,
        _search_entry: &SearchEntry,
        _list: &ListBox,
        _detail_page: &NavigationPage,
        _detail_scrolled: &ScrolledWindow,
        _detail_box: &GtkBox,
    ) -> Self {
        Self
    }
}

pub fn register_open_docs_action(_window: &ApplicationWindow, _state: &DocumentationPageState) {}
