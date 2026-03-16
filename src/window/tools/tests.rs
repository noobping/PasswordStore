use super::field_values::{
    field_value_catalog_from_entries, format_exact_field_query, matching_items_subtitle,
    unique_values_subtitle, FieldCatalogEntry, ValueCatalogEntry,
};
use super::tool_rows_enabled;
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
