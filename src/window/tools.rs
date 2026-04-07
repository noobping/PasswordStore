mod audit;
mod field_values;
mod menu;
#[cfg(test)]
mod tests;
mod unlock;
mod weak_passwords;

use crate::i18n::gettext;
use crate::password::list::password_list_render_generation;
use crate::password::page::PasswordPageState;
use crate::preferences::Preferences;
use crate::store::management::StoreImportToolRowState;
use crate::store::support::StoreSupportCache;
use crate::support::actions::register_window_action;
use crate::support::object_data::non_null_to_string_option;
use crate::support::ui::{
    append_info_row, append_spinner_row, connect_keyboard_focusable_search_list_arrow_navigation,
    focus_first_keyboard_focusable_list_row, reveal_navigation_page, visible_navigation_page_is,
};
use crate::window::navigation::{
    show_secondary_page_chrome, HasWindowChrome, WindowNavigationState,
};
use adw::gio::{prelude::*, SimpleAction};
use adw::gtk::{
    Box as GtkBox, Button, Image, ListBox, MenuButton, Popover, ScrolledWindow, SearchEntry,
    Spinner, Stack,
};
use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, NavigationPage, StatusPage, ToastOverlay};
use std::cell::RefCell;
use std::rc::Rc;

use self::audit::AuditToolState;
use self::field_values::FieldValueBrowserState;
use self::menu::{
    append_optional_pass_import_row, append_optional_setup_row, configure_optional_doc_row,
    configure_optional_log_rows, sync_optional_setup_row,
};
use self::weak_passwords::WeakPasswordToolState;

const TOOLS_PAGE_TITLE: &str = "Tools";
const TOOLS_PAGE_SUBTITLE: &str = "Utilities and maintenance";
const FIELD_VALUES_TITLE: &str = "Browse field values";
const FIELD_VALUES_FIELDS_SUBTITLE: &str = "Pick a field from the current list.";
const FIELD_VALUES_VALUES_SUBTITLE: &str = "Pick a value from the current list.";
const FIELD_VALUES_ROW_SUBTITLE: &str = "Browse unique field values from the current list.";
const FIELD_VALUES_ROW_DISABLED_SUBTITLE: &str =
    "Unavailable because all configured stores use FIDO2 security keys.";
const FIELD_VALUES_LOADING_TITLE: &str = "Loading field values";
const FIELD_VALUES_LOADING_SUBTITLE: &str = "Reading searchable pass fields from the current list.";
const FIELD_VALUES_EMPTY_TITLE: &str = "No searchable fields";
const FIELD_VALUES_EMPTY_SUBTITLE: &str =
    "The current list doesn't have any searchable pass fields.";
const FIELD_VALUES_FILTER_EMPTY_TITLE: &str = "No matching fields";
const FIELD_VALUES_FILTER_EMPTY_SUBTITLE: &str = "Try a different field filter.";
const VALUE_VALUES_EMPTY_TITLE: &str = "No values";
const VALUE_VALUES_EMPTY_SUBTITLE: &str = "This field has no searchable values.";
const VALUE_VALUES_FILTER_EMPTY_TITLE: &str = "No matching values";
const VALUE_VALUES_FILTER_EMPTY_SUBTITLE: &str = "Try a different value filter.";
const WEAK_PASSWORDS_TITLE: &str = "Find weak passwords";
const WEAK_PASSWORDS_SUBTITLE: &str = "Scan the current list for passwords that fail basic checks.";
const WEAK_PASSWORDS_ROW_SUBTITLE: &str =
    "Scan the current list for passwords that fail basic checks.";
const WEAK_PASSWORDS_ROW_DISABLED_SUBTITLE: &str =
    "Unavailable because all configured stores use FIDO2 security keys.";
const WEAK_PASSWORDS_LOADING_TITLE: &str = "Scanning passwords";
const WEAK_PASSWORDS_LOADING_SUBTITLE: &str = "Reading password lines from the current list.";
const WEAK_PASSWORDS_EMPTY_TITLE: &str = "No weak passwords found";
const WEAK_PASSWORDS_EMPTY_SUBTITLE: &str =
    "No loaded pass files matched the current weak-password checks.";
