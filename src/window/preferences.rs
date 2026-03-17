use crate::logging::log_error;
use crate::password::generation::{PasswordGenerationControls, PasswordGenerationSettings};
use crate::preferences::{BackendKind, Preferences, UsernameFallbackMode};
use crate::private_key::sync::{
    preflight_host_to_app_private_key_sync, sync_private_keys_with_host, PrivateKeySyncDirection,
};
use crate::store::management::{rebuild_store_list, StoreRecipientsPageState};
use crate::support::actions::activate_widget_action;
use crate::support::actions::register_window_action;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::support::runtime::has_host_permission;
use crate::support::ui::push_navigation_page_if_needed;
use crate::window::navigation::{
    show_secondary_page_chrome, HasWindowChrome, WindowPageState, APP_WINDOW_TITLE,
};
use adw::gtk::{CheckButton, ListBox, TextView};
use adw::prelude::*;
use adw::{ActionRow, AlertDialog, ComboRow, EntryRow};
use adw::{Toast, ToastOverlay};
use std::cell::Cell;
use std::rc::Rc;

fn sync_backend_preferences_rows(
    backend_row: &ComboRow,
    pass_row: &EntryRow,
    sync_row: &ActionRow,
    sync_check: &CheckButton,
    preferences: &Preferences,
) {
    let backend = preferences.backend_kind();
    if backend_row.selected() != backend.combo_position() {
        backend_row.set_selected(backend.combo_position());
    }
    pass_row.set_visible(preferences.uses_host_command_backend());
    sync_private_key_sync_row(sync_row, sync_check, preferences);
}

fn backend_row_model() -> adw::gtk::StringList {
    adw::gtk::StringList::new(&[
        BackendKind::Integrated.label(),
        BackendKind::HostCommand.label(),
    ])
}

pub fn initialize_backend_row(
    backend_row: &ComboRow,
    pass_row: &EntryRow,
    sync_row: &ActionRow,
    sync_check: &CheckButton,
    preferences: &Preferences,
) {
    let model = backend_row_model();
    backend_row.set_model(Some(&model));
    backend_row.set_visible(true);
    sync_backend_preferences_rows(backend_row, pass_row, sync_row, sync_check, preferences);
}

#[cfg(target_os = "linux")]
const SYNC_PRIVATE_KEYS_AVAILABLE_SUBTITLE: &str =
    "Keep Keycord's private keys and your computer's GPG private keys in step when host access is available.";

#[cfg(target_os = "linux")]
const SYNC_PRIVATE_KEYS_UNAVAILABLE_SUBTITLE: &str =
    "Grant host access first if you want Keycord to keep its private keys in step with your computer's GPG private keys.";

#[cfg(not(target_os = "linux"))]
const SYNC_PRIVATE_KEYS_AVAILABLE_SUBTITLE: &str =
    "Private-key sync with the host is only available on Linux.";

fn host_private_key_sync_is_available() -> bool {
    #[cfg(all(target_os = "linux", feature = "flatpak"))]
    {
        has_host_permission()
    }

    #[cfg(all(target_os = "linux", not(feature = "flatpak")))]
    {
        true
    }

    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

fn sync_private_key_sync_row(row: &ActionRow, check: &CheckButton, preferences: &Preferences) {
    let supported = cfg!(target_os = "linux");
    row.set_visible(supported);
    if !supported {
        return;
    }

    let available = host_private_key_sync_is_available();
    let enabled = preferences.sync_private_keys_with_host();
    row.set_sensitive(available);
    check.set_sensitive(available);
    row.set_subtitle(if available {
        SYNC_PRIVATE_KEYS_AVAILABLE_SUBTITLE
    } else {
        SYNC_PRIVATE_KEYS_UNAVAILABLE_SUBTITLE
    });

    if check.is_active() != enabled {
        check.set_active(enabled);
    }
}

fn present_private_key_sync_confirmation(
    window: &adw::ApplicationWindow,
    on_response: impl FnOnce(bool) + 'static,
) {
    let dialog = AlertDialog::builder()
        .heading("Turn on private-key sync?")
        .body("Keycord will first make its private-key list match the GPG private keys on your computer. After that, creating, importing, or deleting a private key in Keycord will update the host too. Keys that only exist in Keycord may be removed during the first sync.")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("sync", "Turn On")]);
    dialog.set_close_response("cancel");
    dialog.set_default_response(Some("sync"));
    dialog.choose(window, None::<&adw::gio::Cancellable>, move |response| {
        on_response(response == "sync");
    });
}

