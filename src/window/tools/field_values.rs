use super::{
    append_loading_rows, collect_loaded_entry_requests, next_generation, FieldValueRequest,
    ToolReadMode, ToolsPageState, FIELD_VALUES_EMPTY_SUBTITLE, FIELD_VALUES_EMPTY_TITLE,
    FIELD_VALUES_FIELDS_SUBTITLE, FIELD_VALUES_FILTER_EMPTY_SUBTITLE,
    FIELD_VALUES_FILTER_EMPTY_TITLE, FIELD_VALUES_LOADING_SUBTITLE, FIELD_VALUES_LOADING_TITLE,
    FIELD_VALUES_TITLE, FIELD_VALUES_VALUES_SUBTITLE, VALUE_VALUES_EMPTY_SUBTITLE,
    VALUE_VALUES_EMPTY_TITLE, VALUE_VALUES_FILTER_EMPTY_SUBTITLE, VALUE_VALUES_FILTER_EMPTY_TITLE,
};
use crate::backend::read_password_entry;
use crate::i18n::gettext;
use crate::password::file::{searchable_pass_fields, SearchablePassField};
use crate::password::opened::clear_opened_pass_file;
use crate::preferences::Preferences;
use crate::support::background::spawn_result_task;
use crate::support::ui::{
    append_action_row_with_button, append_info_row, clear_list_box, pop_navigation_to_root,
    reveal_navigation_page,
};
use crate::window::navigation::{
    show_primary_page_chrome, show_secondary_page_chrome, HasWindowChrome,
};
use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Default)]
pub(super) struct FieldValueBrowserState {
    pub(super) generation: Cell<u64>,
    pub(super) in_flight: Cell<bool>,
    pub(super) tool_busy: Cell<bool>,
    pub(super) catalog: RefCell<Option<FieldValueCatalog>>,
    pub(super) selected_field: RefCell<Option<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct FieldValueCatalog {
    pub(super) fields: Vec<FieldCatalogEntry>,
    pub(super) values_by_field: BTreeMap<String, Vec<ValueCatalogEntry>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FieldCatalogEntry {
    pub(super) key: String,
    pub(super) unique_value_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ValueCatalogEntry {
    pub(super) display_value: String,
    pub(super) normalized_value: String,
    pub(super) match_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FieldValueCatalogBatch {
    generation: u64,
    catalog: FieldValueCatalog,
}

impl ToolsPageState {
    pub(super) fn prepare_field_values_browser(&self) {
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

    pub(super) fn render_field_list(&self) {
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

    pub(super) fn render_value_list(&self) {
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

    fn apply_root_search(&self, query: &str) {
        self.reset_browser_state();
        pop_navigation_to_root(&self.navigation.nav);
        clear_opened_pass_file(&self.navigation.nav);

        let has_store_dirs = !Preferences::new().stores().is_empty();
        let chrome = self.navigation.window_chrome();
        show_primary_page_chrome(&chrome, has_store_dirs);

        self.root_search_entry.set_visible(true);
        self.root_search_entry.set_text(query);
        self.root_list.invalidate_filter();
        self.root_search_entry.grab_focus();
    }

    pub(super) fn clear_browser_state(&self) {
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

    pub(super) fn reset_browser_state(&self) {
        self.clear_browser_state();
        self.set_field_values_tool_busy(false);
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

pub(super) fn field_value_catalog_from_entries(
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

pub(super) fn format_exact_field_query(field: &str, value: &str) -> String {
    format!(
        "find \"{}\" is \"{}\"",
        escape_quoted_search_component(field),
        escape_quoted_search_component(value)
    )
}

fn escape_quoted_search_component(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(super) fn unique_values_subtitle(count: usize) -> String {
    let template = if count == 1 {
        gettext("{count} unique value")
    } else {
        gettext("{count} unique values")
    };
    template.replace("{count}", &count.to_string())
}

pub(super) fn matching_items_subtitle(count: usize) -> String {
    let template = if count == 1 {
        gettext("{count} matching item")
    } else {
        gettext("{count} matching items")
    };
    template.replace("{count}", &count.to_string())
}
