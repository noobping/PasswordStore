use crate::password::page::PasswordPageState;
use crate::store::git_page::StoreGitPageState;
use crate::store::management::StoreRecipientsPageState;
use adw::gtk::Button;
use adw::{ApplicationWindow, EntryRow, NavigationPage, NavigationView, WindowTitle};

#[derive(Clone)]
pub struct WindowNavigationState {
    pub nav: NavigationView,
    pub password_page: NavigationPage,
    pub raw_text_page: NavigationPage,
    pub settings_page: NavigationPage,
    pub tools_page: NavigationPage,
    pub docs_page: NavigationPage,
    pub docs_detail_page: NavigationPage,
    pub tools_field_values_page: NavigationPage,
    pub tools_value_values_page: NavigationPage,
    pub tools_weak_passwords_page: NavigationPage,
    pub tools_audit_page: NavigationPage,
    pub store_import_page: NavigationPage,
    pub log_page: NavigationPage,
    pub back: Button,
    pub add: Button,
    pub find: Button,
    pub git: Button,
    pub store: Button,
    pub save: Button,
    pub raw: Button,
    pub win: WindowTitle,
    pub username: EntryRow,
}

#[derive(Clone)]
pub struct WindowPageState {
    pub window: ApplicationWindow,
    pub nav: NavigationView,
    pub page: NavigationPage,
    pub back: Button,
    pub add: Button,
    pub find: Button,
    pub git: Button,
    pub store: Button,
    pub save: Button,
    pub raw: Button,
    pub win: WindowTitle,
}

pub struct WindowChrome<'a> {
    pub back: &'a Button,
    pub add: &'a Button,
    pub find: &'a Button,
    pub git: &'a Button,
    pub store: &'a Button,
    pub save: &'a Button,
    pub raw: &'a Button,
    pub win: &'a WindowTitle,
}

pub trait HasWindowChrome {
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
    WindowPageState,
    PasswordPageState,
    StoreRecipientsPageState,
    StoreGitPageState,
);
