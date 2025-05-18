use adw::prelude::{ActionRowExt, PreferencesRowExt};
use adw::subclass::prelude::*;
use gtk::gio;
use gtk::prelude::*;
use passcore::{exists_store_dir, PassStore};
use secrecy::{zeroize::Zeroize, ExposeSecret, SecretString};
use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use crate::extension::{GPairToPath, StringExt};

#[derive(Debug, Default)]
pub struct SharedData {
    path: String,
    passphrase: SecretString,
    unlocked: bool,
}

impl SharedData {
    pub fn new() -> Self {
        Self {
            path: String::new(),
            passphrase: SecretString::default(),
            unlocked: false,
        }
    }
}

thread_local! {
    static THREAD_DATA: RefCell<AppData> = RefCell::new(AppData::new().unwrap_or_default());
}

#[derive(Debug)]
pub struct AppData {
    store: PassStore,
    shared: Arc<Mutex<SharedData>>,
}

impl Default for AppData {
    fn default() -> Self {
        let store = PassStore::default();
        let shared = Arc::new(Mutex::new(SharedData::new()));
        Self { store, shared }
    }
}

impl AppData {
    fn new() -> anyhow::Result<Self> {
        let store = PassStore::new()?;
        let shared = Arc::new(Mutex::new(SharedData::new()));
        Ok(Self { store, shared })
    }

    pub fn instance<F, R>(f: F) -> R
    where
        F: FnOnce(&mut AppData) -> R,
    {
        THREAD_DATA.with(|data| {
            let mut data = data.borrow_mut();
            f(&mut *data)
        })
    }

    pub fn from_git(url: String) -> Result<Self, String> {
        if url.is_empty() {
            return Err("Git URL cannot be empty".to_string());
        }
        let store = PassStore::from_git(url.clone()).map_err(|e| {
            e.to_string()
                .split_once(';')
                .map(|(s, _)| s)
                .unwrap_or(&e.to_string())
                .to_string()
        })?;
        if store.ok() {
            let shared = Arc::new(Mutex::new(SharedData::new()));
            Ok(Self { store, shared })
        } else {
            Err("Password store is not initialized".to_string())
        }
    }

    // ----------------- Core Methods -----------------

    pub fn set_path(&self, path: &str) -> bool {
        if self.validate_path(path).is_err() {
            return false;
        }
        let mut shared = match self.shared.lock() {
            Ok(guard) => guard,
            Err(p) => p.into_inner(),
        };
        shared.path = path.to_string();
        true
    }

    pub fn is_unlocked(&self) -> bool {
        let shared = match self.shared.lock() {
            Ok(guard) => guard,
            Err(p) => p.into_inner(),
        };
        shared.unlocked
    }

    pub fn unlock(&self, passphrase: SecretString) {
        let mut shared = match self.shared.lock() {
            Ok(guard) => guard,
            Err(p) => p.into_inner(),
        };
        shared.passphrase = passphrase;
        shared.unlocked = true;
    }

    pub fn lock(&self) {
        let mut shared = match self.shared.lock() {
            Ok(guard) => guard,
            Err(p) => p.into_inner(),
        };
        shared.unlocked = false;
        shared.passphrase.zeroize();
    }

    pub fn list_paths(&self) -> Result<Vec<String>, String> {
        self.store.list().map_err(|e| {
            e.to_string()
                .split_once(';')
                .map(|(s, _)| s)
                .unwrap_or(&e.to_string())
                .to_string()
        })
    }

    pub fn save_pass(&self, entry: &passcore::Entry) -> Result<String, String> {
        let shared = match self.shared.lock() {
            Ok(guard) => guard,
            Err(p) => p.into_inner(),
        };
        let path = shared.path.clone();
        self.validate_path(&path)?;
        self.store
            .add(&path, entry)
            .map(|_| format!("Password {} saved", path))
            .map_err(|e| {
                e.to_string()
                    .split_once(';')
                    .map(|(s, _)| s)
                    .unwrap_or(&e.to_string())
                    .to_string()
            })
    }

