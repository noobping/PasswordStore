/* application.rs
 *
 * Copyright 2025 noobping
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0
 */

use crate::block_on;
use crate::config::VERSION;
use crate::PasswordstoreWindow;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};
use log::{error, info};
use passcore::PassStore;
use search_provider::{self, ResultID, ResultMeta, SearchProvider};

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct PasswordstoreApplication {}

    #[glib::object_subclass]
    impl ObjectSubclass for PasswordstoreApplication {
        const NAME: &'static str = "PasswordstoreApplication";
        type Type = super::PasswordstoreApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for PasswordstoreApplication {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_gactions();
            obj.set_accels_for_action("app.quit", &["<primary>q"]);
            obj.set_accels_for_action("app.preferences", &["<primary>p"]);
            obj.set_accels_for_action("win.toggle-search", &["<primary>f"]);
            obj.set_accels_for_action("win.add-password", &["<primary>n"]);
            obj.set_accels_for_action("win.synchronize", &["<primary>s"]);
            obj.set_accels_for_action("win.save-password", &["<primary><shift>s"]);
            obj.set_accels_for_action("win.git-page", &["<primary>r"]);
        }
    }

    impl ApplicationImpl for PasswordstoreApplication {
        // We connect to the activate callback to create a window when the application
        // has been launched. Additionally, this callback notifies us when the user
        // tries to launch a "second instance" of the application. When they try
        // to do that, we'll just present any existing window.
        fn activate(&self) {
            let application = self.obj();
            // Get the current window or create one if necessary
            let window = application.active_window().unwrap_or_else(|| {
                let window = PasswordstoreWindow::new(&*application);
                window.upcast()
            });

            // Ask the window manager/compositor to present the window
            window.present();
        }

        fn startup(&self) {
            self.parent_startup();

            let is_service = self
                .obj()
                .flags()
                .contains(gio::ApplicationFlags::IS_SERVICE);
            if is_service {
                let provider = SearchProvider::new(
                    self.obj().clone(),
                    "io.github.noobping.PasswordStore.SearchProvider",
                    "/io/github/noobping/PasswordStore/SearchProvider",
                );
                block_on::block_on(async {
                    info!("[passwordstore] Registering search provider...");
                    provider
                        .await
                        .expect("[passwordstore] Failed to register search provider");
                });
            }
        }
    }

    impl GtkApplicationImpl for PasswordstoreApplication {}
    impl AdwApplicationImpl for PasswordstoreApplication {}
}

glib::wrapper! {
    pub struct PasswordstoreApplication(ObjectSubclass<imp::PasswordstoreApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl search_provider::SearchProviderImpl for PasswordstoreApplication {
    fn activate_result(&self, identifier: ResultID, _terms: &[String], _timestamp: u32) {
        info!("[passwordstore] Activating result for `{}`", identifier);
        let store = PassStore::new().unwrap();
        if let Ok(_entry) = store.ask(&identifier) {
            info!(
                "[passwordstore] Copied password for `{}` to clipboard",
                identifier
            );
        } else {
            error!("[passwordstore] Failed to get entry for `{}`", identifier);
        }
    }

    fn initial_result_set(&self, terms: &[String]) -> Vec<ResultID> {
        info!("[passwordstore] Searching for `{}`", terms.join(", "));
        let needle = terms.join(" ").to_lowercase();
        let store = PassStore::default();

        store
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|id| id.to_lowercase().contains(&needle))
            .collect()
    }

    fn result_metas(&self, identifiers: &[ResultID]) -> Vec<ResultMeta> {
        info!(
            "[passwordstore] Getting result metas for `{}`",
            identifiers.join(", ")
        );
        let store = PassStore::new().unwrap();
        store
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|path| {
                if identifiers.contains(&path) {
                    Some(
                        ResultMeta::builder(path.clone(), &path)
                            .description("Copy password to clipboard")
                            .clipboard_text(&store.ask(&path).ok()?.password)
                            .build(),
                    )
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    }
}

impl PasswordstoreApplication {
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", flags)
            .property("resource-base-path", "/io/github/noobping/PasswordStore")
            .build()
    }

    fn setup_gactions(&self) {
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| app.quit())
            .build();
        let about_action = gio::ActionEntry::builder("about")
            .activate(move |app: &Self, _, _| app.show_about())
            .build();
        self.add_action_entries([quit_action, about_action]);
    }

    fn show_about(&self) {
        let window = self.active_window().unwrap();
        let about = adw::AboutDialog::builder()
            .application_name("Password Store")
            .application_icon("io.github.noobping.PasswordStore")
            .developer_name("noobping")
            .version(VERSION)
            .developers(vec!["noobping"])
            // Translators: Replace "translator-credits" with your name/username, and optionally an email or URL.
            .translator_credits(&gettext("translator-credits"))
            .copyright("© 2025 noobping")
            .build();

        about.present(Some(&window));
    }
}
