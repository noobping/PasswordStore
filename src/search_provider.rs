use crate::logging::{log_error, log_info};
#[cfg(feature = "hardening")]
use crate::password::model::{
    collect_all_password_items_with_options, CollectItemsOptions, PassEntry,
};
#[cfg(not(feature = "hardening"))]
use crate::password::{list::search_password_entries, model::PassEntry};
use crate::store::labels::shortened_store_labels;

use adw::gio::{self, BusNameOwnerFlags, BusType, DBusConnection, DBusInterfaceInfo, DBusNodeInfo};
use adw::glib::{self, ExitCode, MainLoop, Variant};
use adw::prelude::ToVariant;
#[cfg(feature = "hardening")]
use sha2::{Digest, Sha256};

use std::collections::HashMap;
use std::ffi::OsString;
use std::process::Command;
use std::rc::Rc;

const APP_ID: &str = env!("APP_ID");
const SEARCH_PROVIDER_BUS_NAME: &str = env!("SEARCH_PROVIDER_BUS_NAME");
const SEARCH_PROVIDER_OBJECT_PATH: &str = env!("SEARCH_PROVIDER_OBJECT_PATH");
const SEARCH_PROVIDER_INTERFACE: &str = "org.gnome.Shell.SearchProvider2";
const SEARCH_PROVIDER_RESULT_LIMIT: usize = 24;
const RESULT_ID_SEPARATOR: char = '\u{1f}';
const SEARCH_PROVIDER_XML: &str = r#"
<node>
  <interface name="org.gnome.Shell.SearchProvider2">
    <method name="GetInitialResultSet">
      <arg type="as" name="terms" direction="in" />
      <arg type="as" name="results" direction="out" />
    </method>
    <method name="GetSubsearchResultSet">
      <arg type="as" name="previous_results" direction="in" />
      <arg type="as" name="terms" direction="in" />
      <arg type="as" name="results" direction="out" />
    </method>
    <method name="GetResultMetas">
      <arg type="as" name="identifiers" direction="in" />
      <arg type="aa{sv}" name="metas" direction="out" />
    </method>
    <method name="ActivateResult">
      <arg type="s" name="identifier" direction="in" />
      <arg type="as" name="terms" direction="in" />
      <arg type="u" name="timestamp" direction="in" />
    </method>
    <method name="LaunchSearch">
      <arg type="as" name="terms" direction="in" />
      <arg type="u" name="timestamp" direction="in" />
    </method>
  </interface>
</node>
"#;

pub(crate) fn is_search_provider_command(args: &[OsString]) -> bool {
    args.get(1).is_some_and(|arg| arg == "--search-provider")
}

pub(crate) fn run() -> ExitCode {
    let node_info = match DBusNodeInfo::for_xml(SEARCH_PROVIDER_XML) {
        Ok(node_info) => node_info,
        Err(err) => {
            log_error(format!("Failed to parse search provider D-Bus XML: {err}"));
            return ExitCode::FAILURE;
        }
    };
    let Some(interface_info) = node_info.lookup_interface(SEARCH_PROVIDER_INTERFACE) else {
        log_error("Search provider interface metadata is missing.".to_string());
        return ExitCode::FAILURE;
    };

    let main_loop = MainLoop::new(None, false);
    let service = Rc::new(SearchProviderService::new(interface_info));
    let service_for_bus = service.clone();
    let loop_for_failure = main_loop.clone();
    let owner_id = gio::bus_own_name(
        BusType::Session,
        SEARCH_PROVIDER_BUS_NAME,
        BusNameOwnerFlags::NONE,
        move |connection, _name| {
            if let Err(err) = service_for_bus.register(&connection) {
                log_error(format!("Failed to export search provider object: {err}"));
                loop_for_failure.quit();
            }
        },
        |_connection, name| {
            log_info(format!("Search provider bus name acquired: {name}."));
        },
        {
            let main_loop = main_loop.clone();
            move |_connection, name| {
                log_info(format!("Search provider bus name released: {name}."));
                main_loop.quit();
            }
        },
    );

    log_info(format!(
        "Search provider listening on {SEARCH_PROVIDER_BUS_NAME}{SEARCH_PROVIDER_OBJECT_PATH}."
    ));
    main_loop.run();
    gio::bus_unown_name(owner_id);
    ExitCode::SUCCESS
}

