use crate::password::file::sync_username_row;
use crate::password::opened::get_opened_pass_file;
use crate::password::page::PasswordPageState;
use crate::preferences::Preferences;
use crate::store::management::{sync_store_recipients_page_header, StoreRecipientsPageState};
use crate::support::runtime::git_integration_available;
use crate::support::ui::{navigation_stack_is_root, visible_navigation_page_is};
use crate::window::preferences::PreferencesActionState;
use adw::gtk::Button;
use adw::prelude::*;
use adw::{EntryRow, NavigationPage, NavigationView, WindowTitle};

mod standard;
pub(crate) use self::standard::{finish_git_busy_page, show_git_busy_page, show_log_page};

#[derive(Clone)]
pub(crate) struct WindowNavigationState {
    pub(crate) nav: NavigationView,
    pub(crate) text_page: NavigationPage,
    pub(crate) raw_text_page: NavigationPage,
    pub(crate) settings_page: NavigationPage,
    pub(crate) log_page: NavigationPage,
    pub(crate) back: Button,
    pub(crate) add: Button,
    pub(crate) find: Button,
    pub(crate) git: Button,
    pub(crate) store: Button,
    pub(crate) save: Button,
    pub(crate) raw: Button,
    pub(crate) win: WindowTitle,
    pub(crate) username: EntryRow,
}

pub(crate) struct WindowChrome<'a> {
    pub(crate) back: &'a Button,
    pub(crate) add: &'a Button,
    pub(crate) find: &'a Button,
    pub(crate) git: &'a Button,
    pub(crate) store: &'a Button,
    pub(crate) save: &'a Button,
    pub(crate) raw: &'a Button,
    pub(crate) win: &'a WindowTitle,
}

pub(crate) const APP_WINDOW_TITLE: &str = "Keycord";
pub(crate) const APP_WINDOW_SUBTITLE: &str = "Browse and edit password stores";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestoredPageKind {
    Root,
    Text,
    Raw,
    Settings,
    Recipients,
    Log,
    Other,
}

fn restored_page_kind(
    at_root: bool,
    is_text_page: bool,
    is_raw_page: bool,
    is_settings_page: bool,
    is_recipients_page: bool,
    is_log_page: bool,
) -> RestoredPageKind {
    if at_root {
        return RestoredPageKind::Root;
    }
    if is_text_page {
        return RestoredPageKind::Text;
    }
    if is_raw_page {
        return RestoredPageKind::Raw;
    }
    if is_settings_page {
        return RestoredPageKind::Settings;
    }
    if is_recipients_page {
        return RestoredPageKind::Recipients;
    }
    if is_log_page {
        return RestoredPageKind::Log;
    }

    RestoredPageKind::Other
}

pub(crate) trait HasWindowChrome {
    fn window_chrome(&self) -> WindowChrome<'_>;
}

macro_rules! impl_has_window_chrome {
    ($($state:ty),+ $(,)?) => {
        $(
            impl HasWindowChrome for $state {
                fn window_chrome(&self) -> WindowChrome<'_> {
                    WindowChrome {
                        back: &self.back,
                        add: &self.add,
                        find: &self.find,
                        git: &self.git,
                        store: &self.store,
                        save: &self.save,
                        raw: &self.raw,
                        win: &self.win,
                    }
                }
            }
        )+
    };
}

impl_has_window_chrome!(
    WindowNavigationState,
    PasswordPageState,
    StoreRecipientsPageState,
    PreferencesActionState,
);

pub(crate) fn set_save_button_for_password(save: &Button) {
    save.set_action_name(Some("win.save-password"));
    save.set_tooltip_text(Some("Save changes"));
}

pub(crate) fn show_primary_page_chrome(chrome: &WindowChrome<'_>, has_store_dirs: bool) {
    let git_available = git_integration_available();
    chrome.back.set_visible(false);
    chrome.save.set_visible(false);
    set_save_button_for_password(chrome.save);
    chrome.add.set_visible(has_store_dirs);
    chrome.find.set_visible(true);
    chrome.git.set_visible(!has_store_dirs && git_available);
    #[cfg(feature = "flatpak")]
    {
        chrome.store.set_visible(!has_store_dirs);
    }
    #[cfg(not(feature = "flatpak"))]
    {
        chrome.store.set_visible(false);
    }
    chrome.win.set_title(APP_WINDOW_TITLE);
    chrome.win.set_subtitle(APP_WINDOW_SUBTITLE);
    chrome.raw.set_visible(false);
}

