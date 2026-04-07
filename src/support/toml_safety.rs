use serde::de::DeserializeOwned;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TomlParseLimits {
    pub(crate) max_bytes: usize,
    pub(crate) max_nesting_depth: usize,
}

impl TomlParseLimits {
    pub(crate) const fn new(max_bytes: usize, max_nesting_depth: usize) -> Self {
        Self {
            max_bytes,
            max_nesting_depth,
        }
    }
}

pub(crate) const PREFERENCE_FILE_TOML_LIMITS: TomlParseLimits = TomlParseLimits::new(64 * 1024, 16);
pub(crate) const MANAGED_KEY_MANIFEST_TOML_LIMITS: TomlParseLimits =
    TomlParseLimits::new(128 * 1024, 16);
#[cfg(any(test, feature = "fidostore", feature = "fidokey"))]
pub(crate) const FIDO2_TEXT_ENVELOPE_TOML_LIMITS: TomlParseLimits =
    TomlParseLimits::new(1024 * 1024, 16);

pub(crate) fn validate_toml_input(
    contents: &str,
    limits: TomlParseLimits,
    context: &str,
) -> Result<(), String> {
    if contents.len() > limits.max_bytes {
        return Err(format!(
            "{context} exceeds the supported TOML size limit of {} bytes.",
            limits.max_bytes
        ));
    }

    let depth = toml_max_nesting_depth(contents);
    if depth > limits.max_nesting_depth {
        return Err(format!(
            "{context} exceeds the supported TOML nesting depth of {}.",
            limits.max_nesting_depth
        ));
    }

    Ok(())
}

