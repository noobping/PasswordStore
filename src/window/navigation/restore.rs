use super::chrome::{
    set_save_button_for_password, show_primary_page_chrome, show_secondary_page_chrome,
    APP_WINDOW_SUBTITLE, APP_WINDOW_TITLE,
};
use super::state::{HasWindowChrome, WindowNavigationState};
use crate::password::file::sync_username_row;
use crate::password::opened::get_opened_pass_file;
use crate::preferences::Preferences;
use crate::store::management::{sync_store_recipients_page_header, StoreRecipientsPageState};
use crate::support::ui::{navigation_stack_is_root, visible_navigation_page_is};
use adw::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestoredPageKind {
    Root,
    Text,
    Raw,
    Settings,
    Tools,
    ToolFieldValues,
    ToolValueValues,
    Recipients,
    Log,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RestoredPageState {
    at_root: bool,
    current_page: Option<RestoredPageKind>,
}

const fn restored_page_kind(state: RestoredPageState) -> RestoredPageKind {
    if state.at_root {
        return RestoredPageKind::Root;
    }

    match state.current_page {
        Some(page_kind) => page_kind,
        None => RestoredPageKind::Other,
    }
}

pub fn restore_window_for_current_page(
    state: &WindowNavigationState,
    recipients_page: &StoreRecipientsPageState,
) -> bool {
    let chrome = state.window_chrome();
    if visible_store_import_page(state) {
        show_secondary_page_chrome(
            &chrome,
            "Import passwords",
            "Use pass import to import into an existing store.",
            false,
        );
        return false;
    }

    if visible_private_key_generation_page(state, recipients_page) {
        show_secondary_page_chrome(
            &chrome,
            "Generate private key",
            "Create a password-protected private key for password stores.",
            false,
        );
        return false;
    }

    let page_kind = restored_page_kind(RestoredPageState {
        at_root: navigation_stack_is_root(&state.nav),
        current_page: visible_secondary_page_kind(state, recipients_page),
    });

    if page_kind == RestoredPageKind::Root {
        show_primary_page_chrome(&chrome, !Preferences::new().stores().is_empty());
        return true;
    }

    state.save.set_visible(matches!(
        page_kind,
        RestoredPageKind::Text | RestoredPageKind::Raw
    ));
    if page_kind == RestoredPageKind::Text {
        if let Some(pass_file) = get_opened_pass_file() {
            let label = pass_file.label();
            show_secondary_page_chrome(&chrome, pass_file.title(), &label, true);
            state.raw.set_visible(true);
            sync_username_row(&state.username, Some(&pass_file));
        } else {
            show_secondary_page_chrome(&chrome, APP_WINDOW_TITLE, APP_WINDOW_SUBTITLE, true);
            sync_username_row(&state.username, None);
        }
    } else if page_kind == RestoredPageKind::Raw {
        let subtitle = get_opened_pass_file().map_or_else(
            || APP_WINDOW_TITLE.to_string(),
            |pass_file| pass_file.label(),
        );
        show_secondary_page_chrome(&chrome, "Raw Pass File", &subtitle, true);
    } else if page_kind == RestoredPageKind::Settings {
        show_secondary_page_chrome(&chrome, "Preferences", APP_WINDOW_TITLE, false);
    } else if page_kind == RestoredPageKind::Tools {
        show_secondary_page_chrome(&chrome, "Tools", "Utilities and maintenance", false);
    } else if page_kind == RestoredPageKind::ToolFieldValues {
        show_secondary_page_chrome(
            &chrome,
            "Browse field values",
            "Pick a field from the current list.",
            false,
        );
    } else if page_kind == RestoredPageKind::ToolValueValues {
        show_secondary_page_chrome(
            &chrome,
            "Browse field values",
            "Pick a value from the current list.",
            false,
        );
    } else if page_kind == RestoredPageKind::Recipients {
        set_save_button_for_password(&state.save);
        sync_store_recipients_page_header(recipients_page);
    } else if page_kind == RestoredPageKind::Log {
        show_secondary_page_chrome(&chrome, "Logs", "Details", false);
    }

    false
}

fn visible_secondary_page_kind(
    state: &WindowNavigationState,
    recipients_page: &StoreRecipientsPageState,
) -> Option<RestoredPageKind> {
    if visible_navigation_page_is(&state.nav, &state.text_page) {
        return Some(RestoredPageKind::Text);
    }
    if visible_navigation_page_is(&state.nav, &state.raw_text_page) {
        return Some(RestoredPageKind::Raw);
    }
    if visible_navigation_page_is(&state.nav, &state.settings_page) {
        return Some(RestoredPageKind::Settings);
    }
    if visible_navigation_page_is(&state.nav, &state.tools_page) {
        return Some(RestoredPageKind::Tools);
    }
    if visible_navigation_page_is(&state.nav, &state.tools_field_values_page) {
        return Some(RestoredPageKind::ToolFieldValues);
    }
    if visible_navigation_page_is(&state.nav, &state.tools_value_values_page) {
        return Some(RestoredPageKind::ToolValueValues);
    }
    if visible_navigation_page_is(&state.nav, &recipients_page.page) {
        return Some(RestoredPageKind::Recipients);
    }
    if visible_log_page(state) {
        return Some(RestoredPageKind::Log);
    }

    None
}

fn visible_log_page(state: &WindowNavigationState) -> bool {
    visible_navigation_page_is(&state.nav, &state.log_page)
}

fn visible_store_import_page(state: &WindowNavigationState) -> bool {
    visible_navigation_page_is(&state.nav, &state.store_import_page)
}

fn visible_private_key_generation_page(
    state: &WindowNavigationState,
    recipients_page: &StoreRecipientsPageState,
) -> bool {
    visible_navigation_page_is(
        &state.nav,
        &recipients_page.platform.private_key_generation_page,
    )
}

#[cfg(test)]
mod tests {
    use super::{restored_page_kind, RestoredPageKind, RestoredPageState};

    #[test]
    fn restored_page_kind_prefers_root_before_any_other_page() {
        assert_eq!(
            restored_page_kind(RestoredPageState {
                at_root: true,
                current_page: Some(RestoredPageKind::Text),
            }),
            RestoredPageKind::Root
        );
    }

    #[test]
    fn restored_page_kind_matches_each_known_page() {
        assert_eq!(
            restored_page_kind(RestoredPageState {
                at_root: false,
                current_page: Some(RestoredPageKind::Text),
            }),
            RestoredPageKind::Text
        );
        assert_eq!(
            restored_page_kind(RestoredPageState {
                at_root: false,
                current_page: Some(RestoredPageKind::Raw),
            }),
            RestoredPageKind::Raw
        );
        assert_eq!(
            restored_page_kind(RestoredPageState {
                at_root: false,
                current_page: Some(RestoredPageKind::Settings),
            }),
            RestoredPageKind::Settings
        );
        assert_eq!(
            restored_page_kind(RestoredPageState {
                at_root: false,
                current_page: Some(RestoredPageKind::Tools),
            }),
            RestoredPageKind::Tools
        );
        assert_eq!(
            restored_page_kind(RestoredPageState {
                at_root: false,
                current_page: Some(RestoredPageKind::ToolFieldValues),
            }),
            RestoredPageKind::ToolFieldValues
        );
        assert_eq!(
            restored_page_kind(RestoredPageState {
                at_root: false,
                current_page: Some(RestoredPageKind::ToolValueValues),
            }),
            RestoredPageKind::ToolValueValues
        );
        assert_eq!(
            restored_page_kind(RestoredPageState {
                at_root: false,
                current_page: Some(RestoredPageKind::Recipients),
            }),
            RestoredPageKind::Recipients
        );
        assert_eq!(
            restored_page_kind(RestoredPageState {
                at_root: false,
                current_page: Some(RestoredPageKind::Log),
            }),
            RestoredPageKind::Log
        );
    }

    #[test]
    fn restored_page_kind_falls_back_to_other_when_nothing_matches() {
        assert_eq!(
            restored_page_kind(RestoredPageState {
                at_root: false,
                current_page: None,
            }),
            RestoredPageKind::Other
        );
    }
}
