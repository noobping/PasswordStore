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
enum StructuredSearchQuery {
    Clause(SearchClause),
    And(Box<StructuredSearchQuery>, Box<StructuredSearchQuery>),
    Or(Box<StructuredSearchQuery>, Box<StructuredSearchQuery>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SearchClause {
    field: String,
    comparison: SearchComparison,
    value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchComparison {
    Contains,
    Exact,
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

impl SearchQuery {
    const fn is_structured(&self) -> bool {
        matches!(self, Self::Structured(_))
    }
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
    parse_structured_search_query(remainder)
        .map_or(SearchQuery::InvalidStructured, SearchQuery::Structured)
}

fn parse_structured_search_query(query: &str) -> Option<StructuredSearchQuery> {
    StructuredSearchParser::new(query).parse()
}

struct StructuredSearchParser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> StructuredSearchParser<'a> {
    const fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn parse(mut self) -> Option<StructuredSearchQuery> {
        let query = self.parse_or()?;
        self.skip_whitespace();
        self.is_eof().then_some(query)
    }

    fn parse_or(&mut self) -> Option<StructuredSearchQuery> {
        let mut query = self.parse_and()?;
        loop {
            self.skip_whitespace();
            if !self.consume_symbol("||") && !self.consume_keyword("OR") {
                break;
            }

            let right = self.parse_and()?;
            query = StructuredSearchQuery::Or(Box::new(query), Box::new(right));
        }

        Some(query)
    }

    fn parse_and(&mut self) -> Option<StructuredSearchQuery> {
        let mut query = self.parse_primary()?;
        loop {
            self.skip_whitespace();
            if !self.consume_symbol("&&") && !self.consume_keyword("AND") {
                break;
            }

            let right = self.parse_primary()?;
            query = StructuredSearchQuery::And(Box::new(query), Box::new(right));
        }

        Some(query)
    }

    fn parse_primary(&mut self) -> Option<StructuredSearchQuery> {
        self.skip_whitespace();
        if self.consume_char('(') {
            let query = self.parse_or()?;
            self.skip_whitespace();
            self.consume_char(')').then_some(query)
        } else {
            Some(StructuredSearchQuery::Clause(self.parse_clause()?))
        }
    }

    fn parse_clause(&mut self) -> Option<SearchClause> {
        let field = canonical_search_field_key(&self.parse_field()?)?;
        self.skip_whitespace();
        let comparison = if self.consume_symbol("==") {
            SearchComparison::Exact
        } else if self.consume_symbol("=") {
            SearchComparison::Contains
        } else {
            return None;
        };
        self.skip_whitespace();
        let value = if self.peek_char() == Some('"') {
            self.parse_quoted_value()?
        } else {
            self.parse_unquoted_value()?
        };
        if value.is_empty() {
            return None;
        }

        Some(SearchClause {
            field,
            comparison,
            value: value.to_lowercase(),
        })
    }

    fn parse_field(&mut self) -> Option<String> {
        self.skip_whitespace();
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_whitespace() || matches!(ch, '(' | ')' | '=') {
                break;
            }
            self.advance_char();
        }

        let field = self.input.get(start..self.pos)?.trim();
        (!field.is_empty()).then(|| field.to_string())
    }

    fn parse_quoted_value(&mut self) -> Option<String> {
        if !self.consume_char('"') {
            return None;
        }

        let mut value = String::new();
        loop {
            let ch = self.peek_char()?;
            self.advance_char();
            match ch {
                '"' => return Some(value),
                '\\' => {
                    let escaped = self.peek_char()?;
                    self.advance_char();
                    value.push(escaped);
                }
                _ => value.push(ch),
            }
        }
    }

    fn parse_unquoted_value(&mut self) -> Option<String> {
        let start = self.pos;
        let mut scan = self.pos;
        let mut end = None;
        while scan < self.input.len() {
            if matches!(self.peek_char_at(scan), Some('(' | ')'))
                || self.starts_with_symbol_at(scan, "&&")
                || self.starts_with_symbol_at(scan, "||")
                || self.keyword_starts_at(scan, "AND")
                || self.keyword_starts_at(scan, "OR")
            {
                break;
            }

            let ch = self.peek_char_at(scan)?;
            scan += ch.len_utf8();
            if !ch.is_ascii_whitespace() {
                end = Some(scan);
            }
        }

        let end = end?;
        self.pos = end;
        Some(self.input.get(start..end)?.trim_end().to_string())
    }

    fn skip_whitespace(&mut self) {
        while self.peek_char().is_some_and(|ch| ch.is_ascii_whitespace()) {
            self.advance_char();
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn peek_char(&self) -> Option<char> {
        self.peek_char_at(self.pos)
    }

    fn peek_char_at(&self, pos: usize) -> Option<char> {
        self.input.get(pos..)?.chars().next()
    }

    fn advance_char(&mut self) {
        if let Some(ch) = self.peek_char() {
            self.pos += ch.len_utf8();
        }
    }

    fn consume_char(&mut self, ch: char) -> bool {
        if self.peek_char() == Some(ch) {
            self.advance_char();
            true
        } else {
            false
        }
    }

    fn consume_symbol(&mut self, symbol: &str) -> bool {
        if self.starts_with_symbol_at(self.pos, symbol) {
            self.pos += symbol.len();
            true
        } else {
            false
        }
    }

    fn consume_keyword(&mut self, keyword: &str) -> bool {
        if self.keyword_starts_at(self.pos, keyword) {
            self.pos += keyword.len();
            true
        } else {
            false
        }
    }

    fn starts_with_symbol_at(&self, pos: usize, symbol: &str) -> bool {
        self.input
            .get(pos..)
            .is_some_and(|rest| rest.starts_with(symbol))
    }

    fn keyword_starts_at(&self, pos: usize, keyword: &str) -> bool {
        let Some(candidate) = self.input.get(pos..pos + keyword.len()) else {
            return false;
        };
        if !candidate.eq_ignore_ascii_case(keyword) {
            return false;
        }

        operator_boundary(self.peek_char_before(pos))
            && operator_boundary(self.peek_char_at(pos + keyword.len()))
    }

    fn peek_char_before(&self, pos: usize) -> Option<char> {
        self.input.get(..pos)?.chars().next_back()
    }
}

fn operator_boundary(ch: Option<char>) -> bool {
    matches!(ch, None | Some('(' | ')')) || ch.is_some_and(|ch| ch.is_ascii_whitespace())
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
    match query {
        StructuredSearchQuery::Clause(clause) => fields.iter().any(|field| {
            field.key == clause.field
                && match clause.comparison {
                    SearchComparison::Contains => field.value.contains(&clause.value),
                    SearchComparison::Exact => field.value == clause.value,
                }
        }),
        StructuredSearchQuery::And(left, right) => {
            structured_query_matches(fields, left) && structured_query_matches(fields, right)
        }
        StructuredSearchQuery::Or(left, right) => {
            structured_query_matches(fields, left) || structured_query_matches(fields, right)
        }
    }
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
        is_stale_index_batch, parse_search_query, row_matches_query, SearchClause,
        SearchComparison, SearchQuery, SearchRowFieldIndexState, StructuredSearchQuery,
    };
    use crate::password::file::SearchablePassField;

    fn clause(field: &str, comparison: SearchComparison, value: &str) -> StructuredSearchQuery {
        StructuredSearchQuery::Clause(SearchClause {
            field: field.to_string(),
            comparison,
            value: value.to_string(),
        })
    }

    fn and(left: StructuredSearchQuery, right: StructuredSearchQuery) -> StructuredSearchQuery {
        StructuredSearchQuery::And(Box::new(left), Box::new(right))
    }

    fn or(left: StructuredSearchQuery, right: StructuredSearchQuery) -> StructuredSearchQuery {
        StructuredSearchQuery::Or(Box::new(left), Box::new(right))
    }

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
            SearchQuery::Structured(clause("username", SearchComparison::Contains, "noobping",))
        );
    }

    #[test]
    fn structured_queries_trim_key_and_value() {
        assert_eq!(
            parse_search_query("find: user = NoobPing "),
            SearchQuery::Structured(clause("username", SearchComparison::Contains, "noobping",))
        );
    }

    #[test]
    fn structured_queries_keep_additional_equals_in_the_value() {
        assert_eq!(
            parse_search_query("find:url=https://example.com?a=b=c"),
            SearchQuery::Structured(clause(
                "url",
                SearchComparison::Contains,
                "https://example.com?a=b=c",
            ))
        );
    }

    #[test]
    fn exact_match_queries_use_double_equals() {
        assert_eq!(
            parse_search_query("find:username==NoobPing"),
            SearchQuery::Structured(clause("username", SearchComparison::Exact, "noobping",))
        );
    }

    #[test]
    fn symbolic_and_keyword_operators_parse_equivalently() {
        assert_eq!(
            parse_search_query("find:username=noob && url=gitlab || email==alice@example.com"),
            parse_search_query("find:username=noob AND url=gitlab OR email==alice@example.com")
        );
    }

    #[test]
    fn and_takes_precedence_over_or() {
        assert_eq!(
            parse_search_query("find:username=noob OR url=gitlab AND email==alice@example.com"),
            SearchQuery::Structured(or(
                clause("username", SearchComparison::Contains, "noob"),
                and(
                    clause("url", SearchComparison::Contains, "gitlab"),
                    clause("email", SearchComparison::Exact, "alice@example.com"),
                ),
            ))
        );
    }

    #[test]
    fn parentheses_override_default_precedence() {
        assert_eq!(
            parse_search_query("find:(username=noob OR url=gitlab) AND email==alice@example.com"),
            SearchQuery::Structured(and(
                or(
                    clause("username", SearchComparison::Contains, "noob"),
                    clause("url", SearchComparison::Contains, "gitlab"),
                ),
                clause("email", SearchComparison::Exact, "alice@example.com"),
            ))
        );
    }

    #[test]
    fn quoted_values_preserve_spaces_keywords_and_escapes() {
        assert_eq!(
            parse_search_query(r#"find:notes=="Personal OR Work \"vault\"""#),
            SearchQuery::Structured(clause(
                "notes",
                SearchComparison::Exact,
                r#"personal or work "vault""#,
            ))
        );
    }

    #[test]
    fn malformed_boolean_queries_do_not_fall_back_to_plain_search() {
        assert_eq!(
            parse_search_query(r#"find:notes=="unterminated"#),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query("find:username=noob AND"),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query("find:(username=noob OR email==alice@example.com"),
            SearchQuery::InvalidStructured
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
            &SearchQuery::Structured(clause("username", SearchComparison::Contains, "noob",)),
        ));
    }

    #[test]
    fn exact_match_uses_full_case_insensitive_equality() {
        let query =
            SearchQuery::Structured(clause("username", SearchComparison::Exact, "noobping"));
        assert!(row_matches_query(
            "work/noobping/github",
            &indexed_fields(&[("username", "noobping")]),
            &query,
        ));
        assert!(!row_matches_query(
            "work/noobping/github",
            &indexed_fields(&[("username", "noob")]),
            &query,
        ));
    }

    #[test]
    fn boolean_queries_evaluate_mixed_contains_and_exact_matches() {
        let query = parse_search_query(
            "find:(username=noob OR username==alice) AND email==alice@example.com",
        );
        assert!(row_matches_query(
            "work/noobping/github",
            &indexed_fields(&[("username", "noobping"), ("email", "alice@example.com"),]),
            &query,
        ));
        assert!(row_matches_query(
            "work/alice/github",
            &indexed_fields(&[("username", "alice"), ("email", "alice@example.com"),]),
            &query,
        ));
        assert!(!row_matches_query(
            "work/bob/github",
            &indexed_fields(&[("username", "bob"), ("email", "alice@example.com"),]),
            &query,
        ));
    }

    #[test]
    fn unreadable_rows_do_not_match_structured_queries() {
        let query =
            SearchQuery::Structured(clause("username", SearchComparison::Contains, "noobping"));
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
