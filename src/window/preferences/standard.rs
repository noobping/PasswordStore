use super::{toast_preferences_save_error, PreferencesActionState};
use crate::preferences::{BackendKind, Preferences};
use adw::prelude::*;
use adw::{ComboRow, EntryRow, Toast, ToastOverlay};
use std::rc::Rc;

fn backend_pass_row_visible(backend: BackendKind) -> bool {
    backend.uses_host_command()
}

fn sync_backend_preferences_rows(
    backend_row: &ComboRow,
    pass_row: &EntryRow,
    preferences: &Preferences,
) {
    let backend = preferences.backend_kind();
    if backend_row.selected() != backend.combo_position() {
        backend_row.set_selected(backend.combo_position());
    }
    pass_row.set_visible(backend_pass_row_visible(backend));
}

fn backend_row_model() -> adw::gtk::StringList {
    adw::gtk::StringList::new(&[
        BackendKind::Integrated.label(),
        BackendKind::HostCommand.label(),
    ])
}

pub(crate) fn initialize_backend_row(
    backend_row: &ComboRow,
    pass_row: &EntryRow,
    preferences: &Preferences,
) {
    let model = backend_row_model();
    backend_row.set_model(Some(&model));
    backend_row.set_visible(true);
    sync_backend_preferences_rows(backend_row, pass_row, preferences);
}

pub(crate) fn connect_pass_command_row<F>(
    pass_row: &EntryRow,
    overlay: &ToastOverlay,
    preferences: &Preferences,
    on_changed: F,
) where
    F: Fn() + 'static,
{
    let overlay = overlay.clone();
    let preferences = preferences.clone();
    let on_changed = Rc::new(on_changed);
    pass_row.connect_apply(move |row| {
        let text = row.text().to_string();
        let text = text.trim();
        if text.is_empty() {
            overlay.add_toast(Toast::new("Enter a command."));
            return;
        }
        if let Err(err) = preferences.set_command(text) {
            toast_preferences_save_error(&overlay, "host command", &err);
        } else {
            on_changed();
        }
    });
}

pub(crate) fn connect_backend_row<F>(
    backend_row: &ComboRow,
    pass_row: &EntryRow,
    overlay: &ToastOverlay,
    preferences: &Preferences,
    on_changed: F,
) where
    F: Fn() + 'static,
{
    let overlay = overlay.clone();
    let preferences = preferences.clone();
    let pass_row = pass_row.clone();
    let on_changed = Rc::new(on_changed);
    backend_row.connect_selected_notify(move |row| {
        let selected_backend = BackendKind::from_combo_position(row.selected());
        let current_backend = preferences.backend_kind();
        if selected_backend == current_backend {
            pass_row.set_visible(backend_pass_row_visible(selected_backend));
            return;
        }

        if let Err(err) = preferences.set_backend_kind(selected_backend) {
            pass_row.set_visible(backend_pass_row_visible(current_backend));
            row.set_selected(current_backend.combo_position());
            toast_preferences_save_error(&overlay, "backend", &err);
            return;
        }

        pass_row.set_visible(backend_pass_row_visible(selected_backend));
        on_changed();
    });
}

pub(super) fn refresh_open_preferences_state(
    state: &PreferencesActionState,
    settings: &Preferences,
) {
    state.pass_row.set_text(&settings.command_value());
    sync_backend_preferences_rows(&state.backend_row, &state.pass_row, settings);
}

#[cfg(test)]
mod tests {
    use super::backend_pass_row_visible;
    use crate::preferences::BackendKind;

    #[test]
    fn host_command_backend_shows_the_pass_command_row() {
        assert!(backend_pass_row_visible(BackendKind::HostCommand));
    }

    #[test]
    fn integrated_backend_hides_the_pass_command_row() {
        assert!(!backend_pass_row_visible(BackendKind::Integrated));
    }
}
