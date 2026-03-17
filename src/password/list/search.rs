mod index;
mod query;
#[cfg(test)]
mod tests;

use self::index::{
    build_search_index_batch, collect_unindexed_requests, find_row, is_stale_index_batch,
    list_is_empty, row_field_index_state, SearchIndexBatch,
};
use self::query::{parse_search_query, row_matches_query, SearchQuery};
use super::placeholder::{loading_placeholder, resolved_placeholder};
use crate::password::file::SearchablePassField;
use crate::support::background::spawn_result_task;
use crate::support::object_data::{cloned_data, non_null_to_string_option, set_cloned_data};
use adw::gtk::{ListBox, ListBoxRow};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const SEARCH_CONTROLLER_KEY: &str = "search-controller";
pub(super) const SEARCH_FIELDS_KEY: &str = "search-fields";

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

    pub(super) fn matches_row(&self, row: &ListBoxRow) -> bool {
        let query = self.state.query.borrow().clone();
        let label = non_null_to_string_option(row, "label").unwrap_or_default();
        let fields = row_field_index_state(row);
        row_matches_query(&label, &fields, &query)
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
        self.start_indexing_if_needed(list);
        self.update_placeholder(list);
        list.invalidate_filter();
    }

    pub(super) fn finish_reload_failure(&self, list: &ListBox) {
        self.state.loading.set(false);
        self.update_placeholder(list);
        list.invalidate_filter();
    }

    pub(super) fn start_indexing_if_needed(&self, list: &ListBox) {
        if !self.state.query.borrow().is_structured() {
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
            list.set_placeholder(Some(&loading_placeholder()));
            return;
        }

        list.set_placeholder(Some(&resolved_placeholder(
            list_is_empty(list),
            self.state.has_store_dirs.get(),
        )));
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

        self.update_placeholder(list);
        list.invalidate_filter();
    }

    fn handle_index_disconnect(&self, list: &ListBox, generation: u64) {
        if is_stale_index_batch(self.state.generation.get(), generation) {
            return;
        }

        if self.state.indexing_generation.get() == Some(generation) {
            self.state.indexing_generation.set(None);
        }

        self.update_placeholder(list);
        list.invalidate_filter();
    }

    fn should_show_loading_placeholder(&self, list: &ListBox) -> bool {
        self.state.loading.get()
            || (self.state.query.borrow().is_structured()
                && self.state.indexing_generation.get() == Some(self.state.generation.get())
                && !list_is_empty(list))
    }
}

pub(super) fn search_controller_for_list(list: &ListBox) -> Option<SearchFilterController> {
    cloned_data(list, SEARCH_CONTROLLER_KEY)
}
