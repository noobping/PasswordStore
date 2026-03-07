use adw::gtk::Spinner;
use adw::prelude::*;
use adw::{
    ApplicationWindow, Dialog, HeaderBar, PasswordEntryRow, PreferencesGroup, PreferencesPage,
    StatusPage, Toast, ToastOverlay, ToolbarView,
};

pub fn build_private_key_progress_dialog(
    window: &ApplicationWindow,
    title: &str,
    description: &str,
) -> Dialog {
    let status = StatusPage::builder()
        .title(title)
        .description(description)
        .build();
    status.set_child(Some(&Spinner::builder().spinning(true).build()));

    let header = HeaderBar::new();
    let toolbar_view = ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&status));

    let dialog = Dialog::builder()
        .title(title)
        .content_width(460)
        .child(&toolbar_view)
        .build();
    dialog.set_can_close(false);
    dialog.present(Some(window));
    dialog
}

pub fn present_private_key_password_dialog<F>(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    title: &str,
    on_submit: F,
) where
    F: Fn(String) + 'static,
{
    let password_row = PasswordEntryRow::new();
    password_row.set_title("Private key password");
    password_row.set_show_apply_button(true);

    let password_group = PreferencesGroup::builder().build();
    password_group.add(&password_row);

    let page = PreferencesPage::new();
    page.add(&password_group);

    let header = HeaderBar::new();
    let toolbar_view = ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&page));

    let dialog = Dialog::builder()
        .title(title)
        .content_width(460)
        .child(&toolbar_view)
        .build();
    dialog.set_focus(Some(&password_row));

    let dialog_clone = dialog.clone();
    let overlay_clone = overlay.clone();
    password_row.connect_apply(move |row| {
        let passphrase = row.text().to_string();
        if passphrase.is_empty() {
            let toast = Toast::new("Enter the private key password.");
            overlay_clone.add_toast(toast);
            return;
        }

        dialog_clone.close();
        on_submit(passphrase);
    });

    dialog.present(Some(window));
}
