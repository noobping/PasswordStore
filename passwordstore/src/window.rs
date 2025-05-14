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
use passcore::{PassStore, StringExt};
use secrecy::SecretString;

mod imp {
    use adw::prelude::{ActionRowExt, EntryRowExt, PreferencesRowExt};
    use gettextrs::gettext;
    use passcore::exists_store_dir;
    use secrecy::{zeroize::Zeroize, ExposeSecret};
    use std::sync::Mutex;

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
        pub passphrase_entry: TemplateChild<adw::PasswordEntryRow>,

        // ③ Text editor page
        #[template_child]
        pub text_page: TemplateChild<adw::NavigationPage>,

        #[template_child]
        pub text_view: TemplateChild<gtk::TextView>,

        #[template_child]
        pub path_entry: TemplateChild<adw::EntryRow>,

        #[template_child]
        pub save_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub password_entry: TemplateChild<adw::PasswordEntryRow>,

        #[template_child]
        pub dynamic_box: TemplateChild<gtk::Box>,

        // ④ Git clone page
        #[template_child]
        pub git_page: TemplateChild<adw::NavigationPage>,

        #[template_child]
        pub git_url_entry: TemplateChild<adw::EntryRow>,

        passphrase: Mutex<SecretString>,
    }

    impl PasswordstoreWindow {
        pub fn is_passphrase_empty(&self) -> bool {
            self.passphrase
                .try_lock()
                .is_ok_and(|guard| guard.expose_secret().is_empty())
        }

        pub fn get_passphrase(&self) -> SecretString {
            match self.passphrase.try_lock() {
                Ok(guard) => guard.clone(),
                Err(_) => SecretString::new("".into()),
            }
        }

        pub fn clear_passphrase(&self) {
            self.passphrase_entry.set_text("");
            match self.passphrase.try_lock() {
                Ok(mut guard) => guard.zeroize(),
                Err(_) => self.show_toast("Failed to clear passphrase"),
            }
        }

        fn set_passphrase(&self, secret: SecretString) {
            self.passphrase_entry.set_text("");
            match self.passphrase.try_lock() {
                Ok(mut guard) => *guard = secret,
                Err(_) => self.show_toast("Failed to set passphrase"),
            }
        }

        pub fn ask_or_decrypt(&self) {
            if self.is_passphrase_empty() {
                self.push(imp::Pages::AskPage);
                return;
            }
            self.decrypt_and_open();
        }

        pub fn decrypt_and_open(&self) {
            self.start_loading();
            if self.is_text_page() {
                self.pop();
            }
            let path = self.get_path();
            self.path_entry.set_text(&path);
            self.path_entry.grab_focus();

            let passphrase = self.get_passphrase();
            let obj_clone = self.to_owned();
            glib::idle_add_local_once(move || {
                let store = match PassStore::new() {
                    Ok(store) => store,
                    Err(e) => {
                        error!("Failed to open password store: {}", e);
                        obj_clone.stop_loading();
                        obj_clone.show_toast(&format!("Failed to open password store: {}", e));
                        return;
                    }
                };
                if !store.exists(&path) {
                    obj_clone.show_toast("Password not found");
                    let list_page = obj_clone.list_page.clone();
                    obj_clone.stop_loading();
                    if !&list_page.is_visible() {
                        obj_clone.pop();
                    }
                    return;
                }
                match store.get(&path, passphrase) {
                    Ok(entry) => {
                        let password = entry.password.expose_secret();
                        obj_clone.password_entry.set_text(password);

                        let mut text = String::new();
                        for line in entry.extra.iter() {
                            let exposed = line.expose_secret();
                            if exposed.contains(':') {
                                let (field, value) = &exposed.to_string().split_field();
                                let row = adw::EntryRow::builder()
                                    .title(field)
                                    .margin_start(15)
                                    .margin_end(15)
                                    .margin_bottom(5)
                                    .build();
                                row.set_text(value);
                                let obj_clone2 = obj_clone.clone().to_owned();
                                row.connect_changed(move |row| {
                                    let text = row.text().to_string();
                                    obj_clone2.save_button.set_sensitive(!text.is_empty());
                                    obj_clone2.save_button.set_can_focus(!text.is_empty());
                                });
                                obj_clone.dynamic_box.append(&row);
                            } else {
                                text.push_str(&format!("{}\n", exposed));
                            }
                        }
                        let buffer = gtk::TextBuffer::new(None);
                        buffer.set_text(&text);
                        let save_button = obj_clone.save_button.clone();
                        buffer.connect_changed(move |buffer| {
                            let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
                            let is_not_empty = !text.is_empty();
                            save_button.set_sensitive(is_not_empty);
                            save_button.set_can_focus(is_not_empty);
                        });
                        let text_view = obj_clone.text_view.clone();
                        text_view.set_buffer(Some(&buffer));
                        obj_clone.stop_loading();
                        obj_clone.push(imp::Pages::TextPage);
                    }
                    Err(e) => {
                        error!("Failed to open password: {}", e);
                        let message = e.to_string();
                        let idx = message.find(';').unwrap_or(message.len());
                        let before_semicolon = &message[..idx];
                        obj_clone.stop_loading();
                        obj_clone.show_toast(before_semicolon);

                        obj_clone.clear_passphrase();
                        obj_clone.push(imp::Pages::AskPage);
                        obj_clone.passphrase_entry.grab_focus();
                    }
                }
            });
        }

        fn init_list(&self, store: &PassStore) -> () {
            let items = store.list().unwrap_or_default();
            let list = self.list.clone();
            for id in items {
                let (path, name) = id.clone().split_path();
                let row = adw::ActionRow::builder()
                    .title(&name)
                    .subtitle(&path.replace("/", " / "))
                    .activatable(true)
                    .build();

                let id_clone = id.clone();
                let obj_clone = self.to_owned();
                row.connect_activated(move |row| {
                    let title = row.title();
                    let subtitle = row.subtitle().unwrap_or_default();
                    info!("Select {} in {}", title, subtitle);
                    obj_clone.set_path(id_clone.clone());
                    obj_clone.path_entry.set_text(&id_clone);
                    obj_clone.path_entry.grab_focus();
                    obj_clone.ask_or_decrypt();
                });

                // build the menu model
                let menu = gio::Menu::new();
                // COPY
                let copy_item =
                    gio::MenuItem::new(Some("Copy password"), Some("win.copy-password"));
                copy_item.set_attribute_value("target", Some(&id.to_variant()));
                menu.append_item(&copy_item);
                // RENAME
                let rename_item = gio::MenuItem::new(Some("Rename…"), Some("win.rename-password"));
                rename_item.set_attribute_value("target", Some(&id.to_variant()));
                menu.append_item(&rename_item);
                // DELETE (destructive section)
                let delete_item = gio::MenuItem::new(Some("Delete"), Some("win.remove-password"));
                delete_item.set_attribute_value("target", Some(&id.to_variant()));
                // mark destructive so it’s red
                delete_item.set_attribute_value("section", Some(&"destructive".to_variant()));
                menu.append_item(&delete_item);

                // attach it to a “three-dots” button
                let menu_button = gtk::MenuButton::builder()
                    .icon_name("view-more-symbolic")
                    .menu_model(&menu)
                    .build();
                row.add_suffix(&menu_button);

                list.append(&row);
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
            if store.ok() {
                self.list.remove_all();
                self.init_list(&store);
                self.pop();
            }
        }

        fn is_default_page(&self) -> bool {
            self.navigation_view.navigation_stack().n_items() <= 1
        }

        pub fn is_text_page(&self) -> bool {
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
        }

        pub fn pop(&self) {
            debug!("Popping page");
            if self.is_text_page() {
                self.text_view.set_buffer(Some(&gtk::TextBuffer::new(None)));
                self.path_entry.set_text("");
                self.password_entry.set_text("");
                while let Some(child) = self.dynamic_box.first_child() {
                    self.dynamic_box.remove(&child);
                }
            }
            if self.is_default_page() {
                self.path_entry.set_text("");
            } else {
                self.navigation_view
                    .pop_to_page(&self.list_page.as_ref() as &adw::NavigationPage);
            }
            self.update_navigation_buttons();
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
            if self.is_text_page() {
                self.text_view.set_buffer(Some(&gtk::TextBuffer::new(None)));
                self.password_entry.set_text("");
                while let Some(child) = self.dynamic_box.first_child() {
                    self.dynamic_box.remove(&child);
                }
            }
            let (path, _) = self.get_path().split_path();
            let path = path + "/";
            self.path_entry.set_text(&path);
            self.set_path(path);

            let buffer = gtk::TextBuffer::new(None);
            buffer.set_text(&"username: ");
            let save_button = self.save_button.clone();
            buffer.connect_changed(move |buffer| {
                let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
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
                self.pop();
                entry.grab_focus();
                self.set_path("".to_string());
                entry.set_visible(true);
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
            let store = match PassStore::from_git(url) {
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
                self.pop();
            }
        }

        pub fn start_loading(&self) {
            info!("Loading...");
            self.add_button.set_can_focus(false);
            self.add_button.set_sensitive(false);
            self.back_button.set_can_focus(false);
            self.back_button.set_sensitive(false);
            self.git_button.set_can_focus(false);
            self.git_button.set_sensitive(false);
            self.passphrase_entry.set_can_focus(false);
            self.passphrase_entry.set_sensitive(false);
            self.search_button.set_can_focus(false);
            self.search_button.set_sensitive(false);
            self.save_button.set_can_focus(false);
            self.save_button.set_sensitive(false);
            self.text_view.set_editable(false);
            self.path_entry.set_can_focus(false);
            self.path_entry.set_sensitive(false);
        }

        pub fn stop_loading(&self) {
            info!("Done!");
            self.passphrase_entry.set_can_focus(true);
            self.passphrase_entry.set_sensitive(true);
            self.passphrase_entry.grab_focus();
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

        pub fn add_or_update_password(&self) {
            self.start_loading();
            let path = self.get_path();
            let new_path = self.path_entry.text().to_string();
            if path.is_empty() {
                self.show_toast("Name the new password");
                self.stop_loading();
                return;
            }
            let store = match PassStore::new() {
                Ok(store) => store,
                Err(e) => {
                    self.show_toast(&format!("Can not save password: {}", e));
                    return;
                }
            };
            let entry = self.to_store_entry();
            let saved: bool = self.save_pass(&store, &path, &entry);
            let renamed = if !new_path.is_empty() && path != new_path {
                self.rename_pass(&path, &new_path)
            } else {
                false
            };

            if saved || renamed {
                self.refresh_list();
            }
            self.stop_loading();
            self.pop();
        }

        fn to_store_entry(&self) -> passcore::Entry {
            let password = SecretString::from(self.password_entry.text().to_string());

            let mut children = Vec::new();
            let mut maybe_child = self.dynamic_box.first_child();
            while let Some(child) = maybe_child {
                children.push(child.clone());
                maybe_child = child.next_sibling();
            }

            let mut extra = Vec::new();
            for widget in children {
                if let Ok(entry) = widget.downcast::<adw::EntryRow>() {
                    let field = entry.title().trim().to_owned();
                    let value = entry.text().trim().to_owned();
                    extra.push(SecretString::from(format!("{}: {}", field, value)));
                }
            }
            let buffer = self.text_view.buffer();
            let mut lines = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .lines()
                .map(|s| SecretString::from(s.to_string()))
                .collect::<Vec<_>>();
            extra.append(&mut lines);

            passcore::Entry { password, extra }
        }

        fn save_pass(&self, store: &PassStore, path: &String, entry: &passcore::Entry) -> bool {
            return match store.add(&path, &entry) {
                Ok(_) => {
                    self.show_toast(&format!("Password {} saved", path));
                    true
                }
                Err(e) => {
                    let message = e.to_string();
                    let idx = message.find(';').unwrap_or(message.len());
                    let before_semicolon = &message[..idx];
                    self.show_toast(before_semicolon);
                    error!("Failed to save password: {}", e);
                    false
                }
            };
        }

        fn rename_pass(&self, path: &String, new_path: &String) -> bool {
            let store = match PassStore::new() {
                Ok(store) => store,
                Err(e) => {
                    self.show_toast(&format!("Failed to open password store: {}", e));
                    return false;
                }
            };
            if !store.ok() || !store.exists(&path) {
                self.show_toast("Password not found");
                return false;
            }
            return match store.rename(&path, &new_path) {
                Ok(_) => {
                    self.show_toast(&format!("Password {} renamed to {}", path, new_path));
                    true
                }
                Err(e) => {
                    let message = e.to_string();
                    let idx = message.find(';').unwrap_or(message.len());
                    let before_semicolon = &message[..idx];
                    self.show_toast(before_semicolon);
                    error!("Failed to rename password: {}", e);
                    false
                }
            };
        }

        fn remove_pass(&self, path: &String) -> bool {
            let store = match PassStore::new() {
                Ok(store) => store,
                Err(e) => {
                    self.show_toast(&format!("Failed to open password store: {}", e));
                    return false;
                }
            };
            if !store.ok() || !store.exists(&path) {
                self.show_toast("Password not found");
                return false;
            }
            return match store.remove(&path) {
                Ok(_) => {
                    self.show_toast(&format!("Password {} removed", path));
                    true
                }
                Err(e) => {
                    let message = e.to_string();
                    let idx = message.find(';').unwrap_or(message.len());
                    let before_semicolon = &message[..idx];
                    self.show_toast(before_semicolon);
                    error!("Failed to remove password: {}", e);
                    false
                }
            };
        }

        fn copy_pass(&self, path: &String) -> bool {
            let store = match PassStore::new() {
                Ok(store) => store,
                Err(e) => {
                    self.show_toast(&format!("Failed to open password store: {}", e));
                    return false;
                }
            };
            if !store.ok() || !store.exists(&path) {
                self.show_toast("Password not found");
                return false;
            }
            let entry = match store.get(&path, self.get_passphrase()) {
                Ok(entry) => entry,
                Err(e) => {
                    let message = e.to_string();
                    let idx = message.find(';').unwrap_or(message.len());
                    let before_semicolon = &message[..idx];
                    self.show_toast(before_semicolon);
                    error!("Failed to copy password: {}", e);
                    return false;
                }
            };
            let password = entry.password.expose_secret();
            let clipboard = gtk::gdk::Display::default().unwrap().clipboard();
            clipboard.set_text(&password);
            self.show_toast(&format!("Password {} copied", path));
            return true;
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
            toggle_action.connect_activate(move |_, _| {
                let obj_clone2 = obj_clone.clone();
                glib::idle_add_local_once(move || obj_clone2.imp().refresh_list());
            });
            obj.add_action(&toggle_action);

            let obj_clone = obj.clone();
            let toggle_action = gio::SimpleAction::new("toggle-search", None);
            toggle_action.connect_activate(move |_, _| obj_clone.imp().toggle_search());
            obj.add_action(&toggle_action);

            let obj_clone = obj.clone();
            let add_action = gio::SimpleAction::new("add-password", None);
            add_action.connect_activate(move |_, _| obj_clone.imp().add_new_password());
            obj.add_action(&add_action);

            let obj_clone = obj.clone();
            let toggle_action = gio::SimpleAction::new("remove-selected-password", None);
            toggle_action.connect_activate(move |_, _| {
                info!("Removing selected password: {}", obj_clone.imp().get_path());
                let obj_clone2 = obj_clone.clone();
                glib::idle_add_local_once(move || {
                    obj_clone2.imp().start_loading();
                    let path = obj_clone2.imp().get_path();
                    if obj_clone2.imp().remove_pass(&path) {
                        obj_clone2.imp().set_path("".to_string());
                        obj_clone2.imp().refresh_list();
                        obj_clone2.imp().update_navigation_buttons();
                    }
                    obj_clone2.imp().stop_loading();
                });
            });
            obj.add_action(&toggle_action);

            let obj_clone = obj.clone();
            let add_action = gio::SimpleAction::new("save-password", None);
            add_action.connect_activate(move |_, _| {
                let obj_clone2 = obj_clone.clone();
                glib::idle_add_local_once(move || obj_clone2.imp().add_or_update_password());
            });
            obj.add_action(&add_action);

            let obj_clone = obj.clone();
            let add_action = gio::SimpleAction::new("decrypt-password", None);
            add_action.connect_activate(move |_, _| obj_clone.imp().decrypt_and_open());
            obj.add_action(&add_action);

            let obj_clone = obj.clone();
            let add_action = gio::SimpleAction::new("back", None);
            add_action.connect_activate(move |_, _| obj_clone.imp().pop());
            obj.add_action(&add_action);

            let obj_clone = obj.clone();
            let add_action = gio::SimpleAction::new("git-page", None);
            add_action.connect_activate(move |_, _| obj_clone.imp().push(Pages::GitPage));
            obj.add_action(&add_action);

            let obj_clone = obj.clone();
            let add_action = gio::SimpleAction::new("git-clone", None);
            add_action.connect_activate(move |_, _| {
                let obj_clone2 = obj_clone.clone();
                glib::idle_add_local_once(move || obj_clone2.imp().git_clone());
            });
            obj.add_action(&add_action);

            let obj_clone = obj.clone();
            obj.imp().git_url_entry.connect_activate(move |_| {
                let obj_clone2 = obj_clone.clone();
                glib::idle_add_local_once(move || obj_clone2.imp().git_clone());
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

                obj_clone.imp().init_list(&store); // Initialize store and list
                obj_clone.imp().update_navigation_buttons();

                // synchronize action
                let obj_clone2 = obj_clone.clone();
                let sync_action = gio::SimpleAction::new("synchronize", None);
                let store_clone = store.clone();
                sync_action.connect_activate(move |_, _| {
                    obj_clone2.imp().start_loading();
                    let overlay = obj_clone2.imp().toast_overlay.clone();
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
                    obj_clone2.imp().stop_loading();
                    obj_clone2.imp().init_list(&store_clone);
                });
                obj_clone.add_action(&sync_action);
            });

            // Real-time filter: hide/show action rows based on search text
            let list = self.list.clone();
            let search = self.search_entry.clone();
            search.connect_changed(move |entry| {
                let binding = entry.text().to_string().to_lowercase();
                let pattern = binding.trim();

                // Iterate through álle children van je list-container
                let mut child = list.first_child();
                while let Some(widget) = child.take() {
                    // Sla alvast de volgende op
                    child = widget.next_sibling();

                    // Probeer rechtstreeks naar ActionRow te downcasten
                    if let Ok(row) = widget.clone().downcast::<adw::ActionRow>() {
                        let title = row.title().to_lowercase();
                        row.set_visible(title.contains(&pattern));
                    }
                }
            });

            // COPY
            let obj_clone = obj.clone();
            let copy =
                gio::SimpleAction::new("copy-password", Some(&String::static_variant_type()));
            copy.connect_activate(move |_, param| {
                let path: String = param.and_then(|v| v.str().map(str::to_string)).unwrap();
                if obj_clone.imp().is_passphrase_empty() {
                    obj_clone.imp().set_path(path.clone());
                    obj_clone.imp().push(Pages::AskPage);
                } else {
                    obj_clone.imp().copy_pass(&path);
                }
            });
            obj.add_action(&copy);

            // rename
            let obj_clone = obj.clone();
            let rename =
                gio::SimpleAction::new("rename-password", Some(&String::static_variant_type()));
            rename.connect_activate(move |_, param| {
                let path: String = param.and_then(|v| v.str().map(str::to_string)).unwrap();
                if obj_clone.imp().is_passphrase_empty() {
                    obj_clone.imp().set_path(path.clone());
                    obj_clone.imp().push(Pages::AskPage);
                } else if obj_clone.imp().rename_pass(&path, &path) {
                    obj_clone.imp().refresh_list();
                    obj_clone.imp().update_navigation_buttons();
                }
            });
            obj.add_action(&rename);

            // DELETE
            let obj_clone = obj.clone();
            let remove =
                gio::SimpleAction::new("remove-password", Some(&String::static_variant_type()));
            remove.connect_activate(move |_, param| {
                let path: String = param.and_then(|v| v.str().map(str::to_string)).unwrap();
                if obj_clone.imp().remove_pass(&path) {
                    obj_clone.imp().refresh_list();
                    obj_clone.imp().update_navigation_buttons();
                }
            });
            obj.add_action(&remove);

            // Enable or disable the buttons if the entry is empty
            let obj_clone = obj.clone();
            self.passphrase_entry.connect_apply(move |row| {
                obj_clone.imp().set_passphrase(row.text().trim().into());
                obj_clone.imp().decrypt_and_open();
            });

            let obj_clone = obj.clone();
            self.git_url_entry
                .connect_apply(move |_row| obj_clone.imp().git_clone());

            let obj_clone = obj.clone();
            let path_entry = self.path_entry.clone();
            path_entry.connect_changed(move |entry| {
                let is_not_empty = !entry.text().to_string().is_empty();
                obj_clone.imp().save_button.set_sensitive(is_not_empty);
                obj_clone.imp().save_button.set_can_focus(is_not_empty);
            });

            let obj_clone = obj.clone();
            let password_entry = self.password_entry.clone();
            password_entry.connect_changed(move |entry| {
                let is_not_empty = !entry.text().to_string().is_empty();
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
            .property("icon-name", "io.github.noobping.PasswordStore")
            .build()
    }
}
