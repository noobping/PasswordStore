use adw::glib;
use adw::gtk::Widget;
use adw::prelude::*;
use adw::{EntryRow, PasswordEntryRow};

const USERNAME_FIELD_KEYS: [&str; 3] = ["login", "username", "user"];
const SENSITIVE_FIELD_HINTS: [&str; 8] = [
    "pass",
    "secret",
    "token",
    "pin",
    "key",
    "code",
    "phrase",
    "credential",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynamicFieldTemplate {
    pub(super) raw_key: String,
    pub(super) title: String,
    pub(super) separator_spacing: String,
    pub(super) sensitive: bool,
}

impl DynamicFieldTemplate {
    pub fn new(title: &str, sensitive: Option<bool>) -> Result<Self, &'static str> {
        let title = title.trim();
        if title.is_empty() {
            return Err("Enter a field name.");
        }
        if title.contains(':') {
            return Err("Field names can't contain ':'.");
        }
        if is_username_field_key(title) {
            return Err("Use the username field instead.");
        }
        if title.eq_ignore_ascii_case("otpauth") {
            return Err("Use Add OTP secret instead.");
        }

        Ok(Self {
            raw_key: title.to_string(),
            title: title.to_string(),
            separator_spacing: " ".to_string(),
            sensitive: sensitive.unwrap_or_else(|| Self::suggested_sensitive(title)),
        })
    }

    pub fn suggested_sensitive(title: &str) -> bool {
        is_sensitive_field(title)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsernameFieldTemplate {
    pub raw_key: String,
    pub separator_spacing: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OtpFieldTemplate {
    BareUrl,
    Field {
        raw_key: String,
        separator_spacing: String,
    },
}

impl OtpFieldTemplate {
    pub(super) fn line(&self, url: &str) -> String {
        match self {
            Self::BareUrl => url.to_string(),
            Self::Field {
                raw_key,
                separator_spacing,
            } => format!("{raw_key}:{separator_spacing}{url}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuredPassLine {
    Field(DynamicFieldTemplate),
    Username(UsernameFieldTemplate),
    Otp(OtpFieldTemplate),
    Preserved(String),
}

#[derive(Clone)]
pub enum DynamicFieldRow {
    Plain(EntryRow),
    Secret(PasswordEntryRow),
}

impl DynamicFieldRow {
    pub(super) fn text(&self) -> String {
        match self {
            Self::Plain(row) => row.text().to_string(),
            Self::Secret(row) => row.text().to_string(),
        }
    }

    pub(crate) fn widget(&self) -> Widget {
        match self {
            Self::Plain(row) => row.clone().upcast(),
            Self::Secret(row) => row.clone().upcast(),
        }
    }

    pub(crate) fn focus_editor(&self) {
        match self {
            Self::Plain(row) => focus_entry_row(row),
            Self::Secret(row) => focus_password_row(row),
        }
    }
}

fn focus_entry_row(row: &EntryRow) {
    if let Some(delegate) = row.delegate() {
        glib::idle_add_local_once(move || {
            delegate.grab_focus();
            delegate.select_region(0, -1);
        });
    } else {
        row.grab_focus();
    }
}

fn focus_password_row(row: &PasswordEntryRow) {
    if let Some(delegate) = row.delegate() {
        glib::idle_add_local_once(move || {
            delegate.grab_focus();
            delegate.select_region(0, -1);
        });
    } else {
        row.grab_focus();
    }
}

pub(super) fn is_username_field_key(key: &str) -> bool {
    let key = key.trim().to_ascii_lowercase();
    USERNAME_FIELD_KEYS.contains(&key.as_str())
}

pub(super) fn is_otpauth_line(key: &str, value: &str, raw_line: &str) -> bool {
    let key = key.trim().to_ascii_lowercase();
    key == "otpauth" || value.contains("otpauth://") || raw_line.contains("otpauth://")
}

pub(super) fn is_sensitive_field(key: &str) -> bool {
    let key = key.trim().to_ascii_lowercase();
    SENSITIVE_FIELD_HINTS.iter().any(|hint| key.contains(hint))
}

pub(super) fn is_url_field_key(key: &str) -> bool {
    key.trim().eq_ignore_ascii_case("url")
}

#[cfg(test)]
mod tests {
    use super::DynamicFieldTemplate;

    #[test]
    fn custom_fields_trim_names_and_default_sensitive_hints() {
        let template = DynamicFieldTemplate::new("  API Token  ", None).expect("create template");

        assert_eq!(template.raw_key, "API Token");
        assert_eq!(template.title, "API Token");
        assert_eq!(template.separator_spacing, " ");
        assert!(template.sensitive);
    }

    #[test]
    fn custom_fields_reject_reserved_or_invalid_names() {
        assert_eq!(
            DynamicFieldTemplate::new("", None),
            Err("Enter a field name.")
        );
        assert_eq!(
            DynamicFieldTemplate::new("user", None),
            Err("Use the username field instead.")
        );
        assert_eq!(
            DynamicFieldTemplate::new("otpauth", None),
            Err("Use Add OTP secret instead.")
        );
        assert_eq!(
            DynamicFieldTemplate::new("api:key", None),
            Err("Field names can't contain ':'.")
        );
    }
}
