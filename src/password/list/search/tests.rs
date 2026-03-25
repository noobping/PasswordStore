use super::index::is_stale_index_batch;
use super::query::{
    parse_search_query, row_matches_query, SearchClause, SearchComparison, SearchQuery,
    StructuredSearchQuery, OTP_SEARCH_KEY, WEAK_PASSWORD_SEARCH_KEY,
};
use super::SearchRowFieldIndexState;
use crate::password::file::SearchablePassField;

fn clause(field: &str, comparison: SearchComparison, value: &str) -> StructuredSearchQuery {
    StructuredSearchQuery::Clause(
        SearchClause::new(field.to_string(), comparison, value.to_string()).unwrap(),
    )
}

fn field_ref_clause(
    field: &str,
    comparison: SearchComparison,
    referenced_field: &str,
) -> StructuredSearchQuery {
    StructuredSearchQuery::Clause(
        SearchClause::field_reference(field.to_string(), comparison, referenced_field.to_string())
            .unwrap(),
    )
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

fn otp() -> StructuredSearchQuery {
    StructuredSearchQuery::Otp
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
        SearchQuery::Structured(clause("username", SearchComparison::Contains, "noobping"))
    );
    assert_eq!(
        parse_search_query("FiNd user noobping"),
        SearchQuery::Structured(clause("username", SearchComparison::Contains, "noobping"))
    );
}

