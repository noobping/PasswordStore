use super::super::file::{
    parse_structured_pass_lines, rebuild_dynamic_fields_from_lines, structured_pass_contents,
    sync_username_row_from_parsed_lines,
};
use super::PasswordPageState;
use crate::password::model::OpenPassFile;
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
    let raw_visible = state
        .nav
        .visible_page()
        .as_ref()
        .map(|page| page == &state.raw_page)
        .unwrap_or(false);
    if raw_visible {
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
}
