#[cfg(feature = "setup")]
use crate::setup::*;
use crate::preferences::Preferences;
#[cfg(feature = "flatpak")]
use crate::ripasso_keys::{rebuild_ripasso_private_keys_list, RipassoPrivateKeysState};
use crate::store_management::{rebuild_store_list, StoreRecipientsPageState};
use crate::window_navigation::set_save_button_for_password;
use crate::logging::log_error;
#[cfg(all(feature = "setup", not(feature = "flatpak")))]
use adw::ComboRow;
use adw::gio::SimpleAction;
#[cfg(feature = "setup")]
use adw::gio::{Menu, MenuItem};
use adw::prelude::*;
use adw::{ApplicationWindow, NavigationPage, NavigationView, Toast, ToastOverlay, WindowTitle};
#[cfg(not(feature = "flatpak"))]
use adw::EntryRow;
use adw::gtk::{Button, ListBox, TextView};
#[cfg(all(feature = "setup", not(feature = "flatpak")))]
use crate::preferences::BackendKind;

fn toast_preferences_save_error(overlay: &ToastOverlay, context: &str, err: &adw::glib::BoolError) {
    log_error(format!("Failed to save preference ({context}): {}", err.message));
    overlay.add_toast(Toast::new("Couldn't save that setting."));
}

#[derive(Clone)]
pub(crate) struct PreferencesActionState {
    pub(crate) window: ApplicationWindow,
    pub(crate) nav: NavigationView,
    pub(crate) page: NavigationPage,
    pub(crate) back: Button,
    pub(crate) add: Button,
    pub(crate) find: Button,
    pub(crate) git: Button,
    pub(crate) save: Button,
    pub(crate) win: WindowTitle,
    pub(crate) template_view: TextView,
    pub(crate) stores_list: ListBox,
    pub(crate) overlay: ToastOverlay,
    pub(crate) recipients_page: StoreRecipientsPageState,
    #[cfg(not(feature = "flatpak"))]
    pub(crate) pass_row: EntryRow,
    #[cfg(all(feature = "setup", not(feature = "flatpak")))]
    pub(crate) backend_row: ComboRow,
    #[cfg(feature = "flatpak")]
    pub(crate) ripasso_keys_state: RipassoPrivateKeysState,
}

#[cfg(all(feature = "setup", not(feature = "flatpak")))]
fn sync_backend_preferences_rows(
    backend_row: &ComboRow,
    pass_row: &EntryRow,
    preferences: &Preferences,
) {
    let backend = preferences.backend_kind();
    if backend_row.selected() != backend.combo_position() {
        backend_row.set_selected(backend.combo_position());
    }
    pass_row.set_visible(backend.uses_host_command());
}

#[cfg(all(feature = "setup", not(feature = "flatpak")))]
pub(crate) fn initialize_backend_row(
    backend_row: &ComboRow,
    pass_row: &EntryRow,
    preferences: &Preferences,
) {
    backend_row.set_model(Some(&adw::gtk::StringList::new(&[
        BackendKind::Integrated.label(),
        BackendKind::HostCommand.label(),
    ])));
    backend_row.set_visible(true);
    sync_backend_preferences_rows(backend_row, pass_row, preferences);
}

#[cfg(not(feature = "flatpak"))]
pub(crate) fn connect_pass_command_row(
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
            toast_preferences_save_error(&overlay, "host command", &err);
        }
    });
}

#[cfg(all(feature = "setup", not(feature = "flatpak")))]
pub(crate) fn connect_backend_row(
    backend_row: &ComboRow,
    pass_row: &EntryRow,
    overlay: &ToastOverlay,
    preferences: &Preferences,
) {
    let overlay = overlay.clone();
    let preferences = preferences.clone();
    let pass_row = pass_row.clone();
    backend_row.connect_selected_notify(move |row| {
        let selected_backend = BackendKind::from_combo_position(row.selected());
        let current_backend = preferences.backend_kind();
        if selected_backend == current_backend {
            pass_row.set_visible(selected_backend.uses_host_command());
            return;
        }

        if let Err(err) = preferences.set_backend_kind(selected_backend) {
            pass_row.set_visible(current_backend.uses_host_command());
            row.set_selected(current_backend.combo_position());
            toast_preferences_save_error(&overlay, "backend", &err);
            return;
        }

        pass_row.set_visible(selected_backend.uses_host_command());
    });
}

pub(crate) fn connect_new_password_template_autosave(
    template_view: &TextView,
    overlay: &ToastOverlay,
) {
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

pub(crate) fn register_open_preferences_action(
    window: &ApplicationWindow,
    state: &PreferencesActionState,
) {
    let state = state.clone();
    let action = SimpleAction::new("open-preferences", None);
    action.connect_activate(move |_, _| {
        state.add.set_visible(false);
        state.find.set_visible(false);
        state.git.set_visible(false);
        state.back.set_visible(true);
        state.save.set_visible(false);
        set_save_button_for_password(&state.save);
        state.win.set_title("Preferences");
        state.win.set_subtitle("Password Store");

        let already_visible = state
            .nav
            .visible_page()
            .as_ref()
            .map(|visible| visible == &state.page)
            .unwrap_or(false);
        if !already_visible {
            state.nav.push(&state.page);
        }

        let settings = Preferences::new();
        #[cfg(not(feature = "flatpak"))]
        state.pass_row.set_text(&settings.command_value());
        #[cfg(all(feature = "setup", not(feature = "flatpak")))]
        sync_backend_preferences_rows(&state.backend_row, &state.pass_row, &settings);
        state
            .template_view
            .buffer()
            .set_text(&settings.new_pass_file_template());
        #[cfg(feature = "flatpak")]
        rebuild_ripasso_private_keys_list(&state.ripasso_keys_state);
        rebuild_store_list(
            &state.stores_list,
            &settings,
            &state.window,
            &state.overlay,
            &state.recipients_page,
        );
    });
    window.add_action(&action);
}

#[cfg(feature = "setup")]
pub(crate) fn register_install_locally_action(
    window: &ApplicationWindow,
    menu: &Menu,
    overlay: &ToastOverlay,
) {
    let menu = menu.clone();
    let overlay = overlay.clone();
    let action = SimpleAction::new("install-locally", None);
    action.connect_activate(move |_, _| {
        if !can_install_locally() {
            overlay.add_toast(Toast::new("This app can't be installed here."));
            return;
        }
        let items = menu.n_items();
        if items > 0 {
            menu.remove(items - 1);
        }
        let installed = is_installed_locally();
        let ok = !installed && install_locally().is_ok();
        let uninstalled = installed && uninstall_locally().is_ok();
        let item = if ok || !uninstalled {
            MenuItem::new(Some("Uninstall this App"), Some("win.install-locally"))
        } else {
            MenuItem::new(Some("Install this App"), Some("win.install-locally"))
        };
        menu.append_item(&item);
    });
    window.add_action(&action);
}
