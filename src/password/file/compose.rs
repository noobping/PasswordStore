use super::parse::{
    canonical_search_field_key, parse_structured_pass_lines, structured_username_value,
};
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

pub fn apply_pass_file_template_contents(contents: &str, template: &str) -> String {
    let template_contents = new_pass_file_contents_from_template(template);
    if template_contents.is_empty() {
        return contents.to_string();
    }

    let (password, mut current_lines) = parse_structured_pass_lines(contents);
    let (_, template_lines) = parse_structured_pass_lines(&template_contents);
    let mut insert_at = current_lines
        .iter()
        .position(|(line, _)| matches!(line, StructuredPassLine::Preserved(_)))
        .unwrap_or(current_lines.len());
    let original_len = current_lines.len();

    for (line, value) in template_lines {
        if matches!(line, StructuredPassLine::Preserved(_))
            || has_matching_template_line(&current_lines, &line)
        {
            continue;
        }

        current_lines.insert(insert_at, (line, value));
        insert_at += 1;
    }

    if current_lines.len() == original_len {
        return contents.to_string();
    }

    structured_pass_contents_from_lines(&password, &current_lines)
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

fn structured_pass_contents_from_lines(
    password: &str,
    lines: &[(StructuredPassLine, Option<String>)],
) -> String {
    let mut output = String::new();
    output.push_str(password);

    for (line, value) in lines {
        output.push('\n');
        output.push_str(&line_contents(line, value.as_deref()));
    }

    output
}

fn line_contents(line: &StructuredPassLine, value: Option<&str>) -> String {
    match line {
        StructuredPassLine::Field(template) => {
            format!(
                "{}:{}{}",
                template.raw_key,
                template.separator_spacing,
                value.unwrap_or_default()
            )
        }
        StructuredPassLine::Username(template) => {
            format!(
                "{}:{}{}",
                template.raw_key,
                template.separator_spacing,
                value.unwrap_or_default()
            )
        }
        StructuredPassLine::Otp(template) => template.line(value.unwrap_or_default()),
        StructuredPassLine::Preserved(line) => line.clone(),
    }
}

fn has_matching_template_line(
    lines: &[(StructuredPassLine, Option<String>)],
    candidate: &StructuredPassLine,
) -> bool {
    let Some(candidate_identity) = template_line_identity(candidate) else {
        return false;
    };

    lines.iter().any(|(line, _)| {
        template_line_identity(line)
            .as_ref()
            .is_some_and(|identity| identity == &candidate_identity)
    })
}

#[derive(Debug, PartialEq, Eq)]
enum TemplateLineIdentity {
    Username,
    Otp,
    Field(String),
}

fn template_line_identity(line: &StructuredPassLine) -> Option<TemplateLineIdentity> {
    match line {
        StructuredPassLine::Username(_) => Some(TemplateLineIdentity::Username),
        StructuredPassLine::Otp(_) => Some(TemplateLineIdentity::Otp),
        StructuredPassLine::Field(template) => {
            canonical_search_field_key(&template.title).map(TemplateLineIdentity::Field)
        }
        StructuredPassLine::Preserved(_) => None,
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
