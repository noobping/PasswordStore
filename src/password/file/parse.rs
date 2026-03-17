use super::types::{
    is_otpauth_line, is_sensitive_field, is_username_field_key, DynamicFieldTemplate,
    OtpFieldTemplate, StructuredPassLine, UsernameFieldTemplate,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchablePassField {
    pub key: String,
    pub value: String,
    pub normalized_value: String,
}

pub fn structured_username_value(lines: &[(StructuredPassLine, Option<String>)]) -> Option<String> {
    lines.iter().find_map(|(line, value)| match line {
        StructuredPassLine::Username(_) => value.clone(),
        _ => None,
    })
}

pub fn structured_otp_line(
    lines: &[(StructuredPassLine, Option<String>)],
) -> Option<(OtpFieldTemplate, String)> {
    lines.iter().find_map(|(line, value)| match line {
        StructuredPassLine::Otp(template) => value.clone().map(|url| (template.clone(), url)),
        _ => None,
    })
}

pub fn pass_file_has_otp(contents: &str) -> bool {
    let (_, structured_lines) = parse_structured_pass_lines(contents);
    structured_otp_line(&structured_lines).is_some()
}

pub fn canonical_search_field_key(key: &str) -> Option<String> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }

    if is_username_field_key(key) {
        return Some("username".to_string());
    }
    if key.eq_ignore_ascii_case("otpauth") {
        return None;
    }

    Some(key.to_ascii_lowercase())
}

pub fn searchable_pass_fields(contents: &str) -> Vec<SearchablePassField> {
    let (_, structured_lines) = parse_structured_pass_lines(contents);
    structured_lines
        .into_iter()
        .filter_map(|(line, value)| {
            let value = value?;
            let key = match line {
                StructuredPassLine::Username(_) => Some("username".to_string()),
                StructuredPassLine::Otp(_) => None,
                StructuredPassLine::Field(template) => canonical_search_field_key(&template.title),
                StructuredPassLine::Preserved(_) => None,
            }?;
            let normalized_value = value.to_lowercase();

            Some(SearchablePassField {
                key,
                value,
                normalized_value,
            })
        })
        .collect()
}

pub fn parse_structured_pass_lines(
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
                return (
                    StructuredPassLine::Username(UsernameFieldTemplate {
                        raw_key: raw_key.to_string(),
                        separator_spacing: leading_spacing(raw_value),
                    }),
                    Some(raw_value.trim().to_string()),
                );
            }

            if is_otpauth_line(&title, raw_value, line) {
                return (
                    StructuredPassLine::Otp(OtpFieldTemplate::Field {
                        raw_key: raw_key.to_string(),
                        separator_spacing: leading_spacing(raw_value),
                    }),
                    Some(trim_leading_spacing(raw_value)),
                );
            }

            (
                StructuredPassLine::Field(DynamicFieldTemplate {
                    raw_key: raw_key.to_string(),
                    title,
                    separator_spacing: leading_spacing(raw_value),
                    sensitive: is_sensitive_field(raw_key),
                }),
                Some(trim_leading_spacing(raw_value)),
            )
        })
        .collect();

    (password, structured)
}

fn leading_spacing(value: &str) -> String {
    value
        .chars()
        .take_while(char::is_ascii_whitespace)
        .collect()
}

fn trim_leading_spacing(value: &str) -> String {
    value
        .trim_start_matches(|c: char| c.is_ascii_whitespace())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{pass_file_has_otp, searchable_pass_fields, SearchablePassField};

    fn field(key: &str, value: &str) -> SearchablePassField {
        SearchablePassField {
            key: key.to_string(),
            value: value.to_string(),
            normalized_value: value.to_lowercase(),
        }
    }

    #[test]
    fn username_aliases_share_the_username_key() {
        assert_eq!(
            searchable_pass_fields("secret\nlogin: Alice\nuser: Bob\nusername: Carol"),
            vec![
                field("username", "Alice"),
                field("username", "Bob"),
                field("username", "Carol"),
            ]
        );
    }

    #[test]
    fn dynamic_fields_are_indexed_by_their_pass_file_keys() {
        assert_eq!(
            searchable_pass_fields("secret\nUrl: https://example.com\nemail: Person@Example.com"),
            vec![
                field("url", "https://example.com"),
                field("email", "Person@Example.com"),
            ]
        );
    }

    #[test]
    fn otp_lines_are_not_indexed_for_search() {
        assert_eq!(
            searchable_pass_fields("secret\notpauth://totp/Example\notpauth: otpauth://totp/Alt"),
            Vec::<SearchablePassField>::new()
        );
    }

    #[test]
    fn pass_file_otp_detection_tracks_structured_or_bare_urls() {
        assert!(pass_file_has_otp("secret\notpauth://totp/Example?secret=ABC"));
        assert!(pass_file_has_otp(
            "secret\notpauth: otpauth://totp/Example?secret=ABC"
        ));
        assert!(!pass_file_has_otp("secret\nusername: alice\nurl: https://example.com"));
    }

    #[test]
    fn password_lines_and_preserved_text_do_not_become_search_fields() {
        assert_eq!(
            searchable_pass_fields(
                "secret-value\nnotes without colon\n  \nurl: https://example.com"
            ),
            vec![field("url", "https://example.com")]
        );
    }
}
