#[cfg(feature = "flatpak")]
use crate::clipboard::set_clipboard_text;
use crate::i18n::gettext;
#[cfg(feature = "flatpak")]
use crate::logging::{log_error, run_command_output, CommandLogOptions};
#[cfg(feature = "flatpak")]
use crate::preferences::Preferences;
#[cfg(feature = "flatpak")]
use crate::support::background::spawn_result_task;
#[cfg(feature = "flatpak")]
use crate::support::runtime::{
    has_fido2_permission, has_host_permission, has_smartcard_permission,
};
#[cfg(feature = "flatpak")]
use crate::support::ui::{add_persistent_hide_button, flat_icon_button_with_tooltip};
#[cfg(feature = "flatpak")]
use adw::gtk::Button;
use adw::prelude::*;
#[cfg(feature = "flatpak")]
use adw::{ActionRow, PreferencesGroup, Toast, ToastOverlay};
#[cfg(feature = "flatpak")]
use std::process::Output;
#[cfg(feature = "flatpak")]
use std::rc::Rc;

#[cfg(feature = "flatpak")]
const APP_ID: &str = env!("APP_ID");

#[cfg(feature = "flatpak")]
const FLATPAK_HOST_OVERRIDE_COMMAND: &str = concat!(
    "flatpak override --user --talk-name=org.freedesktop.Flatpak ",
    env!("APP_ID")
);

#[cfg(feature = "flatpak")]
const FLATPAK_HOST_OVERRIDE_ARGS: &[&str] = &[
    "override",
    "--user",
    "--talk-name=org.freedesktop.Flatpak",
    APP_ID,
];

#[cfg(feature = "flatpak")]
const FLATPAK_SMARTCARD_OVERRIDE_COMMAND: &str =
    concat!("flatpak override --user --socket=pcsc ", env!("APP_ID"));

#[cfg(feature = "flatpak")]
const FLATPAK_SMARTCARD_OVERRIDE_ARGS: &[&str] = &["override", "--user", "--socket=pcsc", APP_ID];

#[cfg(feature = "flatpak")]
const FLATPAK_FIDO2_OVERRIDE_COMMAND: &str =
    concat!("flatpak override --user --device=all ", env!("APP_ID"));

#[cfg(feature = "flatpak")]
const FLATPAK_FIDO2_OVERRIDE_ARGS: &[&str] = &["override", "--user", "--device=all", APP_ID];

#[cfg(feature = "flatpak")]
const OPTIONAL_PERMISSION_SUCCESS_TOAST: &str = "Restart app to apply.";

#[cfg(feature = "flatpak")]
const OPTIONAL_PERMISSION_ERROR_TOAST: &str = "Couldn't grant permission.";

const FIDO2_BACKEND_REQUIRED_TOOLTIP: &str =
    "Switch to the Integrated backend to use FIDO2 security keys.";
#[cfg(feature = "flatpak")]
const FIDO2_PERMISSION_REQUIRED_TOOLTIP: &str = "Grant USB security key access first.";

#[cfg(feature = "flatpak")]
const OPTIONAL_FIDO2_ACCESS_ROW_NAME: &str = "keycord-optional-fido2-access-row";
#[cfg(feature = "flatpak")]
const OPTIONAL_SMARTCARD_ACCESS_ROW_NAME: &str = "keycord-optional-smartcard-access-row";
#[cfg(feature = "flatpak")]
const OPTIONAL_HOST_ACCESS_ROW_NAME: &str = "keycord-optional-host-access-row";
#[cfg(feature = "flatpak")]
const OPTIONAL_HOST_ACCESS_NOTICE_ID: &str = "optional-host-access";
#[cfg(feature = "flatpak")]
const OPTIONAL_SMARTCARD_ACCESS_NOTICE_ID: &str = "optional-smartcard-access";
#[cfg(feature = "flatpak")]
const OPTIONAL_FIDO2_ACCESS_NOTICE_ID: &str = "optional-fido2-access";

#[cfg(feature = "flatpak")]
#[derive(Clone, Copy)]
struct OptionalPermissionCommand {
    copy_command: &'static str,
    host_args: &'static [&'static str],
    context: &'static str,
}

#[cfg(feature = "flatpak")]
fn build_optional_permission_row(
    overlay: &ToastOverlay,
    title: &str,
    subtitle: &str,
    command: OptionalPermissionCommand,
) -> ActionRow {
    let title = gettext(title);
    let subtitle = gettext(subtitle);
    let row = ActionRow::builder()
        .title(&title)
        .subtitle(&subtitle)
        .build();
    row.set_activatable(false);

    if optional_permission_uses_in_app_grant(has_host_permission()) {
        let button = Button::with_label(&gettext("Grant permission"));
        button.add_css_class("suggested-action");
        connect_optional_permission_grant_button(&button, overlay, command);
        row.add_suffix(&button);
    } else {
        let button = flat_icon_button_with_tooltip("edit-copy-symbolic", "Copy permission command");
        row.add_suffix(&button);

        let overlay = overlay.clone();
        let feedback_button = button.clone();
        let copy_action = Rc::new(move || {
            if set_clipboard_text(command.copy_command, &overlay, Some(&feedback_button)) {
                overlay.add_toast(Toast::new(&gettext("Copied.")));
            }
        });

        button.connect_clicked(move |_| copy_action());
    }

    row
}

