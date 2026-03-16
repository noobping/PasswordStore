use super::placeholder::{loading_placeholder, resolved_placeholder};
use crate::backend::read_password_entry;
use crate::password::file::{
    canonical_search_field_key, searchable_pass_fields, SearchablePassField,
};
use crate::password::strength::weak_password_reason;
use crate::support::background::spawn_result_task;
use crate::support::object_data::{cloned_data, non_null_to_string_option, set_cloned_data};
use adw::gtk::{ListBox, ListBoxRow};
use regex::Regex;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const SEARCH_CONTROLLER_KEY: &str = "search-controller";
pub(super) const SEARCH_FIELDS_KEY: &str = "search-fields";
const WEAK_PASSWORD_SEARCH_KEY: &str = "__meta_weak_password";

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
    WeakPassword,
    Not(Box<StructuredSearchQuery>),
    And(Box<StructuredSearchQuery>, Box<StructuredSearchQuery>),
    Or(Box<StructuredSearchQuery>, Box<StructuredSearchQuery>),
}

#[derive(Clone, Debug)]
struct SearchClause {
    field: String,
    comparison: SearchComparison,
    value: String,
    compiled_regex: Option<Regex>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchComparison {
    Contains,
    ContainsNot,
    Exact,
    ExactNot,
    RegexMatch,
    RegexNotMatch,
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

impl SearchComparison {
    const fn is_regex(self) -> bool {
        matches!(self, Self::RegexMatch | Self::RegexNotMatch)
    }
}

impl SearchClause {
    fn new(field: String, comparison: SearchComparison, value: String) -> Option<Self> {
        if value.is_empty() {
            return None;
        }

        let compiled_regex = if comparison.is_regex() {
            Some(Regex::new(&value).ok()?)
        } else {
            None
        };
        let value = if comparison.is_regex() {
            value
        } else {
            value.to_lowercase()
        };

        Some(Self {
            field,
            comparison,
            value,
            compiled_regex,
        })
    }
}

impl PartialEq for SearchClause {
    fn eq(&self, other: &Self) -> bool {
        self.field == other.field
            && self.comparison == other.comparison
            && self.value == other.value
    }
}

impl Eq for SearchClause {}

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

    let Some(remainder) = strip_structured_query_prefix(query) else {
        return SearchQuery::Plain(query.to_lowercase());
    };

    parse_structured_search_query(remainder)
        .map_or(SearchQuery::InvalidStructured, SearchQuery::Structured)
}

fn parse_structured_search_query(query: &str) -> Option<StructuredSearchQuery> {
    StructuredSearchParser::new(query).parse()
}

fn strip_structured_query_prefix(query: &str) -> Option<&str> {
    let find = query.get(..4)?;
    if !find.eq_ignore_ascii_case("find") {
        return None;
    }

    match query.get(4..)?.chars().next() {
        Some(':') => query.get(5..),
        Some(ch) if ch.is_ascii_whitespace() => {
            let separator = query
                .get(4..)?
                .char_indices()
                .find(|(_, ch)| !ch.is_ascii_whitespace())
                .map_or(query.len(), |(index, _)| 4 + index);
            query.get(separator..)
        }
        _ => None,
    }
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
        let mut query = self.parse_not()?;
        loop {
            self.skip_whitespace();
            if !self.consume_symbol("&&")
                && !self.consume_keyword("AND")
                && !self.consume_keyword("WITH")
            {
                break;
            }

            let right = self.parse_not()?;
            query = StructuredSearchQuery::And(Box::new(query), Box::new(right));
        }

        Some(query)
    }

