use regex::Regex;
use std::sync::OnceLock;

fn email_regex() -> &'static Regex {
    static EMAIL_REGEX: OnceLock<Regex> = OnceLock::new();
    EMAIL_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)^[a-z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)+$",
        )
        .expect("email validation regex should compile")
    })
}

pub fn email_field_value(contents: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case("email")
            .then(|| value.trim().to_string())
    })
}

pub fn is_valid_email_address(email: &str) -> bool {
    let email = email.trim();
    !email.is_empty() && email_regex().is_match(email)
}

pub fn validate_email_address(email: &str) -> Result<String, &'static str> {
    let email = email.trim();
    if email.is_empty() {
        return Err("Enter an email address.");
    }
    if !is_valid_email_address(email) {
        return Err("Enter a valid email address.");
    }

    Ok(email.to_string())
}

pub fn validate_pass_file_email_fields(contents: &str) -> Result<(), &'static str> {
    for line in contents.lines().skip(1) {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("email") {
            continue;
        }
        if !is_valid_email_address(value) {
            return Err("Email fields must use a valid email address.");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        email_field_value, is_valid_email_address, validate_email_address,
        validate_pass_file_email_fields,
    };

    #[test]
    fn email_addresses_require_a_domain_and_local_part() {
        assert!(is_valid_email_address("person@example.com"));
        assert!(is_valid_email_address("PERSON+tag@sub.example.com"));
        assert!(!is_valid_email_address("person"));
        assert!(!is_valid_email_address("person@localhost"));
        assert!(!is_valid_email_address("person@"));
    }

    #[test]
    fn email_validation_trims_input() {
        assert_eq!(
            validate_email_address("  person@example.com  "),
            Ok("person@example.com".to_string())
        );
        assert_eq!(validate_email_address(""), Err("Enter an email address."));
        assert_eq!(
            validate_email_address("invalid"),
            Err("Enter a valid email address.")
        );
    }

    #[test]
    fn pass_files_reject_invalid_email_fields() {
        assert_eq!(
            validate_pass_file_email_fields("secret\nemail: person@example.com"),
            Ok(())
        );
        assert_eq!(
            validate_pass_file_email_fields("secret\nEmail: invalid"),
            Err("Email fields must use a valid email address.")
        );
        assert_eq!(
            validate_pass_file_email_fields("secret\nnotes without separator"),
            Ok(())
        );
    }

    #[test]
    fn email_field_value_reads_the_first_email_line() {
        assert_eq!(
            email_field_value("secret\nusername: alice\nemail: alice@example.com\nemail: later"),
            Some("alice@example.com".to_string())
        );
        assert_eq!(email_field_value("secret\nusername: alice"), None);
    }
}
