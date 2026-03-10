use super::super::file::{
    parse_structured_pass_lines, rebuild_dynamic_fields_from_lines, structured_pass_contents,
    sync_username_row_from_parsed_lines, OtpFieldTemplate, StructuredPassLine,
};
use super::PasswordPageState;
use crate::password::model::OpenPassFile;
use crate::support::ui::visible_navigation_page_is;
use adw::prelude::*;

pub(super) fn structured_editor_contents(state: &PasswordPageState) -> String {
    structured_pass_contents(
        &state.entry.text(),
        &state.username.text(),
        state.otp.current_url().as_deref(),
        &state.structured_templates.borrow(),
        &state.dynamic_rows.borrow(),
    )
}

pub(super) fn current_editor_contents(state: &PasswordPageState) -> String {
    if visible_navigation_page_is(&state.nav, &state.raw_page) {
        let buffer = state.text.buffer();
        let (start, end) = buffer.bounds();
        buffer.text(&start, &end, false).to_string()
    } else {
        structured_editor_contents(state)
    }
}

pub(super) fn sync_editor_contents(
    state: &PasswordPageState,
    contents: &str,
    pass_file: Option<&OpenPassFile>,
) {
    let (password, structured_lines) = parse_structured_pass_lines(contents);
    state.entry.set_text(&password);
    state.text.buffer().set_text(contents);
    rebuild_dynamic_fields_from_lines(
        &state.dynamic_box,
        &state.overlay,
        &state.structured_templates,
        &state.dynamic_rows,
        &structured_lines,
    );
    sync_username_row_from_parsed_lines(&state.username, pass_file, &structured_lines);
    state.otp.sync_from_parsed_lines(&structured_lines, true);
    sync_otp_add_button(state);
}

pub(super) fn add_empty_otp_secret(state: &PasswordPageState) {
    if !ensure_otp_template(&mut state.structured_templates.borrow_mut()) {
        sync_otp_add_button(state);
        return;
    }

    state.otp.add_empty_secret();
    sync_otp_add_button(state);
    state
        .text
        .buffer()
        .set_text(&structured_editor_contents(state));
}

fn sync_otp_add_button(state: &PasswordPageState) {
    state.otp_add_button.set_visible(!state.otp.has_otp());
}

fn ensure_otp_template(templates: &mut Vec<StructuredPassLine>) -> bool {
    if templates
        .iter()
        .any(|line| matches!(line, StructuredPassLine::Otp(_)))
    {
        return false;
    }

    let insert_at = templates
        .iter()
        .position(|line| matches!(line, StructuredPassLine::Preserved(_)))
        .unwrap_or(templates.len());
    templates.insert(
        insert_at,
        StructuredPassLine::Otp(OtpFieldTemplate::BareUrl),
    );
    true
}

#[cfg(test)]
mod tests {
    use super::ensure_otp_template;
    use crate::password::file::{OtpFieldTemplate, StructuredPassLine};

    #[test]
    fn otp_template_is_inserted_before_preserved_lines() {
        let mut templates = vec![
            StructuredPassLine::Preserved("Notes".to_string()),
            StructuredPassLine::Preserved("More notes".to_string()),
        ];

        assert!(ensure_otp_template(&mut templates));
        assert!(matches!(
            templates[0],
            StructuredPassLine::Otp(OtpFieldTemplate::BareUrl)
        ));
    }

    #[test]
    fn existing_otp_template_is_not_duplicated() {
        let mut templates = vec![StructuredPassLine::Otp(OtpFieldTemplate::BareUrl)];

        assert!(!ensure_otp_template(&mut templates));
        assert_eq!(templates.len(), 1);
    }
}
