mod field_values;
mod menu;
#[cfg(test)]
mod tests;
mod unlock;
mod weak_passwords;

use crate::password::page::PasswordPageState;
use crate::support::actions::register_window_action;
use crate::support::object_data::non_null_to_string_option;
use crate::support::ui::{
    append_action_row_with_button, append_info_row, append_spinner_row, clear_list_box,
    reveal_navigation_page, visible_navigation_page_is,
};
use crate::window::navigation::{
    show_secondary_page_chrome, HasWindowChrome, WindowNavigationState,
};
use adw::gtk::{ListBox, SearchEntry};
use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, NavigationPage, ToastOverlay};
use std::cell::RefCell;
use std::rc::Rc;

use self::field_values::FieldValueBrowserState;
use self::menu::{
    append_optional_doc_row, append_optional_log_rows, append_optional_pass_import_row,
    append_optional_setup_row,
};
use self::weak_passwords::WeakPasswordToolState;

const TOOLS_PAGE_TITLE: &str = "Tools";
const TOOLS_PAGE_SUBTITLE: &str = "Utilities and maintenance";
const FIELD_VALUES_TITLE: &str = "Browse field values";
const FIELD_VALUES_FIELDS_SUBTITLE: &str = "Pick a field from the current list.";
const FIELD_VALUES_VALUES_SUBTITLE: &str = "Pick a value from the current list.";
const FIELD_VALUES_ROW_TITLE: &str = "Browse field values";
const FIELD_VALUES_ROW_SUBTITLE: &str = "Browse unique field values from the current list.";
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
const WEAK_PASSWORDS_ROW_TITLE: &str = "Find weak passwords";
const WEAK_PASSWORDS_ROW_SUBTITLE: &str =
    "Scan the current list for passwords that fail basic checks.";
const WEAK_PASSWORDS_LOADING_TITLE: &str = "Scanning passwords";
const WEAK_PASSWORDS_LOADING_SUBTITLE: &str = "Reading password lines from the current list.";
const WEAK_PASSWORDS_EMPTY_TITLE: &str = "No weak passwords found";
const WEAK_PASSWORDS_EMPTY_SUBTITLE: &str =
    "No loaded pass files matched the current weak-password checks.";
const WEAK_PASSWORDS_FILTER_EMPTY_TITLE: &str = "No matching results";
const WEAK_PASSWORDS_FILTER_EMPTY_SUBTITLE: &str = "Try a different search term.";

#[derive(Clone)]
pub struct ToolsPageState {
    pub window: ApplicationWindow,
    pub navigation: WindowNavigationState,
    pub page: NavigationPage,
    pub list: ListBox,
    pub logs_list: ListBox,
    pub overlay: ToastOverlay,
    pub password_page: PasswordPageState,
    pub field_values_page: NavigationPage,
    pub field_values_search_entry: SearchEntry,
    pub field_values_list: ListBox,
    pub value_values_page: NavigationPage,
    pub value_values_search_entry: SearchEntry,
    pub value_values_list: ListBox,
    pub weak_passwords_page: NavigationPage,
    pub weak_passwords_search_entry: SearchEntry,
    pub weak_passwords_list: ListBox,
    pub root_list: ListBox,
    pub root_search_entry: SearchEntry,
    browser: Rc<FieldValueBrowserState>,
    weak_passwords: Rc<WeakPasswordToolState>,
    field_values_tool_row: Rc<RefCell<Option<ActionRow>>>,
    weak_passwords_tool_row: Rc<RefCell<Option<ActionRow>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FieldValueRequest {
    root: String,
    label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolReadMode {
    PasswordContents,
    PasswordLine,
}

impl ToolsPageState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        window: &ApplicationWindow,
        navigation: &WindowNavigationState,
        page: &NavigationPage,
        list: &ListBox,
        logs_list: &ListBox,
        overlay: &ToastOverlay,
        password_page: &PasswordPageState,
        field_values_page: &NavigationPage,
        field_values_search_entry: &SearchEntry,
        field_values_list: &ListBox,
        value_values_page: &NavigationPage,
        value_values_search_entry: &SearchEntry,
        value_values_list: &ListBox,
        weak_passwords_page: &NavigationPage,
        weak_passwords_search_entry: &SearchEntry,
        weak_passwords_list: &ListBox,
        root_list: &ListBox,
        root_search_entry: &SearchEntry,
    ) -> Self {
        let state = Self {
            window: window.clone(),
            navigation: navigation.clone(),
            page: page.clone(),
            list: list.clone(),
            logs_list: logs_list.clone(),
            overlay: overlay.clone(),
            password_page: password_page.clone(),
            field_values_page: field_values_page.clone(),
            field_values_search_entry: field_values_search_entry.clone(),
            field_values_list: field_values_list.clone(),
            value_values_page: value_values_page.clone(),
            value_values_search_entry: value_values_search_entry.clone(),
            value_values_list: value_values_list.clone(),
            weak_passwords_page: weak_passwords_page.clone(),
            weak_passwords_search_entry: weak_passwords_search_entry.clone(),
            weak_passwords_list: weak_passwords_list.clone(),
            root_list: root_list.clone(),
            root_search_entry: root_search_entry.clone(),
            browser: Rc::new(FieldValueBrowserState::default()),
            weak_passwords: Rc::new(WeakPasswordToolState::default()),
            field_values_tool_row: Rc::new(RefCell::new(None)),
            weak_passwords_tool_row: Rc::new(RefCell::new(None)),
        };
        state.connect_browser_handlers();
        state
    }

