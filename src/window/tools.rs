use crate::backend::{
    preferred_ripasso_private_key_fingerprint_for_entry, read_password_entry, read_password_line,
    PasswordEntryError,
};
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::clipboard::set_clipboard_text;
#[cfg(all(target_os = "linux", feature = "setup"))]
use crate::logging::log_error;
use crate::password::file::{searchable_pass_fields, SearchablePassField};
use crate::password::model::OpenPassFile;
use crate::password::opened::clear_opened_pass_file;
use crate::password::page::{open_password_entry_page, PasswordPageState};
use crate::password::strength::weak_password_reason;
use crate::preferences::Preferences;
use crate::private_key::unlock::prompt_private_key_unlock_for_action;
#[cfg(all(target_os = "linux", feature = "setup"))]
use crate::setup::{
    can_install_locally, install_locally, is_installed_locally, local_menu_action_label,
    uninstall_locally,
};
use crate::store::management::schedule_store_import_row;
use crate::support::actions::register_window_action;
use crate::support::background::spawn_result_task;
use crate::support::object_data::non_null_to_string_option;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::support::runtime::has_host_permission;
use crate::support::ui::{
    append_action_row_with_button, append_info_row, append_spinner_row, clear_list_box,
    pop_navigation_to_root, reveal_navigation_page, visible_navigation_page_is,
};
#[cfg(debug_assertions)]
use crate::window::navigation::show_log_page;
use crate::window::navigation::{
    show_primary_page_chrome, show_secondary_page_chrome, HasWindowChrome, WindowNavigationState,
};
use adw::gtk::{ListBox, SearchEntry};
use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, NavigationPage, Toast, ToastOverlay};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

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

#[cfg(all(target_os = "linux", feature = "flatpak"))]
const FLATPAK_HOST_OVERRIDE_COMMAND: &str =
    "flatpak override --user --talk-name=org.freedesktop.Flatpak io.github.noobping.keycord";

