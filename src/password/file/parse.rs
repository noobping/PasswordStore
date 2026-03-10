use super::types::{
    is_otpauth_line, is_sensitive_field, is_username_field_key, DynamicFieldTemplate,
    OtpFieldTemplate, StructuredPassLine, UsernameFieldTemplate,
};

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
        StructuredPassLine::Otp(template) => value.clone().map(|url| (template.clone(), url)),
        _ => None,
    })
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
        .take_while(|c| c.is_ascii_whitespace())
        .collect()
}

fn trim_leading_spacing(value: &str) -> String {
    value
        .trim_start_matches(|c: char| c.is_ascii_whitespace())
        .to_string()
}
