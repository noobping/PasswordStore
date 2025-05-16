use adw::prelude::{ActionRowExt, EntryRowExt, PreferencesRowExt};
use adw::subclass::prelude::*;
use anyhow::{anyhow, Context};
use gettextrs::gettext;
use gtk::prelude::*;
use gtk::{gio, glib};
use passcore::{exists_store_dir, Entry, PassStore};
use secrecy::{zeroize::Zeroize, ExposeSecret, SecretString};
use std::collections::HashMap;
use std::sync::Mutex;

use crate::extension::StringExt;
use crate::pages::Pages;

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

#[derive(Debug, Default)]
pub struct Data {
    store: PassStore,
    path: Mutex<String>,
    passphrase: Mutex<SecretString>,
    unlocked: Mutex<bool>,
}

impl Data {
    pub fn new() -> anyhow::Result<Self> {
        let store = PassStore::new()?;
        let path = Mutex::new(String::new());
        let passphrase = Mutex::new(SecretString::default());
        let unlocked = Mutex::new(false);

        Ok(Self {
            store,
            path,
            passphrase,
            unlocked,
        })
    }

    pub fn is_unlocked(&self) -> bool {
        match self.unlocked.try_lock() {
            Ok(guard) => *guard,
            Err(_) => false,
        }
    }

    pub fn unlock(&self, passphrase: SecretString) -> anyhow::Result<()> {
        let mut guard = self
            .passphrase
            .lock()
            .map_err(|_| anyhow!("Con not use passphrase"))?;
        *guard = passphrase;

        let mut unlocked = self
            .unlocked
            .lock()
            .map_err(|_| anyhow!("Con not remember passphrase"))?;
        *unlocked = true;
        Ok(())
    }

    pub fn lock(&self) {
        if let Ok(mut unlocked) = self.unlocked.lock() {
            *unlocked = false;
            if let Ok(mut guard) = self.passphrase.lock() {
                guard.zeroize();
            }
        }
    }

    pub fn build_list(&self) -> gtk::ListBox {
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);

        // TODO: Build list

        list
    }

    pub fn build_form(
        &self,
        method: Method,
        button: &gtk::Button,
        rows: &gtk::Box,
        password: &TemplateChild<adw::PasswordEntryRow>,
        view: &TemplateChild<gtk::TextView>,
    ) -> anyhow::Result<()> {
        let path = self.get_path();
        self.validate_name(&path)?;
        let entry = if method == Method::Pinantry {
            self.store.ask(&path)?
        } else {
            self.store.get(&path, self.get_passphrase())?
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

    fn validate_store(&self) -> anyhow::Result<()> {
        if !self.is_unlocked() {
            return Err(anyhow!("Store is locked"));
        }
        if !exists_store_dir() {
            return Err(anyhow!("Store directory does not exist"));
        }
        if !self.store.ok() {
            return Err(anyhow!("Password store is not is not initialized"));
        }
        Ok(())
    }

    fn validate_name(&self, name: &str) -> anyhow::Result<()> {
        self.validate_store()?;
        if name.is_empty() {
            return Err(anyhow!("Name is empty"));
        }
        if !self.store.exists(name) {
            return Err(anyhow!("Entry does not exist"));
        }
        Ok(())
    }

    fn get_passphrase(&self) -> SecretString {
        match self.passphrase.try_lock() {
            Ok(guard) => guard.clone(),
            Err(_) => SecretString::default(),
        }
    }

    fn get_path(&self) -> String {
        match self.path.try_lock() {
            Ok(guard) => guard.clone(),
            Err(_) => String::new(),
        }
    }
}
