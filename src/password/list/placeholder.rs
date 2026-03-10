use adw::gtk::Spinner;
use adw::StatusPage;

const APP_ID: &str = env!("APP_ID");

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
