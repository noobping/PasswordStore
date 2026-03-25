mod build;
mod controls;
#[cfg(target_os = "linux")]
mod git;
mod host_access;
#[cfg(target_os = "linux")]
mod logs;
pub mod navigation;
mod preferences;
#[cfg(target_os = "linux")]
mod tools;

pub use self::build::create_main_window;
pub use self::git::clone_store_repository;

#[cfg(test)]
mod tests {
    use crate::password::file::{
        clean_pass_file_contents, new_pass_file_contents_from_template,
        parse_structured_pass_lines, structured_otp_line, structured_pass_contents_from_values,
        structured_username_value, uri_to_open, OtpFieldTemplate, StructuredPassLine,
        UsernameFieldTemplate,
    };

    #[test]
    fn structured_fields_strip_display_spacing_but_preserve_it_on_save() {
        let contents = "secret\nemail: hello@example.com\nname:hello";
        let (password, parsed) = parse_structured_pass_lines(contents);
        assert_eq!(password, "secret");

        let templates = parsed
            .iter()
            .map(|(line, _)| line.clone())
            .collect::<Vec<_>>();
        let values = parsed
            .iter()
            .filter_map(|(line, value)| match line {
                StructuredPassLine::Field(_) => value.clone(),
                StructuredPassLine::Username(_)
                | StructuredPassLine::Otp(_)
                | StructuredPassLine::Preserved(_) => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            values,
            vec!["hello@example.com".to_string(), "hello".to_string()]
        );
        assert_eq!(
            structured_pass_contents_from_values(&password, "", None, &templates, &values),
            contents
        );
    }

    #[test]
    fn username_and_otpauth_lines_stay_out_of_dynamic_fields() {
        let contents = "secret\nusername:alice\notpauth://totp/example\nurl: https://example.com";
        let (_, parsed) = parse_structured_pass_lines(contents);

        assert!(matches!(parsed[0].0, StructuredPassLine::Username(_)));
        assert_eq!(parsed[0].1.as_deref(), Some("alice"));
        assert!(matches!(
            parsed[1].0,
            StructuredPassLine::Otp(OtpFieldTemplate::BareUrl)
        ));
        assert_eq!(
            structured_otp_line(&parsed).map(|(_, url)| url),
            Some("otpauth://totp/example".to_string())
        );
        assert!(matches!(parsed[2].0, StructuredPassLine::Field(_)));
        assert_eq!(parsed[2].1.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn new_password_template_becomes_body_after_password_line() {
        assert_eq!(
            new_pass_file_contents_from_template("username:alice\nurl:https://example.com"),
            "\nusername:alice\nurl:https://example.com".to_string()
        );
    }

    #[test]
    fn new_password_template_trims_only_edge_newlines() {
        assert_eq!(
            new_pass_file_contents_from_template("\nusername:alice\n\nurl:https://example.com\n"),
            "\nusername:alice\n\nurl:https://example.com".to_string()
        );
    }

    #[test]
    fn bare_urls_get_https_when_opened() {
        assert_eq!(
            uri_to_open("example.com/path"),
            Some("https://example.com/path".to_string())
        );
    }

    #[test]
    fn explicit_url_schemes_are_preserved() {
        assert_eq!(
            uri_to_open("https://example.com/path"),
            Some("https://example.com/path".to_string())
        );
    }

    #[test]
    fn blank_username_line_is_detected() {
        let (_, parsed) = parse_structured_pass_lines("secret\nusername:\nurl:https://example.com");
        assert_eq!(structured_username_value(&parsed), Some(String::new()));
    }

    #[test]
    fn structured_save_preserves_username_field_template() {
        let templates = vec![
            StructuredPassLine::Username(UsernameFieldTemplate {
                raw_key: "username".to_string(),
                separator_spacing: String::new(),
            }),
            StructuredPassLine::Preserved("url: https://example.com".to_string()),
        ];
        let values = Vec::<String>::new();

        assert_eq!(
            structured_pass_contents_from_values(
                "secret",
                "alice@example.com",
                None,
                &templates,
                &values,
            ),
            "secret\nusername:alice@example.com\nurl: https://example.com".to_string()
        );
    }

    #[test]
    fn clean_pass_file_removes_empty_structured_fields() {
        assert_eq!(
            clean_pass_file_contents(
                "secret\nusername:   \nemail:   hello@example.com\npin:\nurl:https://example.com"
            ),
            "secret\nemail:   hello@example.com\nurl:https://example.com".to_string()
        );
    }

    #[test]
    fn clean_pass_file_removes_blank_otp_entries() {
        assert_eq!(
            clean_pass_file_contents(
                "secret\notpauth://totp/Keycord?issuer=Keycord&secret=&digits=6&period=30\notpauth:   \nurl: https://example.com"
            ),
            "secret\nurl: https://example.com".to_string()
        );
    }

    #[test]
    fn clean_pass_file_keeps_preserved_lines_and_blank_notes() {
        assert_eq!(
            clean_pass_file_contents(
                "secret\nnotes without colon\n\n  \nurl: https://example.com\nusername:"
            ),
            "secret\nnotes without colon\n\n  \nurl: https://example.com".to_string()
        );
    }
}