const WEAK_PASSWORDS_FILTER_EMPTY_TITLE: &str = "No matching results";
const WEAK_PASSWORDS_FILTER_EMPTY_SUBTITLE: &str = "Try a different search term.";
const AUDIT_TITLE: &str = "Audit";
const AUDIT_SUBTITLE: &str = "Git history and verification";
const AUDIT_ROW_SUBTITLE: &str = "Inspect Git history across stores and verify commit signatures.";
const AUDIT_ROW_DISABLED_SUBTITLE: &str = "No Git-backed stores available.";
const AUDIT_LOADING_TITLE: &str = "Loading data";
const AUDIT_LOADING_SUBTITLE: &str = "Inspecting Git-backed stores and discovering branches.";
const AUDIT_NO_STORES_TITLE: &str = "No Git-backed stores";
const AUDIT_NO_STORES_SUBTITLE: &str = "Add a store with a Git repository to inspect history here.";
const AUDIT_ERROR_TITLE: &str = "Couldn't load data";
const AUDIT_EMPTY_SELECTION_TITLE: &str = "Nothing selected";
const AUDIT_EMPTY_SELECTION_SUBTITLE: &str =
    "Select at least one store and one branch in the filter menu.";
const AUDIT_FILTER_EMPTY_TITLE: &str = "No matching branches";
const AUDIT_FILTER_EMPTY_SUBTITLE: &str =
    "The current filters don't match any branches in the selected stores.";
const AUDIT_SEARCH_EMPTY_TITLE: &str = "No matching results";
const AUDIT_SEARCH_EMPTY_SUBTITLE: &str = "Try a different search term or load more commits.";
const AUDIT_LOADING_COMMITS_TITLE: &str = "Loading commits";
const AUDIT_LOADING_COMMITS_SUBTITLE: &str = "Reading recent Git history for this branch.";
const AUDIT_EMPTY_BRANCH_TITLE: &str = "No commits loaded";
const AUDIT_EMPTY_BRANCH_SUBTITLE: &str =
    "This branch doesn't have any commits available to audit.";
const AUDIT_LOAD_MORE_TITLE: &str = "Load more commits";
const AUDIT_LOAD_MORE_SUBTITLE: &str = "Read the next {count} commits.";

#[derive(Clone)]
struct ToolSelectPageState {
    page: NavigationPage,
    list: ListBox,
    logs_list: ListBox,
    field_values_row: ActionRow,
    field_values_suffix_stack: Stack,
    field_values_suffix_arrow: Image,
    field_values_spinner: Spinner,
    weak_passwords_row: ActionRow,
    weak_passwords_suffix_stack: Stack,
    weak_passwords_suffix_arrow: Image,
    weak_passwords_spinner: Spinner,
    audit_row: ActionRow,
    audit_suffix_stack: Stack,
    audit_suffix_arrow: Image,
    audit_spinner: Spinner,
    docs_row: ActionRow,
    logs_row: ActionRow,
    copy_logs_row: ActionRow,
    copy_logs_button: Button,
    setup_row: Rc<RefCell<Option<ActionRow>>>,
    pass_import_row: Rc<RefCell<Option<StoreImportToolRowState>>>,
}

#[derive(Clone)]
struct ToolFieldBrowserPageState {
    field_page: NavigationPage,
    field_search_entry: SearchEntry,
    field_list: ListBox,
    value_page: NavigationPage,
    value_search_entry: SearchEntry,
    value_list: ListBox,
    browser: Rc<FieldValueBrowserState>,
}

#[derive(Clone)]
struct ToolWeakPasswordPageState {
    page: NavigationPage,
    search_entry: SearchEntry,
    list: ListBox,
    weak_passwords: Rc<WeakPasswordToolState>,
}

#[derive(Clone)]
struct ToolAuditPageState {
    page: NavigationPage,
    search_entry: SearchEntry,
    stack: Stack,
    audit_status: StatusPage,
    scrolled: ScrolledWindow,
    content: GtkBox,
    filter_button: MenuButton,
    filter_popover: Popover,
    filter_store_box: GtkBox,
    filter_branch_box: GtkBox,
    audit: Rc<AuditToolState>,
}

