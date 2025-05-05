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
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/noobping/PasswordStore/window.ui")]
    pub struct PasswordstoreWindow {
        #[template_child]
        pub list: TemplateChild<gtk::ListBox>,

        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,

        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,
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
                        println!("Failed to synchronize: {}", e);
                    }
                }
                obj_clone.imp().init_list(&store_clone);
            });
            obj.add_action(&sync_action);

            self.init_list(&store); // Initialize store and list

            // Connect the ListBoxRow activated signal
            self.list.connect_row_activated(move |_, row| {
                if let Some(inner) = row.child() {
                    if let Ok(label) = inner.downcast::<gtk::Label>() {
                        let text = label.text().to_string();
                        println!("Label text: {}", text);
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
        println!("Open new password");
    }
}
