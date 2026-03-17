use super::SearchRowFieldIndexState;
use crate::password::file::{canonical_search_field_key, SearchablePassField};
use regex::Regex;

pub(super) const OTP_SEARCH_KEY: &str = "__meta_otp";
pub(super) const WEAK_PASSWORD_SEARCH_KEY: &str = "__meta_weak_password";

#[derive(Clone, Debug)]
pub(super) enum SearchQuery {
    Empty,
    Plain(String),
    Regex(RegexSearchQuery),
    Structured(StructuredSearchQuery),
    InvalidRegex,
    InvalidStructured,
}

#[derive(Clone, Debug)]
pub(super) struct RegexSearchQuery {
    pattern: String,
    compiled: Regex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum StructuredSearchQuery {
    Clause(SearchClause),
    Otp,
    WeakPassword,
    Not(Box<StructuredSearchQuery>),
    And(Box<StructuredSearchQuery>, Box<StructuredSearchQuery>),
    Or(Box<StructuredSearchQuery>, Box<StructuredSearchQuery>),
}

#[derive(Clone, Debug)]
pub(super) struct SearchClause {
    field: String,
    comparison: SearchComparison,
    value: String,
    compiled_regex: Option<Regex>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SearchComparison {
    Contains,
    ContainsNot,
    Exact,
    ExactNot,
    RegexMatch,
    RegexNotMatch,
}

impl SearchQuery {
    pub(super) const fn requires_index(&self) -> bool {
        matches!(self, Self::Regex(_) | Self::Structured(_))
    }
}

impl SearchComparison {
    const fn is_regex(self) -> bool {
        matches!(self, Self::RegexMatch | Self::RegexNotMatch)
    }
}

impl SearchClause {
    pub(super) fn new(field: String, comparison: SearchComparison, value: String) -> Option<Self> {
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

impl RegexSearchQuery {
    pub(super) fn new(pattern: &str) -> Option<Self> {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            return None;
        }
        let compiled = Regex::new(pattern).ok()?;
        Some(Self {
            pattern: pattern.to_string(),
            compiled,
        })
    }
}

impl PartialEq for RegexSearchQuery {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern
    }
}

impl Eq for RegexSearchQuery {}

impl PartialEq for SearchClause {
    fn eq(&self, other: &Self) -> bool {
        self.field == other.field
            && self.comparison == other.comparison
            && self.value == other.value
    }
}

impl Eq for SearchClause {}

impl PartialEq for SearchQuery {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty, Self::Empty)
            | (Self::InvalidRegex, Self::InvalidRegex)
            | (Self::InvalidStructured, Self::InvalidStructured) => true,
            (Self::Plain(left), Self::Plain(right)) => left == right,
            (Self::Regex(left), Self::Regex(right)) => left == right,
            (Self::Structured(left), Self::Structured(right)) => left == right,
            _ => false,
        }
    }
}

impl Eq for SearchQuery {}

pub(super) fn parse_search_query(query: &str) -> SearchQuery {
    if query.is_empty() {
        return SearchQuery::Empty;
    }

    if query.trim().eq_ignore_ascii_case("reg") {
        return SearchQuery::InvalidRegex;
    }

    if let Some(remainder) = strip_query_prefix(query, "reg") {
        return RegexSearchQuery::new(remainder)
            .map_or(SearchQuery::InvalidRegex, SearchQuery::Regex);
    }

    let Some(remainder) = strip_structured_query_prefix(query) else {
        return SearchQuery::Plain(query.to_lowercase());
    };

    parse_structured_search_query(remainder)
        .map_or(SearchQuery::InvalidStructured, SearchQuery::Structured)
}

pub(super) fn row_matches_query(
    label: &str,
    fields: &SearchRowFieldIndexState,
    query: &SearchQuery,
) -> bool {
    match query {
        SearchQuery::Empty => true,
        SearchQuery::Plain(query) => label.to_lowercase().contains(query),
        SearchQuery::Regex(query) => regex_query_matches(label, fields, query),
        SearchQuery::Structured(query) => match fields {
            SearchRowFieldIndexState::Indexed(fields) => structured_query_matches(fields, query),
            SearchRowFieldIndexState::Unindexed | SearchRowFieldIndexState::Unavailable => false,
        },
        SearchQuery::InvalidRegex => false,
        SearchQuery::InvalidStructured => false,
    }
}

fn parse_structured_search_query(query: &str) -> Option<StructuredSearchQuery> {
    StructuredSearchParser::new(query).parse()
}

fn strip_structured_query_prefix(query: &str) -> Option<&str> {
    strip_query_prefix(query, "find")
}

fn strip_query_prefix<'a>(query: &'a str, prefix: &str) -> Option<&'a str> {
    let found_prefix = query.get(..prefix.len())?;
    if !found_prefix.eq_ignore_ascii_case(prefix) {
        return None;
    }

    match query.get(prefix.len()..)?.chars().next() {
        Some(':') => query.get(prefix.len() + 1..),
        Some(ch) if ch.is_ascii_whitespace() => {
            let separator = query
                .get(prefix.len()..)?
                .char_indices()
                .find(|(_, ch)| !ch.is_ascii_whitespace())
                .map_or(query.len(), |(index, _)| prefix.len() + index);
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
        } else if self.parse_otp_predicate() {
            Some(StructuredSearchQuery::Otp)
        } else if self.parse_weak_password_predicate() {
            Some(StructuredSearchQuery::WeakPassword)
        } else {
            Some(StructuredSearchQuery::Clause(self.parse_clause()?))
        }
    }

    fn parse_otp_predicate(&mut self) -> bool {
        self.consume_keyword("OTP")
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
        || field.eq_ignore_ascii_case("otp")
        || field.eq_ignore_ascii_case("contain")
        || field.eq_ignore_ascii_case("contains")
}

fn structured_query_matches(fields: &[SearchablePassField], query: &StructuredSearchQuery) -> bool {
    match query {
        StructuredSearchQuery::Clause(clause) => clause_matches(fields, clause),
        StructuredSearchQuery::Otp => has_otp(fields),
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

fn has_otp(fields: &[SearchablePassField]) -> bool {
    fields.iter().any(|field| field.key == OTP_SEARCH_KEY)
}

fn regex_query_matches(
    label: &str,
    fields: &SearchRowFieldIndexState,
    query: &RegexSearchQuery,
) -> bool {
    if query.compiled.is_match(label) {
        return true;
    }

    match fields {
        SearchRowFieldIndexState::Indexed(fields) => query
            .compiled
            .is_match(&regex_search_corpus(label, fields)),
        SearchRowFieldIndexState::Unindexed | SearchRowFieldIndexState::Unavailable => false,
    }
}

fn regex_search_corpus(label: &str, fields: &[SearchablePassField]) -> String {
    let mut corpus = String::from(label);
    for field in fields {
        corpus.push('\n');
        corpus.push_str(&field.key);
        corpus.push(':');
        corpus.push(' ');
        corpus.push_str(&field.value);
    }
    corpus
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
