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
