use super::parse::{parse_structured_pass_lines, structured_username_value};
use super::types::{DynamicFieldRow, StructuredPassLine};
use crate::password::model::OpenPassFile;
use adw::prelude::*;
use adw::EntryRow;

pub fn structured_pass_contents(
    password: &str,
    username_value: &str,
    otp_url: Option<&str>,
    templates: &[StructuredPassLine],
    rows: &[DynamicFieldRow],
) -> String {
    let values = rows.iter().map(DynamicFieldRow::text).collect::<Vec<_>>();
    structured_pass_contents_from_values(password, username_value, otp_url, templates, &values)
}

pub fn structured_pass_contents_from_values(
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

pub fn clean_pass_file_contents(contents: &str) -> String {
    let (password, structured_lines) = parse_structured_pass_lines(contents);
    let mut output = String::new();
    output.push_str(&password);

    for (line, value) in structured_lines {
        let Some(line) = cleaned_line(line, value) else {
            continue;
        };
        output.push('\n');
        output.push_str(&line);
    }

    output
}

pub fn new_pass_file_contents_from_template(template: &str) -> String {
    let template = template.trim_matches('\n');
    if template.is_empty() {
        String::new()
    } else {
        format!("\n{template}")
    }
}

fn cleaned_line(line: StructuredPassLine, value: Option<String>) -> Option<String> {
    match line {
        StructuredPassLine::Field(template) => {
            value.filter(|value| !value.is_empty()).map(|value| {
                format!(
                    "{}:{}{}",
                    template.raw_key, template.separator_spacing, value
                )
            })
        }
        StructuredPassLine::Username(template) => {
            value.filter(|value| !value.is_empty()).map(|value| {
                format!(
                    "{}:{}{}",
                    template.raw_key, template.separator_spacing, value
                )
            })
        }
        StructuredPassLine::Otp(template) => value
            .filter(|url| should_keep_otp_url(url))
            .map(|url| template.line(&url)),
        StructuredPassLine::Preserved(line) => Some(line),
    }
}

fn should_keep_otp_url(url: &str) -> bool {
    !url.trim().is_empty()
        && !otp_secret_from_url(url)
            .unwrap_or_default()
            .trim()
            .is_empty()
}

fn otp_secret_from_url(url: &str) -> Option<String> {
    let query = url.split_once('?')?.1.split('#').next().unwrap_or_default();
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        if key.eq_ignore_ascii_case("secret") {
            Some(value.to_string())
        } else {
            None
        }
    })
}

fn username_row_state(pass_file: Option<&OpenPassFile>) -> (Option<String>, bool) {
    pass_file
        .and_then(OpenPassFile::username)
        .map_or((None, false), |username| (Some(username.to_string()), true))
}

pub fn sync_username_row(row: &EntryRow, pass_file: Option<&OpenPassFile>) {
    let (username, editable) = username_row_state(pass_file);
    if let Some(username) = username {
        row.set_text(&username);
        row.set_visible(true);
        row.set_editable(editable);
    } else {
        row.set_text("");
        row.set_visible(false);
        row.set_editable(false);
    }
}

pub fn sync_username_row_from_parsed_lines(
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

#[cfg(test)]
mod tests {
    use super::username_row_state;
    use crate::password::model::OpenPassFile;
    use crate::preferences::UsernameFallbackMode;

    #[test]
    fn visible_usernames_stay_editable_for_path_and_field_sources() {
        let path_pass_file = OpenPassFile::from_label_with_mode(
            "/tmp/store",
            "work/alice/github",
            UsernameFallbackMode::Folder,
        );
        assert_eq!(
            username_row_state(Some(&path_pass_file)),
            (Some("alice".to_string()), true)
        );

        let mut field_pass_file = path_pass_file;
        field_pass_file.refresh_from_contents("secret\nusername: bob");
        assert_eq!(
            username_row_state(Some(&field_pass_file)),
            (Some("bob".to_string()), true)
        );
    }
}
