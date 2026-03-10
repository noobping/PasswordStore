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
pub(crate) struct DynamicFieldTemplate {
    pub(super) raw_key: String,
    pub(super) title: String,
    pub(super) separator_spacing: String,
    pub(super) sensitive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UsernameFieldTemplate {
    pub(crate) raw_key: String,
    pub(crate) separator_spacing: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OtpFieldTemplate {
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
pub(crate) enum StructuredPassLine {
    Field(DynamicFieldTemplate),
    Username(UsernameFieldTemplate),
    Otp(OtpFieldTemplate),
    Preserved(String),
}

#[derive(Clone)]
pub(crate) enum DynamicFieldRow {
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

    pub(super) fn widget(&self) -> Widget {
        match self {
            Self::Plain(row) => row.clone().upcast(),
            Self::Secret(row) => row.clone().upcast(),
        }
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
