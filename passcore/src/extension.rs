use secrecy::{ExposeSecret, SecretBox, SecretString};
use std::string::String;

pub trait SecretStringExt {
    fn from_secret_utf8(
        bytes: SecretBox<Vec<u8>>,
    ) -> Result<SecretString, std::string::FromUtf8Error>;
}

impl SecretStringExt for SecretString {
    fn from_secret_utf8(
        bytes: SecretBox<Vec<u8>>,
    ) -> Result<SecretString, std::string::FromUtf8Error> {
        let bytes: Vec<u8> = bytes.expose_secret().clone();
        Ok(String::from_utf8(bytes)?.into())
    }
}

pub trait StringExt {
    fn split_path(&self) -> (String, String);
    fn split_field(&self) -> (String, String);
    fn to_secret(&self) -> SecretString;
}

impl StringExt for String {
    fn split_path(&self) -> (String, String) {
        if !self.contains('/') {
            return (String::new(), self.to_string());
        }
        let last_slash = self.rfind('/').unwrap_or(self.len());
        let folder = self[..last_slash].to_string();
        let name = self[last_slash + 1..].to_string();
        (folder, name)
    }

    fn split_field(&self) -> (String, String) {
        let mut parts = self.as_str().splitn(2, ':');
        let field = parts.next().unwrap().trim().to_string();
        let value = parts.next().unwrap_or("").trim().to_string();
        (field, value)
    }

    fn to_secret(&self) -> SecretString {
        SecretString::from(self.to_string())
    }
}
