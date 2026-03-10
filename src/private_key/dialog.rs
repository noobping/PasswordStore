use adw::gtk::{Box as GtkBox, Orientation, Spinner};
use adw::prelude::*;
use adw::{
    ApplicationWindow, Dialog, HeaderBar, PasswordEntryRow, PreferencesGroup, PreferencesPage,
    StatusPage, Toast, ToastOverlay, WindowTitle,
};

fn dialog_content_shell(
    title: &str,
    subtitle: Option<&str>,
    child: &impl IsA<adw::gtk::Widget>,
) -> GtkBox {
    let window_title = WindowTitle::builder().title(title).build();
    if let Some(subtitle) = subtitle.filter(|subtitle| !subtitle.trim().is_empty()) {
        window_title.set_subtitle(subtitle);
    }

    let header = HeaderBar::new();
    header.set_title_widget(Some(&window_title));

    let shell = GtkBox::new(Orientation::Vertical, 0);
    shell.append(&header);
    shell.append(child);
    shell
}

pub fn build_private_key_progress_dialog(
    window: &ApplicationWindow,
    title: &str,
    subtitle: Option<&str>,
    description: &str,
) -> Dialog {
    let status = StatusPage::builder().description(description).build();
    status.set_child(Some(&Spinner::builder().spinning(true).build()));

    let dialog = Dialog::builder()
        .title(title)
        .content_width(460)
        .child(&dialog_content_shell(title, subtitle, &status))
        .build();
    dialog.set_can_close(false);
    dialog.present(Some(window));
    dialog
}

pub fn present_private_key_password_dialog<F>(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    title: &str,
    subtitle: Option<&str>,
    on_submit: F,
) where
    F: Fn(String) + 'static,
{
    let password_row = PasswordEntryRow::new();
    password_row.set_title("Key password");
    password_row.set_show_apply_button(true);

    let password_group = PreferencesGroup::builder().build();
    password_group.add(&password_row);

    let page = PreferencesPage::new();
    page.add(&password_group);

    let dialog = Dialog::builder()
        .title(title)
        .content_width(460)
        .child(&dialog_content_shell(title, subtitle, &page))
        .build();
    dialog.set_focus(Some(&password_row));

    let dialog_clone = dialog.clone();
    let overlay_clone = overlay.clone();
    password_row.connect_apply(move |row| {
        let passphrase = row.text().to_string();
        if passphrase.is_empty() {
            let toast = Toast::new("Enter the key password.");
            overlay_clone.add_toast(toast);
            return;
        }

        dialog_clone.close();
        on_submit(passphrase);
    });

    dialog.present(Some(window));
}