    pub fn move_pass(&self, new_path: &String) -> Result<String, String> {
        let shared = match self.shared.lock() {
            Ok(guard) => guard,
            Err(p) => p.into_inner(),
        };
        let path = shared.path.clone();
        self.validate_path(&path)?;
        self.store
            .rename(&path, new_path)
            .map(|_| format!("Password {} moved to {}", path, new_path))
            .map_err(|e| {
                e.to_string()
                    .split_once(';')
                    .map(|(s, _)| s)
                    .unwrap_or(&e.to_string())
                    .to_string()
            })
    }

    pub fn delete_pass(&self) -> Result<String, String> {
        let shared = match self.shared.lock() {
            Ok(guard) => guard,
            Err(p) => p.into_inner(),
        };
        let path = shared.path.clone();
        self.validate_path(&path)?;
        self.store
            .remove(&path)
            .map(|_| format!("Password {} removed", path))
            .map_err(|e| {
                e.to_string()
                    .split_once(';')
                    .map(|(s, _)| s)
                    .unwrap_or(&e.to_string())
                    .to_string()
            })
    }

    pub fn get_pass_entry(&self) -> Result<passcore::Entry, String> {
        let shared = match self.shared.lock() {
            Ok(guard) => guard,
            Err(p) => p.into_inner(),
        };
        self.validate_path(&shared.path)?;
        self.store
            .get(&shared.path, shared.passphrase.clone())
            .map_err(|e| {
                e.to_string()
                    .split_once(';')
                    .map(|(s, _)| s)
                    .unwrap_or(&e.to_string())
                    .to_string()
            })
    }

    pub fn ask_pass_entry(&self) -> Result<passcore::Entry, String> {
        let shared = match self.shared.lock() {
            Ok(guard) => guard,
            Err(p) => p.into_inner(),
        };
        self.validate_path(&shared.path)?;
        self.store.ask(&shared.path).map_err(|e| {
            e.to_string()
                .split_once(';')
                .map(|(s, _)| s)
                .unwrap_or(&e.to_string())
                .to_string()
        })
    }

    pub fn copy_pass(&self) -> Result<String, String> {
        if !self.is_unlocked() {
            return Err("Store is locked".to_string());
        }
        let shared = match self.shared.lock() {
            Ok(guard) => guard,
            Err(p) => p.into_inner(),
        };
        let entry = self
            .store
            .get(&shared.path, shared.passphrase.clone())
            .map_err(|e| {
                e.to_string()
                    .split_once(';')
                    .map(|(s, _)| s)
                    .unwrap_or(&e.to_string())
                    .to_string()
            })?;
        gtk::gdk::Display::default()
            .ok_or_else(|| "Cannot access clipboard".to_string())?
            .clipboard()
            .set_text(&entry.password.expose_secret());
        Ok(format!("Password {} copied", shared.path))
    }

    pub fn sync_store(&self) -> Result<(), String> {
        self.validate_store()?;
        self.store.sync().map_err(|e| {
            e.to_string()
                .split_once(';')
                .map(|(s, _)| s)
                .unwrap_or(&e.to_string())
                .to_string()
        })
    }

    fn validate_store(&self) -> Result<(), String> {
        if !exists_store_dir() {
            return Err("Store directory missing".to_string());
        }
        if !self.store.ok() {
            return Err("Password store not initialized".to_string());
        }
        Ok(())
    }

    fn validate_path(&self, name: &str) -> Result<(), String> {
        self.validate_store()?;
        if name.is_empty() {
            return Err("Name is empty".to_string());
        }
        if !self.store.exists(name) {
            return Err("Entry does not exist".to_string());
        }
        Ok(())
    }

    // ----------------- UI Helpers -----------------