#[derive(Clone)]
pub struct ToolsPageState {
    window: ApplicationWindow,
    navigation: WindowNavigationState,
    overlay: ToastOverlay,
    password_page: PasswordPageState,
    root_list: ListBox,
    root_search_entry: SearchEntry,
    select_page: ToolSelectPageState,
    field_browser: ToolFieldBrowserPageState,
    weak_password_page: ToolWeakPasswordPageState,
    audit_page: ToolAuditPageState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FieldValueRequest {
    root: String,
    label: String,
}

pub struct ToolBrowserWidgets<'a> {
    pub page: &'a NavigationPage,
    pub search_entry: &'a SearchEntry,
    pub list: &'a ListBox,
}

pub struct ToolAuditWidgets<'a> {
    pub page: &'a NavigationPage,
    pub search_entry: &'a SearchEntry,
    pub stack: &'a Stack,
    pub status: &'a StatusPage,
    pub scrolled: &'a ScrolledWindow,
    pub content: &'a GtkBox,
    pub filter_button: &'a MenuButton,
    pub filter_popover: &'a Popover,
    pub filter_store_box: &'a GtkBox,
    pub filter_branch_box: &'a GtkBox,
}

pub struct ToolsPageWidgets<'a> {
    pub window: &'a ApplicationWindow,
    pub navigation: &'a WindowNavigationState,
    pub page: &'a NavigationPage,
    pub list: &'a ListBox,
    pub field_values_row: &'a ActionRow,
    pub field_values_suffix_stack: &'a Stack,
    pub field_values_suffix_arrow: &'a Image,
    pub field_values_spinner: &'a Spinner,
    pub weak_passwords_row: &'a ActionRow,
    pub weak_passwords_suffix_stack: &'a Stack,
    pub weak_passwords_suffix_arrow: &'a Image,
    pub weak_passwords_spinner: &'a Spinner,
    pub audit_row: &'a ActionRow,
    pub audit_suffix_stack: &'a Stack,
    pub audit_suffix_arrow: &'a Image,
    pub audit_spinner: &'a Spinner,
    pub logs_list: &'a ListBox,
    pub docs_row: &'a ActionRow,
    pub logs_row: &'a ActionRow,
    pub copy_logs_row: &'a ActionRow,
    pub copy_logs_button: &'a Button,
    pub overlay: &'a ToastOverlay,
    pub password_page: &'a PasswordPageState,
    pub field_values: ToolBrowserWidgets<'a>,
    pub value_values: ToolBrowserWidgets<'a>,
    pub weak_passwords: ToolBrowserWidgets<'a>,
    pub audit: ToolAuditWidgets<'a>,
    pub root_list: &'a ListBox,
    pub root_search_entry: &'a SearchEntry,
}

impl ToolsPageState {
    pub fn new(widgets: ToolsPageWidgets<'_>) -> Self {
        let state = Self {
            window: widgets.window.clone(),
            navigation: widgets.navigation.clone(),
            overlay: widgets.overlay.clone(),
            password_page: widgets.password_page.clone(),
            root_list: widgets.root_list.clone(),
            root_search_entry: widgets.root_search_entry.clone(),
            select_page: ToolSelectPageState {
                page: widgets.page.clone(),
                list: widgets.list.clone(),
                field_values_row: widgets.field_values_row.clone(),
                field_values_suffix_stack: widgets.field_values_suffix_stack.clone(),
                field_values_suffix_arrow: widgets.field_values_suffix_arrow.clone(),
                field_values_spinner: widgets.field_values_spinner.clone(),
                weak_passwords_row: widgets.weak_passwords_row.clone(),
                weak_passwords_suffix_stack: widgets.weak_passwords_suffix_stack.clone(),
                weak_passwords_suffix_arrow: widgets.weak_passwords_suffix_arrow.clone(),
                weak_passwords_spinner: widgets.weak_passwords_spinner.clone(),
                audit_row: widgets.audit_row.clone(),
                audit_suffix_stack: widgets.audit_suffix_stack.clone(),
                audit_suffix_arrow: widgets.audit_suffix_arrow.clone(),
                audit_spinner: widgets.audit_spinner.clone(),
                logs_list: widgets.logs_list.clone(),
                docs_row: widgets.docs_row.clone(),
                logs_row: widgets.logs_row.clone(),
                copy_logs_row: widgets.copy_logs_row.clone(),
                copy_logs_button: widgets.copy_logs_button.clone(),
                setup_row: Rc::new(RefCell::new(None)),
                pass_import_row: Rc::new(RefCell::new(None)),
            },
            field_browser: ToolFieldBrowserPageState {
                field_page: widgets.field_values.page.clone(),
                field_search_entry: widgets.field_values.search_entry.clone(),
                field_list: widgets.field_values.list.clone(),
                value_page: widgets.value_values.page.clone(),
                value_search_entry: widgets.value_values.search_entry.clone(),
                value_list: widgets.value_values.list.clone(),
                browser: Rc::new(FieldValueBrowserState::default()),
            },
            weak_password_page: ToolWeakPasswordPageState {
                page: widgets.weak_passwords.page.clone(),
                search_entry: widgets.weak_passwords.search_entry.clone(),
                list: widgets.weak_passwords.list.clone(),
                weak_passwords: Rc::new(WeakPasswordToolState::default()),
            },
            audit_page: ToolAuditPageState {
                page: widgets.audit.page.clone(),
                search_entry: widgets.audit.search_entry.clone(),
                stack: widgets.audit.stack.clone(),
                audit_status: widgets.audit.status.clone(),
                scrolled: widgets.audit.scrolled.clone(),
                content: widgets.audit.content.clone(),
                filter_button: widgets.audit.filter_button.clone(),
                filter_popover: widgets.audit.filter_popover.clone(),
                filter_store_box: widgets.audit.filter_store_box.clone(),
                filter_branch_box: widgets.audit.filter_branch_box.clone(),
                audit: Rc::new(AuditToolState::default()),
            },
        };
        state.initialize_select_page();
        state.connect_browser_handlers();
        state.connect_browser_keyboard_handlers();
        state
    }