pub(crate) fn parse_toml_with_limits<T: DeserializeOwned>(
    contents: &str,
    limits: TomlParseLimits,
    context: &str,
) -> Result<T, String> {
    validate_toml_input(contents, limits, context)?;
    toml::from_str(contents).map_err(|err| err.to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScanMode {
    Normal,
    Comment,
    BasicString,
    MultiLineBasicString,
    LiteralString,
    MultiLineLiteralString,
}

fn toml_max_nesting_depth(contents: &str) -> usize {
    let bytes = contents.as_bytes();
    let mut index = 0usize;
    let mut inline_depth = 0usize;
    let mut max_depth = 0usize;
    let mut mode = ScanMode::Normal;
    let mut at_line_start = true;

    while index < bytes.len() {
        match mode {
            ScanMode::Normal => {
                if at_line_start {
                    while matches!(bytes.get(index), Some(b' ' | b'\t' | b'\r')) {
                        index += 1;
                    }
                    if let Some((line_end, header_depth)) = parse_table_header(bytes, index) {
                        max_depth = max_depth.max(header_depth);
                        index = line_end;
                        at_line_start = false;
                        continue;
                    }
                }

                match bytes[index] {
                    b'\n' => {
                        at_line_start = true;
                        index += 1;
                    }
                    b'#' => {
                        mode = ScanMode::Comment;
                        index += 1;
                    }
                    b'"' => {
                        if bytes[index..].starts_with(br#"""""#) {
                            mode = ScanMode::MultiLineBasicString;
                            index += 3;
                        } else {
                            mode = ScanMode::BasicString;
                            index += 1;
                        }
                        at_line_start = false;
                    }
                    b'\'' => {
                        if bytes[index..].starts_with(b"'''") {
                            mode = ScanMode::MultiLineLiteralString;
                            index += 3;
                        } else {
                            mode = ScanMode::LiteralString;
                            index += 1;
                        }
                        at_line_start = false;
                    }
                    b'[' | b'{' => {
                        inline_depth += 1;
                        max_depth = max_depth.max(inline_depth);
                        index += 1;
                        at_line_start = false;
                    }
                    b']' | b'}' => {
                        inline_depth = inline_depth.saturating_sub(1);
                        index += 1;
                        at_line_start = false;
                    }
                    b' ' | b'\t' | b'\r' => {
                        index += 1;
                    }
                    _ => {
                        at_line_start = false;
                        index += 1;
                    }
                }
            }
            ScanMode::Comment => {
                if bytes[index] == b'\n' {
                    mode = ScanMode::Normal;
                    at_line_start = true;
                }
                index += 1;
            }
            ScanMode::BasicString => {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else {
                    if bytes[index] == b'"' {
                        mode = ScanMode::Normal;
                    }
                    index += 1;
                }
            }
            ScanMode::MultiLineBasicString => {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if bytes[index..].starts_with(br#"""""#) {
                    mode = ScanMode::Normal;
                    index += 3;
                } else {
                    index += 1;
                }
            }
            ScanMode::LiteralString => {
                if bytes[index] == b'\'' {
                    mode = ScanMode::Normal;
                }
                index += 1;
            }
            ScanMode::MultiLineLiteralString => {
                if bytes[index..].starts_with(b"'''") {
                    mode = ScanMode::Normal;
                    index += 3;
                } else {
                    index += 1;
                }
            }
        }
    }

    max_depth
}

fn parse_table_header(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    let open_len = match bytes.get(start) {
        Some(b'[') if bytes.get(start + 1) == Some(&b'[') => 2usize,
        Some(b'[') => 1usize,
        _ => return None,
    };

    let mut index = start + open_len;
    let mut depth = 0usize;
    let mut segment_has_content = false;
    let mut mode = ScanMode::Normal;

    while index < bytes.len() {
        match mode {
            ScanMode::Normal => match bytes[index] {
                b'"' => {
                    mode = ScanMode::BasicString;
                    segment_has_content = true;
                    index += 1;
                }
                b'\'' => {
                    mode = ScanMode::LiteralString;
                    segment_has_content = true;
                    index += 1;
                }
                b'.' => {
                    if !segment_has_content {
                        return None;
                    }
                    depth += 1;
                    segment_has_content = false;
                    index += 1;
                }
                b']' => {
                    if open_len == 2 && bytes.get(index + 1) != Some(&b']') {
                        return None;
                    }
                    if !segment_has_content {
                        return None;
                    }
                    depth += 1;
                    let line_end = skip_header_trailing(bytes, index + open_len)?;
                    return Some((line_end, depth));
                }
                b'\n' => return None,
                b' ' | b'\t' | b'\r' => {
                    index += 1;
                }
                _ => {
                    segment_has_content = true;
                    index += 1;
                }
            },
            ScanMode::BasicString => {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else {
                    if bytes[index] == b'"' {
                        mode = ScanMode::Normal;
                    }
                    index += 1;
                }
            }
            ScanMode::LiteralString => {
                if bytes[index] == b'\'' {
                    mode = ScanMode::Normal;
                }
                index += 1;
            }
            ScanMode::Comment
            | ScanMode::MultiLineBasicString
            | ScanMode::MultiLineLiteralString => {
                return None;
            }
        }
    }

    None
}

fn skip_header_trailing(bytes: &[u8], mut index: usize) -> Option<usize> {
    while index < bytes.len() {
        match bytes[index] {
            b' ' | b'\t' | b'\r' => index += 1,
            b'#' => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                return Some(index);
            }
            b'\n' => return Some(index),
            _ => return None,
        }
    }

    Some(index)
}

#[cfg(test)]
mod tests {
    use super::{
        toml_max_nesting_depth, validate_toml_input, TomlParseLimits,
        FIDO2_TEXT_ENVELOPE_TOML_LIMITS,
    };

    #[test]
    fn toml_depth_counts_nested_arrays() {
        assert_eq!(toml_max_nesting_depth("value = [[[[1]]]]\n"), 4);
    }

    #[test]
    fn toml_depth_counts_table_header_segments() {
        assert_eq!(
            toml_max_nesting_depth("[keys.hardware.token]\nvalue = 1\n"),
            3
        );
    }

    #[test]
    fn toml_depth_ignores_brackets_in_strings_and_comments() {
        let input = "value = \"[[not nesting]]\"\n# [not.a.table]\narray = [1]\n";
        assert_eq!(toml_max_nesting_depth(input), 1);
    }

    #[test]
    fn toml_limits_reject_deep_inputs() {
        let input = format!("value = {}\n", "[".repeat(20) + "1" + &"]".repeat(20));
        let err = validate_toml_input(&input, TomlParseLimits::new(1024, 8), "test input")
            .expect_err("deep input should be rejected");

        assert!(err.contains("nesting depth"));
    }

    #[test]
    fn toml_limits_accept_current_fido2_envelope_shapes() {
        let input = "format = 1\nprotection = \"fido2\"\npayload = \"AAAA\"\n";
        validate_toml_input(input, FIDO2_TEXT_ENVELOPE_TOML_LIMITS, "FIDO2 envelope")
            .expect("expected shallow envelope to pass");
    }
}
