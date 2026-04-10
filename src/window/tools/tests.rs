use super::field_values::{
    field_value_catalog_from_entries, format_exact_field_query, matching_items_subtitle,
    unique_values_subtitle, FieldCatalogEntry, ValueCatalogEntry,
};
use super::{
    advanced_search_tool_rows_enabled, audit_tool_cache_should_clear, filter_tool_requests,
    password_read_tools_available_for_store_roots_with, tool_browser_flow_is_visible,
    tool_row_matches_query, FieldValueRequest,
};
use crate::i18n::gettext;
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
    assert_eq!(
        unique_values_subtitle(1),
        gettext("{count} unique value").replace("{count}", "1")
    );
    assert_eq!(
        unique_values_subtitle(2),
        gettext("{count} unique values").replace("{count}", "2")
    );
    assert_eq!(
        matching_items_subtitle(1),
        gettext("{count} matching item").replace("{count}", "1")
    );
    assert_eq!(
        matching_items_subtitle(3),
        gettext("{count} matching items").replace("{count}", "3")
    );
}

#[test]
fn advanced_search_tool_rows_disable_while_any_advanced_search_tool_is_busy() {
    assert!(advanced_search_tool_rows_enabled(false, false));
    assert!(!advanced_search_tool_rows_enabled(true, false));
    assert!(!advanced_search_tool_rows_enabled(false, true));
    assert!(!advanced_search_tool_rows_enabled(true, true));
}

#[test]
fn tool_browser_flow_stays_visible_while_a_password_entry_is_open() {
    assert!(tool_browser_flow_is_visible(
        false, false, false, true, false, false, false
    ));
    assert!(tool_browser_flow_is_visible(
        false, false, false, false, false, true, false
    ));
    assert!(tool_browser_flow_is_visible(
        false, false, false, false, false, false, true
    ));
    assert!(tool_browser_flow_is_visible(
        false, false, false, false, true, false, false
    ));
    assert!(!tool_browser_flow_is_visible(
        false, false, false, false, false, false, false
    ));
}

#[test]
fn audit_tool_cache_persists_while_page_remains_in_navigation_stack() {
    assert!(!audit_tool_cache_should_clear(true, true));
    assert!(!audit_tool_cache_should_clear(false, true));
    assert!(audit_tool_cache_should_clear(false, false));
}

#[test]
fn password_read_tools_are_disabled_when_every_store_is_fido_only() {
    assert!(password_read_tools_available_for_store_roots_with(
        &[],
        |_| false
    ));

    let stores = vec!["/stores/fido".to_string(), "/stores/backup".to_string()];

    assert!(!password_read_tools_available_for_store_roots_with(
        &stores,
        |_| false,
    ));
    assert!(password_read_tools_available_for_store_roots_with(
        &stores,
        |store| { store == "/stores/backup" }
    ));
}

#[test]
fn tool_requests_skip_fido_only_stores() {
    let requests = vec![
        FieldValueRequest {
            root: "/stores/fido".to_string(),
            label: "mail".to_string(),
        },
        FieldValueRequest {
            root: "/stores/standard".to_string(),
            label: "chat".to_string(),
        },
    ];

    assert_eq!(
        filter_tool_requests(requests, |store| store == "/stores/standard"),
        vec![FieldValueRequest {
            root: "/stores/standard".to_string(),
            label: "chat".to_string(),
        }]
    );
}

#[test]
fn tool_root_search_matches_titles_and_subtitles_case_insensitively() {
    assert!(tool_row_matches_query(
        "Browse field values",
        Some("Browse unique field values from the current list."),
        "field",
    ));
    assert!(tool_row_matches_query(
        "Open logs",
        Some("Inspect recent app and command output."),
        "COMMAND OUTPUT",
    ));
    assert!(!tool_row_matches_query(
        "Documentation",
        Some("Open guides and reference."),
        "history",
    ));
    assert!(tool_row_matches_query("Documentation", None, ""));
}