    pub fn ui_to_pass_entry(
        rows: &gtk::Box,
        password: &TemplateChild<adw::PasswordEntryRow>,
        view: &TemplateChild<gtk::TextView>,
    ) -> passcore::Entry {
        let password = password.text().to_string().to_secret();

        let mut children = Vec::new();
        let mut maybe_child = rows.first_child();
        while let Some(child) = maybe_child {
            children.push(child.clone());
            maybe_child = child.next_sibling();
        }

        let mut extra = Vec::new();
        for widget in children {
            if let Ok(entry) = widget.downcast::<adw::EntryRow>() {
                let field = entry.title().trim().to_owned();
                let value = entry.text().trim().to_owned();
                extra.push(format!("{}:{}", field, value).to_secret());
            }
        }
        let buffer = view.buffer();
        let mut lines = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .lines()
            .map(|s| s.to_string().to_secret())
            .collect::<Vec<_>>();
        extra.append(&mut lines);

        passcore::Entry { password, extra }
    }

    pub fn populate_list(
        &self,
        list: &gtk::ListBox,
        paths: Vec<String>,
        row_decrypt_callback: impl Fn(String) + 'static,
        row_unlock_callback: impl Fn(usize, String) + 'static,
    ) {
        list.set_selection_mode(gtk::SelectionMode::Single);
        let decrypt = Arc::new(row_decrypt_callback);
        let unlock = Arc::new(row_unlock_callback);
        for (index, path) in paths.into_iter().enumerate() {
            let path = path.clone();
            let index = index.clone();
            let (folder, name) = path.split_path();
            let row = adw::ActionRow::builder()
                .title(&name)
                .subtitle(&folder.replace("/", " / "))
                .activatable(true)
                .build();
            let d_cb = decrypt.clone();
            let u_cb = unlock.clone();
            row.connect_activated({
                let path = path.clone();
                let index = index.clone();
                move |_| {
                    AppData::instance(|data| {
                        if data.set_path(&path) {
                            if data.is_unlocked() {
                                d_cb(path.clone());
                            } else {
                                u_cb(index.clone(), path.clone());
                            }
                        }
                    });
                }
            });
            // context menu
            let menu = gio::Menu::new();
            let copy_item = gio::MenuItem::new(Some("Copy password"), Some("win.copy-password"));
            copy_item.set_attribute_value("target", Some(&path.to_variant()));
            menu.append_item(&copy_item);
            let edit_item = gio::MenuItem::new(Some("Edit password"), Some("win.decrypt-password"));
            edit_item.set_attribute_value("target", Some(&path.to_variant()));
            menu.append_item(&edit_item);
            let rename_item = gio::MenuItem::new(Some("Rename…"), Some("win.rename-password"));
            let target = (path.to_string(), index as u64);
            rename_item.set_attribute_value("target", Some(&target.to_variant()));
            menu.append_item(&rename_item);
            let delete_item = gio::MenuItem::new(Some("Delete"), Some("win.remove-password"));
            delete_item.set_attribute_value("target", Some(&path.to_variant()));
            menu.append_item(&delete_item);
            let menu_button = gtk::MenuButton::builder()
                .icon_name("view-more-symbolic")
                .menu_model(&menu)
                .build();
            row.add_suffix(&menu_button);
            list.append(&row);
        }
    }

    pub fn populate_form(
        &self,
        entry: passcore::Entry,
        button: &gtk::Button,
        rows: &gtk::Box,
        password: &adw::PasswordEntryRow,
        view: &gtk::TextView,
    ) -> Result<(), String> {
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
                let button_clone = button.clone();
                row.connect_changed(move |row| {
                    let is_not_empty = !row.text().to_string().is_empty();
                    button_clone.set_sensitive(is_not_empty);
                    button_clone.set_can_focus(is_not_empty);
                });
                rows.append(&row);
            } else {
                text.push_str(&format!("{}\n", exposed));
            }
        }
        let buffer = gtk::TextBuffer::new(None);
        buffer.set_text(&text);
        let button_clone = button.clone();
        buffer.connect_changed(move |buffer| {
            let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
            let is_not_empty: bool = !text.is_empty();
            button_clone.set_sensitive(is_not_empty);
            button_clone.set_can_focus(is_not_empty);
        });
        view.set_buffer(Some(&buffer));
        password.set_text(entry.password.expose_secret());
        Ok(())
    }
}