struct SearchProviderService {
    interface_info: DBusInterfaceInfo,
}

impl SearchProviderService {
    fn new(interface_info: DBusInterfaceInfo) -> Self {
        Self { interface_info }
    }

    fn register(&self, connection: &DBusConnection) -> Result<(), glib::Error> {
        let _registration_id = connection
            .register_object(SEARCH_PROVIDER_OBJECT_PATH, &self.interface_info)
            .method_call(
                |_connection,
                 _sender,
                 _object_path,
                 _interface_name,
                 method_name,
                 parameters,
                 invocation| {
                    match method_name {
                        "GetInitialResultSet" => {
                            invocation.return_result(handle_get_initial_result_set(&parameters));
                        }
                        "GetSubsearchResultSet" => {
                            invocation.return_result(handle_get_subsearch_result_set(&parameters));
                        }
                        "GetResultMetas" => {
                            invocation.return_result(handle_get_result_metas(&parameters));
                        }
                        "ActivateResult" => {
                            invocation.return_result(handle_activate_result(&parameters));
                        }
                        "LaunchSearch" => {
                            invocation.return_result(handle_launch_search(&parameters));
                        }
                        _ => {
                            log_error(format!("Unknown search provider method: {method_name}."));
                            invocation.return_result(Ok(None));
                        }
                    }
                },
            )
            .build()?;

        Ok(())
    }
}

fn handle_get_initial_result_set(parameters: &Variant) -> Result<Option<Variant>, glib::Error> {
    let Some((terms,)) = parameters.get::<(Vec<String>,)>() else {
        log_error("Search provider GetInitialResultSet received invalid parameters.".to_string());
        return Ok(Some((Vec::<String>::new(),).to_variant()));
    };

    Ok(Some((search_result_ids(&terms),).to_variant()))
}

fn handle_get_subsearch_result_set(parameters: &Variant) -> Result<Option<Variant>, glib::Error> {
    let Some((_previous_results, terms)) = parameters.get::<(Vec<String>, Vec<String>)>() else {
        log_error("Search provider GetSubsearchResultSet received invalid parameters.".to_string());
        return Ok(Some((Vec::<String>::new(),).to_variant()));
    };

    Ok(Some((search_result_ids(&terms),).to_variant()))
}

fn handle_get_result_metas(parameters: &Variant) -> Result<Option<Variant>, glib::Error> {
    let Some((identifiers,)) = parameters.get::<(Vec<String>,)>() else {
        log_error("Search provider GetResultMetas received invalid parameters.".to_string());
        return Ok(Some((Vec::<HashMap<String, Variant>>::new(),).to_variant()));
    };

    let store_labels = store_label_map();
    let metas = identifiers
        .into_iter()
        .map(|identifier| {
            meta_for_identifier(&identifier, &store_labels)
                .unwrap_or_else(|| fallback_meta(&identifier))
        })
        .collect::<Vec<_>>();
    Ok(Some((metas,).to_variant()))
}

fn handle_activate_result(parameters: &Variant) -> Result<Option<Variant>, glib::Error> {
    let Some((identifier, terms, _timestamp)) = parameters.get::<(String, Vec<String>, u32)>()
    else {
        log_error("Search provider ActivateResult received invalid parameters.".to_string());
        return Ok(None);
    };

    match decode_result_target(&identifier) {
        Some((store_path, label)) => {
            if let Err(err) = launch_app(
                [
                    OsString::from("--open-entry"),
                    OsString::from(store_path),
                    OsString::from(label),
                ]
                .as_slice(),
            ) {
                log_error(format!("Failed to launch Keycord search result: {err}"));
                let query = join_search_terms(&terms);
                let _ = launch_search_query(&query);
            }
        }
        None => {
            log_error(format!(
                "Search provider received an invalid result identifier: {identifier:?}."
            ));
            let query = join_search_terms(&terms);
            let _ = launch_search_query(&query);
        }
    }

    Ok(None)
}

fn handle_launch_search(parameters: &Variant) -> Result<Option<Variant>, glib::Error> {
    let Some((terms, _timestamp)) = parameters.get::<(Vec<String>, u32)>() else {
        log_error("Search provider LaunchSearch received invalid parameters.".to_string());
        return Ok(None);
    };

    let query = join_search_terms(&terms);
    if let Err(err) = launch_search_query(&query) {
        log_error(format!("Failed to launch Keycord search window: {err}"));
    }
    Ok(None)
}

