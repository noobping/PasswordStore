use super::placeholder::{loading_placeholder, resolved_placeholder};
use crate::backend::read_password_entry;
use crate::password::file::{
    canonical_search_field_key, searchable_pass_fields, SearchablePassField,
};
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
enum SearchQuery {
    Empty,
    Plain(String),
    Structured(StructuredSearchQuery),
    InvalidStructured,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StructuredSearchQuery {
    field: String,
    value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SearchRowFieldIndexState {
    Unindexed,
    Unavailable,
    Indexed(Vec<SearchablePassField>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SearchIndexRequest {
    root: String,
    label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SearchIndexResult {
    root: String,
    label: String,
    state: SearchRowFieldIndexState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SearchIndexBatch {
    generation: u64,
    results: Vec<SearchIndexResult>,
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
        if !matches!(&*self.state.query.borrow(), SearchQuery::Structured(_)) {
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
            || (matches!(*self.state.query.borrow(), SearchQuery::Structured(_))
                && self.state.indexing_generation.get() == Some(self.state.generation.get())
                && !list_is_empty(list))
    }
}

pub(super) fn search_controller_for_list(list: &ListBox) -> Option<SearchFilterController> {
    cloned_data(list, SEARCH_CONTROLLER_KEY)
}

fn parse_search_query(query: &str) -> SearchQuery {
    if query.is_empty() {
        return SearchQuery::Empty;
    }

    if !query
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("find:"))
    {
        return SearchQuery::Plain(query.to_lowercase());
    }

    let Some(remainder) = query.get(5..) else {
        return SearchQuery::InvalidStructured;
    };
    let Some((field, value)) = remainder.split_once('=') else {
        return SearchQuery::InvalidStructured;
    };
    let Some(field) = canonical_search_field_key(field) else {
        return SearchQuery::InvalidStructured;
    };
    let value = value.trim().to_lowercase();
    if value.is_empty() {
        return SearchQuery::InvalidStructured;
    }

    SearchQuery::Structured(StructuredSearchQuery { field, value })
}

fn row_matches_query(label: &str, fields: &SearchRowFieldIndexState, query: &SearchQuery) -> bool {
    match query {
        SearchQuery::Empty => true,
        SearchQuery::Plain(query) => label.to_lowercase().contains(query),
        SearchQuery::Structured(query) => match fields {
            SearchRowFieldIndexState::Indexed(fields) => structured_query_matches(fields, query),
            SearchRowFieldIndexState::Unindexed | SearchRowFieldIndexState::Unavailable => false,
        },
        SearchQuery::InvalidStructured => false,
    }
}

fn structured_query_matches(fields: &[SearchablePassField], query: &StructuredSearchQuery) -> bool {
    fields
        .iter()
        .any(|field| field.key == query.field && field.value.contains(&query.value))
}

fn build_search_index_batch(
    generation: u64,
    requests: Vec<SearchIndexRequest>,
) -> SearchIndexBatch {
    let results = requests
        .into_iter()
        .map(|request| SearchIndexResult {
            root: request.root.clone(),
            label: request.label.clone(),
            state: match read_password_entry(&request.root, &request.label) {
                Ok(contents) => {
                    SearchRowFieldIndexState::Indexed(searchable_pass_fields(&contents))
                }
                Err(_) => SearchRowFieldIndexState::Unavailable,
            },
        })
        .collect();

    SearchIndexBatch {
        generation,
        results,
    }
}

fn collect_unindexed_requests(list: &ListBox) -> Vec<SearchIndexRequest> {
    let mut requests = Vec::new();
    for_each_row(list, |row| {
        if !matches!(
            row_field_index_state(&row),
            SearchRowFieldIndexState::Unindexed
        ) {
            return;
        }

        let Some(root) = non_null_to_string_option(&row, "root") else {
            return;
        };
        let Some(label) = non_null_to_string_option(&row, "label") else {
            return;
        };
        requests.push(SearchIndexRequest { root, label });
    });
    requests
}

fn row_field_index_state(row: &ListBoxRow) -> SearchRowFieldIndexState {
    cloned_data(row, SEARCH_FIELDS_KEY).unwrap_or(SearchRowFieldIndexState::Unindexed)
}

fn find_row(list: &ListBox, root: &str, label: &str) -> Option<ListBoxRow> {
    let mut found = None;
    for_each_row(list, |row| {
        if found.is_some() {
            return;
        }

        let matches_root = non_null_to_string_option(&row, "root").as_deref() == Some(root);
        let matches_label = non_null_to_string_option(&row, "label").as_deref() == Some(label);
        if matches_root && matches_label {
            found = Some(row);
        }
    });
    found
}

fn list_is_empty(list: &ListBox) -> bool {
    list.row_at_index(0).is_none()
}

fn for_each_row(list: &ListBox, mut f: impl FnMut(ListBoxRow)) {
    let mut index = 0;
    while let Some(row) = list.row_at_index(index) {
        f(row);
        index += 1;
    }
}

fn is_stale_index_batch(current_generation: u64, batch_generation: u64) -> bool {
    batch_generation != current_generation
}

#[cfg(test)]
mod tests {
    use super::{
        is_stale_index_batch, parse_search_query, row_matches_query, SearchQuery,
        SearchRowFieldIndexState, StructuredSearchQuery,
    };
    use crate::password::file::SearchablePassField;

    fn indexed_fields(entries: &[(&str, &str)]) -> SearchRowFieldIndexState {
        SearchRowFieldIndexState::Indexed(
            entries
                .iter()
                .map(|(key, value)| SearchablePassField {
                    key: (*key).to_string(),
                    value: (*value).to_string(),
                })
                .collect(),
        )
    }

    #[test]
    fn plain_queries_stay_plain() {
        assert_eq!(
            parse_search_query("alice/github"),
            SearchQuery::Plain("alice/github".to_string())
        );
    }

    #[test]
    fn structured_queries_parse_with_case_insensitive_prefix() {
        assert_eq!(
            parse_search_query("FiNd:username=noobping"),
            SearchQuery::Structured(StructuredSearchQuery {
                field: "username".to_string(),
                value: "noobping".to_string(),
            })
        );
    }

    #[test]
    fn structured_queries_trim_key_and_value() {
        assert_eq!(
            parse_search_query("find: user = NoobPing "),
            SearchQuery::Structured(StructuredSearchQuery {
                field: "username".to_string(),
                value: "noobping".to_string(),
            })
        );
    }

    #[test]
    fn structured_queries_keep_additional_equals_in_the_value() {
        assert_eq!(
            parse_search_query("find:url=https://example.com?a=b=c"),
            SearchQuery::Structured(StructuredSearchQuery {
                field: "url".to_string(),
                value: "https://example.com?a=b=c".to_string(),
            })
        );
    }

    #[test]
    fn malformed_find_queries_do_not_fall_back_to_plain_search() {
        assert_eq!(
            parse_search_query("find:username"),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query("find:=noobping"),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query("find:username="),
            SearchQuery::InvalidStructured
        );
    }

    #[test]
    fn plain_label_search_matches_only_the_label() {
        assert!(row_matches_query(
            "work/noobping/github",
            &SearchRowFieldIndexState::Unavailable,
            &SearchQuery::Plain("github".to_string()),
        ));
        assert!(!row_matches_query(
            "work/noobping/github",
            &indexed_fields(&[("username", "alice")]),
            &SearchQuery::Plain("alice".to_string()),
        ));
    }

    #[test]
    fn structured_queries_match_indexed_fields_with_case_insensitive_contains() {
        assert!(row_matches_query(
            "work/noobping/github",
            &indexed_fields(&[("username", "noobping"), ("url", "https://example.com")]),
            &SearchQuery::Structured(StructuredSearchQuery {
                field: "username".to_string(),
                value: "noob".to_string(),
            }),
        ));
    }

    #[test]
    fn unreadable_rows_do_not_match_structured_queries() {
        let query = SearchQuery::Structured(StructuredSearchQuery {
            field: "username".to_string(),
            value: "noobping".to_string(),
        });
        assert!(!row_matches_query(
            "work/noobping/github",
            &SearchRowFieldIndexState::Unindexed,
            &query,
        ));
        assert!(!row_matches_query(
            "work/noobping/github",
            &SearchRowFieldIndexState::Unavailable,
            &query,
        ));
    }

    #[test]
    fn generation_checks_mark_mismatched_batches_as_stale() {
        assert!(is_stale_index_batch(2, 1));
        assert!(!is_stale_index_batch(2, 2));
    }
}
