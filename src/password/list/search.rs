mod index;
mod query;
#[cfg(test)]
mod tests;

#[cfg(target_os = "linux")]
use self::index::indexed_fields_for_contents;
use self::index::{
    build_search_index_batch, collect_unindexed_requests, find_row, is_stale_index_batch,
    list_is_empty, row_field_index_state, SearchIndexBatch,
};
use self::query::{parse_search_query, row_matches_query, SearchQuery};
use super::placeholder::{show_loading_placeholder, show_resolved_placeholder};
use super::{
    password_list_folder_row_is_expanded, password_list_row_depth, password_list_row_is_folder,
    password_list_row_store_path,
};
#[cfg(target_os = "linux")]
use crate::backend::{password_entry_is_readable, read_password_entry};
use crate::password::file::SearchablePassField;
#[cfg(target_os = "linux")]
use crate::password::model::{
    collect_all_password_items_with_options, CollectItemsOptions, PassEntry,
};
#[cfg(target_os = "linux")]
use crate::store::labels::shortened_store_label_map;
use crate::store::recipients::store_uses_fido2_recipients;
use crate::support::background::spawn_result_task;
use crate::support::object_data::{cloned_data, non_null_to_string_option, set_cloned_data};
use adw::gtk::{ListBox, ListBoxRow};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

const SEARCH_CONTROLLER_KEY: &str = "search-controller";
pub(super) const SEARCH_FIELDS_KEY: &str = "search-fields";
const SEARCH_VISIBILITY_KEY: &str = "search-visibility";

#[derive(Clone, Debug, PartialEq, Eq)]
enum FilterablePasswordListRow {
    Folder {
        store_path: String,
        depth: usize,
        expanded: bool,
    },
    Entry {
        store_path: String,
        depth: usize,
        matches_query: bool,
    },
}

#[derive(Clone)]
pub(super) struct SearchFilterController {
    state: Rc<SearchFilterState>,
}

struct SearchFilterState {
    query: RefCell<SearchQuery>,
    generation: Cell<u64>,
    indexing_generation: Cell<Option<u64>>,
    has_store_dirs: Cell<bool>,
    loading: Cell<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SearchRowFieldIndexState {
    Unindexed,
    Unavailable,
    Indexed(Vec<SearchablePassField>),
}

impl SearchFilterController {
    pub(super) fn new() -> Self {
        Self {
            state: Rc::new(SearchFilterState {
                query: RefCell::new(SearchQuery::Empty),
                generation: Cell::new(0),
                indexing_generation: Cell::new(None),
                has_store_dirs: Cell::new(false),
                loading: Cell::new(false),
            }),
        }
    }

    pub(super) fn register_for_list(&self, list: &ListBox) {
        set_cloned_data(list, SEARCH_CONTROLLER_KEY, self.clone());
    }

    pub(super) fn update_query(&self, query: &str) {
        *self.state.query.borrow_mut() = parse_search_query(query);
    }

    pub(super) fn refresh_row_visibility(&self, list: &ListBox) {
        let query = self.state.query.borrow().clone();
        let query_is_empty = query.is_empty();
        let rows = collect_filterable_rows(list, &query);
        let visibility = password_list_row_visibility(&rows, query_is_empty);

        for (row, visible) in visibility {
            set_cloned_data(&row, SEARCH_VISIBILITY_KEY, visible);
        }
    }

    pub(super) fn matches_row(&self, row: &ListBoxRow) -> bool {
        cloned_data(row, SEARCH_VISIBILITY_KEY).unwrap_or(true)
    }

    pub(super) fn begin_reload(&self, has_store_dirs: bool) {
        self.state.has_store_dirs.set(has_store_dirs);
        self.state
            .generation
            .set(self.state.generation.get().wrapping_add(1).max(1));
        self.state.indexing_generation.set(None);
        self.state.loading.set(true);
    }

    pub(super) fn finish_reload(&self, list: &ListBox) {
        self.state.loading.set(false);
        self.refresh_row_visibility(list);
        self.start_indexing_if_needed(list);
        list.invalidate_filter();
        self.update_placeholder(list);
    }

    pub(super) fn finish_reload_failure(&self, list: &ListBox) {
        self.state.loading.set(false);
        list.invalidate_filter();
        self.update_placeholder(list);
    }