fn meta_for_identifier(
    identifier: &str,
    store_labels: &HashMap<String, String>,
) -> Option<HashMap<String, Variant>> {
    #[cfg(feature = "hardening")]
    let entry = decode_result_id(identifier)?;
    #[cfg(not(feature = "hardening"))]
    let entry = {
        let (store_path, label) = decode_result_id(identifier)?;
        PassEntry::from_label(store_path, label)
    };

    let mut meta = HashMap::new();
    meta.insert("id".to_string(), identifier.to_variant());
    meta.insert("name".to_string(), entry.basename.to_variant());
    let description = entry_description(&entry, store_labels);
    if !description.is_empty() {
        meta.insert("description".to_string(), description.to_variant());
    }
    meta.insert("gicon".to_string(), APP_ID.to_variant());
    Some(meta)
}

fn fallback_meta(identifier: &str) -> HashMap<String, Variant> {
    let mut meta = HashMap::new();
    meta.insert("id".to_string(), identifier.to_variant());
    #[cfg(feature = "hardening")]
    meta.insert("name".to_string(), "Password entry".to_variant());
    #[cfg(not(feature = "hardening"))]
    meta.insert("name".to_string(), identifier.to_variant());
    meta.insert("gicon".to_string(), APP_ID.to_variant());
    meta
}

#[cfg(feature = "hardening")]
fn entry_description(entry: &PassEntry, store_labels: &HashMap<String, String>) -> String {
    store_labels
        .get(&entry.store_path)
        .cloned()
        .unwrap_or_default()
}

#[cfg(not(feature = "hardening"))]
fn entry_description(entry: &PassEntry, store_labels: &HashMap<String, String>) -> String {
    let label = entry.label();
    if let Some(store_label) = store_labels.get(&entry.store_path) {
        format!("{store_label}: {label}")
    } else {
        label
    }
}

fn store_label_map() -> HashMap<String, String> {
    let stores = crate::preferences::Preferences::new().store_roots();
    let labels = shortened_store_labels(&stores);
    stores.into_iter().zip(labels).collect()
}

fn join_search_terms(terms: &[String]) -> String {
    terms
        .iter()
        .map(String::as_str)
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn launch_search_query(query: &str) -> Result<(), String> {
    if query.is_empty() {
        launch_app(&[])
    } else {
        launch_app([OsString::from(query)].as_slice())
    }
}

fn launch_app(args: &[OsString]) -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|err| format!("Failed to resolve current executable path: {err}"))?;
    Command::new(executable)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("Failed to spawn Keycord: {err}"))
}

fn decode_result_target(identifier: &str) -> Option<(String, String)> {
    #[cfg(feature = "hardening")]
    {
        let entry = decode_result_id(identifier)?;
        Some((entry.store_path.clone(), entry.label()))
    }

    #[cfg(not(feature = "hardening"))]
    {
        decode_result_id(identifier)
    }
}

#[cfg(feature = "hardening")]
fn encode_result_id(entry: &PassEntry) -> String {
    let mut digest = Sha256::new();
    digest.update(entry.store_path.as_bytes());
    digest.update([RESULT_ID_SEPARATOR as u8]);
    digest.update(entry.label().as_bytes());
    digest
        .finalize()
        .into_iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(not(feature = "hardening"))]
fn encode_result_id(entry: &PassEntry) -> String {
    format!("{}{RESULT_ID_SEPARATOR}{}", entry.store_path, entry.label())
}

#[cfg(feature = "hardening")]
fn decode_result_id(identifier: &str) -> Option<PassEntry> {
    if identifier.len() != 64 || !identifier.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    collect_all_password_items_with_options(CollectItemsOptions::default())
        .into_iter()
        .find(|entry| encode_result_id(entry) == identifier)
}

#[cfg(not(feature = "hardening"))]
fn decode_result_id(identifier: &str) -> Option<(String, String)> {
    let (store_path, label) = identifier.split_once(RESULT_ID_SEPARATOR)?;
    if store_path.is_empty() || label.is_empty() {
        return None;
    }

    Some((store_path.to_string(), label.to_string()))
}

