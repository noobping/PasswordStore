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
use passcore::PassStore;

mod imp {
    use gettextrs::gettext;

    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/noobping/PasswordStore/window.ui")]
    pub struct PasswordstoreWindow {
        #[template_child]
        pub window_title: TemplateChild<adw::WindowTitle>,

        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,

        #[template_child]
        pub nav: TemplateChild<adw::NavigationView>,

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
        pub password_entry: TemplateChild<gtk::Entry>,

        // ③ Text editor page
        #[template_child]
        pub text_page: TemplateChild<adw::NavigationPage>,

        #[template_child]
        pub text_view: TemplateChild<gtk::TextView>,
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
                let subtitle = if translated.is_empty() {
                    &"Manage your passwords".to_string()
                } else {
                    translated
                };
                self.window_title.set_subtitle(&subtitle);
            } else {
                self.window_title.set_subtitle(path.as_str());
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
            let toggle_action = gio::SimpleAction::new("toggle-search", None);
            toggle_action.connect_activate(move |_, _| obj_clone.toggle_search());
            obj.add_action(&toggle_action);

            let obj_clone = obj.clone();
            let add_action = gio::SimpleAction::new("add-password", None);
            add_action.connect_activate(move |_, _| obj_clone.open_new_password());
            obj.add_action(&add_action);

            let obj_clone = obj.clone();
            let add_action = gio::SimpleAction::new("ask-passphrase", None);
            add_action.connect_activate(move |_, _| obj_clone.ask_for_passphrase());
            obj.add_action(&add_action);

            let obj_clone = obj.clone();
            let add_action = gio::SimpleAction::new("decrypt-password", None);
            add_action.connect_activate(move |_, _| obj_clone.open_text_editor());
            obj.add_action(&add_action);

            let store = PassStore::default();

            // synchronize action
            let obj_clone = obj.clone();
            let overlay = obj_clone.imp().toast_overlay.clone();
            let sync_action = gio::SimpleAction::new("synchronize", None);
            let store_clone = store.clone();
            sync_action.connect_activate(move |_, _| {
                println!("Synchronizing...");
                match store_clone.sync() {
                    Ok(_) => overlay.add_toast(adw::Toast::new("Synchronized successfully")),
                    Err(e) => {
                        let message = e.to_string();
                        let idx = message.find(';').unwrap_or(message.len());
                        let before_semicolon = &message[..idx];

                        overlay.add_toast(adw::Toast::new(before_semicolon));
                        eprintln!("Failed to synchronize: {}", e);
                    }
                }
                obj_clone.imp().init_list(&store_clone);
            });
            obj.add_action(&sync_action);

            self.init_list(&store); // Initialize store and list

            // Connect the ListBoxRow activated signal
            let obj_clone = obj.clone();
            self.list.connect_row_activated(move |_, row| {
                if let Some(inner) = row.child() {
                    if let Ok(label) = inner.downcast::<gtk::Label>() {
                        let path = label.text().to_string().replace(" / ", "/");
                        println!("Selected: {}", path);
                        obj_clone.set_path(path.clone());
                        obj_clone.open_text_editor();
                        return;
                    }
                }
                obj_clone.show_toast("Failed to open password");
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

    pub fn toggle_search(&self) {
        let entry = self.imp().search_entry.clone();
        let visible = !entry.is_visible();
        entry.set_visible(visible);
        if visible {
            entry.grab_focus();
        } else {
            entry.set_text("");
        }
    }

    pub fn open_new_password(&self) {
        let text_view = self.imp().text_view.clone();
        text_view.set_buffer(Some(&gtk::TextBuffer::new(None)));
        text_view.set_editable(true);
        let text_page = self.imp().text_page.clone();
        if !text_page.is_visible() {
            self.imp().nav.push(&text_page.clone());
        }
    }

    pub fn ask_for_passphrase(&self) {
        let ask_page = self.imp().ask_page.clone();
        if ask_page.is_visible() {
            return;
        } else {
            self.imp().nav.push(&ask_page.clone());
        }
    }

    pub fn open_text_editor(&self) {
        let path = self.get_path();
        let passphrase = self.imp().password_entry.text().to_string();
        if passphrase.is_empty() {
            let ask_page = self.imp().ask_page.clone();
            if !&ask_page.is_visible() {
                self.imp().nav.push(&ask_page.clone());
            }
            self.show_toast("Passphrase cannot be empty");
            return;
        }

        let mut store = PassStore::default();
        if store.entry_exists(&path) {
            self.show_toast("Password not found");
            let list_page = self.imp().list_page.clone();
            if !&list_page.is_visible() {
                self.imp().nav.push(&list_page.clone());
            }
            return;
        }

        match store.get(&path, passphrase.as_str()) {
            Ok(item) => {
                println!("Password: {}", item.password);
                // Pass item to the text view
                // Add item.password to the first line of the text view
                // Add the item.extra (a list of strings) after that.
                let mut text = item.password;
                for line in item.extra {
                    text.push_str(&format!("\n{}", line));
                }
                let buffer = gtk::TextBuffer::new(None);
                buffer.set_text(&text);
                let text_view = self.imp().text_view.clone();
                text_view.set_buffer(Some(&buffer));
                text_view.set_editable(false);
                // Open the text page so that I can view (or edit) the encqrypted password file
                self.imp().nav.push(&self.imp().text_page.clone());
            }
            Err(e) => {
                let message = e.to_string();
                let idx = message.find(';').unwrap_or(message.len());
                let before_semicolon = &message[..idx];
                self.show_toast(before_semicolon);
                eprintln!("Failed to open password: {}", e);
            }
        }
    }

    pub fn get_path(&self) -> String {
        self.imp().get_path()
    }

    pub fn set_path(&self, path: String) {
        self.imp().set_path(path.clone());
    }

    pub fn show_toast(&self, message: &str) {
        self.imp().show_toast(message);
    }
}