#[test]
fn regex_queries_parse_with_a_separate_reg_prefix() {
    assert_eq!(
        parse_search_query(r#"reg:(?i)^alice/.+github$"#),
        SearchQuery::Regex(super::query::RegexSearchQuery::new("(?i)^alice/.+github$").unwrap())
    );
    assert_eq!(
        parse_search_query(r#"ReG team/.+service"#),
        SearchQuery::Regex(super::query::RegexSearchQuery::new("team/.+service").unwrap())
    );
}

#[test]
fn structured_queries_trim_key_and_value() {
    assert_eq!(
        parse_search_query("find: user = NoobPing "),
        SearchQuery::Structured(clause("username", SearchComparison::Contains, "noobping"))
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
        SearchQuery::Structured(clause("username", SearchComparison::Exact, "noobping"))
    );
}

#[test]
fn exact_field_reference_queries_parse_and_canonicalize_aliases() {
    assert_eq!(
        parse_search_query("find email is $username"),
        SearchQuery::Structured(field_ref_clause(
            "email",
            SearchComparison::Exact,
            "username",
        ))
    );
    assert_eq!(
        parse_search_query("find email is not $user"),
        SearchQuery::Structured(field_ref_clause(
            "email",
            SearchComparison::ExactNot,
            "username",
        ))
    );
    assert_eq!(
        parse_search_query(r#"find "backup email" == $"security question""#),
        SearchQuery::Structured(field_ref_clause(
            "backup email",
            SearchComparison::Exact,
            "security question",
        ))
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
        SearchQuery::Structured(not(clause("username", SearchComparison::Exact, "alice")))
    );
    assert_eq!(
        parse_search_query("find:!username~=alice"),
        SearchQuery::Structured(not(clause("username", SearchComparison::Contains, "alice")))
    );
}

#[test]
fn not_binds_tighter_than_and_or() {
    assert_eq!(
        parse_search_query("find:NOT username==alice AND email==alice@example.com OR url~=gitlab"),
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
        SearchQuery::Structured(clause("username", SearchComparison::RegexMatch, "^Alice$"))
    );
    assert_eq!(
        parse_search_query("find user regex '^Alice$'"),
        SearchQuery::Structured(clause("username", SearchComparison::RegexMatch, "^Alice$"))
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
fn otp_keyword_parses_as_a_structured_predicate() {
    assert_eq!(
        parse_search_query("find otp"),
        SearchQuery::Structured(otp())
    );
    assert_eq!(
        parse_search_query("find not otp"),
        SearchQuery::Structured(not(otp()))
    );
    assert_eq!(
        parse_search_query("find otp and username==alice"),
        SearchQuery::Structured(and(
            otp(),
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
        SearchQuery::Structured(clause("matches", SearchComparison::Exact, "keyword field"))
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
        parse_search_query("find email contains $username"),
        SearchQuery::InvalidStructured
    );
    assert_eq!(
        parse_search_query("find:email~=$username"),
        SearchQuery::InvalidStructured
    );
    assert_eq!(
        parse_search_query("find user regex $email"),
        SearchQuery::InvalidStructured
    );
    assert_eq!(
        parse_search_query(r#"find "otpauth" is "otpauth://totp/example""#),
        SearchQuery::InvalidStructured
    );
    assert_eq!(
        parse_search_query("find email is $otpauth"),
        SearchQuery::InvalidStructured
    );
}

#[test]
fn malformed_reg_queries_do_not_fall_back_to_plain_search() {
    assert_eq!(parse_search_query("reg"), SearchQuery::InvalidRegex);
    assert_eq!(parse_search_query("reg:["), SearchQuery::InvalidRegex);
    assert_eq!(parse_search_query("reg ["), SearchQuery::InvalidRegex);
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
fn reg_queries_match_labels_and_indexed_field_corpus() {
    let label_query = parse_search_query(r#"reg:^(?i)work/alice/.+$"#);
    let field_query = parse_search_query(r#"reg:(?i)email:\s+alice@example\.com"#);

    assert!(row_matches_query(
        "work/alice/github",
        &SearchRowFieldIndexState::Unavailable,
        &label_query,
    ));
    assert!(row_matches_query(
        "work/bob/github",
        &indexed_fields(&[("email", "alice@example.com")]),
        &field_query,
    ));
    assert!(!row_matches_query(
        "work/bob/github",
        &indexed_fields(&[("email", "bob@example.com")]),
        &field_query,
    ));
}

#[test]
fn structured_queries_match_indexed_fields_with_case_insensitive_contains() {
    assert!(row_matches_query(
        "work/noobping/github",
        &indexed_fields(&[("username", "noobping"), ("url", "https://example.com")]),
        &SearchQuery::Structured(clause("username", SearchComparison::Contains, "noob")),
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
fn otp_queries_match_only_rows_with_the_otp_flag() {
    assert!(row_matches_query(
        "alice",
        &indexed_fields(&[(OTP_SEARCH_KEY, "true")]),
        &SearchQuery::Structured(otp()),
    ));
    assert!(!row_matches_query(
        "alice",
        &indexed_fields(&[("username", "alice")]),
        &SearchQuery::Structured(otp()),
    ));
    assert!(row_matches_query(
        "alice",
        &indexed_fields(&[("username", "alice")]),
        &SearchQuery::Structured(not(otp())),
    ));
}

#[test]
fn exact_match_uses_full_case_insensitive_equality() {
    let query = SearchQuery::Structured(clause("username", SearchComparison::Exact, "noobping"));
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
fn exact_field_reference_queries_match_case_insensitively() {
    let query = SearchQuery::Structured(field_ref_clause(
        "email",
        SearchComparison::Exact,
        "username",
    ));
    assert!(row_matches_query(
        "work/alice/github",
        &indexed_fields(&[("email", "ALICE"), ("username", "alice")]),
        &query,
    ));
    assert!(!row_matches_query(
        "work/alice/github",
        &indexed_fields(&[("email", "alice@example.com"), ("username", "alice")]),
        &query,
    ));
}

#[test]
fn exact_field_reference_queries_match_any_repeated_value_pair() {
    let query = SearchQuery::Structured(field_ref_clause(
        "email",
        SearchComparison::Exact,
        "username",
    ));
    assert!(row_matches_query(
        "work/shared/example",
        &indexed_fields(&[
            ("email", "alice@example.com"),
            ("email", "shared-user"),
            ("username", "owner"),
            ("username", "SHARED-USER"),
        ]),
        &query,
    ));
}

#[test]
fn negative_clause_operators_match_as_expected() {
    let exact_not =
        SearchQuery::Structured(clause("username", SearchComparison::ExactNot, "alice"));
    let contains_not =
        SearchQuery::Structured(clause("url", SearchComparison::ContainsNot, "gitlab"));
    let field_exact_not = SearchQuery::Structured(field_ref_clause(
        "email",
        SearchComparison::ExactNot,
        "username",
    ));

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
    assert!(!row_matches_query(
        "work/shared/example",
        &indexed_fields(&[("email", "shared"), ("username", "SHARED")]),
        &field_exact_not,
    ));
    assert!(row_matches_query(
        "work/missing/example",
        &indexed_fields(&[("email", "shared")]),
        &field_exact_not,
    ));
    assert!(row_matches_query(
        "work/different/example",
        &indexed_fields(&[("email", "shared"), ("username", "owner")]),
        &field_exact_not,
    ));
}

#[test]
fn negated_clauses_and_groups_match_as_expected() {
    let negated_clause =
        SearchQuery::Structured(not(clause("username", SearchComparison::Exact, "alice")));
    let negated_group = parse_search_query("find:!(username~=alice OR email=='a@b.com')");
    let negated_field_ref = parse_search_query("find not email is $username");

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
    assert!(row_matches_query(
        "work/different/github",
        &indexed_fields(&[("username", "alice"), ("email", "alice@example.com")]),
        &negated_field_ref,
    ));
    assert!(!row_matches_query(
        "work/same/github",
        &indexed_fields(&[("username", "shared"), ("email", "SHARED")]),
        &negated_field_ref,
    ));
}

#[test]
fn legacy_equals_operator_still_means_contains() {
    assert!(row_matches_query(
        "work/noobping/github",
        &indexed_fields(&[("username", "noobping")]),
        &SearchQuery::Structured(clause("username", SearchComparison::Contains, "noob")),
    ));
}

#[test]
fn boolean_queries_evaluate_mixed_contains_and_exact_matches() {
    let query =
        parse_search_query("find:(username=noob OR username==alice) AND email==alice@example.com");
    assert!(row_matches_query(
        "work/noobping/github",
        &indexed_fields(&[("username", "noobping"), ("email", "alice@example.com")]),
        &query,
    ));
    assert!(row_matches_query(
        "work/alice/github",
        &indexed_fields(&[("username", "alice"), ("email", "alice@example.com")]),
        &query,
    ));
    assert!(!row_matches_query(
        "work/bob/github",
        &indexed_fields(&[("username", "bob"), ("email", "alice@example.com")]),
        &query,
    ));
}

#[test]
fn exact_field_reference_queries_do_not_match_when_either_side_is_missing() {
    let query = SearchQuery::Structured(field_ref_clause(
        "email",
        SearchComparison::Exact,
        "username",
    ));
    assert!(!row_matches_query(
        "work/missing-right/github",
        &indexed_fields(&[("email", "alice@example.com")]),
        &query,
    ));
    assert!(!row_matches_query(
        "work/missing-left/github",
        &indexed_fields(&[("username", "alice@example.com")]),
        &query,
    ));
}

#[test]
fn unreadable_rows_do_not_match_structured_queries() {
    let query = SearchQuery::Structured(clause("username", SearchComparison::Contains, "noobping"));
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