#[cfg(feature = "hardening")]
fn search_provider_entries(terms: &[String], limit: usize) -> Vec<PassEntry> {
    let terms = normalized_search_terms(terms);
    if terms.is_empty() {
        return Vec::new();
    }

    let store_labels = store_label_map();
    let mut matches = Vec::new();
    for entry in collect_all_password_items_with_options(CollectItemsOptions::default()) {
        if !search_provider_entry_matches(
            &entry,
            store_labels.get(&entry.store_path).map(String::as_str),
            &terms,
        ) {
            continue;
        }

        matches.push(entry);
        if matches.len() >= limit {
            break;
        }
    }

    matches
}

#[cfg(feature = "hardening")]
fn normalized_search_terms(terms: &[String]) -> Vec<String> {
    terms
        .iter()
        .map(|term| term.trim().to_ascii_lowercase())
        .filter(|term| !term.is_empty())
        .collect()
}

#[cfg(feature = "hardening")]
fn search_provider_entry_matches(
    entry: &PassEntry,
    store_label: Option<&str>,
    terms: &[String],
) -> bool {
    let label = entry.label().to_ascii_lowercase();
    let store_label = store_label.unwrap_or_default().to_ascii_lowercase();
    terms
        .iter()
        .all(|term| label.contains(term) || store_label.contains(term))
}

#[cfg(feature = "hardening")]
fn search_result_ids(terms: &[String]) -> Vec<String> {
    search_provider_entries(terms, SEARCH_PROVIDER_RESULT_LIMIT)
        .into_iter()
        .map(|entry| encode_result_id(&entry))
        .collect()
}

#[cfg(not(feature = "hardening"))]
fn search_result_ids(terms: &[String]) -> Vec<String> {
    let query = join_search_terms(terms);
    search_password_entries(&query, Some(SEARCH_PROVIDER_RESULT_LIMIT))
        .into_iter()
        .map(|entry| encode_result_id(&entry))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        decode_result_id, encode_result_id, join_search_terms, normalized_search_terms,
        search_provider_entry_matches,
    };
    use crate::password::model::PassEntry;

    #[cfg(feature = "hardening")]
    #[test]
    fn result_ids_are_opaque_hashes() {
        let entry = PassEntry::from_label("/tmp/store", "work/alice/github");
        let identifier = encode_result_id(&entry);

        assert_eq!(identifier.len(), 64);
        assert!(identifier.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!identifier.contains("/tmp/store"));
        assert!(!identifier.contains("work/alice/github"));
        assert_eq!(decode_result_id(&identifier), None);
    }

    #[cfg(not(feature = "hardening"))]
    #[test]
    fn result_ids_round_trip_without_hardening() {
        let entry = PassEntry::from_label("/tmp/store", "work/alice/github");
        let identifier = encode_result_id(&entry);

        assert_eq!(
            decode_result_id(&identifier),
            Some(("/tmp/store".to_string(), "work/alice/github".to_string()))
        );
    }

    #[test]
    fn invalid_result_ids_are_rejected() {
        assert_eq!(decode_result_id(""), None);
        assert_eq!(decode_result_id("/tmp/store"), None);
        #[cfg(feature = "hardening")]
        assert_eq!(decode_result_id("xyz"), None);
    }

    #[test]
    fn search_terms_join_with_spaces() {
        assert_eq!(
            join_search_terms(&[
                "find".to_string(),
                "otp".to_string(),
                "".to_string(),
                "and".to_string(),
                "user".to_string(),
                "alice".to_string(),
            ]),
            "find otp and user alice".to_string()
        );
    }

    #[cfg(feature = "hardening")]
    #[test]
    fn search_terms_normalization_drops_empty_values() {
        assert_eq!(
            normalized_search_terms(&["  Alice ".to_string(), "".to_string(), "Work".to_string()]),
            vec!["alice".to_string(), "work".to_string()]
        );
    }

    #[cfg(feature = "hardening")]
    #[test]
    fn shell_search_matches_labels_and_store_labels_only() {
        let entry = PassEntry::from_label("/tmp/store", "work/alice/github");

        assert!(search_provider_entry_matches(
            &entry,
            Some("Work"),
            &["alice".to_string(), "work".to_string()]
        ));
        assert!(!search_provider_entry_matches(
            &entry,
            Some("Work"),
            &["example.com".to_string()]
        ));
    }
}
