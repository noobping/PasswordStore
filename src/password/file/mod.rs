mod compose;
mod parse;
mod row_ui;
mod types;
mod url;

pub(crate) use self::compose::{
    new_pass_file_contents_from_template, structured_pass_contents, sync_username_row,
    sync_username_row_from_parsed_lines,
};
#[cfg(test)]
pub(crate) use self::compose::structured_pass_contents_from_values;
pub(crate) use self::parse::{
    parse_structured_pass_lines, structured_otp_line,
};
#[cfg(test)]
pub(crate) use self::parse::structured_username_value;
pub(crate) use self::row_ui::{clear_box_children, rebuild_dynamic_fields_from_lines};
pub(crate) use self::types::{DynamicFieldRow, OtpFieldTemplate, StructuredPassLine};
#[cfg(test)]
pub(crate) use self::types::UsernameFieldTemplate;
#[cfg(test)]
pub(crate) use self::url::uri_to_open;
