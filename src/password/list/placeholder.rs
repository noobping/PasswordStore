use crate::config::APP_ID;
use adw::StatusPage;
use adw::gtk::Spinner;

pub(super) fn loading_placeholder() -> StatusPage {
    let spinner = Spinner::new();
    spinner.start();

    StatusPage::builder()
        .icon_name(format!("{APP_ID}-symbolic"))
        .child(&spinner)
        .build()
}

pub(super) fn resolved_placeholder(empty: bool, has_store_dirs: bool) -> StatusPage {
    if empty {
        build_empty_password_list_placeholder(&format!("{APP_ID}-symbolic"), has_store_dirs)
    } else {
        StatusPage::builder()
            .icon_name("edit-find-symbolic")
            .title("No matches")
            .description("Try another query.")
            .build()
    }
}

#[cfg(not(feature = "flatpak"))]
pub(super) fn should_show_restore_button(
    show_list_actions: bool,
    has_store_dirs: bool,
    empty: bool,
) -> bool {
    show_list_actions && empty && !has_store_dirs
}

#[cfg(feature = "flatpak")]
pub(super) fn should_show_restore_button(
    _show_list_actions: bool,
    _has_store_dirs: bool,
    _empty: bool,
) -> bool {
    false
}

fn build_empty_password_list_placeholder(symbolic: &str, has_store_dirs: bool) -> StatusPage {
    let builder = StatusPage::builder().icon_name(symbolic);
    if has_store_dirs {
        builder
            .title("No items yet")
            .description("Create a new item to get started.")
            .build()
    } else {
        builder
            .title("No folders added")
            .description("Open Preferences to add a password store folder.")
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::should_show_restore_button;

    #[test]
    fn restore_button_is_hidden_for_an_empty_existing_store() {
        assert!(!should_show_restore_button(true, true, true));
    }

    #[test]
    fn restore_button_stays_hidden_off_the_list_page() {
        assert!(!should_show_restore_button(false, false, true));
    }
}