    fn parse_not(&mut self) -> Option<StructuredSearchQuery> {
        self.skip_whitespace();
        if self.consume_symbol("!") || self.consume_keyword("NOT") {
            return Some(StructuredSearchQuery::Not(Box::new(self.parse_not()?)));
        }

        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<StructuredSearchQuery> {
        self.skip_whitespace();
        if self.consume_char('(') {
            let query = self.parse_or()?;
            self.skip_whitespace();
            self.consume_char(')').then_some(query)
        } else if self.parse_weak_password_predicate() {
            Some(StructuredSearchQuery::WeakPassword)
        } else {
            Some(StructuredSearchQuery::Clause(self.parse_clause()?))
        }
    }

    fn parse_weak_password_predicate(&mut self) -> bool {
        if !self.consume_keyword("WEAK") {
            return false;
        }

        self.skip_whitespace();
        let _ = self.consume_keyword("PASSWORDS") || self.consume_keyword("PASSWORD");
        true
    }

    fn parse_clause(&mut self) -> Option<SearchClause> {
        let (raw_field, field_was_quoted) = self.parse_field()?;
        self.skip_whitespace();
        let field = canonical_search_field_key(&raw_field)?;
        let comparison = if let Some(comparison) = self.parse_symbolic_comparison() {
            comparison
        } else {
            if !field_was_quoted && is_reserved_human_field_keyword(&raw_field) {
                return None;
            }
            match self.parse_human_comparison() {
                Ok(Some(comparison)) => comparison,
                Ok(None) => SearchComparison::Contains,
                Err(()) => return None,
            }
        };
        self.skip_whitespace();
        let value = self.parse_value()?;
        SearchClause::new(field, comparison, value)
    }

    fn parse_symbolic_comparison(&mut self) -> Option<SearchComparison> {
        if self.consume_symbol("==") {
            Some(SearchComparison::Exact)
        } else if self.consume_symbol("!=") {
            Some(SearchComparison::ExactNot)
        } else if self.consume_symbol("~=") {
            Some(SearchComparison::Contains)
        } else if self.consume_symbol("!~") {
            Some(SearchComparison::ContainsNot)
        } else if self.consume_symbol("=") {
            Some(SearchComparison::Contains)
        } else {
            None
        }
    }

    fn parse_human_comparison(&mut self) -> Result<Option<SearchComparison>, ()> {
        if self.keyword_starts_at(self.pos, "IS") {
            self.consume_keyword("IS");
            self.skip_whitespace();
            return Ok(Some(if self.consume_keyword("NOT") {
                SearchComparison::ExactNot
            } else {
                SearchComparison::Exact
            }));
        }

        if self.keyword_starts_at(self.pos, "DOES") {
            self.consume_keyword("DOES");
            self.skip_whitespace();
            if !self.consume_keyword("NOT") {
                return Err(());
            }
            self.skip_whitespace();
            if self.consume_keyword("CONTAIN") || self.consume_keyword("CONTAINS") {
                return Ok(Some(SearchComparison::ContainsNot));
            }
            if self.consume_keyword("MATCH") || self.consume_keyword("MATCHES") {
                return Ok(Some(SearchComparison::RegexNotMatch));
            }
            return Err(());
        }

        if self.keyword_starts_at(self.pos, "NOT") {
            self.consume_keyword("NOT");
            self.skip_whitespace();
            if self.consume_keyword("REGEX") {
                return Ok(Some(SearchComparison::RegexNotMatch));
            }
            return Err(());
        }

        if self.keyword_starts_at(self.pos, "MATCHES") || self.keyword_starts_at(self.pos, "MATCH")
        {
            let _ = self.consume_keyword("MATCHES") || self.consume_keyword("MATCH");
            return Ok(Some(SearchComparison::RegexMatch));
        }

        if self.keyword_starts_at(self.pos, "REGEX") {
            self.consume_keyword("REGEX");
            return Ok(Some(SearchComparison::RegexMatch));
        }

        if self.keyword_starts_at(self.pos, "CONTAINS")
            || self.keyword_starts_at(self.pos, "CONTAIN")
        {
            let _ = self.consume_keyword("CONTAINS") || self.consume_keyword("CONTAIN");
            return Ok(Some(SearchComparison::Contains));
        }

        Ok(None)
    }

    fn parse_field(&mut self) -> Option<(String, bool)> {
        self.skip_whitespace();
        if matches!(self.peek_char(), Some('"') | Some('\'')) {
            return Some((self.parse_quoted_value()?, true));
        }

        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_whitespace() || matches!(ch, '(' | ')' | '=' | '!' | '~') {
                break;
            }
            self.advance_char();
        }

        let field = self.input.get(start..self.pos)?.trim();
        (!field.is_empty()).then(|| (field.to_string(), false))
    }

    fn parse_value(&mut self) -> Option<String> {
        if matches!(self.peek_char(), Some('"') | Some('\'')) {
            self.parse_quoted_value()
        } else {
            self.parse_unquoted_value()
        }
    }