pub fn connect_pass_command_row(
    pass_row: &EntryRow,
    overlay: &ToastOverlay,
    preferences: &Preferences,
) {
    let overlay = overlay.clone();
    let preferences = preferences.clone();
    pass_row.connect_apply(move |row| {
        let text = row.text().to_string();
        let text = text.trim();
        if text.is_empty() {
            overlay.add_toast(Toast::new("Enter a command."));
            return;
        }
        if let Err(err) = preferences.set_command(text) {
            toast_preferences_save_error(&overlay, "host", &err);
        }
    });
}

pub fn connect_backend_row(
    backend_row: &ComboRow,
    pass_row: &EntryRow,
    overlay: &ToastOverlay,
    preferences: &Preferences,
    on_changed: impl Fn() + 'static,
) {
    let overlay = overlay.clone();
    let preferences = preferences.clone();
    let pass_row = pass_row.clone();
    let on_changed = Rc::new(on_changed);
    backend_row.connect_selected_notify(move |row| {
        let selected_backend = BackendKind::from_combo_position(row.selected());
        let current_backend = preferences.backend_kind();
        if selected_backend == current_backend {
            pass_row.set_visible(preferences.uses_host_command_backend());
            return;
        }

        if let Err(err) = preferences.set_backend_kind(selected_backend) {
            pass_row.set_visible(preferences.uses_host_command_backend());
            row.set_selected(current_backend.combo_position());
            toast_preferences_save_error(&overlay, "backend", &err);
            return;
        }

        pass_row.set_visible(preferences.uses_host_command_backend());
        on_changed();
    });
}

pub fn connect_private_key_sync_row(state: &PreferencesActionState) {
    let row = state.sync_private_keys_row.clone();
    let check = state.sync_private_keys_check.clone();
    let check_for_row = check.clone();
    row.connect_activated(move |_| {
        if !check_for_row.is_sensitive() {
            return;
        }
        check_for_row.set_active(!check_for_row.is_active());
    });

    let overlay = state.overlay.clone();
    let window = state.page_state.window.clone();
    let preferences = Preferences::new();
    let syncing = Rc::new(Cell::new(false));
    check.connect_toggled(move |button| {
        if syncing.get() {
            return;
        }

        let desired = button.is_active();
        let stored = preferences.sync_private_keys_with_host();
        if desired == stored {
            return;
        }

        if desired {
            if !host_private_key_sync_is_available() {
                syncing.set(true);
                button.set_active(false);
                syncing.set(false);
                overlay.add_toast(Toast::new("Grant host access first."));
                return;
            }

            let confirm_button = button.clone();
            let confirm_overlay = overlay.clone();
            let confirm_preferences = preferences.clone();
            let confirm_syncing = syncing.clone();
            let confirm_window = window.clone();
            present_private_key_sync_confirmation(&window, move |confirmed| {
                if !confirmed {
                    confirm_syncing.set(true);
                    confirm_button.set_active(false);
                    confirm_syncing.set(false);
                    return;
                }

                match preflight_host_to_app_private_key_sync()
                    .and_then(|_| sync_private_keys_with_host(PrivateKeySyncDirection::HostToApp))
                {
                    Ok(()) => {
                        if let Err(err) = confirm_preferences.set_sync_private_keys_with_host(true)
                        {
                            toast_preferences_save_error(
                                &confirm_overlay,
                                "private-key sync",
                                &err,
                            );
                            confirm_syncing.set(true);
                            confirm_button.set_active(false);
                            confirm_syncing.set(false);
                            return;
                        }

                        activate_widget_action(&confirm_window, "win.reload-store-recipients-list");
                        activate_widget_action(&confirm_window, "win.reload-password-list");
                        confirm_overlay.add_toast(Toast::new("Private keys synced."));
                    }
                    Err(err) => {
                        log_error(format!("Failed to enable private-key sync: {err}"));
                        confirm_syncing.set(true);
                        confirm_button.set_active(false);
                        confirm_syncing.set(false);
                        confirm_overlay.add_toast(Toast::new("Couldn't sync private keys."));
                    }
                }
            });
            return;
        }

        if let Err(err) = preferences.set_sync_private_keys_with_host(false) {
            toast_preferences_save_error(&overlay, "private-key sync", &err);
            syncing.set(true);
            button.set_active(true);
            syncing.set(false);
        }
    });
}

