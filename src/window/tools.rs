#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::clipboard::set_clipboard_text;
#[cfg(all(target_os = "linux", feature = "setup"))]
use crate::logging::log_error;
use crate::preferences::Preferences;
#[cfg(all(target_os = "linux", feature = "setup"))]
use crate::setup::{
    can_install_locally, install_locally, is_installed_locally, local_menu_action_label,
    uninstall_locally,
};
use crate::store::management::schedule_store_import_row;
use crate::support::actions::register_window_action;
#[cfg(any(
    debug_assertions,
    all(target_os = "linux", any(feature = "setup", feature = "flatpak"))
))]
use crate::support::ui::append_action_row_with_button;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::support::runtime::has_host_permission;
use crate::support::ui::{clear_list_box, reveal_navigation_page};
#[cfg(debug_assertions)]
use crate::window::navigation::show_log_page;
use crate::window::navigation::{
    show_secondary_page_chrome, HasWindowChrome, WindowNavigationState,
};
use adw::gtk::ListBox;
#[cfg(any(
    all(target_os = "linux", feature = "setup"),
    all(target_os = "linux", feature = "flatpak")
))]
use adw::Toast;
use adw::{ApplicationWindow, NavigationPage, ToastOverlay};

const TOOLS_PAGE_TITLE: &str = "Tools";
const TOOLS_PAGE_SUBTITLE: &str = "Utilities and maintenance";
#[cfg(all(target_os = "linux", feature = "flatpak"))]
const FLATPAK_HOST_OVERRIDE_COMMAND: &str =
    "flatpak override --user --talk-name=org.freedesktop.Flatpak io.github.noobping.keycord";

#[derive(Clone)]
pub struct ToolsPageState {
    pub window: ApplicationWindow,
    pub navigation: WindowNavigationState,
    pub page: NavigationPage,
    pub list: ListBox,
    pub overlay: ToastOverlay,
}

impl ToolsPageState {
    pub fn new(
        window: &ApplicationWindow,
        navigation: &WindowNavigationState,
        page: &NavigationPage,
        list: &ListBox,
        overlay: &ToastOverlay,
    ) -> Self {
        Self {
            window: window.clone(),
            navigation: navigation.clone(),
            page: page.clone(),
            list: list.clone(),
            overlay: overlay.clone(),
        }
    }

    pub fn rebuild(&self) {
        clear_list_box(&self.list);
        append_optional_log_row(self);
        append_optional_setup_row(self);
        append_optional_flatpak_override_row(self);
        append_optional_pass_import_row(self);
    }
}

#[cfg(debug_assertions)]
fn append_optional_log_row(state: &ToolsPageState) {
    let navigation = state.navigation.clone();
    append_action_row_with_button(
        &state.list,
        "Open logs",
        "Inspect recent app and command output.",
        "document-open-symbolic",
        move || show_log_page(&navigation),
    );
}

#[cfg(not(debug_assertions))]
fn append_optional_log_row(_state: &ToolsPageState) {}

#[cfg(all(target_os = "linux", feature = "setup"))]
fn append_optional_setup_row(state: &ToolsPageState) {
    if !can_install_locally() {
        return;
    }

    let title = local_menu_action_label(is_installed_locally());
    let overlay = state.overlay.clone();
    let refresh_state = state.clone();
    append_action_row_with_button(
        &state.list,
        title,
        "Add or remove this build from the local app menu.",
        "emblem-system-symbolic",
        move || {
            let installed = is_installed_locally();
            let result = if installed {
                uninstall_locally()
            } else {
                install_locally()
            };

            match result {
                Ok(()) => refresh_state.rebuild(),
                Err(err) => {
                    log_error(format!("Failed to update local app menu entry: {err}"));
                    overlay.add_toast(Toast::new("Couldn't update the app menu."));
                }
            }
        },
    );
}

#[cfg(not(feature = "setup"))]
const fn append_optional_setup_row(_state: &ToolsPageState) {}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn append_optional_flatpak_override_row(state: &ToolsPageState) {
    if has_host_permission() {
        return;
    }

    let overlay = state.overlay.clone();
    append_action_row_with_button(
        &state.list,
        "Enable Flatpak host access",
        "Copy the override command needed for Flatpak host integration.",
        "edit-copy-symbolic",
        move || {
            if set_clipboard_text(FLATPAK_HOST_OVERRIDE_COMMAND, &overlay, None) {
                overlay.add_toast(Toast::new("Copied."));
            }
        },
    );
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
const fn append_optional_flatpak_override_row(_state: &ToolsPageState) {}

fn append_optional_pass_import_row(state: &ToolsPageState) {
    let settings = Preferences::new();
    schedule_store_import_row(&state.list, &settings, &state.window, &state.overlay);
}

pub fn register_open_tools_action(window: &ApplicationWindow, state: &ToolsPageState) {
    let state = state.clone();
    register_window_action(window, "open-tools", move || {
        let chrome = state.navigation.window_chrome();
        show_secondary_page_chrome(&chrome, TOOLS_PAGE_TITLE, TOOLS_PAGE_SUBTITLE, false);
        state.rebuild();
        reveal_navigation_page(&state.navigation.nav, &state.page);
    });
}
