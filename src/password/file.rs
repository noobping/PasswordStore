use crate::clipboard::add_copy_suffix;
use crate::password::model::OpenPassFile;
use crate::logging::log_error;
use adw::{prelude::*, EntryRow, PasswordEntryRow, Toast, ToastOverlay};
use adw::gtk::{Box as GtkBox, Widget, gdk::Display};
use std::cell::RefCell;
use std::rc::Rc;

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
    raw_key: String,
    title: String,
    separator_spacing: String,
    sensitive: bool,
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
    fn line(&self, url: &str) -> String {
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
    fn text(&self) -> String {
        match self {
            Self::Plain(row) => row.text().to_string(),
            Self::Secret(row) => row.text().to_string(),
        }
    }

    fn widget(&self) -> Widget {
        match self {
            Self::Plain(row) => row.clone().upcast(),
            Self::Secret(row) => row.clone().upcast(),
        }
    }
}

fn is_username_field_key(key: &str) -> bool {
    let key = key.trim().to_ascii_lowercase();
    USERNAME_FIELD_KEYS.contains(&key.as_str())
}

fn is_otpauth_line(key: &str, value: &str, raw_line: &str) -> bool {
    let key = key.trim().to_ascii_lowercase();
    key == "otpauth" || value.contains("otpauth://") || raw_line.contains("otpauth://")
}

fn is_sensitive_field(key: &str) -> bool {
    let key = key.trim().to_ascii_lowercase();
    SENSITIVE_FIELD_HINTS
        .iter()
        .any(|hint| key.contains(hint))
}

fn is_url_field_key(key: &str) -> bool {
    key.trim().eq_ignore_ascii_case("url")
}

pub(crate) fn structured_username_value(
    lines: &[(StructuredPassLine, Option<String>)],
) -> Option<String> {
    lines.iter().find_map(|(line, value)| match line {
        StructuredPassLine::Username(_) => value.clone(),
        _ => None,
    })
}

pub(crate) fn structured_otp_line(
    lines: &[(StructuredPassLine, Option<String>)],
) -> Option<(OtpFieldTemplate, String)> {
    lines.iter().find_map(|(line, value)| match line {
        StructuredPassLine::Otp(template) => {
            value.clone().map(|url| (template.clone(), url))
        }
        _ => None,
    })
}

pub(crate) fn uri_to_open(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if value.contains("://") {
        Some(value.to_string())
    } else {
        Some(format!("https://{value}"))
    }
}

pub(crate) fn clear_box_children(box_widget: &GtkBox) {
    while let Some(child) = box_widget.first_child() {
        box_widget.remove(&child);
    }
}

#[cfg(target_os = "linux")]
fn add_open_url_suffix(
    row: &EntryRow,
    text: impl Fn() -> String + 'static,
    overlay: &ToastOverlay,
) {
    let button = adw::gtk::Button::from_icon_name("adw-external-link-symbolic");
    button.set_tooltip_text(Some("Open URL"));
    button.add_css_class("flat");
    let overlay = overlay.clone();
    button.connect_clicked(move |_| {
        let Some(uri) = uri_to_open(&text()) else {
            overlay.add_toast(Toast::new("Enter a URL."));
            return;
        };

        let launch_result = Display::default().map_or_else(
            || adw::gio::AppInfo::launch_default_for_uri(&uri, None::<&adw::gio::AppLaunchContext>),
            |display| {
                let context = display.app_launch_context();
                adw::gio::AppInfo::launch_default_for_uri(&uri, Some(&context))
            },
        );

        if let Err(error) = launch_result {
            log_error(format!(
                "Failed to open URL in the default browser.\nURL: {uri}\nerror: {error}"
            ));
            overlay.add_toast(Toast::new("Couldn't open the link."));
        }
    });
    row.add_suffix(&button);
}

fn apply_field_row_style<W: IsA<Widget>>(widget: &W) {
    widget.set_margin_start(15);
    widget.set_margin_end(15);
    widget.set_margin_bottom(6);
}

fn build_dynamic_field_row(
    template: &DynamicFieldTemplate,
    value: &str,
    overlay: &ToastOverlay,
) -> DynamicFieldRow {
    if template.sensitive {
        let row = PasswordEntryRow::new();
        row.set_title(&template.title);
        row.set_text(value);
        apply_field_row_style(&row);
        let row_clone = row.clone();
        add_copy_suffix(&row, move || row_clone.text().to_string(), overlay);
        DynamicFieldRow::Secret(row)
    } else {
        let row = EntryRow::new();
        row.set_title(&template.title);
        row.set_text(value);
        apply_field_row_style(&row);
        let row_clone = row.clone();
        add_copy_suffix(&row, move || row_clone.text().to_string(), overlay);
        #[cfg(target_os = "linux")]
        if is_url_field_key(&template.raw_key) {
            let row_clone = row.clone();
            add_open_url_suffix(&row, move || row_clone.text().to_string(), overlay);
        }
        DynamicFieldRow::Plain(row)
    }
}