fn refresh_open_preferences_state(state: &PreferencesActionState, settings: &Preferences) {
    state.pass_row.set_text(&settings.command_value());
    sync_backend_preferences_rows(
        &state.backend_row,
        &state.pass_row,
        &state.sync_private_keys_row,
        &state.sync_private_keys_check,
        settings,
    );
}

pub(super) fn toast_preferences_save_error(
    overlay: &ToastOverlay,
    context: &str,
    err: &adw::glib::BoolError,
) {
    log_error(format!(
        "Failed to save preference ({context}): {}",
        err.message
    ));
    overlay.add_toast(Toast::new("Couldn't save that setting."));
}

#[derive(Clone)]
pub struct PreferencesActionState {
    pub page_state: WindowPageState,
    pub template_view: TextView,
    pub username_folder_check: CheckButton,
    pub username_filename_check: CheckButton,
    pub generator_controls: PasswordGenerationControls,
    pub stores_list: ListBox,
    pub store_actions_list: ListBox,
    pub overlay: ToastOverlay,
    pub recipients_page: StoreRecipientsPageState,
    pub pass_row: EntryRow,
    pub backend_row: ComboRow,
    pub sync_private_keys_row: ActionRow,
    pub sync_private_keys_check: CheckButton,
}

pub fn connect_new_password_template_autosave(template_view: &TextView, overlay: &ToastOverlay) {
    let overlay = overlay.clone();
    let preferences = Preferences::new();
    let buffer = template_view.buffer();
    buffer.connect_changed(move |buffer| {
        let (start, end) = buffer.bounds();
        let template = buffer.text(&start, &end, false).to_string();
        if template == preferences.new_pass_file_template() {
            return;
        }
        if let Err(err) = preferences.set_new_pass_file_template(&template) {
            toast_preferences_save_error(&overlay, "new item template", &err);
        }
    });
}

fn sync_username_fallback_checks(
    folder_check: &CheckButton,
    filename_check: &CheckButton,
    mode: UsernameFallbackMode,
) {
    let (folder_active, filename_active) = username_fallback_check_state(mode);
    folder_check.set_active(folder_active);
    filename_check.set_active(filename_active);
}

const fn username_fallback_check_state(mode: UsernameFallbackMode) -> (bool, bool) {
    match mode {
        UsernameFallbackMode::Folder => (true, false),
        UsernameFallbackMode::Filename => (false, true),
    }
}