    fn initialize_select_page(&self) {
        let state = self.clone();
        self.select_page
            .field_values_row
            .connect_activated(move |_| state.prepare_field_values_browser());

        let state = self.clone();
        self.select_page
            .weak_passwords_row
            .connect_activated(move |_| state.prepare_weak_passwords_browser());

        let state = self.clone();
        self.select_page
            .audit_row
            .connect_activated(move |_| state.prepare_audit_page());

        configure_optional_doc_row(self);
        configure_optional_log_rows(self);
        *self.select_page.setup_row.borrow_mut() = append_optional_setup_row(self);
        *self.select_page.pass_import_row.borrow_mut() = append_optional_pass_import_row(self);
        self.sync_action_availability();
        self.sync_tool_rows();
        sync_optional_setup_row(self.select_page.setup_row.borrow().as_ref());
    }

    pub fn refresh_select_page(&self) {
        self.clear_audit_transient_state();
        self.invalidate_stale_tool_cache();
        self.sync_action_availability();
        self.sync_tool_rows();
        sync_optional_setup_row(self.select_page.setup_row.borrow().as_ref());
        if let Some(pass_import_row) = self.select_page.pass_import_row.borrow().as_ref() {
            pass_import_row.refresh();
        }
    }

    fn connect_browser_handlers(&self) {
        {
            let state = self.clone();
            self.field_browser
                .field_search_entry
                .connect_search_changed(move |_| state.render_field_list());
        }

        {
            let state = self.clone();
            self.field_browser
                .value_search_entry
                .connect_search_changed(move |_| state.render_value_list());
        }

        {
            let state = self.clone();
            self.weak_password_page
                .search_entry
                .connect_search_changed(move |_| state.render_weak_passwords_list());
        }

        {
            let state = self.clone();
            self.audit_page
                .search_entry
                .connect_search_changed(move |_| state.render_audit_page());
        }

        {
            let state = self.clone();
            self.navigation
                .nav
                .connect_notify_local(Some("visible-page"), move |_, _| {
                    state.handle_navigation_visibility_change();
                });
        }
    }

    fn connect_browser_keyboard_handlers(&self) {
        connect_keyboard_focusable_search_list_arrow_navigation(
            &self.field_browser.field_list,
            &self.field_browser.field_search_entry,
        );
        connect_keyboard_focusable_search_list_arrow_navigation(
            &self.field_browser.value_list,
            &self.field_browser.value_search_entry,
        );
        connect_keyboard_focusable_search_list_arrow_navigation(
            &self.weak_password_page.list,
            &self.weak_password_page.search_entry,
        );
    }

