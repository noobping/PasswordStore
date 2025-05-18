use adw::subclass::prelude::*;
use gtk::glib::MainContext;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::data::AppData;
use crate::extension::{GPairToPath, StringExt};

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
    use adw::prelude::{ActionRowExt, EntryRowExt, PreferencesRowExt};
    use gettextrs::gettext;
    use passcore::exists_store_dir;
    use secrecy::ExposeSecret;

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
        fn populate_form(&self, entry: passcore::Entry) {
            let mut text = String::new();
            for line in entry.extra.iter() {
                let exposed = line.expose_secret();
                if exposed.contains(':') {
                    let (field, value) = &exposed.to_string().split_field();
                    let row: adw::EntryRow = adw::EntryRow::builder()
                        .title(field)
                        .margin_start(15)
                        .margin_end(15)
                        .margin_bottom(5)
                        .build();
                    row.set_text(value);
                    let button_clone = self.save_button.clone();
                    row.connect_changed(move |row| {
                        let is_not_empty = !row.text().to_string().is_empty();
                        button_clone.set_sensitive(is_not_empty);
                        button_clone.set_can_focus(is_not_empty);
                    });
                    self.dynamic_box.append(&row);
                } else {
                    text.push_str(&format!("{}\n", exposed));
                }
            }
            let buffer = gtk::TextBuffer::new(None);
            buffer.set_text(&text);
            let button_clone = self.save_button.clone();
            buffer.connect_changed(move |buffer| {
                let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
                let is_not_empty: bool = !text.is_empty();
                button_clone.set_sensitive(is_not_empty);
                button_clone.set_can_focus(is_not_empty);
            });
            self.text_view.set_buffer(Some(&buffer));
            self.password_entry.set_text(entry.password.expose_secret());
        }

        fn populate_list(&self, paths: Vec<String>) {
            let obj_weak = self.downgrade();
            glib::idle_add_local_once(move || {
                if let Some(obj) = obj_weak.upgrade() {
                    for (index, path) in paths.into_iter().enumerate() {
                        let (folder, name) = path.split_path();
                        let row = adw::ActionRow::builder()
                            .title(&name)
                            .subtitle(&folder.replace("/", " / "))
                            .activatable(true)
                            .build();

                        let cloned = obj.clone();
                        row.connect_activated(move |activated_row| {
                            let active_path = (activated_row.title(), activated_row.subtitle()).to_path();
                            AppData::instance(|data| {
                                if data.set_path(&active_path) {
                                    PasswordstoreWindow::update_title(&cloned.window_title, active_path.clone());
                                    if data.is_unlocked() {
                                        cloned.populate_form(data.get_pass_entry().unwrap_or_default());
                                        cloned.navigation_view.push(cloned.text_page.as_ref() as &adw::NavigationPage);
                                        cloned.update_navigation_buttons(false);
                                    } else {
                                        cloned.passphrase_popover
                                            .set_parent(activated_row.upcast_ref::<gtk::Widget>());
                                        cloned.passphrase_popover.popup();
                                        cloned.passphrase_entry.grab_focus();
                                    }
                                }
                            });
                        });

                        // context menu
                        let menu = gio::Menu::new();
                        let copy_item =
                            gio::MenuItem::new(Some("Copy password"), Some("win.copy-password"));
                        copy_item.set_attribute_value("target", Some(&path.to_variant()));
                        menu.append_item(&copy_item);
                        let edit_item =
                            gio::MenuItem::new(Some("Edit password"), Some("win.decrypt-password"));
                        edit_item.set_attribute_value("target", Some(&path.to_variant()));
                        menu.append_item(&edit_item);
                        let rename_item =
                            gio::MenuItem::new(Some("Rename…"), Some("win.rename-password"));
                        let target = (path.to_string(), index as u64);
                        rename_item.set_attribute_value("target", Some(&target.to_variant()));
                        menu.append_item(&rename_item);
                        let delete_item =
                            gio::MenuItem::new(Some("Delete"), Some("win.remove-password"));
                        delete_item.set_attribute_value("target", Some(&path.to_variant()));
                        menu.append_item(&delete_item);
                        let menu_button = gtk::MenuButton::builder()
                            .icon_name("view-more-symbolic")
                            .menu_model(&menu)
                            .build();
                        row.add_suffix(&menu_button);

                        obj.list.append(&row);
                    }
                }
            });
        }

        fn rebuild_list(&self) {
            self.load();
            self.list.remove_all();
            self.list.set_selection_mode(gtk::SelectionMode::Single);
            let paths = AppData::instance(|data| data.list_paths()).unwrap_or_default();
            self.populate_list(paths);
            self.done();
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
            let search_entry = obj.imp().search_entry.clone();
            action.connect_activate(move |_, _| {
                let visible = !search_entry.is_visible();
                search_entry.set_visible(visible);
                if visible {
                    search_entry.grab_focus();
                } else {
                    search_entry.set_text("");
                }
            });
            obj.add_action(&action);

            let add_action = gio::SimpleAction::new("back", None);
            let navigation_view = obj.imp().navigation_view.clone();
            let list_page = obj.imp().list_page.clone();
            add_action.connect_activate(move |_, _| {
                navigation_view.pop_to_page(&list_page.as_ref() as &adw::NavigationPage);
            });
            obj.add_action(&add_action);

            let add_action = gio::SimpleAction::new("git-url", None);
            let git_popover = obj.imp().git_popover.clone();
            let git_url_entry = obj.imp().git_url_entry.clone();
            add_action.connect_activate(move |_, _| {
                git_popover.popup();
                git_url_entry.grab_focus();
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

            let self_clone = obj.clone();
            glib::idle_add_local_once(move || {
                let self_clone2 = self_clone.clone();
                glib::MainContext::default().spawn_local(async move {
                    self_clone2
                        .imp()
                        .list
                        .set_selection_mode(gtk::SelectionMode::Single);
                    let paths = AppData::instance(|data| data.list_paths()).unwrap_or_default();
                    self_clone2.imp().populate_list(paths);
                });
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
