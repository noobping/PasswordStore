const FIDO2_RECIPIENT_METADATA_PREFIX: &str = "keycord-fido2-recipient-v1=";
const FIDO2_RECIPIENT_FALLBACK_LABEL: &str = "FIDO2 security key";
pub const FIDO2_RECIPIENTS_FILE_NAME: &str = ".fido-id";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fido2StoreRecipient {
    pub id: String,
    pub label: String,
    pub credential_id: Vec<u8>,
}

pub fn build_fido2_recipient_string(
    id: &str,
    label: &str,
    credential_id: &[u8],
) -> Result<String, String> {
    let recipient = Fido2StoreRecipient {
        id: normalize_fido2_recipient_id(id)?,
        label: normalize_fido2_recipient_label(label),
        credential_id: credential_id.to_vec(),
    };
    Ok(format!(
        "{FIDO2_RECIPIENT_METADATA_PREFIX}{}:{}:{}",
        recipient.id,
        encode_hex(recipient.label.as_bytes()),
        encode_hex(&recipient.credential_id),
    ))
}

pub fn parse_fido2_recipient_string(value: &str) -> Result<Option<Fido2StoreRecipient>, String> {
    let Some(payload) = value
        .trim()
        .strip_prefix(FIDO2_RECIPIENT_METADATA_PREFIX)
        .map(str::trim)
    else {
        return Ok(None);
    };

    let mut parts = payload.splitn(3, ':');
    let Some(id) = parts.next() else {
        return Err("Invalid FIDO2 store recipient.".to_string());
    };
    let Some(label_hex) = parts.next() else {
        return Err("Invalid FIDO2 store recipient.".to_string());
    };
    let Some(credential_hex) = parts.next() else {
        return Err("Invalid FIDO2 store recipient.".to_string());
    };

    let id = normalize_fido2_recipient_id(id)?;
    let label = if label_hex.is_empty() {
        FIDO2_RECIPIENT_FALLBACK_LABEL.to_string()
    } else {
        String::from_utf8(decode_hex(label_hex)?)
            .map_err(|err| format!("Invalid FIDO2 recipient label: {err}"))?
    };
    let credential_id = decode_hex(credential_hex)?;
    if credential_id.is_empty() {
        return Err("Invalid FIDO2 store recipient.".to_string());
    }

    Ok(Some(Fido2StoreRecipient {
        id,
        label,
        credential_id,
    }))
}

pub fn parse_fido2_recipient_metadata_line(line: &str) -> Result<Option<String>, String> {
    let Some(value) = line.trim().strip_prefix('#').map(str::trim) else {
        return Ok(None);
    };
    parse_fido2_recipient_string(value).map(|parsed| parsed.map(|_| value.to_string()))
}

pub fn is_fido2_recipient_string(value: &str) -> bool {
    parse_fido2_recipient_string(value).ok().flatten().is_some()
}

pub fn fido2_recipient_title(value: &str) -> Option<String> {
    parse_fido2_recipient_string(value)
        .ok()
        .flatten()
        .map(|recipient| recipient.label)
}

pub fn fido2_recipient_subtitle(value: &str) -> Option<String> {
    parse_fido2_recipient_string(value)
        .ok()
        .flatten()
        .map(|recipient| format!("{} - FIDO2 recipient", recipient.id))
}

fn normalize_fido2_recipient_id(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.len() != 40 || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid FIDO2 store recipient.".to_string());
    }

    Ok(trimmed.to_ascii_uppercase())
}

fn normalize_fido2_recipient_label(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        FIDO2_RECIPIENT_FALLBACK_LABEL.to_string()
    } else {
        trimmed.to_string()
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from_digit((byte >> 4) as u32, 16).expect("hex digit"));
        encoded.push(char::from_digit((byte & 0x0f) as u32, 16).expect("hex digit"));
    }
    encoded
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err("Invalid FIDO2 store recipient.".to_string());
    }

    let mut decoded = Vec::with_capacity(value.len() / 2);
    let mut index = 0;
    while index < value.len() {
        let byte = u8::from_str_radix(&value[index..index + 2], 16)
            .map_err(|_| "Invalid FIDO2 store recipient.".to_string())?;
        decoded.push(byte);
        index += 2;
    }

    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::{
        build_fido2_recipient_string, fido2_recipient_subtitle, fido2_recipient_title,
        parse_fido2_recipient_metadata_line, parse_fido2_recipient_string,
    };

    #[test]
    fn fido2_recipient_strings_round_trip() {
        let encoded = build_fido2_recipient_string(
            "0123456789abcdef0123456789abcdef01234567",
            "Desk Key",
            b"cred",
        )
        .expect("build recipient string");
        let parsed = parse_fido2_recipient_string(&encoded)
            .expect("parse recipient string")
            .expect("expected a FIDO2 recipient");

        assert_eq!(parsed.id, "0123456789ABCDEF0123456789ABCDEF01234567");
        assert_eq!(parsed.label, "Desk Key");
        assert_eq!(parsed.credential_id, b"cred");
        assert_eq!(fido2_recipient_title(&encoded).as_deref(), Some("Desk Key"));
        assert_eq!(
            fido2_recipient_subtitle(&encoded).as_deref(),
            Some("0123456789ABCDEF0123456789ABCDEF01234567 - FIDO2 recipient")
        );
    }

    #[test]
    fn fido2_recipient_metadata_lines_are_recognized() {
        let encoded =
            build_fido2_recipient_string("89abcdef0123456789abcdef0123456789abcdef", "", b"cred")
                .expect("build recipient string");

        assert_eq!(
            parse_fido2_recipient_metadata_line(&format!("# {encoded}"))
                .expect("parse metadata line"),
            Some(encoded)
        );
    }
}
