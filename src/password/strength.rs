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
        return Some("Password is empty".to_string());
    }
    if password.trim().is_empty() {
        return Some("Password is only whitespace".to_string());
    }

    let normalized = password.trim().to_ascii_lowercase();
    if COMMON_WEAK_PASSWORDS.contains(&normalized.as_str()) {
        return Some("Matches a common password".to_string());
    }
    if repeated_single_character(password) {
        return Some("Repeated single character".to_string());
    }

    let length = password.chars().count();
    if length < 8 {
        return Some(format!("Too short ({length} characters)"));
    }
    if simple_ascii_sequence(&normalized) {
        return Some("Sequential characters".to_string());
    }

    if looks_like_multiword_passphrase(password) {
        return None;
    }

    let class_count = character_class_count(password);
    let unique_count = unique_char_count(password);

    if length < 10 && class_count <= 2 {
        return Some("Short with limited character variety".to_string());
    }
    if length < 12 && unique_count <= 4 {
        return Some("Very low character variety".to_string());
    }
    if length < 12 && class_count <= 1 {
        return Some("Single character class and short".to_string());
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

    #[test]
    fn weak_password_checks_flag_common_short_and_repetitive_passwords() {
        assert_eq!(
            weak_password_reason("password"),
            Some("Matches a common password".to_string())
        );
        assert_eq!(
            weak_password_reason("1234567"),
            Some("Too short (7 characters)".to_string())
        );
        assert_eq!(
            weak_password_reason("aaaaaa"),
            Some("Repeated single character".to_string())
        );
        assert_eq!(
            weak_password_reason("abcdef"),
            Some("Too short (6 characters)".to_string())
        );
    }

    #[test]
    fn weak_password_checks_allow_longer_passphrases_and_varied_passwords() {
        assert_eq!(weak_password_reason("correct horse battery staple"), None);
        assert_eq!(weak_password_reason("Aq7!mB9#zR4@tN2$"), None);
    }
}
