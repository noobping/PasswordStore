use super::query::{OTP_SEARCH_KEY, WEAK_PASSWORD_SEARCH_KEY};
use super::{SearchRowFieldIndexState, SEARCH_FIELDS_KEY};
use crate::backend::read_password_entry;
use crate::password::file::{pass_file_has_otp, searchable_pass_fields, SearchablePassField};
use crate::password::strength::weak_password_reason;
use crate::store::support::StoreSupportCache;
use crate::support::object_data::{cloned_data, non_null_to_string_option};
use adw::gtk::{ListBox, ListBoxRow};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SearchIndexRequest {
    pub(super) root: String,
    pub(super) label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SearchIndexResult {
    pub(super) root: String,
    pub(super) label: String,
    pub(super) state: SearchRowFieldIndexState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SearchIndexBatch {
    pub(super) generation: u64,
    pub(super) results: Vec<SearchIndexResult>,
}

pub(super) fn build_search_index_batch(
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
                    SearchRowFieldIndexState::Indexed(indexed_fields_for_contents(&contents))
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

pub(super) fn collect_unindexed_requests(list: &ListBox) -> Vec<SearchIndexRequest> {
    let mut requests = Vec::new();
    let mut store_support = StoreSupportCache::default();
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
        if !store_support.supports_password_read_tools(&root) {
            return;
        }
        let Some(label) = non_null_to_string_option(&row, "label") else {
            return;
        };
        requests.push(SearchIndexRequest { root, label });
    });
    requests
}

pub(super) fn row_field_index_state(row: &ListBoxRow) -> SearchRowFieldIndexState {
    cloned_data(row, SEARCH_FIELDS_KEY).unwrap_or(SearchRowFieldIndexState::Unindexed)
}

pub(super) fn find_row(list: &ListBox, root: &str, label: &str) -> Option<ListBoxRow> {
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

pub(super) fn list_is_empty(list: &ListBox) -> bool {
    list.row_at_index(0).is_none()
}

pub(super) fn is_stale_index_batch(current_generation: u64, batch_generation: u64) -> bool {
    batch_generation != current_generation
}

pub(super) fn indexed_fields_for_contents(contents: &str) -> Vec<SearchablePassField> {
    let mut fields = searchable_pass_fields(contents);
    if pass_file_has_otp(contents) {
        fields.push(SearchablePassField {
            key: OTP_SEARCH_KEY.to_string(),
            value: "true".to_string(),
            normalized_value: "true".to_string(),
        });
    }
    if let Some(reason) = weak_password_reason(contents.lines().next().unwrap_or_default()) {
        fields.push(SearchablePassField {
            key: WEAK_PASSWORD_SEARCH_KEY.to_string(),
            value: reason.clone(),
            normalized_value: reason.to_lowercase(),
        });
    }

    fields
}

fn for_each_row(list: &ListBox, mut f: impl FnMut(ListBoxRow)) {
    let mut index = 0;
    while let Some(row) = list.row_at_index(index) {
        f(row);
        index += 1;
    }
}