    fn parse_quoted_value(&mut self) -> Option<String> {
        let quote = self.peek_char()?;
        if !matches!(quote, '"' | '\'') {
            return None;
        }
        if !self.consume_char(quote) {
            return None;
        }

        let mut value = String::new();
        loop {
            let ch = self.peek_char()?;
            self.advance_char();
            match ch {
                ch if ch == quote => return Some(value),
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
                || self.keyword_starts_at(scan, "WITH")
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

fn is_reserved_human_field_keyword(field: &str) -> bool {
    field.eq_ignore_ascii_case("and")
        || field.eq_ignore_ascii_case("with")
        || field.eq_ignore_ascii_case("or")
        || field.eq_ignore_ascii_case("not")
        || field.eq_ignore_ascii_case("is")
        || field.eq_ignore_ascii_case("does")
        || field.eq_ignore_ascii_case("match")
        || field.eq_ignore_ascii_case("matches")
        || field.eq_ignore_ascii_case("regex")
        || field.eq_ignore_ascii_case("contain")
        || field.eq_ignore_ascii_case("contains")
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
        StructuredSearchQuery::Clause(clause) => clause_matches(fields, clause),
        StructuredSearchQuery::WeakPassword => has_weak_password(fields),
        StructuredSearchQuery::Not(query) => !structured_query_matches(fields, query),
        StructuredSearchQuery::And(left, right) => {
            structured_query_matches(fields, left) && structured_query_matches(fields, right)
        }
        StructuredSearchQuery::Or(left, right) => {
            structured_query_matches(fields, left) || structured_query_matches(fields, right)
        }
    }
}

fn has_weak_password(fields: &[SearchablePassField]) -> bool {
    fields
        .iter()
        .any(|field| field.key == WEAK_PASSWORD_SEARCH_KEY)
}

fn clause_matches(fields: &[SearchablePassField], clause: &SearchClause) -> bool {
    let matches_positive = |predicate: fn(&SearchablePassField, &SearchClause) -> bool| {
        fields
            .iter()
            .filter(|field| field.key == clause.field)
            .any(|field| predicate(field, clause))
    };

    match clause.comparison {
        SearchComparison::Contains => {
            matches_positive(|field, clause| field.normalized_value.contains(&clause.value))
        }
        SearchComparison::ContainsNot => {
            !matches_positive(|field, clause| field.normalized_value.contains(&clause.value))
        }
        SearchComparison::Exact => {
            matches_positive(|field, clause| field.normalized_value == clause.value)
        }
        SearchComparison::ExactNot => {
            !matches_positive(|field, clause| field.normalized_value == clause.value)
        }
        SearchComparison::RegexMatch => matches_positive(|field, clause| {
            clause
                .compiled_regex
                .as_ref()
                .is_some_and(|regex| regex.is_match(&field.value))
        }),
        SearchComparison::RegexNotMatch => !matches_positive(|field, clause| {
            clause
                .compiled_regex
                .as_ref()
                .is_some_and(|regex| regex.is_match(&field.value))
        }),
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

fn indexed_fields_for_contents(contents: &str) -> Vec<SearchablePassField> {
    let mut fields = searchable_pass_fields(contents);
    if let Some(reason) = weak_password_reason(contents.lines().next().unwrap_or_default()) {
        fields.push(SearchablePassField {
            key: WEAK_PASSWORD_SEARCH_KEY.to_string(),
            value: reason.clone(),
            normalized_value: reason.to_lowercase(),
        });
    }

    fields
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
        WEAK_PASSWORD_SEARCH_KEY,
    };
    use crate::password::file::SearchablePassField;

    fn clause(field: &str, comparison: SearchComparison, value: &str) -> StructuredSearchQuery {
        StructuredSearchQuery::Clause(SearchClause {
            field: field.to_string(),
            comparison,
            value: value.to_string(),
            compiled_regex: None,
        })
    }

    fn and(left: StructuredSearchQuery, right: StructuredSearchQuery) -> StructuredSearchQuery {
        StructuredSearchQuery::And(Box::new(left), Box::new(right))
    }

    fn or(left: StructuredSearchQuery, right: StructuredSearchQuery) -> StructuredSearchQuery {
        StructuredSearchQuery::Or(Box::new(left), Box::new(right))
    }

    fn not(query: StructuredSearchQuery) -> StructuredSearchQuery {
        StructuredSearchQuery::Not(Box::new(query))
    }

    fn weak_password() -> StructuredSearchQuery {
        StructuredSearchQuery::WeakPassword
    }

    fn indexed_fields(entries: &[(&str, &str)]) -> SearchRowFieldIndexState {
        SearchRowFieldIndexState::Indexed(
            entries
                .iter()
                .map(|(key, value)| SearchablePassField {
                    key: (*key).to_string(),
                    value: (*value).to_string(),
                    normalized_value: value.to_lowercase(),
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
        assert_eq!(
            parse_search_query("user alice"),
            SearchQuery::Plain("user alice".to_string())
        );
    }

    #[test]
    fn structured_queries_parse_with_case_insensitive_prefix() {
        assert_eq!(
            parse_search_query("FiNd:username=noobping"),
            SearchQuery::Structured(clause("username", SearchComparison::Contains, "noobping",))
        );
        assert_eq!(
            parse_search_query("FiNd user noobping"),
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
        assert_eq!(
            parse_search_query("find user nick with weak password"),
            SearchQuery::Structured(and(
                clause("username", SearchComparison::Contains, "nick"),
                weak_password(),
            ))
        );
    }

    #[test]
    fn not_queries_parse_with_word_and_symbol_forms() {
        assert_eq!(
            parse_search_query("find:NOT username==alice"),
            SearchQuery::Structured(not(clause("username", SearchComparison::Exact, "alice",)))
        );
        assert_eq!(
            parse_search_query("find:!username~=alice"),
            SearchQuery::Structured(not(
                clause("username", SearchComparison::Contains, "alice",)
            ))
        );
    }

    #[test]
    fn not_binds_tighter_than_and_or() {
        assert_eq!(
            parse_search_query(
                "find:NOT username==alice AND email==alice@example.com OR url~=gitlab"
            ),
            SearchQuery::Structured(or(
                and(
                    not(clause("username", SearchComparison::Exact, "alice")),
                    clause("email", SearchComparison::Exact, "alice@example.com"),
                ),
                clause("url", SearchComparison::Contains, "gitlab"),
            ))
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
    fn single_quoted_values_preserve_spaces_keywords_and_escapes() {
        assert_eq!(
            parse_search_query(r#"find:notes=='Personal OR Work \'vault\''"#),
            SearchQuery::Structured(clause(
                "notes",
                SearchComparison::Exact,
                "personal or work 'vault'",
            ))
        );
    }

    #[test]
    fn operator_shorthands_parse_as_expected() {
        assert_eq!(
            parse_search_query("find:username!=alice"),
            SearchQuery::Structured(clause("username", SearchComparison::ExactNot, "alice"))
        );
        assert_eq!(
            parse_search_query("find:url~=gitlab"),
            SearchQuery::Structured(clause("url", SearchComparison::Contains, "gitlab"))
        );
        assert_eq!(
            parse_search_query("find:url!~gitlab"),
            SearchQuery::Structured(clause("url", SearchComparison::ContainsNot, "gitlab"))
        );
    }

    #[test]
    fn human_friendly_clauses_parse_as_expected() {
        assert_eq!(
            parse_search_query("find user alice"),
            SearchQuery::Structured(clause("username", SearchComparison::Contains, "alice"))
        );
        assert_eq!(
            parse_search_query("find:user alice"),
            SearchQuery::Structured(clause("username", SearchComparison::Contains, "alice"))
        );
        assert_eq!(
            parse_search_query("find user is alice"),
            SearchQuery::Structured(clause("username", SearchComparison::Exact, "alice"))
        );
        assert_eq!(
            parse_search_query("find user is not alice"),
            SearchQuery::Structured(clause("username", SearchComparison::ExactNot, "alice"))
        );
        assert_eq!(
            parse_search_query("find url contains gitlab"),
            SearchQuery::Structured(clause("url", SearchComparison::Contains, "gitlab"))
        );
        assert_eq!(
            parse_search_query("find url does not contain gitlab"),
            SearchQuery::Structured(clause("url", SearchComparison::ContainsNot, "gitlab"))
        );
        assert_eq!(
            parse_search_query("find user matches '^Alice$'"),
            SearchQuery::Structured(clause("username", SearchComparison::RegexMatch, "^Alice$",))
        );
        assert_eq!(
            parse_search_query("find user regex '^Alice$'"),
            SearchQuery::Structured(clause("username", SearchComparison::RegexMatch, "^Alice$",))
        );
        assert_eq!(
            parse_search_query("find user does not match '^Alice$'"),
            SearchQuery::Structured(clause(
                "username",
                SearchComparison::RegexNotMatch,
                "^Alice$",
            ))
        );
        assert_eq!(
            parse_search_query("find url not regex 'gitlab|github'"),
            SearchQuery::Structured(clause(
                "url",
                SearchComparison::RegexNotMatch,
                "gitlab|github",
            ))
        );
    }

    #[test]
    fn weak_password_keyword_parses_as_a_structured_predicate() {
        assert_eq!(
            parse_search_query("find weak password"),
            SearchQuery::Structured(weak_password())
        );
        assert_eq!(
            parse_search_query("find weak"),
            SearchQuery::Structured(weak_password())
        );
        assert_eq!(
            parse_search_query("find not weak password"),
            SearchQuery::Structured(not(weak_password()))
        );
        assert_eq!(
            parse_search_query("find weak password and username==alice"),
            SearchQuery::Structured(and(
                weak_password(),
                clause("username", SearchComparison::Exact, "alice"),
            ))
        );
    }

    #[test]
    fn mixed_human_and_symbolic_syntax_parses_with_existing_precedence() {
        assert_eq!(
            parse_search_query("find user alice and not url gitlab"),
            SearchQuery::Structured(and(
                clause("username", SearchComparison::Contains, "alice"),
                not(clause("url", SearchComparison::Contains, "gitlab")),
            ))
        );
        assert_eq!(
            parse_search_query("find user alice and url~=gitlab"),
            SearchQuery::Structured(and(
                clause("username", SearchComparison::Contains, "alice"),
                clause("url", SearchComparison::Contains, "gitlab"),
            ))
        );
    }

    #[test]
    fn human_friendly_queries_support_quoted_values() {
        assert_eq!(
            parse_search_query(r#"find notes contains 'Personal OR Work'"#),
            SearchQuery::Structured(clause(
                "notes",
                SearchComparison::Contains,
                "personal or work",
            ))
        );
        assert_eq!(
            parse_search_query(r#"find (user alice or email is "a@b.com") and not url gitlab"#),
            SearchQuery::Structured(and(
                or(
                    clause("username", SearchComparison::Contains, "alice"),
                    clause("email", SearchComparison::Exact, "a@b.com"),
                ),
                not(clause("url", SearchComparison::Contains, "gitlab")),
            ))
        );
        assert_eq!(
            parse_search_query(r#"find notes matches 'Personal (OR|AND) Work'"#),
            SearchQuery::Structured(clause(
                "notes",
                SearchComparison::RegexMatch,
                "Personal (OR|AND) Work",
            ))
        );
        assert_eq!(
            parse_search_query(r#"find "security question" is "first pet""#),
            SearchQuery::Structured(clause(
                "security question",
                SearchComparison::Exact,
                "first pet",
            ))
        );
        assert_eq!(
            parse_search_query(r#"find "matches" is "keyword field""#),
            SearchQuery::Structured(clause("matches", SearchComparison::Exact, "keyword field",))
        );
        assert_eq!(
            parse_search_query(r#"find:"security question"=="first pet""#),
            SearchQuery::Structured(clause(
                "security question",
                SearchComparison::Exact,
                "first pet",
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
            parse_search_query("find:notes=='unterminated"),
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
        assert_eq!(
            parse_search_query("find:NOT"),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query("find:username<>alice"),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query("find not"),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query("find user matches '['"),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query(r#"find "otpauth" is "otpauth://totp/example""#),
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
        assert_eq!(
            parse_search_query("find user"),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query("find user is"),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query("find url does not"),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query("find url does not contain"),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query("find contains alice"),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query("find user matches"),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query("find user does not match"),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query("find user not regex"),
            SearchQuery::InvalidStructured
        );
        assert_eq!(
            parse_search_query("find otpauth contains totp"),
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
    fn human_friendly_queries_match_indexed_fields() {
        let query = parse_search_query("find user alice and not url gitlab");
        assert!(row_matches_query(
            "work/alice/example",
            &indexed_fields(&[("username", "alice"), ("url", "https://example.com")]),
            &query,
        ));
        assert!(!row_matches_query(
            "work/alice/gitlab",
            &indexed_fields(&[("username", "alice"), ("url", "https://gitlab.com")]),
            &query,
        ));
    }

    #[test]
    fn regex_queries_match_case_sensitive_patterns() {
        let exact_case = parse_search_query("find user matches '^Alice$'");
        let wrong_case = parse_search_query("find user matches '^alice$'");
        let ignore_case = parse_search_query(r#"find user regex '(?i)^alice$'"#);
        let negative = parse_search_query("find user does not match '^Alice$'");

        assert!(row_matches_query(
            "work/Alice/example",
            &indexed_fields(&[("username", "Alice")]),
            &exact_case,
        ));
        assert!(!row_matches_query(
            "work/Alice/example",
            &indexed_fields(&[("username", "Alice")]),
            &wrong_case,
        ));
        assert!(row_matches_query(
            "work/Alice/example",
            &indexed_fields(&[("username", "Alice")]),
            &ignore_case,
        ));
        assert!(!row_matches_query(
            "work/Alice/example",
            &indexed_fields(&[("username", "Alice")]),
            &negative,
        ));
        assert!(row_matches_query(
            "work/Bob/example",
            &indexed_fields(&[("username", "Bob")]),
            &negative,
        ));
    }

    #[test]
    fn weak_password_queries_match_only_rows_with_the_weak_password_flag() {
        assert!(row_matches_query(
            "alice",
            &indexed_fields(&[(WEAK_PASSWORD_SEARCH_KEY, "Too short (6 characters)")]),
            &SearchQuery::Structured(weak_password()),
        ));
        assert!(!row_matches_query(
            "alice",
            &indexed_fields(&[("username", "alice")]),
            &SearchQuery::Structured(weak_password()),
        ));
        assert!(row_matches_query(
            "alice",
            &indexed_fields(&[("username", "alice")]),
            &SearchQuery::Structured(not(weak_password())),
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
    fn negative_clause_operators_match_as_expected() {
        let exact_not =
            SearchQuery::Structured(clause("username", SearchComparison::ExactNot, "alice"));
        let contains_not =
            SearchQuery::Structured(clause("url", SearchComparison::ContainsNot, "gitlab"));

        assert!(row_matches_query(
            "work/noobping/github",
            &indexed_fields(&[("username", "noobping"), ("url", "https://example.com")]),
            &exact_not,
        ));
        assert!(!row_matches_query(
            "work/alice/github",
            &indexed_fields(&[("username", "alice"), ("url", "https://example.com")]),
            &exact_not,
        ));
        assert!(!row_matches_query(
            "work/multi/github",
            &indexed_fields(&[("username", "alice"), ("username", "bob")]),
            &exact_not,
        ));
        assert!(row_matches_query(
            "work/noobping/github",
            &indexed_fields(&[("username", "noobping"), ("url", "https://example.com")]),
            &contains_not,
        ));
        assert!(!row_matches_query(
            "work/noobping/gitlab",
            &indexed_fields(&[("username", "noobping"), ("url", "https://gitlab.com")]),
            &contains_not,
        ));
    }

    #[test]
    fn negated_clauses_and_groups_match_as_expected() {
        let negated_clause =
            SearchQuery::Structured(not(clause("username", SearchComparison::Exact, "alice")));
        let negated_group = parse_search_query("find:!(username~=alice OR email=='a@b.com')");

        assert!(row_matches_query(
            "work/noobping/github",
            &indexed_fields(&[("username", "noobping"), ("email", "c@d.com")]),
            &negated_clause,
        ));
        assert!(!row_matches_query(
            "work/alice/github",
            &indexed_fields(&[("username", "alice"), ("email", "c@d.com")]),
            &negated_clause,
        ));
        assert!(row_matches_query(
            "work/noobping/github",
            &indexed_fields(&[("username", "noobping"), ("email", "c@d.com")]),
            &negated_group,
        ));
        assert!(!row_matches_query(
            "work/alice/github",
            &indexed_fields(&[("username", "alice"), ("email", "c@d.com")]),
            &negated_group,
        ));
    }

    #[test]
    fn legacy_equals_operator_still_means_contains() {
        assert!(row_matches_query(
            "work/noobping/github",
            &indexed_fields(&[("username", "noobping")]),
            &SearchQuery::Structured(clause("username", SearchComparison::Contains, "noob",)),
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
