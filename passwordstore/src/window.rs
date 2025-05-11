/* window.rs
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

use adw::subclass::prelude::*;
use gtk::prelude::*;
use gtk::{gio, glib};
use log::{debug, error, info};
use passcore::PassStore;

mod imp {
    use gettextrs::gettext;
    use gtk::PasswordEntry;
    use passcore::exists_store_dir;

    use super::*;

    // Add to string
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Pages {
        ListPage,
        AskPage,
        TextPage,
        GitPage,
    }

    impl Default for Pages {
        fn default() -> Self {
            Pages::ListPage
        }
    }

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/noobping/PasswordStore/window.ui")]
    pub struct PasswordstoreWindow {
        #[template_child]
        pub window_title: TemplateChild<adw::WindowTitle>,

        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,

        #[template_child]
        pub navigation_view: TemplateChild<adw::NavigationView>,

        #[template_child]
        pub back_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub add_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub git_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub search_button: TemplateChild<gtk::Button>,

        // ① List page
        #[template_child]
        pub list_page: TemplateChild<adw::NavigationPage>,

        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,

        #[template_child]
        pub list: TemplateChild<gtk::ListBox>,

        // ② Ask for password page
        #[template_child]
        pub ask_page: TemplateChild<adw::NavigationPage>,

        #[template_child]
        pub password_entry: TemplateChild<PasswordEntry>,

        #[template_child]
        pub decrypt_button: TemplateChild<gtk::Button>,

        // ③ Text editor page
        #[template_child]
        pub text_page: TemplateChild<adw::NavigationPage>,

        #[template_child]
        pub text_view: TemplateChild<gtk::TextView>,

        #[template_child]
        pub path_entry: TemplateChild<gtk::Entry>,

        #[template_child]
        pub save_button: TemplateChild<gtk::Button>,

        // ④ Git clone page
        #[template_child]
        pub git_page: TemplateChild<adw::NavigationPage>,

        #[template_child]
        pub git_url_entry: TemplateChild<gtk::Entry>,

        #[template_child]
        pub git_clone_button: TemplateChild<gtk::Button>,
    }

    impl PasswordstoreWindow {
        fn init_list(&self, store: &PassStore) -> () {
            let items = store.list().unwrap_or_default();
            let list = self.list.clone();
            for id in items {
                let label = gtk::Label::new(Some(&id.replace("/", " / ")));
                label.set_halign(gtk::Align::Start);
                label.set_hexpand(true);
                label.set_wrap(true);
                label.set_wrap_mode(gtk::pango::WrapMode::Word);
                label.set_margin_bottom(5);
                label.set_margin_end(5);
                label.set_margin_start(5);
                label.set_margin_top(5);
                label.set_valign(gtk::Align::Center);
                label.set_vexpand(false);
                list.append(&label);
            }
        }

        pub fn refresh_list(&self) {
            let store = match PassStore::new() {
                Ok(store) => store,
                Err(e) => {
                    error!("Failed to open password store: {}", e);
                    PassStore::default()
                }
            };
            self.list.remove_all();
            self.init_list(&store);
            self.navigation_view
                .pop_to_page(&self.list_page.as_ref() as &adw::NavigationPage);
            self.update_navigation_buttons();
        }

        fn is_default_page(&self) -> bool {
            self.navigation_view.navigation_stack().n_items() <= 1
        }

        fn is_text_page(&self) -> bool {
            let last_page = self
                .navigation_view
                .navigation_stack()
                .iter::<adw::NavigationPage>()
                .last()
                .unwrap()
                .ok()
                .unwrap();
            last_page.as_ptr() == self.text_page.as_ptr()
        }

        fn update_navigation_buttons(&self) {
            self.save_button.set_can_focus(false);
            self.save_button.set_sensitive(false);
            self.save_button.set_visible(self.is_text_page());

            let default_page = self.is_default_page();
            self.add_button.set_can_focus(default_page);
            self.add_button.set_sensitive(default_page);
            self.add_button.set_visible(default_page);
            self.back_button.set_can_focus(!default_page);
            self.back_button.set_sensitive(!default_page);
            self.back_button.set_visible(!default_page);

            let exists_store = default_page && exists_store_dir();
            self.git_button.set_can_focus(default_page && !exists_store);
            self.git_button.set_sensitive(default_page && !exists_store);
            self.git_button.set_visible(default_page && !exists_store);
            self.search_button
                .set_can_focus(default_page && exists_store);
            self.search_button
                .set_sensitive(default_page && exists_store);
            self.search_button.set_visible(default_page && exists_store);

            let has_password = !self.password_entry.text().is_empty();
            self.decrypt_button.set_can_focus(has_password);
            self.decrypt_button.set_sensitive(has_password);

            let has_git_url = !self.git_url_entry.text().is_empty();
            self.git_clone_button.set_can_focus(has_git_url);
            self.git_clone_button.set_sensitive(has_git_url);
        }

        pub fn pop(&self) {
            debug!("Popping page");
            self.navigation_view.pop();
            self.update_navigation_buttons();
            if self.is_default_page() {
                self.set_path("".to_string());
            }
        }

        pub fn push(&self, page: Pages) {
            let page_ref = match page {
                Pages::ListPage => &self.list_page,
                Pages::AskPage => &self.ask_page,
                Pages::TextPage => &self.text_page,
                Pages::GitPage => &self.git_page,
            };
            debug!("Pushing page: {:?}", page);
            self.navigation_view
                .push(page_ref.as_ref() as &adw::NavigationPage);
            self.update_navigation_buttons();
        }

        pub fn add_new_password(&self) {
            let path = self.get_path();
            if !path.is_empty() {
                if path.contains('/') {
                    let last_slash = path.rfind('/').unwrap_or(path.len());
                    let new_path = path[..last_slash + 1].to_string();
                    self.path_entry.set_text(&new_path);
                    self.set_path(new_path);
                } else {
                    self.set_path("".to_string());
                }
            }

            let buffer = gtk::TextBuffer::new(None);
            let save_button = self.save_button.clone();
            buffer.connect_changed(move |buffer| {
                let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
                debug!("Text changed: {}", text);
                let is_not_empty = !text.is_empty();
                save_button.set_sensitive(is_not_empty);
                save_button.set_can_focus(is_not_empty);
            });
            self.text_view.set_buffer(Some(&buffer));
            self.push(Pages::TextPage);
            self.path_entry.grab_focus();
        }

        pub fn toggle_search(&self) {
            let entry = self.search_entry.clone();
            if !self.is_default_page() {
                self.navigation_view
                    .pop_to_page(&self.list_page.as_ref() as &adw::NavigationPage);
                entry.grab_focus();
                self.set_path("".to_string());
                entry.set_visible(true);
                self.update_navigation_buttons();
                return;
            }

            let visible = !entry.is_visible();
            entry.set_visible(visible);
            if visible {
                entry.grab_focus();
            } else {
                entry.set_text("");
            }
        }

        pub fn git_clone(&self) {
            let url = self.git_url_entry.text().to_string();
            if url.is_empty() {
                self.show_toast("Git URL cannot be empty");
                return;
            }
            let store = match PassStore::git_clone(url) {
                Ok(store) => store,
                Err(e) => {
                    let message = e.to_string();
                    let idx = message.find(';').unwrap_or(message.len());
                    let before_semicolon = &message[..idx];
                    self.show_toast(before_semicolon);
                    error!("Failed to clone git repository: {}", e);
                    PassStore::default()
                }
            };
            if store.ok() {
                self.init_list(&store);
                self.navigation_view
                    .pop_to_page(&self.list_page.as_ref() as &adw::NavigationPage);
                self.update_navigation_buttons();
            }
        }

        pub fn start_loading(&self) {
            info!("Loading...");
            self.add_button.set_can_focus(false);
            self.add_button.set_sensitive(false);
            self.back_button.set_can_focus(false);
            self.back_button.set_sensitive(false);
            self.decrypt_button.set_can_focus(false);
            self.decrypt_button.set_sensitive(false);
            self.git_button.set_can_focus(false);
            self.git_button.set_sensitive(false);
            self.password_entry.set_can_focus(false);
            self.password_entry.set_sensitive(false);
            self.search_button.set_can_focus(false);
            self.search_button.set_sensitive(false);
            self.save_button.set_can_focus(false);
            self.save_button.set_sensitive(false);
            self.text_view.set_editable(false);
            self.path_entry.set_can_focus(false);
            self.path_entry.set_sensitive(false);
            self.git_clone_button.set_can_focus(false);
            self.git_clone_button.set_sensitive(false);
        }

        pub fn stop_loading(&self) {
            info!("Done!");
            self.password_entry.set_can_focus(true);
            self.password_entry.set_sensitive(true);
            self.password_entry.grab_focus();
            self.text_view.set_editable(true);
            self.path_entry.set_can_focus(true);
            self.path_entry.set_sensitive(true);
            self.update_navigation_buttons();
        }

        pub fn show_toast(&self, message: &str) {
            let overlay = self.toast_overlay.clone();
            overlay.add_toast(adw::Toast::new(message));
        }

        pub fn get_path(&self) -> String {
            let subtitle = self.window_title.subtitle().to_string();
            let values = ["Manage your passwords", &gettext("subtitle")];
            return if subtitle.is_empty() || values.contains(&subtitle.as_str()) {
                String::from("")
            } else {
                subtitle
            };
        }

        pub fn set_path(&self, path: String) {
            if path.is_empty() {
                let translated = &gettext("subtitle");
                let subtitle = if translated.is_empty() || translated.contains("subtitle") {
                    &"Manage your passwords".to_string()
                } else {
                    translated
                };
                self.window_title.set_subtitle(&subtitle);
            } else {
                self.window_title.set_subtitle(path.trim());
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PasswordstoreWindow {
        const NAME: &'static str = "PasswordstoreWindow";
        type Type = super::PasswordstoreWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PasswordstoreWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Actions
            let obj_clone = obj.clone();
            let toggle_action = gio::SimpleAction::new("refresh", None);
            toggle_action.connect_activate(move |_, _| obj_clone.refresh_list());
            obj.add_action(&toggle_action);

            let obj_clone = obj.clone();
            let toggle_action = gio::SimpleAction::new("toggle-search", None);
            toggle_action.connect_activate(move |_, _| obj_clone.toggle_search());
            obj.add_action(&toggle_action);

            let obj_clone = obj.clone();
            let add_action = gio::SimpleAction::new("add-password", None);
            add_action.connect_activate(move |_, _| obj_clone.open_new_password());
            obj.add_action(&add_action);

            let obj_clone = obj.clone();
            let toggle_action = gio::SimpleAction::new("remove-password", None);
            toggle_action.connect_activate(move |_, _| {
                println!("Removing password: {}", obj_clone.get_path());
                let obj_clone2 = obj_clone.clone();
                glib::idle_add_local_once(move || {
                    obj_clone2.start_loading();
                    let path = obj_clone2.get_path();
                    if path.is_empty() {
                        obj_clone2.show_toast("Can not remove unknown password");
                        obj_clone2.stop_loading();
                        return;
                    }
                    println!("Removing password {}", path);
                    let store = match PassStore::new() {
                        Ok(store) => store,
                        Err(e) => {
                            obj_clone2.show_toast(&format!("Failed to open password store: {}", e));
                            PassStore::default()
                        }
                    };
                    if store.exists(&path) {
                        match store.remove(&path) {
                            Ok(_) => {
                                obj_clone2.show_toast(&format!("{} removed", path));
                                obj_clone2.refresh_list();
                            }
                            Err(e) => {
                                let message = e.to_string();
                                let idx = message.find(';').unwrap_or(message.len());
                                let before_semicolon = &message[..idx];
                                obj_clone2.show_toast(before_semicolon);
                                eprintln!("Failed to remove password: {}", e);
                            }
                        }
                    } else {
                        obj_clone2.show_toast("Password not found");
                    }
                    obj_clone2.stop_loading();
                });
            });
            obj.add_action(&toggle_action);

            let obj_clone = obj.clone();
            let add_action = gio::SimpleAction::new("save-password", None);
            add_action.connect_activate(move |_, _| {
                let obj_clone2 = obj_clone.clone();
                glib::idle_add_local_once(move || {
                    obj_clone2.start_loading();
                    let path = obj_clone2.get_path();
                    let new_path = obj_clone2.imp().path_entry.text().to_string();
                    if path.is_empty() {
                        obj_clone2.show_toast("Name the new password");
                        obj_clone2.stop_loading();
                        return;
                    }
                    info!("Saving password to {} from {}", new_path, path);
                    let store = match PassStore::new() {
                        Ok(store) => store,
                        Err(e) => {
                            obj_clone2.show_toast(&format!("Can not save password: {}", e));
                            PassStore::default()
                        }
                    };
                    let buffer = obj_clone2.imp().text_view.buffer();
                    // first line is password, the rest are extra
                    let lines = buffer
                        .text(&buffer.start_iter(), &buffer.end_iter(), false)
                        .lines()
                        .map(|s| s.to_string())
                        .collect::<Vec<String>>();
                    let password = lines.get(0).unwrap_or(&"".to_string()).to_string();
                    let extra = lines[1..].to_vec();
                    let item = passcore::Entry { password, extra };
                    let recipients = match store.get_recipients() {
                        Ok(recipients) => recipients,
                        Err(e) => {
                            obj_clone2.show_toast(&format!("Failed to get recipients: {}", e));
                            return;
                        }
                    };
                    if store.exists(&path) {
                        match store.update(&path, &item, &recipients) {
                            Ok(_) => {
                                if !new_path.is_empty() && path != new_path {
                                    match store.rename(&path, &new_path) {
                                        Ok(_) => {
                                            obj_clone2.show_toast(&format!(
                                                "Password updated and renamed to {}",
                                                new_path
                                            ));
                                            obj_clone2.refresh_list();
                                        }
                                        Err(e) => {
                                            let message = e.to_string();
                                            let idx = message.find(';').unwrap_or(message.len());
                                            let before_semicolon = &message[..idx];
                                            obj_clone2.show_toast(before_semicolon);
                                            error!("Failed to rename password: {}", e);
                                        }
                                    }
                                } else {
                                    obj_clone2.show_toast(&format!("Updated {}", path));
                                }
                            }
                            Err(e) => {
                                let message = e.to_string();
                                let idx = message.find(';').unwrap_or(message.len());
                                let before_semicolon = &message[..idx];
                                obj_clone2.show_toast(before_semicolon);
                                error!("Failed to update password: {}", e);
                            }
                        }
                    } else {
                        match store.add(&path, &item, &recipients) {
                            Ok(_) => {
                                obj_clone2.show_toast(&format!("Password {} added", path));
                                obj_clone2.refresh_list();
                            }
                            Err(e) => {
                                let message = e.to_string();
                                let idx = message.find(';').unwrap_or(message.len());
                                let before_semicolon = &message[..idx];
                                obj_clone2.show_toast(before_semicolon);
                                error!("Failed to add password: {}", e);
                            }
                        }
                    }
                    obj_clone2.stop_loading();
                });
            });
            obj.add_action(&add_action);

            let obj_clone = obj.clone();
            let add_action = gio::SimpleAction::new("decrypt-password", None);
            add_action.connect_activate(move |_, _| obj_clone.open_text_editor());
            obj.add_action(&add_action);

            let obj_clone = obj.clone();
            let add_action = gio::SimpleAction::new("back", None);
            add_action.connect_activate(move |_, _| obj_clone.pop());
            obj.add_action(&add_action);

            let obj_clone = obj.clone();
            let add_action = gio::SimpleAction::new("git-page", None);
            add_action.connect_activate(move |_, _| obj_clone.git_page());
            obj.add_action(&add_action);

            let obj_clone = obj.clone();
            let add_action = gio::SimpleAction::new("git-clone", None);
            add_action.connect_activate(move |_, _| {
                let obj_clone2 = obj_clone.clone();
                glib::idle_add_local_once(move || obj_clone2.git_clone());
            });
            obj.add_action(&add_action);

            let obj_clone = obj.clone();
            obj.imp().password_entry.connect_activate(move |_| {
                obj_clone.open_text_editor();
            });

            let obj_clone = obj.clone();
            obj.imp().git_url_entry.connect_activate(move |_| {
                let obj_clone2 = obj_clone.clone();
                glib::idle_add_local_once(move || obj_clone2.git_clone());
            });

            let obj_clone = obj.clone();
            glib::idle_add_local_once(move || {
                let store = match PassStore::new() {
                    Ok(store) => store,
                    Err(e) => {
                        error!("Failed to open password store: {}", e);
                        PassStore::default()
                    }
                };

                // synchronize action
                let obj_clone2 = obj_clone.clone();
                let overlay = obj_clone.imp().toast_overlay.clone();
                let sync_action = gio::SimpleAction::new("synchronize", None);
                let store_clone = store.clone();
                sync_action.connect_activate(move |_, _| {
                    obj_clone2.start_loading();
                    info!("Synchronizing...");
                    match store_clone.sync() {
                        Ok(_) => overlay.add_toast(adw::Toast::new("Synchronized successfully")),
                        Err(e) => {
                            let message = e.to_string();
                            let idx = message.find(';').unwrap_or(message.len());
                            let before_semicolon = &message[..idx];

                            overlay.add_toast(adw::Toast::new(before_semicolon));
                            error!("Failed to synchronize: {}", e);
                        }
                    }
                    obj_clone2.stop_loading();
                    obj_clone2.imp().init_list(&store_clone);
                });
                obj_clone.add_action(&sync_action);

                obj_clone.imp().init_list(&store); // Initialize store and list
                obj_clone.imp().update_navigation_buttons();
            });

            // Select a password with the activated signal
            let obj_clone = obj.clone();
            self.list.connect_row_activated(move |_, row| {
                if let Some(inner) = row.child() {
                    if let Ok(label) = inner.downcast::<gtk::Label>() {
                        let path = label.text().to_string().replace(" / ", "/");
                        debug!("Selected: {}", path);
                        obj_clone.set_path(path.clone());
                        obj_clone.open_text_editor_or_ask_password();
                        return;
                    }
                }
                obj_clone.show_toast("Failed to open password");
            });

            // Selected a password with the keyboard
            let obj_clone = obj.clone();
            self.list.connect_row_selected(move |_, row| {
                if let Some(row) = row {
                    let inner = row.child().unwrap();
                    if let Ok(label) = inner.downcast::<gtk::Label>() {
                        let path = label.text().to_string().replace(" / ", "/");
                        debug!("Selected: {}", path);
                        obj_clone.set_path(path.clone());
                        return;
                    }
                }
            });

            // Real-time filter: hide/show rows based on search text
            let list = self.list.clone();
            let search = self.search_entry.clone();
            search.connect_changed(move |entry| {
                let pattern = entry
                    .text()
                    .to_string()
                    .to_lowercase()
                    .trim()
                    .replace("/", " / ");

                // Walk each row in the ListBox
                let mut row_widget = list.first_child();
                while let Some(w) = row_widget.take() {
                    // Prepare for next iteration
                    row_widget = w.next_sibling();

                    // Downcast to ListBoxRow
                    if let Ok(row) = w.clone().downcast::<gtk::ListBoxRow>() {
                        // Get the widget you originally packed (your Label)
                        if let Some(inner) = row.child() {
                            if let Ok(label) = inner.downcast::<gtk::Label>() {
                                let text = label.text().to_string().to_lowercase();
                                // Show/hide the entire row
                                row.set_visible(text.contains(&pattern));
                            }
                        }
                    }
                }
            });

            // Enable or disable the buttons if the entry is empty
            let obj_clone = obj.clone();
            let password_entry = self.password_entry.clone();
            password_entry.connect_changed(move |entry| {
                let text = entry.text().to_string();
                let is_not_empty = !text.is_empty();
                obj_clone.imp().decrypt_button.set_sensitive(is_not_empty);
                obj_clone.imp().decrypt_button.set_can_focus(is_not_empty);
            });

            let obj_clone = obj.clone();
            let git_url_entry = self.git_url_entry.clone();
            git_url_entry.connect_changed(move |entry| {
                let text = entry.text().to_string();
                let is_not_empty = !text.is_empty();
                obj_clone.imp().git_clone_button.set_sensitive(is_not_empty);
                obj_clone.imp().git_clone_button.set_can_focus(is_not_empty);
            });

            let obj_clone = obj.clone();
            let path_entry = self.path_entry.clone();
            path_entry.connect_changed(move |entry| {
                let text = entry.text().to_string();
                obj_clone.set_path(text.clone());
                let is_not_empty = !text.is_empty();
                obj_clone.imp().save_button.set_sensitive(is_not_empty);
                obj_clone.imp().save_button.set_can_focus(is_not_empty);
            });
        }
    }

    impl WidgetImpl for PasswordstoreWindow {}
    impl WindowImpl for PasswordstoreWindow {}
    impl ApplicationWindowImpl for PasswordstoreWindow {}
    impl AdwApplicationWindowImpl for PasswordstoreWindow {}
}

glib::wrapper! {
    pub struct PasswordstoreWindow(ObjectSubclass<imp::PasswordstoreWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl PasswordstoreWindow {
    pub fn new<P: IsA<gtk::Application>>(application: &P) -> Self {
        glib::Object::builder()
            .property("application", application)
            .build()
    }

    pub fn git_page(&self) {
        self.imp().push(imp::Pages::GitPage);
    }

    pub fn git_clone(&self) {
        self.imp().git_clone();
    }

    pub fn toggle_search(&self) {
        self.imp().toggle_search();
    }

    pub fn open_new_password(&self) {
        self.imp().add_new_password();
    }

    pub fn open_text_editor_or_ask_password(&self) {
        let passphrase = self.imp().password_entry.text().to_string();
        if passphrase.is_empty() {
            self.push(imp::Pages::AskPage);
            return;
        }
        self.open_text_editor();
    }

    pub fn open_text_editor(&self) {
        self.start_loading();
        info!("Opening text editor for {}", self.get_path());

        let passphrase = self.imp().password_entry.text().to_string();
        if passphrase.is_empty() {
            let ask_page = self.imp().ask_page.clone();
            self.stop_loading();
            if !&ask_page.is_visible() {
                self.push(imp::Pages::AskPage);
            }
            self.show_toast("Passphrase cannot be empty");
            return;
        }

        let obj_clone = self.clone();
        glib::idle_add_local_once(move || {
            let path = obj_clone.get_path();
            obj_clone.imp().path_entry.set_text(&path);

            let store = match PassStore::new() {
                Ok(store) => store,
                Err(e) => {
                    obj_clone.stop_loading();
                    obj_clone.show_toast(&format!("Failed to open password store: {}", e));
                    return;
                }
            };
            if !store.exists(&path) {
                obj_clone.show_toast("Password not found");
                let list_page = obj_clone.imp().list_page.clone();
                obj_clone.stop_loading();
                if !&list_page.is_visible() {
                    obj_clone.push(imp::Pages::ListPage);
                }
                return;
            }

            match store.get(&path, passphrase.as_str()) {
                Ok(item) => {
                    debug!("Password: {}", item.password);
                    // Pass item to the text view
                    // Add item.password to the first line of the text view
                    // Add the item.extra (a list of strings) after that.
                    let mut text = item.password;
                    for line in item.extra {
                        text.push_str(&format!("\n{}", line));
                    }
                    let buffer = gtk::TextBuffer::new(None);
                    buffer.set_text(&text);
                    let save_button = obj_clone.imp().save_button.clone();
                    buffer.connect_changed(move |buffer| {
                        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
                        debug!("Text changed: {}", text);
                        let is_not_empty = !text.is_empty();
                        save_button.set_sensitive(is_not_empty);
                        save_button.set_can_focus(is_not_empty);
                    });
                    let text_view = obj_clone.imp().text_view.clone();
                    text_view.set_buffer(Some(&buffer));
                    obj_clone.stop_loading();
                    // Open the text page so that I can view (or edit) the encqrypted password file
                    obj_clone.push(imp::Pages::TextPage);
                }
                Err(e) => {
                    let message = e.to_string();
                    let idx = message.find(';').unwrap_or(message.len());
                    let before_semicolon = &message[..idx];
                    obj_clone.stop_loading();
                    obj_clone.show_toast(before_semicolon);
                    error!("Failed to open password: {}", e);
                    obj_clone.imp().password_entry.set_text("");
                    obj_clone.push(imp::Pages::AskPage);
                    obj_clone.imp().password_entry.grab_focus();
                }
            }
        });
    }

    pub fn refresh_list(&self) {
        self.imp().refresh_list();
    }

    fn stop_loading(&self) {
        self.imp().stop_loading();
    }

    fn start_loading(&self) {
        self.imp().start_loading();
    }

    fn pop(&self) {
        self.imp().pop();
    }

    fn push(&self, page: imp::Pages) {
        self.imp().push(page);
    }

    fn get_path(&self) -> String {
        self.imp().get_path()
    }

    fn set_path(&self, path: String) {
        self.imp().set_path(path.clone());
    }

    fn show_toast(&self, message: &str) {
        self.imp().show_toast(message);
    }
}
