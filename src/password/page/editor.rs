use super::super::file::{
    dynamic_field_row, parse_structured_pass_lines, rebuild_dynamic_fields_from_lines,
    structured_pass_contents, sync_username_row_from_parsed_lines, DynamicFieldTemplate,
    OtpFieldTemplate, StructuredPassLine,
};
use super::{refresh_apply_template_button, PasswordPageState};
use crate::password::model::OpenPassFile;
use crate::preferences::Preferences;
use crate::support::ui::visible_navigation_page_is;
use adw::glib;
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
    state.field_add_row.set_text("");
    refresh_apply_template_button(state);
    state
        .generator_controls
        .set_settings(&Preferences::new().password_generation_settings());
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
    refresh_apply_template_button(state);
}

pub(super) fn add_empty_dynamic_field(
    state: &PasswordPageState,
    title: &str,
    sensitive: Option<bool>,
) -> Result<(), &'static str> {
    let template = DynamicFieldTemplate::new(title, sensitive)?;
    let row = dynamic_field_row(&template, "", &state.overlay);
    state.dynamic_box.append(&row.widget());
    state.dynamic_box.set_visible(true);
    row.focus_editor();

    let mut templates = state.structured_templates.borrow_mut();
    let insert_at = dynamic_field_insert_index(&templates);
    templates.insert(insert_at, StructuredPassLine::Field(template));
    drop(templates);

    state.dynamic_rows.borrow_mut().push(row);
    state
        .text
        .buffer()
        .set_text(&structured_editor_contents(state));
    refresh_apply_template_button(state);
    Ok(())
}

pub(super) fn focus_field_add_row(state: &PasswordPageState) {
    if let Some(delegate) = state.field_add_row.delegate() {
        glib::idle_add_local_once(move || {
            delegate.grab_focus();
            delegate.select_region(0, -1);
        });
    } else {
        state.field_add_row.grab_focus();
    }
}

pub(super) fn focus_password_row(state: &PasswordPageState) {
    if let Some(delegate) = state.entry.delegate() {
        glib::idle_add_local_once(move || {
            delegate.grab_focus();
            delegate.select_region(0, -1);
        });
    } else {
        state.entry.grab_focus();
    }
}

fn sync_otp_add_button(state: &PasswordPageState) {
    state.otp_add_button.set_visible(!state.otp.has_otp());
}

fn dynamic_field_insert_index(templates: &[StructuredPassLine]) -> usize {
    templates
        .iter()
        .position(|line| matches!(line, StructuredPassLine::Preserved(_)))
        .unwrap_or(templates.len())
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
    use super::{dynamic_field_insert_index, ensure_otp_template};
    use crate::password::file::{DynamicFieldTemplate, OtpFieldTemplate, StructuredPassLine};

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

    #[test]
    fn new_fields_are_inserted_before_preserved_lines() {
        let templates = vec![
            StructuredPassLine::Field(
                DynamicFieldTemplate::new("url", Some(false)).expect("url field"),
            ),
            StructuredPassLine::Preserved("notes".to_string()),
        ];

        assert_eq!(dynamic_field_insert_index(&templates), 1);
    }
}
