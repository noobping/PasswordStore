use super::parse::structured_username_value;
use super::types::{DynamicFieldRow, StructuredPassLine};
use crate::password::model::OpenPassFile;
use adw::prelude::*;
use adw::EntryRow;

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