    pub(super) fn start_indexing_if_needed(&self, list: &ListBox) {
        if !self.state.query.borrow().requires_index() {
            return;
        }

        let generation = self.state.generation.get();
        if self.state.indexing_generation.get() == Some(generation) {
            return;
        }

        let requests = collect_unindexed_requests(list);
        if requests.is_empty() {
            return;
        }

        self.state.indexing_generation.set(Some(generation));
        let controller_for_result = self.clone();
        let list_for_result = list.clone();
        let controller_for_disconnect = self.clone();
        let list_for_disconnect = list.clone();
        spawn_result_task(
            move || build_search_index_batch(generation, requests),
            move |batch| controller_for_result.apply_index_batch(&list_for_result, batch),
            move || {
                controller_for_disconnect.handle_index_disconnect(&list_for_disconnect, generation);
            },
        );
    }

    pub(super) fn update_placeholder(&self, list: &ListBox) {
        if self.should_show_loading_placeholder(list) {
            show_loading_placeholder(list);
            return;
        }

        show_resolved_placeholder(list, list_is_empty(list), self.state.has_store_dirs.get());
    }

    fn apply_index_batch(&self, list: &ListBox, batch: SearchIndexBatch) {
        if is_stale_index_batch(self.state.generation.get(), batch.generation) {
            return;
        }

        if self.state.indexing_generation.get() == Some(batch.generation) {
            self.state.indexing_generation.set(None);
        }

        for result in batch.results {
            if let Some(row) = find_row(list, &result.root, &result.label) {
                set_cloned_data(&row, SEARCH_FIELDS_KEY, result.state);
            }
        }

        self.refresh_row_visibility(list);
        list.invalidate_filter();
        self.update_placeholder(list);
    }

    fn handle_index_disconnect(&self, list: &ListBox, generation: u64) {
        if is_stale_index_batch(self.state.generation.get(), generation) {
            return;
        }

        if self.state.indexing_generation.get() == Some(generation) {
            self.state.indexing_generation.set(None);
        }

        list.invalidate_filter();
        self.update_placeholder(list);
    }

