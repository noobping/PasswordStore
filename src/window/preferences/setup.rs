use crate::setup::{
    can_install_locally, install_locally, is_installed_locally, local_menu_action_label,
    uninstall_locally,
};
use crate::support::actions::register_window_action;
use adw::gio::{Menu, MenuItem};
use adw::prelude::*;
use adw::{ApplicationWindow, Toast, ToastOverlay};

pub(crate) fn register_install_locally_action(
    window: &ApplicationWindow,
    menu: &Menu,
    overlay: &ToastOverlay,
) {
    let menu = menu.clone();
    let overlay = overlay.clone();
    register_window_action(window, "install-locally", move || {
        if !can_install_locally() {
            overlay.add_toast(Toast::new("This app can't be added to the app menu here."));
            return;
        }
        let items = menu.n_items();
        if items > 0 {
            menu.remove(items - 1);
        }
        let installed = is_installed_locally();
        let ok = !installed && install_locally().is_ok();
        let uninstalled = installed && uninstall_locally().is_ok();
        let item = MenuItem::new(
            Some(local_menu_action_label(ok || !uninstalled)),
            Some("win.install-locally"),
        );
        menu.append_item(&item);
    });
}
