use crate::i18n::gettext;
use crate::support::ui::dialog_content_shell;
use adw::gtk::Spinner;
use adw::prelude::*;
use adw::{ApplicationWindow, Dialog, StatusPage};

pub(super) fn build_progress_dialog(
    window: &ApplicationWindow,
    title: &str,
    subtitle: Option<&str>,
    description: &str,
) -> Dialog {
    let description = gettext(description);
    let status = StatusPage::builder().description(&description).build();
    status.set_child(Some(&Spinner::builder().spinning(true).build()));

    let dialog = Dialog::builder()
        .title(gettext(title))
        .content_width(460)
        .child(&dialog_content_shell(title, subtitle, &status))
        .build();
    dialog.set_can_close(false);
    dialog.present(Some(window));
    dialog
}
