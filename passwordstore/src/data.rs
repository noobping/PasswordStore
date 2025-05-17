use adw::prelude::{ActionRowExt, PreferencesRowExt};
use adw::subclass::prelude::*;
use anyhow::anyhow;
use gtk::gio;
use gtk::prelude::*;
use once_cell::sync::Lazy;
use passcore::{exists_store_dir, PassStore};
use secrecy::{zeroize::Zeroize, ExposeSecret, SecretString};
use std::sync::{Arc, Mutex};

use crate::extension::{GPairToPath, StringExt};

static GLOBAL_DATA: Lazy<Mutex<Data>> = Lazy::new(|| {
    let d = Data::default();
    Mutex::new(d)
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Passphrase,
    Pinantry,
}

impl Default for Method {
    fn default() -> Self {
        Self::Passphrase
    }
}

#[derive(Debug, Clone, Default)]
pub struct Data {
    store: PassStore,
    path: String,
    passphrase: SecretString,
    unlocked: bool,
}

impl Data {
    pub fn new() -> anyhow::Result<Self> {
        let store = PassStore::new()?;
        let path = String::new();
        let passphrase = SecretString::default();
        let unlocked = false;

        Ok(Self {
            store,
            path,
            passphrase,
            unlocked,
        })
    }

    pub fn instance() -> Result<std::sync::MutexGuard<'static, Data>, String> {
        match GLOBAL_DATA.try_lock() {
            Ok(guard) => Ok(guard),
            Err(_) => Err("Application is busy".to_string()),
        }
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
            let path = String::new();
            let passphrase = SecretString::default();
            let unlocked = false;
            return Ok(Self {
                store,
                path,
                passphrase,
                unlocked,
            });
        }
        Err("Password store is not initialized".to_string())
    }

    pub fn set_path(&mut self, path: String) -> Result<(), String> {
        self.validate_path(&path)?;
        self.path = path;
        Ok(())
    }

    pub fn is_unlocked(&self) -> bool {
        self.unlocked
    }

    pub fn unlock(&mut self, passphrase: SecretString) {
        self.passphrase = passphrase;
        self.unlocked = true;
    }

    pub fn lock(&mut self) {
        self.unlocked = false;
        self.passphrase.zeroize();
    }

    pub fn build_list<F1, F2>(
        &self,
        list: gtk::ListBox,
        decrypt_callback: F1,
        ask_callback: F2,
    ) -> anyhow::Result<()>
    where
        F1: Fn() + 'static,
        F2: Fn() + 'static,
    {
        list.set_selection_mode(gtk::SelectionMode::Single);
        let items = self.store.list()?;

        let decrypt_callback = Arc::new(decrypt_callback);
        let ask_callback = Arc::new(ask_callback);
        for (index, path) in items.iter().enumerate() {
            let (folder, name) = path.clone().split_path();
            let row = adw::ActionRow::builder()
                .title(&name)
                .subtitle(&folder.replace("/", " / "))
                .activatable(true)
                .build();

            let self_clone = self.clone();
            let decrypt_callback = Arc::clone(&decrypt_callback);
            let ask_callback = Arc::clone(&ask_callback);

            row.connect_activated(move |row| {
                let new_path = (row.title(), row.subtitle().unwrap_or_default()).to_path();
                let mut self_clone = self_clone.clone();
                if let Err(_) = self_clone.set_path(new_path) {
                    row.set_title("Error");
                    row.set_subtitle("Could not set path");
                } else {
                    if self_clone.is_unlocked() {
                        (decrypt_callback)();
                    } else {
                        (ask_callback)();
                    }
                }
            });

            let menu = gio::Menu::new(); // build the menu model

            let copy_item = gio::MenuItem::new(Some("Copy password"), Some("win.copy-password"));
            copy_item.set_attribute_value("target", Some(&path.to_variant()));
            menu.append_item(&copy_item);

            // use pinantry to open (edit) the password
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
        Ok(())
    }

    pub fn build_form(
        &self,
        method: Method,
        button: &gtk::Button,
        rows: &gtk::Box,
        password: &TemplateChild<adw::PasswordEntryRow>,
        view: &TemplateChild<gtk::TextView>,
    ) -> anyhow::Result<()> {
        if !self.is_unlocked() {
            return Err(anyhow!("Store is locked"));
        }
        let entry = if method == Method::Pinantry {
            self.store.ask(self.path.as_str())?
        } else {
            self.store.get(self.path.as_str(), self.passphrase.clone())?
        };
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

    pub fn to_pass(
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

    pub fn save_pass(&self, entry: &passcore::Entry) -> Result<String, String> {
        let path = self.path.clone();
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

    pub fn move_pass(&self, new_path: &String) -> Result<String, String> {
        let old_path = self.path.clone();
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

    pub fn remove_pass(&self) -> Result<String, String> {
        let path = self.path.clone();
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

    pub fn copy_pass(&self) -> Result<String, String> {
        if !self.is_unlocked() {
            return Err("Store is locked".to_string());
        }
        let path = self.path.clone();
        let entry = match self.store.get(&path, self.passphrase.clone()) {
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

    pub fn sync(&self) -> Result<(), String> {
        self.validate_store()?;
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

    fn validate_store(&self) -> Result<(), String> {
        if !self.is_unlocked() {
            return Err("Store is locked".to_string());
        }
        if !exists_store_dir() {
            return Err("Store directory does not exist".to_string());
        }
        if !self.store.ok() {
            return Err("Password store is not is not initialized".to_string());
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
}
