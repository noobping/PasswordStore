use crate::i18n::gettext;

const COMMON_WEAK_PASSWORDS: &[&str] = &[
    "000000",
    "111111",
    "112233",
    "123123",
    "123456",
    "12345678",
    "123456789",
    "abc123",
    "admin",
    "changeme",
    "letmein",
    "monkey",
    "password",
    "passw0rd",
    "qwerty",
    "secret",
    "welcome",
];

pub fn weak_password_reason(password: &str) -> Option<String> {
    if password.is_empty() {
        return Some(gettext("Password is empty"));
    }
    if password.trim().is_empty() {
        return Some(gettext("Password is only whitespace"));
    }

    let normalized = password.trim().to_ascii_lowercase();
    if COMMON_WEAK_PASSWORDS.contains(&normalized.as_str()) {
        return Some(gettext("Matches a common password"));
    }
    if repeated_single_character(password) {
        return Some(gettext("Repeated single character"));
    }

    let length = password.chars().count();
    if length < 8 {
        return Some(
            gettext("Too short ({length} characters)").replace("{length}", &length.to_string()),
        );
    }
    if simple_ascii_sequence(&normalized) {
        return Some(gettext("Sequential characters"));
    }

    if looks_like_multiword_passphrase(password) {
        return None;
    }

    let class_count = character_class_count(password);
    let unique_count = unique_char_count(password);

    if length < 10 && class_count <= 2 {
        return Some(gettext("Short with limited character variety"));
    }
    if length < 12 && unique_count <= 4 {
        return Some(gettext("Very low character variety"));
    }
    if length < 12 && class_count <= 1 {
        return Some(gettext("Single character class and short"));
    }

    None
}
fn repeated_single_character(password: &str) -> bool {
    let mut chars = password.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    password.chars().count() >= 4 && chars.all(|ch| ch == first)
}

fn simple_ascii_sequence(password: &str) -> bool {
    let bytes = password.as_bytes();
    if bytes.len() < 4 {
        return false;
    }

    bytes.windows(2).all(|pair| pair[1] == pair[0] + 1)
        || bytes.windows(2).all(|pair| pair[0] == pair[1] + 1)
}

fn looks_like_multiword_passphrase(password: &str) -> bool {
    let words = password
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|segment| segment.chars().count() >= 3)
        .count();
    words >= 3 && password.chars().count() >= 20
}

fn character_class_count(password: &str) -> usize {
    let has_lower = password.chars().any(|ch| ch.is_lowercase());
    let has_upper = password.chars().any(|ch| ch.is_uppercase());
    let has_digit = password.chars().any(|ch| ch.is_ascii_digit());
    let has_symbol = password
        .chars()
        .any(|ch| !ch.is_alphanumeric() && !ch.is_whitespace());

    [has_lower, has_upper, has_digit, has_symbol]
        .into_iter()
        .filter(|present| *present)
        .count()
}

fn unique_char_count(password: &str) -> usize {
    let mut unique = Vec::new();
    for ch in password.chars().flat_map(char::to_lowercase) {
        if !unique.contains(&ch) {
            unique.push(ch);
        }
    }

    unique.len()
}

#[cfg(test)]
mod tests {
    use super::weak_password_reason;
    use crate::i18n::gettext;

    #[test]
    fn weak_password_checks_flag_common_short_and_repetitive_passwords() {
        assert_eq!(
            weak_password_reason("password"),
            Some(gettext("Matches a common password"))
        );
        assert_eq!(
            weak_password_reason("1234567"),
            Some(gettext("Too short ({length} characters)").replace("{length}", "7"))
        );
        assert_eq!(
            weak_password_reason("aaaaaa"),
            Some(gettext("Repeated single character"))
        );
        assert_eq!(
            weak_password_reason("abcdef"),
            Some(gettext("Too short ({length} characters)").replace("{length}", "6"))
        );
    }

    #[test]
    fn weak_password_checks_allow_longer_passphrases_and_varied_passwords() {
        assert_eq!(weak_password_reason("correct horse battery staple"), None);
        assert_eq!(weak_password_reason("Aq7!mB9#zR4@tN2$"), None);
    }
}