pub fn connect_username_fallback_autosave(
    folder_check: &CheckButton,
    filename_check: &CheckButton,
    overlay: &ToastOverlay,
) {
    let preferences = Preferences::new();
    sync_username_fallback_checks(
        folder_check,
        filename_check,
        preferences.username_fallback_mode(),
    );

    let syncing = Rc::new(Cell::new(false));
    for (button, mode) in [
        (folder_check.clone(), UsernameFallbackMode::Folder),
        (filename_check.clone(), UsernameFallbackMode::Filename),
    ] {
        let folder_check = folder_check.clone();
        let filename_check = filename_check.clone();
        let overlay = overlay.clone();
        let preferences = preferences.clone();
        let syncing = syncing.clone();
        button.connect_toggled(move |button| {
            if syncing.get() || !button.is_active() {
                return;
            }

            let stored = preferences.username_fallback_mode();
            if stored == mode {
                return;
            }

            syncing.set(true);
            if let Err(err) = preferences.set_username_fallback_mode(mode) {
                toast_preferences_save_error(&overlay, "username fallback", &err);
                sync_username_fallback_checks(&folder_check, &filename_check, stored);
            } else {
                sync_username_fallback_checks(&folder_check, &filename_check, mode);
            }
            syncing.set(false);
        });
    }
}

pub fn connect_password_generation_autosave(
    controls: &PasswordGenerationControls,
    mirrors: &[PasswordGenerationControls],
    overlay: &ToastOverlay,
) {
    let preferences = Preferences::new();
    let initial_settings = preferences.password_generation_settings();
    sync_password_generation_controls(controls, &initial_settings);
    for mirror in mirrors {
        sync_password_generation_controls(mirror, &initial_settings);
    }

    let controls = controls.clone();
    let changed_controls = controls.clone();
    let mirrors = mirrors.to_vec();
    let overlay = overlay.clone();
    let syncing = Rc::new(Cell::new(false));
    let changed: Rc<dyn Fn()> = Rc::new({
        move || {
            if syncing.get() {
                return;
            }

            syncing.set(true);
            let stored = preferences.password_generation_settings();
            let updated = changed_controls.settings().normalized();
            let save_result = preferences.set_password_generation_settings(&updated);
            match save_result {
                Ok(()) => {
                    sync_password_generation_controls(&changed_controls, &updated);
                    for mirror in &mirrors {
                        sync_password_generation_controls(mirror, &updated);
                    }
                }
                Err(err) => {
                    toast_preferences_save_error(&overlay, "password generation", &err);
                    sync_password_generation_controls(&changed_controls, &stored);
                    for mirror in &mirrors {
                        sync_password_generation_controls(mirror, &stored);
                    }
                }
            }
            syncing.set(false);
        }
    });
    controls.connect_changed(&changed);
}

pub fn sync_password_generation_controls(
    controls: &PasswordGenerationControls,
    settings: &PasswordGenerationSettings,
) {
    controls.set_settings(settings);
}

pub fn register_open_preferences_action(
    window: &adw::ApplicationWindow,
    state: &PreferencesActionState,
) {
    let state = state.clone();
    register_window_action(window, "open-preferences", move || {
        let chrome = state.page_state.window_chrome();
        show_secondary_page_chrome(&chrome, "Preferences", APP_WINDOW_TITLE, false);

        push_navigation_page_if_needed(&state.page_state.nav, &state.page_state.page);

        let settings = Preferences::new();
        refresh_open_preferences_state(&state, &settings);
        sync_username_fallback_checks(
            &state.username_folder_check,
            &state.username_filename_check,
            settings.username_fallback_mode(),
        );
        sync_password_generation_controls(
            &state.generator_controls,
            &settings.password_generation_settings(),
        );
        state
            .template_view
            .buffer()
            .set_text(&settings.new_pass_file_template());
        rebuild_store_list(
            &state.stores_list,
            &state.store_actions_list,
            &settings,
            &state.page_state.window,
            &state.overlay,
            &state.recipients_page,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::username_fallback_check_state;
    use crate::preferences::UsernameFallbackMode;

    #[test]
    fn username_fallback_sync_marks_only_the_selected_mode() {
        assert_eq!(
            username_fallback_check_state(UsernameFallbackMode::Folder),
            (true, false)
        );
        assert_eq!(
            username_fallback_check_state(UsernameFallbackMode::Filename),
            (false, true)
        );
    }
}
