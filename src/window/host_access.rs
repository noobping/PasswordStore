#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::clipboard::set_clipboard_text;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::support::runtime::has_host_permission;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::support::ui::flat_icon_button_with_tooltip;
use adw::prelude::*;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use adw::{ActionRow, PreferencesGroup, Toast, ToastOverlay};
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use std::rc::Rc;

#[cfg(all(target_os = "linux", feature = "flatpak"))]
const FLATPAK_HOST_OVERRIDE_COMMAND: &str =
    "flatpak override --user --talk-name=org.freedesktop.Flatpak io.github.noobping.keycord";

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn build_optional_host_access_row(overlay: &ToastOverlay) -> Option<ActionRow> {
    if has_host_permission() {
        return None;
    }

    let row = ActionRow::builder()
        .title("Allow access to apps on this device")
        .subtitle("Keycord is running in a protected space, so some optional features stay off until you allow this. If you allow it, Keycord can use tools from your computer such as GPG, the Host backend, and pass import. If you don't, Keycord still works with the integrated backend.")
        .build();
    row.set_activatable(false);

    let button = flat_icon_button_with_tooltip("edit-copy-symbolic", "Copy permission command");
    row.add_suffix(&button);

    let overlay = overlay.clone();
    let feedback_button = button.clone();
    let copy_action = Rc::new(move || {
        if set_clipboard_text(
            FLATPAK_HOST_OVERRIDE_COMMAND,
            &overlay,
            Some(&feedback_button),
        ) {
            overlay.add_toast(Toast::new("Copied."));
        }
    });

    button.connect_clicked(move |_| copy_action());
    Some(row)
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
pub fn append_optional_host_access_group_row(group: &PreferencesGroup, overlay: &ToastOverlay) {
    group.set_visible(false);
    if let Some(row) = build_optional_host_access_row(overlay) {
        group.add(&row);
        group.set_visible(true);
    }
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
pub fn append_optional_host_access_group_row(
    group: &adw::PreferencesGroup,
    _overlay: &adw::ToastOverlay,
) {
    group.set_visible(false);
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
pub fn append_optional_host_access_list_row(list: &adw::gtk::ListBox, overlay: &ToastOverlay) {
    if let Some(row) = build_optional_host_access_row(overlay) {
        list.append(&row);
    }
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
pub fn append_optional_host_access_list_row(
    _list: &adw::gtk::ListBox,
    _overlay: &adw::ToastOverlay,
) {
}
