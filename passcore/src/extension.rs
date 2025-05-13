use secrecy::{ExposeSecret, SecretBox, SecretString};
use std::string::String;

pub trait SecretStringExt {
    fn from_secret_utf8(lines: SecretBox<Vec<u8>>) -> Self;
    fn from_utf8(bytes: Vec<u8>) -> Self;
}

impl SecretStringExt for SecretString {
    fn from_utf8(bytes: Vec<u8>) -> Self {
        match String::from_utf8(bytes) {
            Ok(s) => SecretString::new(s.into()),
            Err(_) => SecretString::new("".into()),
        }
    }

    fn from_secret_utf8(lines: SecretBox<Vec<u8>>) -> Self {
        let bytes: Vec<u8> = lines.expose_secret().clone();
        let text = match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => String::new(),
        };
        SecretString::from(text)
    }
}
