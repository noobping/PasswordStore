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
            let visible = !self.search_entry.is_visible();
            self.search_entry.set_visible(visible);
            if (visible) {
                self.search_entry.grab_focus();
            } else {
                self.search_entry.set_text("");
            }
        }

        pub fn page_form(&self) {
            self.text_view.set_buffer(Some(&gtk::TextBuffer::new(None)));
            self.password_entry.set_text("");
            Self::update_title(&self.window_title, "".to_string());
            // AppData::instance(|data: &mut AppData| data.set_path(""));
            while let Some(child) = self.dynamic_box.first_child() {
                self.dynamic_box.remove(&child);
            }
            self.navigation_view
                .push(self.text_page.as_ref() as &adw::NavigationPage);
            self.update_navigation_buttons(false);
        }

        pub fn page_list(&self) {
            self.navigation_view
                .pop_to_page(&self.list_page.as_ref() as &adw::NavigationPage);
            self.update_navigation_buttons(true);
        }

        fn update_navigation_buttons(&self, default_page: bool) {
            self.save_button.set_can_focus(false);
            self.save_button.set_sensitive(false);
            self.save_button.set_visible(!default_page);

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
            self.spinner.stop();
            self.spinner.set_visible(false);
        }

        fn rebuild_list(&self) {
            self.load();
            self.list.remove_all();

            let work = || {
                AppData::instance(|data| data.list_paths())
            };
            let update_ui = {
                let list_clone = self.list.clone();
                let title_clone = self.window_title.clone();
                let title_clone2 = self.window_title.clone();
                let toast_overlay = self.toast_overlay.clone();
                move |result: Result<Vec<String>, String>| {
                    match result {
                        Ok(paths) => {
                            AppData::instance(|data| {
                                data.populate_list(
                                    &list_clone,
                                    paths,
                                    move |path| PasswordstoreWindow::update_title(&title_clone, path.clone()),
                                    move |path| PasswordstoreWindow::update_title(&title_clone2, path.clone()),
                                );
                            });
                            self.done();
                        }
                        Err(e) => {
                            toast_overlay.add_toast(adw::Toast::new(&e.to_string()));
                            self.done();
                        }
                    }
                }
            };
            run(work, update_ui);

            //     // Only pass Send types to the background thread
            //     run(
            //         || AppData::instance(|data| data.list_paths()),
            //         {
            //             // All UI objects are cloned and used only in the main thread closure
            //             let list_clone = self.list.clone();
            //             let title_clone = self.window_title.clone();
            //             let title_clone2 = self.window_title.clone();
            //             let toast_overlay = self.toast_overlay.clone();
            //             let this = self.clone();
            //             move |result: Result<Vec<String>, String>| {
            //                 match result {
            //                     Ok(paths) => {
            //                         AppData::instance(|data| {
            //                             data.populate_list(
            //                                 &list_clone,
            //                                 paths,
            //                                 move |path| PasswordstoreWindow::update_title(&title_clone, path.clone()),
            //                                 move |path| PasswordstoreWindow::update_title(&title_clone2, path.clone()),
            //                             );
            //                         });
            //                         this.done();
            //                     }
            //                     Err(e) => {
            //                         toast_overlay.add_toast(adw::Toast::new(&e.to_string()));
            //                         this.done();
            //                     }
            //                 }
            //             }
            //         },
            //     );
            //     self.pop();
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
            add_action.connect_activate(move |_, _| self_clone.imp().page_list());
            obj.add_action(&add_action);

            let add_action = gio::SimpleAction::new("git-page", None);
            let self_clone = obj.clone();
            add_action.connect_activate(move |_, _| {
                self_clone.imp().git_popover.popup();
                self_clone.imp().git_url_entry.grab_focus();
            });
            obj.add_action(&add_action);
            obj.add_action(&add_action);

            // ...

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

            // ...
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