    fn should_show_loading_placeholder(&self, list: &ListBox) -> bool {
        self.state.loading.get()
            || (self.state.query.borrow().requires_index()
                && self.state.indexing_generation.get() == Some(self.state.generation.get())
                && !list_is_empty(list))
    }
}

pub(super) fn search_controller_for_list(list: &ListBox) -> Option<SearchFilterController> {
    cloned_data(list, SEARCH_CONTROLLER_KEY)
}

fn collect_filterable_rows(
    list: &ListBox,
    query: &SearchQuery,
) -> Vec<(ListBoxRow, FilterablePasswordListRow)> {
    let mut rows = Vec::new();
    let mut advanced_search_compatible_stores = HashMap::<String, bool>::new();
    let uses_advanced_features = query.uses_advanced_features();
    for_each_row(list, |row| {
        let Some(store_path) = password_list_row_store_path(&row) else {
            set_cloned_data(&row, SEARCH_VISIBILITY_KEY, true);
            return;
        };
        let depth = password_list_row_depth(&row);

        if password_list_row_is_folder(&row) {
            rows.push((
                row.clone(),
                FilterablePasswordListRow::Folder {
                    store_path,
                    depth,
                    expanded: password_list_folder_row_is_expanded(&row),
                },
            ));
            return;
        }

        rows.push((
            row.clone(),
            FilterablePasswordListRow::Entry {
                matches_query: store_supports_advanced_search(
                    &store_path,
                    uses_advanced_features,
                    &mut advanced_search_compatible_stores,
                ) && password_entry_matches_query(&row, query),
                depth,
                store_path,
            },
        ));
    });
    rows
}

fn store_supports_advanced_search(
    store_path: &str,
    uses_advanced_features: bool,
    cache: &mut HashMap<String, bool>,
) -> bool {
    if !uses_advanced_features {
        return true;
    }

    if let Some(supported) = cache.get(store_path) {
        return *supported;
    }

    let supported = advanced_search_includes_store(
        uses_advanced_features,
        store_uses_fido2_recipients(store_path),
    );
    cache.insert(store_path.to_string(), supported);
    supported
}

const fn advanced_search_includes_store(
    uses_advanced_features: bool,
    store_uses_fido2: bool,
) -> bool {
    !uses_advanced_features || !store_uses_fido2
}

fn password_entry_matches_query(row: &ListBoxRow, query: &SearchQuery) -> bool {
    let label = non_null_to_string_option(row, "label").unwrap_or_default();
    let store_label = non_null_to_string_option(row, "store-label").unwrap_or_default();
    let store_path = non_null_to_string_option(row, "root").unwrap_or_default();
    let fields = row_field_index_state(row);
    row_matches_query(&label, &store_label, &store_path, &fields, query)
}

fn password_list_row_visibility(
    rows: &[(ListBoxRow, FilterablePasswordListRow)],
    query_is_empty: bool,
) -> Vec<(ListBoxRow, bool)> {
    let states = rows.iter().map(|(_, row)| row.clone()).collect::<Vec<_>>();
    let visibility = if query_is_empty {
        password_list_collapsed_visibility(&states)
    } else {
        combine_password_list_visibility(
            password_list_collapsed_visibility(&states),
            password_list_search_visibility(&states),
        )
    };

    rows.iter()
        .zip(visibility)
        .map(|((row, _), visible)| (row.clone(), visible))
        .collect()
}

fn combine_password_list_visibility(left: Vec<bool>, right: Vec<bool>) -> Vec<bool> {
    left.into_iter()
        .zip(right)
        .map(|(left, right)| left && right)
        .collect()
}

fn password_list_collapsed_visibility(rows: &[FilterablePasswordListRow]) -> Vec<bool> {
    let mut visibility = Vec::with_capacity(rows.len());
    let mut current_store = None::<&str>;
    let mut expansion_stack = Vec::<bool>::new();

    for row in rows {
        let store_path = row.store_path();
        if current_store != Some(store_path) {
            current_store = Some(store_path);
            expansion_stack.clear();
        }

        let depth = row.depth();
        expansion_stack.truncate(depth);
        let ancestors_expanded = expansion_stack.iter().all(|expanded| *expanded);
        visibility.push(ancestors_expanded);

        if let FilterablePasswordListRow::Folder { expanded, .. } = row {
            expansion_stack.push(*expanded);
        }
    }

    visibility
}

fn password_list_search_visibility(rows: &[FilterablePasswordListRow]) -> Vec<bool> {
    let mut visibility = vec![false; rows.len()];
    let mut current_store = None::<&str>;
    let mut folder_stack = Vec::<usize>::new();

    for (index, row) in rows.iter().enumerate() {
        let store_path = row.store_path();
        if current_store != Some(store_path) {
            current_store = Some(store_path);
            folder_stack.clear();
        }

        let depth = row.depth();
        folder_stack.truncate(depth);

        match row {
            FilterablePasswordListRow::Folder { .. } => folder_stack.push(index),
            FilterablePasswordListRow::Entry { matches_query, .. } => {
                if *matches_query {
                    visibility[index] = true;
                    for folder_index in &folder_stack {
                        visibility[*folder_index] = true;
                    }
                }
            }
        }
    }

    visibility
}

impl FilterablePasswordListRow {
    fn store_path(&self) -> &str {
        match self {
            Self::Folder { store_path, .. } | Self::Entry { store_path, .. } => store_path,
        }
    }

