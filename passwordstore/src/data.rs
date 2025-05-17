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
        let store = match PassStore::from_git(url) {
            Ok(store) => store,
            Err(e) => {
                let message = e.to_string();
                let idx = message.find(';').unwrap_or(message.len());
                let before_semicolon = &message[..idx];
                return Err(before_semicolon.to_owned());
            }
        };
        if store.ok() {
            let shared = Arc::new(Mutex::new(SharedData::new()));
            return Ok(Self { store, shared });
        }
        Err("Password store is not initialized".to_string())
    }

    // ----------------- path blocking method -----------------

    fn set_path_blocking(&self, path: &str) -> bool {
        match self.validate_path_blocking(&path) {
            Ok(_) => {
                let mut shared = match self.shared.lock() {
                    Ok(shared) => shared,
                    Err(poisoned) => poisoned.into_inner(),
                };
                shared.path = path.to_string();
                true
            }
            Err(_) => false,
        }
    }

    // ----------------- passphrase blocking methods -----------------

    fn is_unlocked_blocking(&self) -> bool {
        let shared = match self.shared.lock() {
            Ok(shared) => shared,
            Err(poisoned) => poisoned.into_inner(),
        };
        shared.unlocked
    }

    fn unlock_blocking(&self, passphrase: SecretString) {
        let mut shared = match self.shared.lock() {
            Ok(shared) => shared,
            Err(poisoned) => poisoned.into_inner(),
        };
        shared.passphrase = passphrase;
        shared.unlocked = true;
    }

    fn lock_blocking(&self) {
        let mut shared = match self.shared.lock() {
            Ok(shared) => shared,
            Err(poisoned) => poisoned.into_inner(),
        };
        shared.unlocked = false;
        shared.passphrase.zeroize();
    }

    // ----------------- Store blocking methods -----------------

    fn list_paths_blocking(&self) -> Result<Vec<String>, String> {
        match self.store.list() {
            Ok(paths) => {
                let mut result = Vec::new();
                for path in paths.iter() {
                    let path = path.to_string();
                    if path.ends_with(".gpg") {
                        result.push(path);
                    }
                }
                Ok(result)
            }
            Err(e) => {
                let message = e.to_string();
                let idx = message.find(';').unwrap_or(message.len());
                let before_semicolon = &message[..idx];
                Err(before_semicolon.to_owned())
            }
        }
    }

    fn save_pass_blocking(&self, entry: &passcore::Entry) -> Result<String, String> {
        let shared = match self.shared.lock() {
            Ok(shared) => shared,
            Err(poisoned) => poisoned.into_inner(),
        };
        let path = shared.path.clone();
        self.validate_path_blocking(&path)?;
        return match self.store.add(&path, &entry) {
            Ok(_) => Ok(format!("Password {} saved", path)),
            Err(e) => {
                let message = e.to_string();
                let idx = message.find(';').unwrap_or(message.len());
                let before_semicolon = &message[..idx];
                Err(before_semicolon.to_owned())
            }
        };
    }

    fn move_pass_blocking(&self, new_path: &String) -> Result<String, String> {
        let shared = match self.shared.lock() {
            Ok(shared) => shared,
            Err(poisoned) => poisoned.into_inner(),
        };
        let old_path = shared.path.clone();
        self.validate_path_blocking(&old_path)?;
        if !self.store.exists(&old_path) {
            return Err("Password not found".to_string());
        }
        return match self.store.rename(&old_path, &new_path) {
            Ok(_) => Ok(format!("Password {} moved to {}", old_path, new_path)),
            Err(e) => {
                let message = e.to_string();
                let idx = message.find(';').unwrap_or(message.len());
                let before_semicolon = &message[..idx];
                Err(before_semicolon.to_owned())
            }
        };
    }

    fn remove_pass_blocking(&self) -> Result<String, String> {
        let shared = match self.shared.lock() {
            Ok(shared) => shared,
            Err(poisoned) => poisoned.into_inner(),
        };
        let path = shared.path.clone();
        self.validate_path_blocking(&path)?;
        return match self.store.remove(&path) {
            Ok(_) => Ok(format!("Password {} removed", path)),
            Err(e) => {
                let message = e.to_string();
                let idx = message.find(';').unwrap_or(message.len());
                let before_semicolon = &message[..idx];
                Err(before_semicolon.to_owned())
            }
        };
    }

    fn get_pass_entry_blocking(&self) -> Result<passcore::Entry, String> {
        let shared = match self.shared.lock() {
            Ok(shared) => shared,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !self.is_unlocked_blocking() {
            return Err("Store is locked".to_string());
        }
        let path = shared.path.clone();
        self.validate_path_blocking(&path)?;
        return match self.store.get(path.as_str(), shared.passphrase.clone()) {
            Ok(entry) => Ok(entry),
            Err(e) => {
                let message = e.to_string();
                let idx = message.find(';').unwrap_or(message.len());
                let before_semicolon = &message[..idx];
                Err(before_semicolon.to_owned())
            }
        };
    }

    fn ask_pass_entry_blocking(&self) -> Result<passcore::Entry, String> {
        let shared = match self.shared.lock() {
            Ok(shared) => shared,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !self.is_unlocked_blocking() {
            return Err("Store is locked".to_string());
        }
        let path = shared.path.clone();
        self.validate_path_blocking(&path)?;
        return match self.store.ask(path.as_str()) {
            Ok(entry) => Ok(entry),
            Err(e) => {
                let message = e.to_string();
                let idx = message.find(';').unwrap_or(message.len());
                let before_semicolon = &message[..idx];
                Err(before_semicolon.to_owned())
            }
        };
    }

    fn copy_pass_blocking(&self) -> Result<String, String> {
        if !self.is_unlocked_blocking() {
            return Err("Store is locked".to_string());
        }
        let shared = match self.shared.lock() {
            Ok(shared) => shared,
            Err(poisoned) => poisoned.into_inner(),
        };
        let path = shared.path.clone();
        self.validate_path_blocking(&path)?;
        let entry = match self.store.get(&path, shared.passphrase.clone()) {
            Ok(entry) => entry,
            Err(e) => {
                let message = e.to_string();
                let idx = message.find(';').unwrap_or(message.len());
                let before_semicolon = &message[..idx];
                return Err(before_semicolon.to_owned());
            }
        };
        match gtk::gdk::Display::default() {
            Some(display) => {
                let clipboard = display.clipboard();
                clipboard.set_text(&entry.password.expose_secret());
            }
            None => {
                return Err("Can not copy password".to_string());
            }
        }
        Ok(format!("Password {} copied", path))
    }

    fn sync_store_blocking(&self) -> Result<(), String> {
        self.validate_store_blocking()?;
        return match self.store.sync() {
            Ok(_) => Ok(()),
            Err(e) => {
                let message = e.to_string();
                let idx = message.find(';').unwrap_or(message.len());
                let before_semicolon = &message[..idx];
                Err(before_semicolon.to_owned())
            }
        };
    }

    fn validate_store_blocking(&self) -> Result<(), String> {
        if !exists_store_dir() {
            return Err("Store directory does not exist".to_string());
        }
        if !self.store.ok() {
            return Err("Password store is not is not initialized".to_string());
        }
        Ok(())
    }

    fn validate_path_blocking(&self, name: &str) -> Result<(), String> {
        self.validate_store_blocking()?;
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

    pub fn populate_list<F1, F2>(
        &self,
        list: &gtk::ListBox,
        items: Vec<String>,
        row_decrypt_callback: F1,
        row_unlock_callback: F2,
    ) where
        F1: Fn(String) + 'static,
        F2: Fn(String) + 'static,
    {
        list.set_selection_mode(gtk::SelectionMode::Single);
        let row_decrypt_callback = Arc::new(row_decrypt_callback);
        let row_unlock_callback = Arc::new(row_unlock_callback);
        for (index, path) in items.iter().enumerate() {
            let (folder, name) = path.clone().split_path();
            let row = adw::ActionRow::builder()
                .title(&name)
                .subtitle(&folder.replace("/", " / "))
                .activatable(true)
                .build();

            let row_decrypt_callback = Arc::clone(&row_decrypt_callback);
            let row_unlock_callback = Arc::clone(&row_unlock_callback);
            row.connect_activated(move |row| {
                AppData::instance(|data| {
                    let new_path = (row.title(), row.subtitle().unwrap_or_default()).to_path();
                    if data.set_path_blocking(&new_path) {
                        if data.is_unlocked_blocking() {
                            (row_decrypt_callback)(new_path);
                        } else {
                            (row_unlock_callback)(new_path);
                        }
                    }
                });
            });

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