    fn handle_navigation_visibility_change(&self) {
        self.sync_audit_filter_button();
        if !visible_navigation_page_is(&self.navigation.nav, &self.audit_page.page)
            && self.audit_has_transient_state()
        {
            self.clear_audit_transient_state();
        }

        if visible_navigation_page_is(&self.navigation.nav, &self.weak_password_page.page) {
            self.refresh_weak_passwords_browser_if_needed();
            return;
        }

        if visible_navigation_page_is(&self.navigation.nav, &self.audit_page.page) {
            return;
        }

        if self.browser_flow_is_visible() {
            return;
        }

        self.reset_field_values_view();
        self.clear_weak_passwords_cache();
        self.clear_audit_transient_state();
        self.invalidate_stale_tool_cache();
    }

    fn browser_flow_is_visible(&self) -> bool {
        tool_browser_flow_is_visible(
            visible_navigation_page_is(&self.navigation.nav, &self.select_page.page),
            visible_navigation_page_is(&self.navigation.nav, &self.field_browser.field_page),
            visible_navigation_page_is(&self.navigation.nav, &self.field_browser.value_page),
            visible_navigation_page_is(&self.navigation.nav, &self.weak_password_page.page),
            visible_navigation_page_is(&self.navigation.nav, &self.audit_page.page),
            visible_navigation_page_is(&self.navigation.nav, &self.password_page.page),
            visible_navigation_page_is(&self.navigation.nav, &self.password_page.raw_page),
        )
    }

    fn current_password_list_generation(&self) -> Option<u64> {
        password_list_render_generation(&self.root_list)
    }

    fn field_values_cache_is_current(&self, generation: Option<u64>) -> bool {
        self.field_browser.browser.source_generation.get() == generation
            && self.field_browser.browser.catalog.borrow().is_some()
    }

    fn invalidate_stale_tool_cache(&self) {
        let generation = self.current_password_list_generation();
        if self.field_browser.browser.source_generation.get() != generation {
            self.clear_field_values_cache();
        }
    }

    fn set_field_values_tool_busy(&self, busy: bool) {
        self.field_browser.browser.tool_busy.set(busy);
        self.sync_tool_rows();
    }

    fn set_weak_passwords_tool_busy(&self, busy: bool) {
        self.weak_password_page.weak_passwords.tool_busy.set(busy);
        self.sync_tool_rows();
    }

    fn tools_are_busy(&self) -> bool {
        self.field_browser.browser.tool_busy.get()
            || self.weak_password_page.weak_passwords.tool_busy.get()
    }

    fn sync_tool_rows(&self) {
        let available =
            password_read_tools_available_for_store_roots(&Preferences::new().store_roots());
        let enabled = available
            && tool_rows_enabled(
                self.field_browser.browser.tool_busy.get(),
                self.weak_password_page.weak_passwords.tool_busy.get(),
            );
        set_tool_action_row_state(
            &self.select_page.field_values_row,
            &self.select_page.field_values_suffix_stack,
            &self.select_page.field_values_suffix_arrow,
            &self.select_page.field_values_spinner,
            enabled,
            self.field_browser.browser.tool_busy.get(),
            if available {
                FIELD_VALUES_ROW_SUBTITLE
            } else {
                FIELD_VALUES_ROW_DISABLED_SUBTITLE
            },
        );
        set_tool_action_row_state(
            &self.select_page.weak_passwords_row,
            &self.select_page.weak_passwords_suffix_stack,
            &self.select_page.weak_passwords_suffix_arrow,
            &self.select_page.weak_passwords_spinner,
            enabled,
            self.weak_password_page.weak_passwords.tool_busy.get(),
            if available {
                WEAK_PASSWORDS_ROW_SUBTITLE
            } else {
                WEAK_PASSWORDS_ROW_DISABLED_SUBTITLE
            },
        );
        self.sync_audit_tool_row(enabled);
    }

    pub fn sync_action_availability(&self) {
        sync_tools_action_availability(&self.window);
    }