pub(crate) fn parse_structured_pass_lines(
    contents: &str,
) -> (String, Vec<(StructuredPassLine, Option<String>)>) {
    let mut lines = contents.lines();
    let password = lines.next().unwrap_or_default().to_string();
    let structured = lines
        .map(|line| {
            if line.trim_start().starts_with("otpauth://") {
                return (
                    StructuredPassLine::Otp(OtpFieldTemplate::BareUrl),
                    Some(line.trim().to_string()),
                );
            }

            let Some((raw_key, raw_value)) = line.split_once(':') else {
                return (StructuredPassLine::Preserved(line.to_string()), None);
            };

            let title = raw_key.trim().to_string();
            if title.is_empty() {
                return (StructuredPassLine::Preserved(line.to_string()), None);
            }

            if is_username_field_key(&title) {
                let separator_spacing = raw_value
                    .chars()
                    .take_while(|c| c.is_ascii_whitespace())
                    .collect::<String>();
                let value = raw_value.trim().to_string();
                let template = UsernameFieldTemplate {
                    raw_key: raw_key.to_string(),
                    separator_spacing,
                };

                return (StructuredPassLine::Username(template), Some(value));
            }

            if is_otpauth_line(&title, raw_value, line) {
                let separator_spacing = raw_value
                    .chars()
                    .take_while(|c| c.is_ascii_whitespace())
                    .collect::<String>();
                let value = raw_value
                    .trim_start_matches(|c: char| c.is_ascii_whitespace())
                    .to_string();
                let template = OtpFieldTemplate::Field {
                    raw_key: raw_key.to_string(),
                    separator_spacing,
                };
                return (StructuredPassLine::Otp(template), Some(value));
            }

            let separator_spacing = raw_value
                .chars()
                .take_while(|c| c.is_ascii_whitespace())
                .collect::<String>();
            let value = raw_value
                .trim_start_matches(|c: char| c.is_ascii_whitespace())
                .to_string();
            let template = DynamicFieldTemplate {
                raw_key: raw_key.to_string(),
                title,
                separator_spacing,
                sensitive: is_sensitive_field(raw_key),
            };

            (StructuredPassLine::Field(template), Some(value))
        })
        .collect();

    (password, structured)
}

pub(crate) fn rebuild_dynamic_fields_from_lines(
    box_widget: &GtkBox,
    overlay: &ToastOverlay,
    templates_state: &Rc<RefCell<Vec<StructuredPassLine>>>,
    rows_state: &Rc<RefCell<Vec<DynamicFieldRow>>>,
    structured_lines: &[(StructuredPassLine, Option<String>)],
) {
    clear_box_children(box_widget);
    templates_state.borrow_mut().clear();
    rows_state.borrow_mut().clear();

    let mut rows = Vec::new();
    let mut templates = Vec::new();

    for (line, value) in structured_lines.iter().cloned() {
        match line {
            StructuredPassLine::Field(template) => {
                let row =
                    build_dynamic_field_row(&template, value.as_deref().unwrap_or_default(), overlay);
                box_widget.append(&row.widget());
                rows.push(row);
                templates.push(StructuredPassLine::Field(template));
            }
            StructuredPassLine::Username(template) => {
                templates.push(StructuredPassLine::Username(template));
            }
            StructuredPassLine::Otp(template) => {
                templates.push(StructuredPassLine::Otp(template));
            }
            StructuredPassLine::Preserved(line) => {
                templates.push(StructuredPassLine::Preserved(line));
            }
        }
    }

    box_widget.set_visible(!rows.is_empty());
    *templates_state.borrow_mut() = templates;
    *rows_state.borrow_mut() = rows;
}

pub(crate) fn structured_pass_contents(
    password: &str,
    username_value: &str,
    otp_url: Option<&str>,
    templates: &[StructuredPassLine],
    rows: &[DynamicFieldRow],
) -> String {
    let values = rows.iter().map(DynamicFieldRow::text).collect::<Vec<_>>();
    structured_pass_contents_from_values(password, username_value, otp_url, templates, &values)
}

pub(crate) fn structured_pass_contents_from_values(
    password: &str,
    username_value: &str,
    otp_url: Option<&str>,
    templates: &[StructuredPassLine],
    values: &[String],
) -> String {
    let mut output = String::new();
    output.push_str(password);

    let mut dynamic_values = values.iter();
    for template in templates {
        output.push('\n');
        match template {
            StructuredPassLine::Field(template) => {
                output.push_str(&template.raw_key);
                output.push(':');
                output.push_str(&template.separator_spacing);
                if let Some(value) = dynamic_values.next() {
                    output.push_str(value);
                }
            }
            StructuredPassLine::Username(template) => {
                output.push_str(&template.raw_key);
                output.push(':');
                output.push_str(&template.separator_spacing);
                output.push_str(username_value);
            }
            StructuredPassLine::Otp(template) => {
                if let Some(otp_url) = otp_url {
                    output.push_str(&template.line(otp_url));
                }
            }
            StructuredPassLine::Preserved(line) => output.push_str(line),
        }
    }

    output
}

pub(crate) fn new_pass_file_contents_from_template(template: &str) -> String {
    let template = template.trim_matches('\n');
    if template.is_empty() {
        String::new()
    } else {
        format!("\n{template}")
    }
}

pub(crate) fn sync_username_row(row: &EntryRow, pass_file: Option<&OpenPassFile>) {
    row.set_editable(false);
    if let Some(username) = pass_file.and_then(OpenPassFile::username) {
        row.set_text(username);
        row.set_visible(true);
    } else {
        row.set_text("");
        row.set_visible(false);
    }
}

pub(crate) fn sync_username_row_from_parsed_lines(
    row: &EntryRow,
    pass_file: Option<&OpenPassFile>,
    lines: &[(StructuredPassLine, Option<String>)],
) {
    if let Some(username) = structured_username_value(lines) {
        row.set_text(&username);
        row.set_visible(true);
        row.set_editable(true);
    } else {
        sync_username_row(row, pass_file);
    }
}