#[cfg(feature = "flatpak")]
const fn optional_permission_uses_in_app_grant(can_run_host_commands: bool) -> bool {
    can_run_host_commands
}

#[cfg(feature = "flatpak")]
fn connect_optional_permission_grant_button(
    button: &Button,
    overlay: &ToastOverlay,
    command: OptionalPermissionCommand,
) {
    let overlay = overlay.clone();
    button.connect_clicked(move |button| {
        button.set_sensitive(false);
        let overlay_for_result = overlay.clone();
        let overlay_for_disconnect = overlay.clone();
        let button_for_result = button.clone();
        let button_for_disconnect = button.clone();
        spawn_result_task(
            move || run_optional_permission_command(command),
            move |result| {
                button_for_result.set_sensitive(true);
                match result {
                    Ok(()) => overlay_for_result
                        .add_toast(Toast::new(&gettext(OPTIONAL_PERMISSION_SUCCESS_TOAST))),
                    Err(err) => {
                        log_error(format!("{}: {err}", command.context));
                        overlay_for_result
                            .add_toast(Toast::new(&gettext(OPTIONAL_PERMISSION_ERROR_TOAST)));
                    }
                }
            },
            move || {
                button_for_disconnect.set_sensitive(true);
                overlay_for_disconnect
                    .add_toast(Toast::new(&gettext(OPTIONAL_PERMISSION_ERROR_TOAST)));
            },
        );
    });
}

#[cfg(feature = "flatpak")]
fn run_optional_permission_command(command: OptionalPermissionCommand) -> Result<(), String> {
    let mut cmd = Preferences::new().host_program_command("flatpak", command.host_args);
    let output = run_command_output(&mut cmd, command.context, CommandLogOptions::DEFAULT)
        .map_err(|err| format!("Failed to start the permission command: {err}"))?;
    if output.status.success() {
        return Ok(());
    }

    Err(optional_permission_command_error(&output))
}

#[cfg(feature = "flatpak")]
fn optional_permission_command_error(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("Permission command failed: {}", output.status)
    }
}

#[cfg(feature = "flatpak")]
pub fn append_optional_host_access_group_row(
    group: &PreferencesGroup,
    overlay: &ToastOverlay,
) -> Option<ActionRow> {
    let show_permission_row = !has_host_permission()
        && !Preferences::new().is_notice_hidden(OPTIONAL_HOST_ACCESS_NOTICE_ID);

    let row = find_optional_permission_group_row(group, OPTIONAL_HOST_ACCESS_ROW_NAME)
        .or_else(|| {
            let row = build_optional_permission_row(
                overlay,
                "Allow access to apps on this device",
                "Keycord is running in a protected space, so some optional features stay off until you allow this. If you allow it, Keycord can use tools from your computer such as GPG, the Host backend, and pass import. If you don't, Keycord still works with the integrated backend.",
                OptionalPermissionCommand {
                    copy_command: FLATPAK_HOST_OVERRIDE_COMMAND,
                    host_args: FLATPAK_HOST_OVERRIDE_ARGS,
                    context: "Grant host access",
                },
            );
            row.set_widget_name(OPTIONAL_HOST_ACCESS_ROW_NAME);
            let group_for_hide = group.clone();
            add_persistent_hide_button(&row, OPTIONAL_HOST_ACCESS_NOTICE_ID, move || {
                group_for_hide.set_visible(false);
            });
            group.add(&row);
            Some(row)
        });

    if let Some(row) = row {
        row.set_visible(show_permission_row);
        group.set_visible(show_permission_row);
        return Some(row);
    }

    group.set_visible(false);
    None
}

#[cfg(feature = "flatpak")]
pub fn append_optional_smartcard_access_group_row(
    group: &PreferencesGroup,
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
    if let Some(row) = find_optional_permission_group_row(group, OPTIONAL_SMARTCARD_ACCESS_ROW_NAME)
    {
        row.set_visible(show_permission_row);
    }
    if !show_permission_row {
        return;
    }

    let row = ensure_optional_smartcard_access_group_row(group, overlay);
    row.set_visible(true);
}

#[cfg(feature = "flatpak")]
pub fn append_optional_fido2_access_group_row(
    group: &PreferencesGroup,
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
    if let Some(row) = find_optional_permission_group_row(group, OPTIONAL_FIDO2_ACCESS_ROW_NAME) {
        row.set_visible(show_permission_row);
    }
    if !show_permission_row {
        return;
    }

    let row = ensure_optional_fido2_access_group_row(group, overlay);
    row.set_visible(true);
}