    pub fn open(&self) {
        let chrome = self.navigation.window_chrome();
        show_secondary_page_chrome(&chrome, TOOLS_PAGE_TITLE, TOOLS_PAGE_SUBTITLE, false);
        self.refresh_select_page();
        reveal_navigation_page(&self.navigation.nav, &self.select_page.page);
        let _ = focus_first_keyboard_focusable_list_row(&self.select_page.list)
            || focus_first_keyboard_focusable_list_row(&self.select_page.logs_list);
    }
}

fn collect_loaded_entry_requests(list: &ListBox) -> Vec<FieldValueRequest> {
    let mut requests = Vec::new();
    let mut child = list.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        let Ok(row) = widget.downcast::<adw::gtk::ListBoxRow>() else {
            child = next;
            continue;
        };
        let Some(root) = non_null_to_string_option(&row, "root") else {
            child = next;
            continue;
        };
        let Some(label) = non_null_to_string_option(&row, "label") else {
            child = next;
            continue;
        };
        requests.push(FieldValueRequest { root, label });
        child = next;
    }

    let mut store_support = StoreSupportCache::default();
    filter_tool_requests(requests, |store_path| {
        store_support.supports_password_read_tools(store_path)
    })
}

fn filter_tool_requests(
    requests: Vec<FieldValueRequest>,
    mut store_is_compatible: impl FnMut(&str) -> bool,
) -> Vec<FieldValueRequest> {
    requests
        .into_iter()
        .filter(|request| store_is_compatible(&request.root))
        .collect()
}

fn password_read_tools_available_for_store_roots(stores: &[String]) -> bool {
    let mut store_support = StoreSupportCache::default();
    password_read_tools_available_for_store_roots_with(stores, |store_path| {
        store_support.supports_password_read_tools(store_path)
    })
}

fn password_read_tools_available_for_store_roots_with(
    stores: &[String],
    mut store_is_compatible: impl FnMut(&str) -> bool,
) -> bool {
    stores.is_empty()
        || stores
            .iter()
            .any(|store_path| store_is_compatible(store_path))
}

fn next_generation(current: u64) -> u64 {
    current.wrapping_add(1).max(1)
}

fn append_loading_rows(list: &ListBox, title: &str, subtitle: &str) {
    append_info_row(list, title, subtitle);
    append_spinner_row(list);
}

fn tool_rows_enabled(field_values_busy: bool, weak_passwords_busy: bool) -> bool {
    !(field_values_busy || weak_passwords_busy)
}

const fn tool_browser_flow_is_visible(
    tools_page_visible: bool,
    field_values_page_visible: bool,
    value_values_page_visible: bool,
    weak_passwords_page_visible: bool,
    audit_page_visible: bool,
    password_page_visible: bool,
    raw_password_page_visible: bool,
) -> bool {
    tools_page_visible
        || field_values_page_visible
        || value_values_page_visible
        || weak_passwords_page_visible
        || audit_page_visible
        || password_page_visible
        || raw_password_page_visible
}

fn set_tool_row_enabled(row: &ActionRow, enabled: bool) {
    row.set_sensitive(enabled);
    row.set_activatable(enabled);
}

fn set_tool_row_suffix_loading(stack: &Stack, arrow: &Image, spinner: &Spinner, loading: bool) {
    spinner.set_spinning(loading);
    if loading {
        stack.set_visible_child(spinner);
    } else {
        stack.set_visible_child(arrow);
    }
}

fn set_tool_action_row_state(
    row: &ActionRow,
    suffix_stack: &Stack,
    suffix_arrow: &Image,
    spinner: &Spinner,
    enabled: bool,
    loading: bool,
    subtitle: &str,
) {
    row.set_subtitle(&gettext(subtitle));
    set_tool_row_suffix_loading(suffix_stack, suffix_arrow, spinner, loading);
    set_tool_row_enabled(row, enabled);
}

fn set_window_action_enabled(window: &ApplicationWindow, name: &str, enabled: bool) {
    let Some(action) = window.lookup_action(name) else {
        return;
    };
    let Ok(action) = action.downcast::<SimpleAction>() else {
        return;
    };
    action.set_enabled(enabled);
}

pub fn sync_tools_action_availability(window: &ApplicationWindow) {
    set_window_action_enabled(window, "open-tools", true);
}

pub fn register_open_tools_action(window: &ApplicationWindow, open_tools: impl Fn() + 'static) {
    register_window_action(window, "open-tools", open_tools);
    sync_tools_action_availability(window);
}
