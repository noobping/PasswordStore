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
use gtk::glib::MainContext;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::data::AppData;

pub fn run<F, R>(work: impl FnOnce() -> R + Send + 'static, update_ui: F)
where
    R: Send + 'static,
    F: FnOnce(R) + Send + 'static,
{
    let work = Arc::new(Mutex::new(Some(work)));
    thread::spawn(move || {
        let result = {
            let mut guard = work.lock().unwrap();
            let work_fn = guard.take().expect("Closure should be present");
            work_fn()
        };
        MainContext::default().invoke(move || update_ui(result));
    });
}

mod imp {
    use adw::prelude::{EntryRowExt, PreferencesRowExt};
    use gettextrs::gettext;
    use passcore::exists_store_dir;

    use super::*;

    // Add to string
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Pages {
        ListPage,
        TextPage,
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
        pub progress_bar: TemplateChild<gtk::ProgressBar>,

        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,

        #[template_child]
        pub navigation_view: TemplateChild<adw::NavigationView>,

        #[template_child]
        pub back_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub add_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub add_button_popover: TemplateChild<gtk::Popover>,

        #[template_child]
        pub git_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub search_button: TemplateChild<gtk::Button>,

        // List page
        #[template_child]
        pub list_page: TemplateChild<adw::NavigationPage>,

        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,

        #[template_child]
        pub list: TemplateChild<gtk::ListBox>,

        #[template_child]
        pub spinner: TemplateChild<gtk::Spinner>,

        // Ask for password page
        #[template_child]
        pub passphrase_popover: TemplateChild<gtk::Popover>,

        #[template_child]
        pub passphrase_entry: TemplateChild<adw::PasswordEntryRow>,

        // Text editor page
        #[template_child]
        pub text_page: TemplateChild<adw::NavigationPage>,

        #[template_child]
        pub text_view: TemplateChild<gtk::TextView>,

        #[template_child]
        pub path_entry: TemplateChild<adw::EntryRow>,

        #[template_child]
        pub new_path_entry: TemplateChild<adw::EntryRow>,

        #[template_child]
        pub rename_popover: TemplateChild<gtk::Popover>,

        #[template_child]
        pub save_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub password_entry: TemplateChild<adw::PasswordEntryRow>,

        #[template_child]
        pub copy_password_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub dynamic_box: TemplateChild<gtk::Box>,

        // Git clone page
        #[template_child]
        pub git_popover: TemplateChild<gtk::Popover>,

        #[template_child]
        pub git_url_entry: TemplateChild<adw::EntryRow>,
    }

    impl PasswordstoreWindow {
        pub fn toggle_search(&self) {
            let entry = self.search_entry.clone();
            if !self.is_default_page() {
                self.pop();
                entry.grab_focus();
                Self::update_title(&self.window_title, "".to_string());
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

        pub fn pop(&self) {
            if self.is_text_page() {
                self.text_view.set_buffer(Some(&gtk::TextBuffer::new(None)));
                self.password_entry.set_text("");
                Self::update_title(&self.window_title, "".to_string());
                AppData::instance(|data: &mut AppData| data.set_path(""));
                while let Some(child) = self.dynamic_box.first_child() {
                    self.dynamic_box.remove(&child);
                }
            }
            if !self.is_default_page() {
                self.navigation_view
                    .pop_to_page(&self.list_page.as_ref() as &adw::NavigationPage);
            }
            self.update_navigation_buttons();
        }

        pub fn push(&self, page: Pages) {
            let page_ref = match page {
                Pages::ListPage => &self.list_page,
                Pages::TextPage => &self.text_page,
            };
            self.navigation_view
                .push(page_ref.as_ref() as &adw::NavigationPage);
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
        }

        fn load(&self) {
            self.progress_bar.set_visible(true);
            self.progress_bar.pulse();
            self.progress_bar.set_pulse_step(10.0);
            self.spinner.start();
            self.spinner.set_visible(true);
            self.add_button.set_can_focus(false);
            self.add_button.set_sensitive(false);
            self.git_button.set_can_focus(false);
            self.git_button.set_sensitive(false);
            self.search_button.set_can_focus(false);
            self.search_button.set_sensitive(false);
            self.save_button.set_can_focus(false);
            self.save_button.set_sensitive(false);
            self.text_view.set_editable(false);
            self.path_entry.set_can_focus(false);
            self.path_entry.set_sensitive(false);
        }

        fn done(&self) {
            self.text_view.set_editable(true);
            self.path_entry.set_can_focus(true);
            self.path_entry.set_sensitive(true);
            self.progress_bar.set_visible(false);
            self.spinner.stop();
            self.spinner.set_visible(false);
            self.update_navigation_buttons();
        }

        fn toast(&self, message: &str) {
            let overlay = self.toast_overlay.clone();
            overlay.add_toast(adw::Toast::new(message));
        }

        fn refresh_list(&self) {
            self.list.remove_all();
            AppData::instance(|data| {
                let list_clone = self.list.clone();
                let title_clone = self.window_title.clone();
                let title_clone2 = title_clone.clone();
                match data.build_list(
                    &list_clone,
                    move |path| {
                        Self::update_title(&title_clone, path.clone());
                    },
                    move |path| {
                        Self::update_title(&title_clone2, path.clone());
                    },
                ) {
                    Ok(_) => {
                        self.done();
                    }
                    Err(e) => {
                        self.done();
                        let message = e.to_string();
                        let idx = message.find(';').unwrap_or(message.len());
                        let before_semicolon = &message[..idx];
                        self.toast(before_semicolon);
                    }
                }
            });
            self.pop();
            self.done();
        }

        fn update_title(title: &adw::WindowTitle, path: String) {
            if path.is_empty() {
                let translated = &gettext("subtitle");
                let subtitle = if translated.is_empty() || translated.contains("subtitle") {
                    &"Manage your passwords".to_string()
                } else {
                    translated
                };
                title.set_subtitle(&subtitle);
            } else {
                title.set_subtitle(path.trim());
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
            obj.imp().load();

            obj.imp().add_button_popover.unparent();
            obj.imp()
                .add_button_popover
                .set_parent(obj.imp().add_button.as_ref() as &gtk::Widget);

            let action = gio::SimpleAction::new("toggle-search", None);
            let self_clone = obj.clone();
            action.connect_activate(move |_, _| self_clone.imp().toggle_search());
            obj.add_action(&action);

            let self_clone = obj.clone();
            let add_action = gio::SimpleAction::new("back", None);
            add_action.connect_activate(move |_, _| self_clone.imp().pop());
            obj.add_action(&add_action);

            let add_action = gio::SimpleAction::new("git-page", None);
            let self_clone = obj.clone();
            add_action.connect_activate(move |_, _| {
                self_clone.imp().git_popover.popup();
                self_clone.imp().git_url_entry.grab_focus();
            });
            obj.add_action(&add_action);
            obj.add_action(&add_action);

            // let add_action = gio::SimpleAction::new("git-clone", None);
            // add_action.connect_activate(|_, _| {
            //     let url = self.git_url_entry.text().to_string();
            //     Data::instance(|data| {
            //         let toast_clone_decrypt = self.toast_overlay.clone();
            //         let toast_clone_ask = self.toast_overlay.clone();

            //         data.build_list(
            //             &list_clone,
            //             move || toast_clone_decrypt.add_toast(adw::Toast::new("Decrypt callback...")),
            //             move || toast_clone_ask.add_toast(adw::Toast::new("Ask callback...")),
            //         );
            //     });

            //     let self_clone2 = self_clone.clone();
            //     run(
            //         move || {
            //             let url: String = self_clone2.git_url_entry.text().to_string();
            //             match Data::from_git(url) {
            //                 Ok(data) => data,
            //                 Err(message) => {
            //                     self_clone2.toast(&message);
            //                     Data::default()
            //                 }
            //             }
            //         },
            //         move |data| self_clone.data = data,
            //     )
            // });
            // obj.add_action(&add_action);

            // let add_action = gio::SimpleAction::new("synchronize", None);
            // add_action.connect_activate(|_, _| {
            //     self.load();
            //     run(
            //         || self.data.sync(),
            //         |res| match res {
            //             Ok() => {
            //                 self.toast("Synchronized successfully");
            //                 self.refresh_list();
            //             }
            //             Err(message) => {
            //                 self.toast(message);
            //                 self.done();
            //             }
            //         },
            //     );
            // });
            // obj.add_action(&add_action);

            // let add_action = gio::SimpleAction::new("new-password-path", None);
            // add_action.connect_activate(|_, _| {
            //     self.add_button_popover.popup();
            //     self.path_entry.grab_focus();
            // });
            // obj.add_action(&add_action);

            // let add_action = gio::SimpleAction::new("save-selected-password", None);
            // add_action.connect_activate(|_, _| {
            //     self.load();
            //     run(
            //         || self.data.save_pass(),
            //         |res| match res {
            //             Ok(message) => {
            //                 Self::update_title(&self.window_title, "".to_string());
            //                 self.toast(message);
            //                 self.refresh_list();
            //             }
            //             Err(message) => {
            //                 self.toast(message);
            //                 self.done();
            //             }
            //         },
            //     );
            // });
            // obj.add_action(&add_action);

            // let action = gio::SimpleAction::new("remove-selected-password", None);
            // action.connect_activate(|_, _| {
            //     self.load();
            //     run(
            //         || self.data.remove_pass(),
            //         |res| match res {
            //             Ok(message) => {
            //                 Self::update_title(&self.window_title, "".to_string());
            //                 self.toast(message);
            //                 self.refresh_list();
            //             }
            //             Err(message) => {
            //                 self.toast(message);
            //                 self.done();
            //             }
            //         },
            //     );
            // });
            // obj.add_action(&action);

            // let add_action = gio::SimpleAction::new("copy-selected-password", None);
            // add_action.connect_activate(|_, _| {
            //     self.load();
            //     run(
            //         || self.data.copy_pass(),
            //         |res| match res {
            //             Ok(message) => {
            //                 Self::update_title(&self.window_title, "".to_string());
            //                 self.toast(message);
            //                 self.done();
            //             }
            //             Err(message) => {
            //                 self.toast(message);
            //                 self.done();
            //             }
            //         },
            //     );
            // });
            // obj.add_action(&add_action);

            // let add_action = gio::SimpleAction::new(
            //     "copy-password-button",
            //     Some(&String::static_variant_type()),
            // );
            // add_action.connect_activate(|_, param| {
            //     self.load();
            //     let path: String = param
            //         .and_then(|v| v.str().map(str::to_string))
            //         .unwrap_or_default();
            //     self.data.set_path(path);
            //     run(
            //         || self.data.copy_pass(),
            //         |res| match res {
            //             Ok(message) => {
            //                 self.toast(message);
            //                 self.done();
            //             }
            //             Err(message) => {
            //                 self.toast(message);
            //                 self.done();
            //             }
            //         },
            //     );
            // });
            // obj.add_action(&add_action);

            // Real-time filter: hide/show action rows based on search text
            let list = obj.imp().list.clone();
            obj.imp().search_entry.connect_changed(move |entry| {
                let binding = entry.text().to_string().to_lowercase();
                let pattern = binding.trim();

                let mut child = list.first_child();
                while let Some(widget) = child.take() {
                    child = widget.next_sibling();

                    if let Ok(row) = widget.clone().downcast::<adw::ActionRow>() {
                        let title = row.title().to_lowercase();
                        row.set_visible(title.contains(&pattern));
                    }
                }
            });

            // let copy = gio::SimpleAction::new("copy-password", Some(&String::static_variant_type()));
            // copy.connect_activate(move |_, param| {
            //     let path: String = param.and_then(|v| v.str().map(str::to_string)).unwrap();
            //     self_clone.imp().set_path(path.clone());
            //     if self_clone.imp().is_unlocked() {
            //         self_clone.imp().copy_pass(&path);
            //     } else {
            //         let self_clone2 = self_clone.clone();
            //         self_clone.imp().ask_passphrase(
            //             self_clone.imp().list.as_ref() as &gtk::Widget,
            //             move || {
            //                 let self_clone3 = self_clone2.clone();
            //                 glib::idle_add_local_once(move || {
            //                     if self_clone3.imp().copy_pass(&path) {
            //                         self_clone3.imp().refresh_list();
            //                     }
            //                 });
            //             },
            //         );
            //     }
            // });
            // obj.add_action(&copy);

            // let edit = gio::SimpleAction::new("decrypt-password", Some(&String::static_variant_type()));
            // edit.connect_activate(move |_, param| {
            //     let path: String = param.and_then(|v| v.str().map(str::to_string)).unwrap();
            //     if self_clone.imp().is_text_page() {
            //         self_clone.imp().pop();
            //     }
            //     self_clone.imp().set_path(path.clone());
            //     let self_clone2 = self_clone.clone();
            //         let store = Arc::new(Mutex::new(match PassStore::new() {
            //             Ok(store) => store,
            //             Err(e) => {
            //                 eprintln!("Failed to open password store: {}", e);
            //                 PassStore::default()
            //             }
            //         }));
            //         };
            //         if !store.exists(&path) {
            //             self_clone2.imp().toast("Password not found");
            //             self_clone2.imp().done();
            //             return;
            //         }
            //         match store.ask(&path) {
            //             Ok(entry) => {
            //                 let password = entry.password.expose_secret();
            //                 self_clone2.imp().password_entry.set_text(password);

            //                 let mut text = String::new();
            //                 for line in entry.extra.iter() {
            //                     let exposed = line.expose_secret();
            //                     if exposed.contains(':') {
            //                         let (field, value) = &exposed.to_string().split_field();
            //                         let row = adw::EntryRow::builder()
            //                             .title(field)
            //                             .margin_start(15)
            //                             .margin_end(15)
            //                             .margin_bottom(5)
            //                             .build();
            //                         row.set_text(value);
            //                         let self_clone3 = self_clone2.imp().to_owned();
            //                         row.connect_changed(move |row| {
            //                             let text = row.text().to_string();
            //                             self_clone3.save_button.set_sensitive(!text.is_empty());
            //                             self_clone3.save_button.set_can_focus(!text.is_empty());
            //                         });
            //                         self_clone2.imp().dynamic_box.append(&row);
            //                     } else {
            //                         text.push_str(&format!("{}\n", exposed));
            //                     }
            //                 }
            //                 let buffer = gtk::TextBuffer::new(None);
            //                 buffer.set_text(&text);
            //                 let save_button = self_clone2.imp().save_button.clone();
            //                 buffer.connect_changed(move |buffer| {
            //                     let text =
            //                         buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
            //                     let is_not_empty = !text.is_empty();
            //                     save_button.set_sensitive(is_not_empty);
            //                     save_button.set_can_focus(is_not_empty);
            //                 });
            //                 let text_view = self_clone2.imp().text_view.clone();
            //                 text_view.set_buffer(Some(&buffer));
            //                 self_clone2.imp().push(imp::Pages::TextPage);
            //             }
            //             Err(e) => {
            //                 eprintln!("Failed to open password: {}", e);
            //                 let message = e.to_string();
            //                 let idx = message.find(';').unwrap_or(message.len());
            //                 let before_semicolon = &message[..idx];
            //                 self_clone2.imp().done();
            //                 self_clone2.imp().toast(before_semicolon);
            //             }
            //         }
            //     });
            // });
            // obj.add_action(&edit);

            // // rename

            // let rename = gio::SimpleAction::new(
            //     "rename-password",
            //     Some(&glib::VariantType::new_tuple(&[
            //         String::static_variant_type(),
            //         u64::static_variant_type(),
            //     ])),
            // );
            // rename.connect_activate(move |_, param| {
            //     let params = param.and_then(|v| v.get::<(String, u64)>()).unwrap();
            //     let path = params.0;
            //     let index = params.1;
            //     if let Some(row) = self_clone
            //         .imp()
            //         .list
            //         .row_at_index(index as i32)
            //         .and_then(|w| w.downcast::<adw::ActionRow>().ok())
            //     {
            //         self_clone.imp().rename_popover.unparent();
            //         self_clone
            //             .imp()
            //             .rename_popover
            //             .set_parent(row.as_ref() as &gtk::Widget);
            //     }
            //     self_clone.imp().new_path_entry.set_text(&path);
            //     self_clone.imp().rename_popover.popup();
            //     self_clone.imp().new_path_entry.grab_focus();
            //     let self_clone2 = self_clone.clone();
            //     let old_path = path.clone();
            //     self_clone.imp().new_path_entry.connect_apply(move |row| {
            //         let old_path2 = old_path.clone();
            //         let new_path = row.text().to_string();
            //         let self_clone3 = self_clone2.clone();
            //         glib::idle_add_local_once(move || {
            //             if self_clone3.imp().rename_pass(&old_path2, &new_path) {
            //                 self_clone3.imp().refresh_list();
            //             }
            //         });
            //     });
            // });
            // obj.add_action(&rename);

            // // DELETE

            // let remove =
            //     gio::SimpleAction::new("remove-password", Some(&String::static_variant_type()));
            // remove.connect_activate(move |_, param| {
            //     let path: String = param.and_then(|v| v.str().map(str::to_string)).unwrap();
            //     if self_clone.imp().remove_pass(&path) {
            //         self_clone.imp().refresh_list();
            //     }
            // });
            // obj.add_action(&remove);

            // // Enable or disable the buttons if the entry is empty

            // self.git_url_entry
            //     .connect_apply(move |_row| self_clone.imp().git_clone());

            // obj.imp().path_entry.connect_changed(move |entry| {
            //     let store = match PassStore::new() {
            //         Ok(store) => store,
            //         Err(_) => PassStore::default(),
            //     };
            //     let path = entry.text().to_string();
            //     let is_valid = !path.is_empty() && !store.exists(&path);
            //     entry.set_show_apply_button(is_valid);
            // });

            // obj.imp().new_path_entry.connect_changed(move |entry| {
            //     let store = match PassStore::new() {
            //         Ok(store) => store,
            //         Err(_) => PassStore::default(),
            //     };
            //     let path = entry.text().to_string();
            //     let is_valid = !path.is_empty() && !store.exists(&path);
            //     entry.set_show_apply_button(is_valid);
            // });

            // obj.imp().path_entry.connect_apply(move |row| {
            //     let path = row.text().to_string();
            //     let self_clone2 = self_clone.clone();
            //     glib::idle_add_local_once(move || {
            //         self_clone2.imp().set_path(path);
            //         self_clone2.imp().add_button_popover.popdown();
            //         let buffer = gtk::TextBuffer::new(None);
            //         buffer.set_text(&"username: ");
            //         let save_button = self_clone2.imp().save_button.clone();
            //         buffer.connect_changed(move |buffer| {
            //             let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
            //             let is_not_empty = !text.is_empty();
            //             save_button.set_sensitive(is_not_empty);
            //             save_button.set_can_focus(is_not_empty);
            //         });
            //         self_clone2.imp().text_view.set_buffer(Some(&buffer));
            //         self_clone2.imp().push(Pages::TextPage);
            //         self_clone2.imp().password_entry.grab_focus();
            //     });
            // });

            // obj.imp().password_entry.connect_changed(move |entry| {
            //     let is_not_empty = !entry.text().to_string().is_empty();
            //     self_clone.imp().save_button.set_sensitive(is_not_empty);
            //     self_clone.imp().save_button.set_can_focus(is_not_empty);
            // });

            // obj.imp().copy_password_button.connect_clicked(move |_| {
            //     let path = self_clone.imp().get_path();
            //     self_clone.imp().set_path(path.clone());
            //     if self_clone.imp().is_unlocked() {
            //         self_clone.imp().copy_pass(&path);
            //     } else {
            //         let self_clone2 = self_clone.clone();
            //         self_clone.imp().ask_passphrase(
            //             self_clone.imp().list.as_ref() as &gtk::Widget,
            //             move || {
            //                 let self_clone3 = self_clone2.clone();
            //                 glib::idle_add_local_once(move || {
            //                     if !self_clone3.imp().copy_pass(&path) {
            //                         self_clone3.imp().toast("Con not copy password");
            //                     }
            //                 });
            //             },
            //         );
            //     }
            // });

            obj.imp().done();
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