#[derive(Clone)]
pub struct ToolsPageState {
    pub window: ApplicationWindow,
    pub navigation: WindowNavigationState,
    pub page: NavigationPage,
    pub list: ListBox,
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

#[derive(Default)]
struct FieldValueBrowserState {
    generation: Cell<u64>,
    in_flight: Cell<bool>,
    tool_busy: Cell<bool>,
    catalog: RefCell<Option<FieldValueCatalog>>,
    selected_field: RefCell<Option<String>>,
}

#[derive(Default)]
struct WeakPasswordToolState {
    generation: Cell<u64>,
    in_flight: Cell<bool>,
    tool_busy: Cell<bool>,
    results: RefCell<Option<Vec<WeakPasswordFinding>>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct FieldValueCatalog {
    fields: Vec<FieldCatalogEntry>,
    values_by_field: BTreeMap<String, Vec<ValueCatalogEntry>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FieldCatalogEntry {
    key: String,
    unique_value_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ValueCatalogEntry {
    display_value: String,
    normalized_value: String,
    match_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FieldValueRequest {
    root: String,
    label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FieldValueCatalogBatch {
    generation: u64,
    catalog: FieldValueCatalog,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WeakPasswordFinding {
    root: String,
    label: String,
    normalized_label: String,
    reason: String,
    normalized_reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WeakPasswordBatch {
    generation: u64,
    results: Vec<WeakPasswordFinding>,
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
            "dialog-warning-symbolic",
            move || state.prepare_weak_passwords_browser(),
        );
        *self.weak_passwords_tool_row.borrow_mut() = Some(weak_passwords_row);
        self.sync_tool_rows();

        append_optional_log_row(self);
        append_optional_setup_row(self);
        append_optional_flatpak_override_row(self);
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

    fn prepare_field_values_browser(&self) {
        if self.tools_are_busy() {
            return;
        }

        self.set_field_values_tool_busy(true);
        let requests = collect_loaded_entry_requests(&self.root_list);
        let state = self.clone();
        self.unlock_tool_keys_if_needed(
            requests,
            ToolReadMode::PasswordContents,
            Rc::new(move |requests| state.open_field_values_browser_with_requests(requests)),
            Rc::new({
                let state = self.clone();
                move || state.set_field_values_tool_busy(false)
            }),
        );
    }

    fn open_field_values_browser_with_requests(&self, requests: Vec<FieldValueRequest>) {
        self.reset_weak_passwords_state();
        self.clear_browser_state();
        let generation = next_generation(self.browser.generation.get());
        self.browser.generation.set(generation);
        self.browser.in_flight.set(true);
        self.render_field_list();

        let chrome = self.navigation.window_chrome();
        show_secondary_page_chrome(
            &chrome,
            FIELD_VALUES_TITLE,
            FIELD_VALUES_FIELDS_SUBTITLE,
            false,
        );
        reveal_navigation_page(&self.navigation.nav, &self.field_values_page);
        self.field_values_search_entry.grab_focus();

        if requests.is_empty() {
            self.apply_field_catalog_batch(FieldValueCatalogBatch {
                generation,
                catalog: FieldValueCatalog::default(),
            });
            return;
        }

        let state_for_result = self.clone();
        let state_for_disconnect = self.clone();
        spawn_result_task(
            move || build_field_value_catalog_batch(generation, requests),
            move |batch| state_for_result.apply_field_catalog_batch(batch),
            move || state_for_disconnect.handle_field_catalog_disconnect(generation),
        );
    }

    fn prepare_weak_passwords_browser(&self) {
        if self.tools_are_busy() {
            return;
        }

        self.set_weak_passwords_tool_busy(true);
        let requests = collect_loaded_entry_requests(&self.root_list);
        let state = self.clone();
        self.unlock_tool_keys_if_needed(
            requests,
            ToolReadMode::PasswordLine,
            Rc::new(move |requests| state.open_weak_passwords_browser_with_requests(requests)),
            Rc::new({
                let state = self.clone();
                move || state.set_weak_passwords_tool_busy(false)
            }),
        );
    }

    fn open_weak_passwords_browser_with_requests(&self, requests: Vec<FieldValueRequest>) {
        self.clear_browser_state();
        self.clear_weak_passwords_state();
        let generation = next_generation(self.weak_passwords.generation.get());
        self.weak_passwords.generation.set(generation);
        self.weak_passwords.in_flight.set(true);
        self.render_weak_passwords_list();

        let chrome = self.navigation.window_chrome();
        show_secondary_page_chrome(
            &chrome,
            WEAK_PASSWORDS_TITLE,
            WEAK_PASSWORDS_SUBTITLE,
            false,
        );
        reveal_navigation_page(&self.navigation.nav, &self.weak_passwords_page);
        self.weak_passwords_search_entry.grab_focus();

        if requests.is_empty() {
            self.apply_weak_password_batch(WeakPasswordBatch {
                generation,
                results: Vec::new(),
            });
            return;
        }

        let state_for_result = self.clone();
        let state_for_disconnect = self.clone();
        spawn_result_task(
            move || build_weak_password_batch(generation, requests),
            move |batch| state_for_result.apply_weak_password_batch(batch),
            move || state_for_disconnect.handle_weak_password_disconnect(generation),
        );
    }

    fn open_value_values_browser(&self, field_key: &str) {
        let field_changed = self.browser.selected_field.borrow().as_deref() != Some(field_key);
        *self.browser.selected_field.borrow_mut() = Some(field_key.to_string());
        if field_changed && !self.value_values_search_entry.text().is_empty() {
            self.value_values_search_entry.set_text("");
        }
        self.render_value_list();

        let chrome = self.navigation.window_chrome();
        show_secondary_page_chrome(
            &chrome,
            FIELD_VALUES_TITLE,
            FIELD_VALUES_VALUES_SUBTITLE,
            false,
        );
        reveal_navigation_page(&self.navigation.nav, &self.value_values_page);
        self.value_values_search_entry.grab_focus();
    }

    fn apply_weak_password_batch(&self, batch: WeakPasswordBatch) {
        if batch.generation != self.weak_passwords.generation.get() {
            return;
        }

        self.weak_passwords.in_flight.set(false);
        self.set_weak_passwords_tool_busy(false);
        *self.weak_passwords.results.borrow_mut() = Some(batch.results);
        self.render_weak_passwords_list();
    }

    fn handle_weak_password_disconnect(&self, generation: u64) {
        if generation != self.weak_passwords.generation.get() {
            return;
        }

        self.weak_passwords.in_flight.set(false);
        self.set_weak_passwords_tool_busy(false);
        self.render_weak_passwords_list();
    }

    fn apply_field_catalog_batch(&self, batch: FieldValueCatalogBatch) {
        if batch.generation != self.browser.generation.get() {
            return;
        }

        self.browser.in_flight.set(false);
        self.set_field_values_tool_busy(false);
        *self.browser.catalog.borrow_mut() = Some(batch.catalog);
        self.render_field_list();
        self.render_value_list();
    }

    fn handle_field_catalog_disconnect(&self, generation: u64) {
        if generation != self.browser.generation.get() {
            return;
        }

        self.browser.in_flight.set(false);
        self.set_field_values_tool_busy(false);
        self.render_field_list();
        self.render_value_list();
    }

    fn render_field_list(&self) {
        clear_list_box(&self.field_values_list);

        if self.browser.in_flight.get() {
            append_loading_rows(
                &self.field_values_list,
                FIELD_VALUES_LOADING_TITLE,
                FIELD_VALUES_LOADING_SUBTITLE,
            );
            return;
        }

        let Some(catalog) = self.browser.catalog.borrow().clone() else {
            append_info_row(
                &self.field_values_list,
                FIELD_VALUES_EMPTY_TITLE,
                FIELD_VALUES_EMPTY_SUBTITLE,
            );
            return;
        };

        let query = self.field_values_search_entry.text();
        let query = query.as_str().trim().to_lowercase();
        let fields = catalog
            .fields
            .iter()
            .filter(|field| query.is_empty() || field.key.contains(&query))
            .cloned()
            .collect::<Vec<_>>();

        if fields.is_empty() {
            append_info_row(
                &self.field_values_list,
                if query.is_empty() {
                    FIELD_VALUES_EMPTY_TITLE
                } else {
                    FIELD_VALUES_FILTER_EMPTY_TITLE
                },
                if query.is_empty() {
                    FIELD_VALUES_EMPTY_SUBTITLE
                } else {
                    FIELD_VALUES_FILTER_EMPTY_SUBTITLE
                },
            );
            return;
        }

        for field in fields {
            let subtitle = unique_values_subtitle(field.unique_value_count);
            let state = self.clone();
            let field_key = field.key.clone();
            append_action_row_with_button(
                &self.field_values_list,
                &field.key,
                &subtitle,
                "go-next-symbolic",
                move || state.open_value_values_browser(&field_key),
            );
        }
    }

    fn render_value_list(&self) {
        clear_list_box(&self.value_values_list);

        let Some(selected_field) = self.browser.selected_field.borrow().clone() else {
            append_info_row(
                &self.value_values_list,
                VALUE_VALUES_EMPTY_TITLE,
                VALUE_VALUES_EMPTY_SUBTITLE,
            );
            return;
        };

        let Some(catalog) = self.browser.catalog.borrow().clone() else {
            if self.browser.in_flight.get() {
                append_loading_rows(
                    &self.value_values_list,
                    FIELD_VALUES_LOADING_TITLE,
                    FIELD_VALUES_LOADING_SUBTITLE,
                );
            } else {
                append_info_row(
                    &self.value_values_list,
                    VALUE_VALUES_EMPTY_TITLE,
                    VALUE_VALUES_EMPTY_SUBTITLE,
                );
            }
            return;
        };

        let query = self.value_values_search_entry.text();
        let query = query.as_str().trim().to_lowercase();
        let values = catalog
            .values_by_field
            .get(&selected_field)
            .into_iter()
            .flatten()
            .filter(|value| query.is_empty() || value.normalized_value.contains(&query))
            .cloned()
            .collect::<Vec<_>>();

        if values.is_empty() {
            append_info_row(
                &self.value_values_list,
                if query.is_empty() {
                    VALUE_VALUES_EMPTY_TITLE
                } else {
                    VALUE_VALUES_FILTER_EMPTY_TITLE
                },
                if query.is_empty() {
                    VALUE_VALUES_EMPTY_SUBTITLE
                } else {
                    VALUE_VALUES_FILTER_EMPTY_SUBTITLE
                },
            );
            return;
        }

        for value in values {
            let subtitle = matching_items_subtitle(value.match_count);
            let state = self.clone();
            let field = selected_field.clone();
            let display_value = value.display_value.clone();
            append_action_row_with_button(
                &self.value_values_list,
                &value.display_value,
                &subtitle,
                "go-next-symbolic",
                move || state.apply_root_search(&format_exact_field_query(&field, &display_value)),
            );
        }
    }

    fn render_weak_passwords_list(&self) {
        clear_list_box(&self.weak_passwords_list);

        if self.weak_passwords.in_flight.get() {
            append_loading_rows(
                &self.weak_passwords_list,
                WEAK_PASSWORDS_LOADING_TITLE,
                WEAK_PASSWORDS_LOADING_SUBTITLE,
            );
            return;
        }

        let Some(results) = self.weak_passwords.results.borrow().clone() else {
            append_info_row(
                &self.weak_passwords_list,
                WEAK_PASSWORDS_EMPTY_TITLE,
                WEAK_PASSWORDS_EMPTY_SUBTITLE,
            );
            return;
        };

        let query = self.weak_passwords_search_entry.text();
        let query = query.as_str().trim().to_lowercase();
        let results = results
            .into_iter()
            .filter(|result| {
                query.is_empty()
                    || result.normalized_label.contains(&query)
                    || result.normalized_reason.contains(&query)
            })
            .collect::<Vec<_>>();

        if results.is_empty() {
            append_info_row(
                &self.weak_passwords_list,
                if query.is_empty() {
                    WEAK_PASSWORDS_EMPTY_TITLE
                } else {
                    WEAK_PASSWORDS_FILTER_EMPTY_TITLE
                },
                if query.is_empty() {
                    WEAK_PASSWORDS_EMPTY_SUBTITLE
                } else {
                    WEAK_PASSWORDS_FILTER_EMPTY_SUBTITLE
                },
            );
            return;
        }

        for result in results {
            let state = self.clone();
            let root = result.root.clone();
            let label = result.label.clone();
            append_action_row_with_button(
                &self.weak_passwords_list,
                &result.label,
                &result.reason,
                "go-next-symbolic",
                move || state.open_weak_password_entry(&root, &label),
            );
        }
    }

    fn apply_root_search(&self, query: &str) {
        self.reset_browser_state();
        pop_navigation_to_root(&self.navigation.nav);
        clear_opened_pass_file();

        let has_store_dirs = !Preferences::new().stores().is_empty();
        let chrome = self.navigation.window_chrome();
        show_primary_page_chrome(&chrome, has_store_dirs);

        self.root_search_entry.set_visible(true);
        self.root_search_entry.set_text(query);
        self.root_list.invalidate_filter();
        self.root_search_entry.grab_focus();
    }

    fn open_weak_password_entry(&self, root: &str, label: &str) {
        open_password_entry_page(
            &self.password_page,
            OpenPassFile::from_label(root, label),
            true,
        );
    }

    fn unlock_tool_keys_if_needed(
        &self,
        requests: Vec<FieldValueRequest>,
        read_mode: ToolReadMode,
        on_ready: Rc<dyn Fn(Vec<FieldValueRequest>)>,
        on_abort: Rc<dyn Fn()>,
    ) {
        if !Preferences::new().uses_integrated_backend() {
            on_ready(requests);
            return;
        }

        let requests_for_unlock = requests.clone();
        let on_ready_for_result = on_ready.clone();
        let on_abort_for_result = on_abort.clone();
        let overlay_for_result = self.overlay.clone();
        let overlay_for_disconnect = self.overlay.clone();
        spawn_result_task(
            move || collect_locked_tool_fingerprints(&requests_for_unlock, read_mode),
            move |fingerprints| {
                if fingerprints.is_empty() {
                    on_ready_for_result(requests);
                    return;
                }

                let on_abort_for_unlock = on_abort_for_result.clone();
                prompt_tool_unlock_sequence(
                    &overlay_for_result,
                    fingerprints,
                    Rc::new(move |success| {
                        if success {
                            on_ready(requests.clone());
                        } else {
                            on_abort_for_unlock();
                        }
                    }),
                );
            },
            move || {
                on_abort();
                overlay_for_disconnect.add_toast(Toast::new("Couldn't prepare tool access."));
            },
        );
    }

    fn clear_browser_state(&self) {
        self.browser
            .generation
            .set(next_generation(self.browser.generation.get()));
        self.browser.in_flight.set(false);
        *self.browser.catalog.borrow_mut() = None;
        *self.browser.selected_field.borrow_mut() = None;

        if !self.field_values_search_entry.text().is_empty() {
            self.field_values_search_entry.set_text("");
        }
        if !self.value_values_search_entry.text().is_empty() {
            self.value_values_search_entry.set_text("");
        }

        clear_list_box(&self.field_values_list);
        clear_list_box(&self.value_values_list);
    }

    fn reset_browser_state(&self) {
        self.clear_browser_state();
        self.set_field_values_tool_busy(false);
    }

    fn clear_weak_passwords_state(&self) {
        self.weak_passwords
            .generation
            .set(next_generation(self.weak_passwords.generation.get()));
        self.weak_passwords.in_flight.set(false);
        *self.weak_passwords.results.borrow_mut() = None;

        if !self.weak_passwords_search_entry.text().is_empty() {
            self.weak_passwords_search_entry.set_text("");
        }

        clear_list_box(&self.weak_passwords_list);
    }

    fn reset_weak_passwords_state(&self) {
        self.clear_weak_passwords_state();
        self.set_weak_passwords_tool_busy(false);
    }

    fn browser_flow_is_visible(&self) -> bool {
        visible_navigation_page_is(&self.navigation.nav, &self.page)
            || visible_navigation_page_is(&self.navigation.nav, &self.field_values_page)
            || visible_navigation_page_is(&self.navigation.nav, &self.value_values_page)
            || visible_navigation_page_is(&self.navigation.nav, &self.weak_passwords_page)
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

#[cfg(debug_assertions)]
fn append_optional_log_row(state: &ToolsPageState) {
    let navigation = state.navigation.clone();
    append_action_row_with_button(
        &state.list,
        "Open logs",
        "Inspect recent app and command output.",
        "document-open-symbolic",
        move || show_log_page(&navigation),
    );
}

#[cfg(not(debug_assertions))]
fn append_optional_log_row(_state: &ToolsPageState) {}

#[cfg(all(target_os = "linux", feature = "setup"))]
fn append_optional_setup_row(state: &ToolsPageState) {
    if !can_install_locally() {
        return;
    }

    let title = local_menu_action_label(is_installed_locally());
    let overlay = state.overlay.clone();
    let refresh_state = state.clone();
    append_action_row_with_button(
        &state.list,
        title,
        "Add or remove this build from the local app menu.",
        "emblem-system-symbolic",
        move || {
            let installed = is_installed_locally();
            let result = if installed {
                uninstall_locally()
            } else {
                install_locally()
            };

            match result {
                Ok(()) => refresh_state.rebuild(),
                Err(err) => {
                    log_error(format!("Failed to update local app menu entry: {err}"));
                    overlay.add_toast(Toast::new("Couldn't update the app menu."));
                }
            }
        },
    );
}

#[cfg(not(feature = "setup"))]
const fn append_optional_setup_row(_state: &ToolsPageState) {}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn append_optional_flatpak_override_row(state: &ToolsPageState) {
    if has_host_permission() {
        return;
    }

    let overlay = state.overlay.clone();
    append_action_row_with_button(
        &state.list,
        "Enable Flatpak host access",
        "Copy the override command needed for Flatpak host integration.",
        "edit-copy-symbolic",
        move || {
            if set_clipboard_text(FLATPAK_HOST_OVERRIDE_COMMAND, &overlay, None) {
                overlay.add_toast(Toast::new("Copied."));
            }
        },
    );
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
const fn append_optional_flatpak_override_row(_state: &ToolsPageState) {}

fn append_optional_pass_import_row(state: &ToolsPageState) {
    let settings = Preferences::new();
    schedule_store_import_row(&state.list, &settings, &state.window, &state.overlay);
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

fn collect_locked_tool_fingerprints(
    requests: &[FieldValueRequest],
    read_mode: ToolReadMode,
) -> Vec<String> {
    let mut fingerprints = Vec::new();
    for request in requests {
        let read_result = match read_mode {
            ToolReadMode::PasswordContents => {
                read_password_entry(&request.root, &request.label).map(|_| ())
            }
            ToolReadMode::PasswordLine => {
                read_password_line(&request.root, &request.label).map(|_| ())
            }
        };

        if !matches!(read_result, Err(PasswordEntryError::LockedPrivateKey(_))) {
            continue;
        }

        let Ok(fingerprint) =
            preferred_ripasso_private_key_fingerprint_for_entry(&request.root, &request.label)
        else {
            continue;
        };
        if !fingerprints.iter().any(|existing| existing == &fingerprint) {
            fingerprints.push(fingerprint);
        }
    }

    fingerprints
}

fn prompt_tool_unlock_sequence(
    overlay: &ToastOverlay,
    fingerprints: Vec<String>,
    on_finish: Rc<dyn Fn(bool)>,
) {
    if fingerprints.is_empty() {
        on_finish(true);
        return;
    }

    prompt_tool_unlock_at_index(overlay.clone(), Rc::new(fingerprints), 0, on_finish);
}

fn prompt_tool_unlock_at_index(
    overlay: ToastOverlay,
    fingerprints: Rc<Vec<String>>,
    index: usize,
    on_finish: Rc<dyn Fn(bool)>,
) {
    let Some(fingerprint) = fingerprints.get(index).cloned() else {
        on_finish(true);
        return;
    };

    let overlay_for_next = overlay.clone();
    let fingerprints_for_next = fingerprints.clone();
    let on_finish_for_next = on_finish.clone();
    let on_finish_for_result = on_finish.clone();
    prompt_private_key_unlock_for_action(
        &overlay,
        fingerprint,
        Rc::new(move || {
            prompt_tool_unlock_at_index(
                overlay_for_next.clone(),
                fingerprints_for_next.clone(),
                index + 1,
                on_finish_for_next.clone(),
            );
        }),
        Rc::new(move |success| {
            if !success {
                on_finish_for_result(false);
            }
        }),
    );
}

fn build_weak_password_batch(
    generation: u64,
    requests: Vec<FieldValueRequest>,
) -> WeakPasswordBatch {
    let results = requests
        .into_iter()
        .filter_map(|request| {
            let password = read_password_line(&request.root, &request.label).ok()?;
            let reason = weak_password_reason(&password)?;
            Some(WeakPasswordFinding {
                root: request.root,
                label: request.label.to_string(),
                normalized_label: request.label.to_lowercase(),
                normalized_reason: reason.to_lowercase(),
                reason,
            })
        })
        .collect();

    WeakPasswordBatch {
        generation,
        results,
    }
}

fn build_field_value_catalog_batch(
    generation: u64,
    requests: Vec<FieldValueRequest>,
) -> FieldValueCatalogBatch {
    let indexed_entries = requests
        .into_iter()
        .filter_map(|request| {
            read_password_entry(&request.root, &request.label)
                .ok()
                .map(|contents| searchable_pass_fields(&contents))
        })
        .collect::<Vec<_>>();

    FieldValueCatalogBatch {
        generation,
        catalog: field_value_catalog_from_entries(indexed_entries),
    }
}

fn field_value_catalog_from_entries(
    indexed_entries: impl IntoIterator<Item = Vec<SearchablePassField>>,
) -> FieldValueCatalog {
    #[derive(Default)]
    struct ValueAccumulator {
        display_value: String,
        match_count: usize,
    }

    let mut values_by_field: BTreeMap<String, BTreeMap<String, ValueAccumulator>> = BTreeMap::new();
    for entry_fields in indexed_entries {
        let mut entry_values: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for field in entry_fields {
            if field.key == "otpauth" {
                continue;
            }

            entry_values
                .entry(field.key)
                .or_default()
                .entry(field.normalized_value)
                .or_insert(field.value);
        }

        for (field_key, entry_unique_values) in entry_values {
            let field_values = values_by_field.entry(field_key).or_default();
            for (normalized_value, display_value) in entry_unique_values {
                let value = field_values.entry(normalized_value).or_default();
                if value.display_value.is_empty() {
                    value.display_value = display_value;
                }
                value.match_count += 1;
            }
        }
    }

    let fields = values_by_field
        .iter()
        .map(|(key, values)| FieldCatalogEntry {
            key: key.clone(),
            unique_value_count: values.len(),
        })
        .collect::<Vec<_>>();

    let values_by_field = values_by_field
        .into_iter()
        .map(|(key, values)| {
            let values = values
                .into_iter()
                .map(|(normalized_value, value)| ValueCatalogEntry {
                    display_value: value.display_value,
                    normalized_value,
                    match_count: value.match_count,
                })
                .collect::<Vec<_>>();
            (key, values)
        })
        .collect();

    FieldValueCatalog {
        fields,
        values_by_field,
    }
}

fn format_exact_field_query(field: &str, value: &str) -> String {
    format!(
        "find \"{}\" is \"{}\"",
        escape_quoted_search_component(field),
        escape_quoted_search_component(value)
    )
}

fn escape_quoted_search_component(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn next_generation(current: u64) -> u64 {
    current.wrapping_add(1).max(1)
}

fn unique_values_subtitle(count: usize) -> String {
    if count == 1 {
        "1 unique value".to_string()
    } else {
        format!("{count} unique values")
    }
}

fn matching_items_subtitle(count: usize) -> String {
    if count == 1 {
        "1 matching item".to_string()
    } else {
        format!("{count} matching items")
    }
}

fn append_loading_rows(list: &ListBox, title: &str, subtitle: &str) {
    append_info_row(list, title, subtitle);
    append_spinner_row(list);
}

fn tool_rows_enabled(field_values_busy: bool, weak_passwords_busy: bool) -> bool {
    !(field_values_busy || weak_passwords_busy)
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

#[cfg(test)]
mod tests {
    use super::{
        field_value_catalog_from_entries, format_exact_field_query, matching_items_subtitle,
        tool_rows_enabled, unique_values_subtitle, FieldCatalogEntry, ValueCatalogEntry,
    };
    use crate::password::file::SearchablePassField;
    use std::collections::BTreeMap;

    fn field(key: &str, value: &str) -> SearchablePassField {
        SearchablePassField {
            key: key.to_string(),
            value: value.to_string(),
            normalized_value: value.to_lowercase(),
        }
    }

    #[test]
    fn tool_catalog_excludes_otpauth_and_deduplicates_case_insensitively() {
        let catalog = field_value_catalog_from_entries([
            vec![
                field("username", "Alice"),
                field("username", "ALICE"),
                field("url", "GitLab"),
                field("otpauth", "otpauth://totp/example"),
            ],
            vec![
                field("username", "alice"),
                field("url", "GitHub"),
                field("email", "alice@example.com"),
            ],
        ]);

        assert_eq!(
            catalog.fields,
            vec![
                FieldCatalogEntry {
                    key: "email".to_string(),
                    unique_value_count: 1,
                },
                FieldCatalogEntry {
                    key: "url".to_string(),
                    unique_value_count: 2,
                },
                FieldCatalogEntry {
                    key: "username".to_string(),
                    unique_value_count: 1,
                },
            ]
        );
        assert_eq!(catalog.values_by_field.get("otpauth"), None);
        assert_eq!(
            catalog.values_by_field.get("username"),
            Some(&vec![ValueCatalogEntry {
                display_value: "Alice".to_string(),
                normalized_value: "alice".to_string(),
                match_count: 2,
            }])
        );
    }

    #[test]
    fn tool_catalog_counts_matching_entries_per_value() {
        let catalog = field_value_catalog_from_entries([
            vec![
                field("email", "alice@example.com"),
                field("email", "alice@example.com"),
            ],
            vec![field("email", "ALICE@EXAMPLE.COM")],
            vec![field("email", "bob@example.com")],
        ]);

        assert_eq!(
            catalog.values_by_field,
            BTreeMap::from([(
                "email".to_string(),
                vec![
                    ValueCatalogEntry {
                        display_value: "alice@example.com".to_string(),
                        normalized_value: "alice@example.com".to_string(),
                        match_count: 2,
                    },
                    ValueCatalogEntry {
                        display_value: "bob@example.com".to_string(),
                        normalized_value: "bob@example.com".to_string(),
                        match_count: 1,
                    },
                ],
            )])
        );
    }

    #[test]
    fn exact_field_queries_escape_quotes_and_backslashes() {
        assert_eq!(
            format_exact_field_query(r#"security "question""#, r#"first\pet "name""#),
            r#"find "security \"question\"" is "first\\pet \"name\"""#
        );
    }

    #[test]
    fn count_subtitles_pluralize() {
        assert_eq!(unique_values_subtitle(1), "1 unique value");
        assert_eq!(unique_values_subtitle(2), "2 unique values");
        assert_eq!(matching_items_subtitle(1), "1 matching item");
        assert_eq!(matching_items_subtitle(3), "3 matching items");
    }

    #[test]
    fn tool_rows_disable_while_any_tool_is_busy() {
        assert!(tool_rows_enabled(false, false));
        assert!(!tool_rows_enabled(true, false));
        assert!(!tool_rows_enabled(false, true));
        assert!(!tool_rows_enabled(true, true));
    }
}