#[cfg(feature = "flatpak")]
fn find_optional_permission_group_row(
    group: &PreferencesGroup,
    widget_name: &str,
) -> Option<ActionRow> {
    find_named_descendant_action_row(group.upcast_ref(), widget_name)
}

#[cfg(feature = "flatpak")]
fn find_named_descendant_action_row(
    widget: &adw::gtk::Widget,
    widget_name: &str,
) -> Option<ActionRow> {
    if widget.widget_name() == widget_name {
        return widget.clone().downcast::<ActionRow>().ok();
    }

    let mut child = widget.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        if let Some(row) = find_named_descendant_action_row(&widget, widget_name) {
            return Some(row);
        }
        child = next;
    }

    None
}

#[cfg(feature = "flatpak")]
fn ensure_optional_fido2_access_group_row(
    group: &PreferencesGroup,
    overlay: &ToastOverlay,
) -> ActionRow {
    if let Some(row) = find_optional_permission_group_row(group, OPTIONAL_FIDO2_ACCESS_ROW_NAME) {
        return row;
    }

    let row = build_optional_permission_row(
        overlay,
        "Allow USB security key access",
        "FIDO2 recipients are optional. Grant USB device access if you want Keycord to use a connected FIDO2 security key directly for Keycord-only encryption, then restart Keycord.",
        OptionalPermissionCommand {
            copy_command: FLATPAK_FIDO2_OVERRIDE_COMMAND,
            host_args: FLATPAK_FIDO2_OVERRIDE_ARGS,
            context: "Grant USB security key access",
        },
    );
    row.set_widget_name(OPTIONAL_FIDO2_ACCESS_ROW_NAME);
    add_persistent_hide_button(&row, OPTIONAL_FIDO2_ACCESS_NOTICE_ID, || {});
    group.add(&row);
    row
}

#[cfg(feature = "flatpak")]
fn ensure_optional_smartcard_access_group_row(
    group: &PreferencesGroup,
    overlay: &ToastOverlay,
) -> ActionRow {
    if let Some(row) = find_optional_permission_group_row(group, OPTIONAL_SMARTCARD_ACCESS_ROW_NAME)
    {
        return row;
    }

    let row = build_optional_permission_row(
        overlay,
        "Allow smartcard access",
        "Hardware keys are optional. Grant PC/SC access if you want Keycord to use connected OpenPGP smartcards or YubiKeys, then restart Keycord. Password-protected keys remain available without this.",
        OptionalPermissionCommand {
            copy_command: FLATPAK_SMARTCARD_OVERRIDE_COMMAND,
            host_args: FLATPAK_SMARTCARD_OVERRIDE_ARGS,
            context: "Grant smartcard access",
        },
    );
    row.set_widget_name(OPTIONAL_SMARTCARD_ACCESS_ROW_NAME);
    add_persistent_hide_button(&row, OPTIONAL_SMARTCARD_ACCESS_NOTICE_ID, || {});
    group.add(&row);
    row
}

#[cfg(not(feature = "flatpak"))]
pub fn append_optional_smartcard_access_group_row(
    _group: &adw::PreferencesGroup,
    _overlay: &adw::ToastOverlay,
    _hardware_rows: &[&adw::ActionRow],
    enabled: bool,
) {
    let blocked_tooltip = gettext("Grant smartcard access first.");
    for row in _hardware_rows {
        row.set_sensitive(enabled);
        row.set_tooltip_text((!enabled).then_some(blocked_tooltip.as_str()));
    }
}

#[cfg(not(feature = "flatpak"))]
pub fn append_optional_fido2_access_group_row(
    _group: &adw::PreferencesGroup,
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

#[cfg(not(feature = "flatpak"))]
pub fn append_optional_host_access_group_row(
    group: &adw::PreferencesGroup,
    _overlay: &adw::ToastOverlay,
) -> Option<adw::ActionRow> {
    group.set_visible(false);
    None
}

#[cfg(all(test, feature = "flatpak"))]
mod tests {
    use super::{optional_permission_command_error, optional_permission_uses_in_app_grant};
    use std::process::Command;

    #[test]
    fn host_command_access_enables_in_app_permission_grants() {
        assert!(!optional_permission_uses_in_app_grant(false));
        assert!(optional_permission_uses_in_app_grant(true));
    }

    #[test]
    fn permission_command_error_prefers_stderr() {
        let output = Command::new("sh")
            .args([
                "-c",
                "printf 'stdout details'; printf 'stderr details' >&2; exit 9",
            ])
            .output()
            .expect("run shell");

        assert_eq!(
            optional_permission_command_error(&output),
            "stderr details".to_string()
        );
    }
}
