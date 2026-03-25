#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::clipboard::set_clipboard_text;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::support::runtime::{has_host_permission, has_smartcard_permission};
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::support::ui::flat_icon_button_with_tooltip;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use adw::gtk::ListBox;
use adw::prelude::*;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use adw::{ActionRow, PreferencesGroup, Toast, ToastOverlay};
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use std::rc::Rc;

#[cfg(all(target_os = "linux", feature = "flatpak"))]
const FLATPAK_HOST_OVERRIDE_COMMAND: &str =
    "flatpak override --user --talk-name=org.freedesktop.Flatpak io.github.noobping.keycord";

#[cfg(all(target_os = "linux", feature = "flatpak"))]
const FLATPAK_SMARTCARD_OVERRIDE_COMMAND: &str =
    "flatpak override --user --socket=pcsc io.github.noobping.keycord";

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn build_optional_permission_row(
    overlay: &ToastOverlay,
    title: &str,
    subtitle: &str,
    command: &'static str,
) -> ActionRow {
    let row = ActionRow::builder().title(title).subtitle(subtitle).build();
    row.set_activatable(false);

    let button = flat_icon_button_with_tooltip("edit-copy-symbolic", "Copy permission command");
    row.add_suffix(&button);

    let overlay = overlay.clone();
    let feedback_button = button.clone();
    let copy_action = Rc::new(move || {
        if set_clipboard_text(command, &overlay, Some(&feedback_button)) {
            overlay.add_toast(Toast::new("Copied."));
        }
    });

    button.connect_clicked(move |_| copy_action());
    row
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn build_optional_host_access_row(overlay: &ToastOverlay) -> Option<ActionRow> {
    if has_host_permission() {
        return None;
    }

    Some(build_optional_permission_row(
        overlay,
        "Allow access to apps on this device",
        "Keycord is running in a protected space, so some optional features stay off until you allow this. If you allow it, Keycord can use tools from your computer such as GPG, the Host backend, and pass import. If you don't, Keycord still works with the integrated backend.",
        FLATPAK_HOST_OVERRIDE_COMMAND,
    ))
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
pub fn append_optional_host_access_group_row(group: &PreferencesGroup, overlay: &ToastOverlay) {
    group.set_visible(false);
    if let Some(row) = build_optional_host_access_row(overlay) {
        group.add(&row);
        group.set_visible(true);
    }
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
pub fn append_optional_smartcard_access_row(
    list: &ListBox,
    overlay: &ToastOverlay,
    hardware_rows: &[&ActionRow],
) {
    let granted = has_smartcard_permission();
    for row in hardware_rows {
        row.set_sensitive(granted);
        row.set_tooltip_text((!granted).then_some("Grant smartcard access first."));
    }

    if granted {
        return;
    }

    let row = build_optional_permission_row(
        overlay,
        "Allow smartcard access",
        "Hardware keys are optional. Grant PC/SC access if you want Keycord to use connected OpenPGP smartcards or YubiKeys, then restart Keycord. Password-protected keys remain available without this.",
        FLATPAK_SMARTCARD_OVERRIDE_COMMAND,
    );
    list.prepend(&row);
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
pub fn append_optional_smartcard_access_row(
    _list: &adw::gtk::ListBox,
    _overlay: &adw::ToastOverlay,
    _hardware_rows: &[&adw::ActionRow],
) {
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
pub fn append_optional_host_access_group_row(
    group: &adw::PreferencesGroup,
    _overlay: &adw::ToastOverlay,
) {
    group.set_visible(false);
}
