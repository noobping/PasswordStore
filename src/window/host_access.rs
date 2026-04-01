#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::clipboard::set_clipboard_text;
use crate::i18n::gettext;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::preferences::Preferences;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::support::runtime::{
    has_fido2_permission, has_host_permission, has_smartcard_permission,
};
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::support::ui::{add_persistent_hide_button, flat_icon_button_with_tooltip};
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
const FLATPAK_FIDO2_OVERRIDE_COMMAND: &str =
    "flatpak override --user --device=all io.github.noobping.keycord";

const FIDO2_BACKEND_REQUIRED_TOOLTIP: &str =
    "Switch to the Integrated backend to use FIDO2 security keys.";
#[cfg(all(target_os = "linux", feature = "flatpak"))]
const FIDO2_PERMISSION_REQUIRED_TOOLTIP: &str = "Grant USB security key access first.";

#[cfg(all(target_os = "linux", feature = "flatpak"))]
const OPTIONAL_FIDO2_ACCESS_ROW_NAME: &str = "keycord-optional-fido2-access-row";
#[cfg(all(target_os = "linux", feature = "flatpak"))]
const OPTIONAL_SMARTCARD_ACCESS_ROW_NAME: &str = "keycord-optional-smartcard-access-row";
#[cfg(all(target_os = "linux", feature = "flatpak"))]
const OPTIONAL_HOST_ACCESS_NOTICE_ID: &str = "optional-host-access";
#[cfg(all(target_os = "linux", feature = "flatpak"))]
const OPTIONAL_SMARTCARD_ACCESS_NOTICE_ID: &str = "optional-smartcard-access";
#[cfg(all(target_os = "linux", feature = "flatpak"))]
const OPTIONAL_FIDO2_ACCESS_NOTICE_ID: &str = "optional-fido2-access";

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn build_optional_permission_row(
    overlay: &ToastOverlay,
    title: &str,
    subtitle: &str,
    command: &'static str,
) -> ActionRow {
    let title = gettext(title);
    let subtitle = gettext(subtitle);
    let row = ActionRow::builder()
        .title(&title)
        .subtitle(&subtitle)
        .build();
    row.set_activatable(false);

    let button = flat_icon_button_with_tooltip("edit-copy-symbolic", "Copy permission command");
    row.add_suffix(&button);

    let overlay = overlay.clone();
    let feedback_button = button.clone();
    let copy_action = Rc::new(move || {
        if set_clipboard_text(command, &overlay, Some(&feedback_button)) {
            overlay.add_toast(Toast::new(&gettext("Copied.")));
        }
    });

    button.connect_clicked(move |_| copy_action());
    row
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn build_optional_host_access_row(overlay: &ToastOverlay) -> Option<ActionRow> {
    if has_host_permission() || Preferences::new().is_notice_hidden(OPTIONAL_HOST_ACCESS_NOTICE_ID)
    {
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
        let group_for_hide = group.clone();
        add_persistent_hide_button(&row, OPTIONAL_HOST_ACCESS_NOTICE_ID, move || {
            group_for_hide.set_visible(false);
        });
        group.add(&row);
        group.set_visible(true);
    }
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
pub fn append_optional_host_access_row(list: &ListBox, overlay: &ToastOverlay) {
    if let Some(row) = build_optional_host_access_row(overlay) {
        add_persistent_hide_button(&row, OPTIONAL_HOST_ACCESS_NOTICE_ID, || {});
        list.prepend(&row);
    }
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
pub fn append_optional_smartcard_access_row(
    list: &ListBox,
    overlay: &ToastOverlay,
    hardware_rows: &[&ActionRow],
    enabled: bool,
) {
    let granted = has_smartcard_permission();
    let blocked_tooltip = gettext("Grant smartcard access first.");
    for row in hardware_rows {
        row.set_sensitive(enabled && granted);
        row.set_tooltip_text((enabled && !granted).then_some(blocked_tooltip.as_str()));
    }

    let show_permission_row = enabled
        && !granted
        && !Preferences::new().is_notice_hidden(OPTIONAL_SMARTCARD_ACCESS_NOTICE_ID);
    if let Some(row) = find_optional_permission_row(list, OPTIONAL_SMARTCARD_ACCESS_ROW_NAME) {
        row.set_visible(show_permission_row);
    }
    if !show_permission_row {
        return;
    }

    let row = ensure_optional_smartcard_access_row(list, overlay);
    row.set_visible(true);
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
pub fn append_optional_fido2_access_row(
    list: &ListBox,
    overlay: &ToastOverlay,
    fido2_rows: &[&ActionRow],
    enabled: bool,
) {
    let granted = has_fido2_permission();
    let blocked_tooltip = if enabled {
        gettext(FIDO2_PERMISSION_REQUIRED_TOOLTIP)
    } else {
        gettext(FIDO2_BACKEND_REQUIRED_TOOLTIP)
    };
    for row in fido2_rows {
        row.set_sensitive(enabled && granted);
        row.set_tooltip_text((!enabled || !granted).then_some(blocked_tooltip.as_str()));
    }

    let show_permission_row = enabled
        && !granted
        && !Preferences::new().is_notice_hidden(OPTIONAL_FIDO2_ACCESS_NOTICE_ID);
    if let Some(row) = find_optional_permission_row(list, OPTIONAL_FIDO2_ACCESS_ROW_NAME) {
        row.set_visible(show_permission_row);
    }
    if !show_permission_row {
        return;
    }

    let row = ensure_optional_fido2_access_row(list, overlay);
    row.set_visible(true);
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn find_optional_permission_row(list: &ListBox, widget_name: &str) -> Option<ActionRow> {
    let mut child = list.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        if widget.widget_name() == widget_name {
            return widget.downcast::<ActionRow>().ok();
        }
        child = next;
    }

    None
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn ensure_optional_fido2_access_row(list: &ListBox, overlay: &ToastOverlay) -> ActionRow {
    if let Some(row) = find_optional_permission_row(list, OPTIONAL_FIDO2_ACCESS_ROW_NAME) {
        return row;
    }

    let row = build_optional_permission_row(
        overlay,
        "Allow USB security key access",
        "FIDO2 recipients are optional. Grant USB device access if you want Keycord to use a connected FIDO2 security key directly for Keycord-only encryption, then restart Keycord.",
        FLATPAK_FIDO2_OVERRIDE_COMMAND,
    );
    row.set_widget_name(OPTIONAL_FIDO2_ACCESS_ROW_NAME);
    add_persistent_hide_button(&row, OPTIONAL_FIDO2_ACCESS_NOTICE_ID, || {});
    list.append(&row);
    row
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn ensure_optional_smartcard_access_row(list: &ListBox, overlay: &ToastOverlay) -> ActionRow {
    if let Some(row) = find_optional_permission_row(list, OPTIONAL_SMARTCARD_ACCESS_ROW_NAME) {
        return row;
    }

    let row = build_optional_permission_row(
        overlay,
        "Allow smartcard access",
        "Hardware keys are optional. Grant PC/SC access if you want Keycord to use connected OpenPGP smartcards or YubiKeys, then restart Keycord. Password-protected keys remain available without this.",
        FLATPAK_SMARTCARD_OVERRIDE_COMMAND,
    );
    row.set_widget_name(OPTIONAL_SMARTCARD_ACCESS_ROW_NAME);
    add_persistent_hide_button(&row, OPTIONAL_SMARTCARD_ACCESS_NOTICE_ID, || {});
    list.prepend(&row);
    row
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
pub fn append_optional_smartcard_access_row(
    _list: &adw::gtk::ListBox,
    _overlay: &adw::ToastOverlay,
    _hardware_rows: &[&adw::ActionRow],
    _enabled: bool,
) {
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
pub fn append_optional_fido2_access_row(
    _list: &adw::gtk::ListBox,
    _overlay: &adw::ToastOverlay,
    _fido2_rows: &[&adw::ActionRow],
    enabled: bool,
) {
    let blocked_tooltip = gettext(FIDO2_BACKEND_REQUIRED_TOOLTIP);
    for row in _fido2_rows {
        row.set_sensitive(enabled);
        row.set_tooltip_text((!enabled).then_some(blocked_tooltip.as_str()));
    }
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
pub fn append_optional_host_access_group_row(
    group: &adw::PreferencesGroup,
    _overlay: &adw::ToastOverlay,
) {
    group.set_visible(false);
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
pub fn append_optional_host_access_row(_list: &adw::gtk::ListBox, _overlay: &adw::ToastOverlay) {}
