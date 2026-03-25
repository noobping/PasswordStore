mod compose;
mod parse;
mod row_ui;
mod types;
mod url;

#[cfg(test)]
pub use self::compose::structured_pass_contents_from_values;
pub use self::compose::{
    apply_pass_file_template_contents, clean_pass_file_contents,
    new_pass_file_contents_from_template, pass_file_has_missing_template_fields,
    structured_pass_contents, sync_username_row, sync_username_row_from_parsed_lines,
};
#[cfg(test)]
pub use self::parse::structured_username_value;
pub use self::parse::{
    canonical_search_field_key, pass_file_has_otp, searchable_pass_fields, SearchablePassField,
};
pub use self::parse::{parse_structured_pass_lines, structured_otp_line};
pub use self::row_ui::{clear_box_children, dynamic_field_row, rebuild_dynamic_fields_from_lines};
#[cfg(test)]
pub use self::types::UsernameFieldTemplate;
pub use self::types::{
    DynamicFieldRow, DynamicFieldTemplate, OtpFieldTemplate, StructuredPassLine,
};
#[cfg(test)]
pub use self::url::uri_to_open;
