mod chrome;
mod pages;
mod restore;
mod state;

pub use self::chrome::{
    set_save_button_for_password, show_primary_page_chrome, show_secondary_page_chrome,
    APP_WINDOW_TITLE,
};
#[cfg(target_os = "linux")]
pub use self::pages::{finish_git_busy_page, show_git_busy_page};
pub use self::pages::{show_docs_page, show_log_page};
pub use self::restore::restore_window_for_current_page;
pub use self::state::{HasWindowChrome, WindowNavigationState, WindowPageState};