pub(crate) fn show_secondary_page_chrome(
    chrome: &WindowChrome<'_>,
    title: &str,
    subtitle: &str,
    save_visible: bool,
) {
    chrome.back.set_visible(true);
    chrome.add.set_visible(false);
    chrome.find.set_visible(false);
    chrome.git.set_visible(false);
    chrome.store.set_visible(false);
    chrome.save.set_visible(save_visible);
    chrome.raw.set_visible(false);
    set_save_button_for_password(chrome.save);
    chrome.win.set_title(title);
    chrome.win.set_subtitle(subtitle);
}

pub(crate) fn restore_window_for_current_page(
    state: &WindowNavigationState,
    recipients_page: &StoreRecipientsPageState,
) -> bool {
    let chrome = state.window_chrome();
    let page_kind = restored_page_kind(
        navigation_stack_is_root(&state.nav),
        visible_navigation_page_is(&state.nav, &state.text_page),
        visible_navigation_page_is(&state.nav, &state.raw_text_page),
        visible_navigation_page_is(&state.nav, &state.settings_page),
        visible_navigation_page_is(&state.nav, &recipients_page.page),
        visible_navigation_page_is(&state.nav, &state.log_page),
    );

    if page_kind == RestoredPageKind::Root {
        show_primary_page_chrome(&chrome, !Preferences::new().stores().is_empty());
        return true;
    }

    state.save.set_visible(matches!(
        page_kind,
        RestoredPageKind::Text | RestoredPageKind::Raw
    ));
    if page_kind == RestoredPageKind::Text {
        if let Some(pass_file) = get_opened_pass_file() {
            let label = pass_file.label();
            show_secondary_page_chrome(&chrome, pass_file.title(), &label, true);
            state.raw.set_visible(true);
            sync_username_row(&state.username, Some(&pass_file));
        } else {
            show_secondary_page_chrome(&chrome, APP_WINDOW_TITLE, APP_WINDOW_SUBTITLE, true);
            sync_username_row(&state.username, None);
        }
    } else if page_kind == RestoredPageKind::Raw {
        let subtitle = get_opened_pass_file()
            .map(|pass_file| pass_file.label())
            .unwrap_or_else(|| APP_WINDOW_TITLE.to_string());
        show_secondary_page_chrome(&chrome, "Raw Pass File", &subtitle, true);
    } else if page_kind == RestoredPageKind::Settings {
        show_secondary_page_chrome(&chrome, "Preferences", APP_WINDOW_TITLE, false);
    } else if page_kind == RestoredPageKind::Recipients {
        set_save_button_for_password(&state.save);
        sync_store_recipients_page_header(recipients_page);
    } else if page_kind == RestoredPageKind::Log {
        show_secondary_page_chrome(&chrome, "Logs", "Details", false);
    }

    false
}

#[cfg(test)]
mod tests {
    use super::{restored_page_kind, RestoredPageKind};

    #[test]
    fn restored_page_kind_prefers_root_before_any_other_page() {
        assert_eq!(
            restored_page_kind(true, true, true, true, true, true),
            RestoredPageKind::Root
        );
    }

    #[test]
    fn restored_page_kind_matches_each_known_page() {
        assert_eq!(
            restored_page_kind(false, true, false, false, false, false),
            RestoredPageKind::Text
        );
        assert_eq!(
            restored_page_kind(false, false, true, false, false, false),
            RestoredPageKind::Raw
        );
        assert_eq!(
            restored_page_kind(false, false, false, true, false, false),
            RestoredPageKind::Settings
        );
        assert_eq!(
            restored_page_kind(false, false, false, false, true, false),
            RestoredPageKind::Recipients
        );
        assert_eq!(
            restored_page_kind(false, false, false, false, false, true),
            RestoredPageKind::Log
        );
    }

    #[test]
    fn restored_page_kind_falls_back_to_other_when_nothing_matches() {
        assert_eq!(
            restored_page_kind(false, false, false, false, false, false),
            RestoredPageKind::Other
        );
    }
}