    const fn depth(&self) -> usize {
        match self {
            Self::Folder { depth, .. } | Self::Entry { depth, .. } => *depth,
        }
    }
}

fn for_each_row(list: &ListBox, mut f: impl FnMut(ListBoxRow)) {
    let mut index = 0;
    while let Some(row) = list.row_at_index(index) {
        f(row);
        index += 1;
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn search_password_entries(query: &str, limit: Option<usize>) -> Vec<PassEntry> {
    let query = parse_search_query(query);
    if matches!(
        query,
        SearchQuery::InvalidRegex | SearchQuery::InvalidStructured
    ) {
        return Vec::new();
    }

    let requires_index = query.requires_index();
    let uses_advanced_features = query.uses_advanced_features();
    let store_labels =
        shortened_store_label_map(&crate::preferences::Preferences::new().store_roots());
    let mut matches = Vec::new();
    let mut advanced_search_compatible_stores = HashMap::<String, bool>::new();
    for item in collect_all_password_items_with_options(CollectItemsOptions::default()) {
        if !store_supports_advanced_search(
            &item.store_path,
            uses_advanced_features,
            &mut advanced_search_compatible_stores,
        ) {
            continue;
        }

        let label = item.label();
        let store_label = store_labels
            .get(&item.store_path)
            .map_or(item.store_path.as_str(), String::as_str);
        let fields = if requires_index {
            match read_password_entry(&item.store_path, &label) {
                Ok(contents) => {
                    SearchRowFieldIndexState::Indexed(indexed_fields_for_contents(&contents))
                }
                Err(_) => SearchRowFieldIndexState::Unavailable,
            }
        } else if password_entry_is_readable(&item.store_path, &label) {
            SearchRowFieldIndexState::Unindexed
        } else {
            SearchRowFieldIndexState::Unavailable
        };

        if !row_matches_query(&label, store_label, &item.store_path, &fields, &query) {
            continue;
        }

        matches.push(item);
        if limit.is_some_and(|limit| matches.len() >= limit) {
            break;
        }
    }

    matches
}

#[cfg(test)]
mod visibility_tests {
    use super::{
        combine_password_list_visibility, password_list_collapsed_visibility,
        password_list_search_visibility, FilterablePasswordListRow,
    };

    #[test]
    fn collapsed_visibility_hides_descendants_of_closed_folders() {
        let rows = vec![
            FilterablePasswordListRow::Folder {
                store_path: "/tmp/personal".to_string(),
                depth: 0,
                expanded: false,
            },
            FilterablePasswordListRow::Entry {
                store_path: "/tmp/personal".to_string(),
                depth: 1,
                matches_query: true,
            },
            FilterablePasswordListRow::Folder {
                store_path: "/tmp/personal".to_string(),
                depth: 1,
                expanded: true,
            },
            FilterablePasswordListRow::Entry {
                store_path: "/tmp/personal".to_string(),
                depth: 2,
                matches_query: true,
            },
            FilterablePasswordListRow::Entry {
                store_path: "/tmp/personal".to_string(),
                depth: 0,
                matches_query: true,
            },
        ];

        assert_eq!(
            password_list_collapsed_visibility(&rows),
            vec![true, false, false, false, true]
        );
    }

    #[test]
    fn search_visibility_shows_matching_entries_and_their_folder_chain() {
        let rows = vec![
            FilterablePasswordListRow::Entry {
                store_path: "/tmp/personal".to_string(),
                depth: 0,
                matches_query: false,
            },
            FilterablePasswordListRow::Folder {
                store_path: "/tmp/personal".to_string(),
                depth: 0,
                expanded: false,
            },
            FilterablePasswordListRow::Folder {
                store_path: "/tmp/personal".to_string(),
                depth: 1,
                expanded: false,
            },
            FilterablePasswordListRow::Entry {
                store_path: "/tmp/personal".to_string(),
                depth: 2,
                matches_query: true,
            },
            FilterablePasswordListRow::Folder {
                store_path: "/tmp/work".to_string(),
                depth: 0,
                expanded: true,
            },
            FilterablePasswordListRow::Entry {
                store_path: "/tmp/work".to_string(),
                depth: 1,
                matches_query: false,
            },
        ];

        assert_eq!(
            password_list_search_visibility(&rows),
            vec![false, true, true, true, false, false]
        );
    }

    #[test]
    fn combined_visibility_keeps_search_results_collapsible() {
        let rows = vec![
            FilterablePasswordListRow::Folder {
                store_path: "/tmp/personal".to_string(),
                depth: 0,
                expanded: false,
            },
            FilterablePasswordListRow::Folder {
                store_path: "/tmp/personal".to_string(),
                depth: 1,
                expanded: true,
            },
            FilterablePasswordListRow::Entry {
                store_path: "/tmp/personal".to_string(),
                depth: 2,
                matches_query: true,
            },
        ];

        assert_eq!(
            combine_password_list_visibility(
                password_list_collapsed_visibility(&rows),
                password_list_search_visibility(&rows),
            ),
            vec![true, false, false]
        );
    }
}