    pub fn rebuild(&self) {
        clear_list_box(&self.list);
        clear_list_box(&self.logs_list);
        *self.field_values_tool_row.borrow_mut() = None;
        *self.weak_passwords_tool_row.borrow_mut() = None;

        let state = self.clone();
        let field_values_row = append_action_row_with_button(
            &self.list,
            FIELD_VALUES_ROW_TITLE,
            FIELD_VALUES_ROW_SUBTITLE,
            "go-next-symbolic",
            move || state.prepare_field_values_browser(),
        );
        *self.field_values_tool_row.borrow_mut() = Some(field_values_row);

        let state = self.clone();
        let weak_passwords_row = append_action_row_with_button(
            &self.list,
            WEAK_PASSWORDS_ROW_TITLE,
            WEAK_PASSWORDS_ROW_SUBTITLE,
            "go-next-symbolic",
            move || state.prepare_weak_passwords_browser(),
        );
        *self.weak_passwords_tool_row.borrow_mut() = Some(weak_passwords_row);
        self.sync_tool_rows();

        append_optional_doc_row(self);
        append_optional_log_rows(self);
        append_optional_setup_row(self);
        append_optional_pass_import_row(self);
    }

    fn connect_browser_handlers(&self) {
        {
            let state = self.clone();
            self.field_values_search_entry
                .connect_search_changed(move |_| state.render_field_list());
        }

        {
            let state = self.clone();
            self.value_values_search_entry
                .connect_search_changed(move |_| state.render_value_list());
        }

        {
            let state = self.clone();
            self.weak_passwords_search_entry
                .connect_search_changed(move |_| state.render_weak_passwords_list());
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

    fn handle_navigation_visibility_change(&self) {
        if visible_navigation_page_is(&self.navigation.nav, &self.field_values_page) {
            self.field_values_search_entry.grab_focus();
        }
        if visible_navigation_page_is(&self.navigation.nav, &self.value_values_page) {
            self.value_values_search_entry.grab_focus();
        }
        if visible_navigation_page_is(&self.navigation.nav, &self.weak_passwords_page) {
            self.weak_passwords_search_entry.grab_focus();
        }
        if !self.browser_flow_is_visible() && self.browser_has_state() {
            self.reset_browser_state();
            self.reset_weak_passwords_state();
        }
    }

    fn browser_flow_is_visible(&self) -> bool {
        tool_browser_flow_is_visible(
            visible_navigation_page_is(&self.navigation.nav, &self.page),
            visible_navigation_page_is(&self.navigation.nav, &self.field_values_page),
            visible_navigation_page_is(&self.navigation.nav, &self.value_values_page),
            visible_navigation_page_is(&self.navigation.nav, &self.weak_passwords_page),
            visible_navigation_page_is(&self.navigation.nav, &self.password_page.page),
            visible_navigation_page_is(&self.navigation.nav, &self.password_page.raw_page),
        )
    }

    fn browser_has_state(&self) -> bool {
        self.browser.in_flight.get()
            || self.browser.catalog.borrow().is_some()
            || self.browser.selected_field.borrow().is_some()
            || self.weak_passwords.in_flight.get()
            || self.weak_passwords.results.borrow().is_some()
            || !self.field_values_search_entry.text().is_empty()
            || !self.value_values_search_entry.text().is_empty()
            || !self.weak_passwords_search_entry.text().is_empty()
    }

    fn set_field_values_tool_busy(&self, busy: bool) {
        self.browser.tool_busy.set(busy);
        self.sync_tool_rows();
    }

    fn set_weak_passwords_tool_busy(&self, busy: bool) {
        self.weak_passwords.tool_busy.set(busy);
        self.sync_tool_rows();
    }

    fn tools_are_busy(&self) -> bool {
        self.browser.tool_busy.get() || self.weak_passwords.tool_busy.get()
    }

    fn sync_tool_rows(&self) {
        let enabled = tool_rows_enabled(
            self.browser.tool_busy.get(),
            self.weak_passwords.tool_busy.get(),
        );
        set_tool_row_enabled(self.field_values_tool_row.borrow().as_ref(), enabled);
        set_tool_row_enabled(self.weak_passwords_tool_row.borrow().as_ref(), enabled);
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

    requests
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
    password_page_visible: bool,
    raw_password_page_visible: bool,
) -> bool {
    tools_page_visible
        || field_values_page_visible
        || value_values_page_visible
        || weak_passwords_page_visible
        || password_page_visible
        || raw_password_page_visible
}

fn set_tool_row_enabled(row: Option<&ActionRow>, enabled: bool) {
    let Some(row) = row else {
        return;
    };
    row.set_sensitive(enabled);
    row.set_activatable(enabled);
}

pub fn register_open_tools_action(window: &ApplicationWindow, state: &ToolsPageState) {
    let state = state.clone();
    register_window_action(window, "open-tools", move || {
        let chrome = state.navigation.window_chrome();
        show_secondary_page_chrome(&chrome, TOOLS_PAGE_TITLE, TOOLS_PAGE_SUBTITLE, false);
        state.rebuild();
        reveal_navigation_page(&state.navigation.nav, &state.page);
    });
}
