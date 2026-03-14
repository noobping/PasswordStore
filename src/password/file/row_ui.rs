use super::types::{is_url_field_key, DynamicFieldRow, DynamicFieldTemplate, StructuredPassLine};
#[cfg(keycord_linux)]
use super::url::add_open_url_suffix;
use crate::clipboard::add_copy_suffix;
use adw::gtk::{Box as GtkBox, Widget};
use adw::{prelude::*, EntryRow, PasswordEntryRow, ToastOverlay};
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) fn clear_box_children(box_widget: &GtkBox) {
    while let Some(child) = box_widget.first_child() {
        box_widget.remove(&child);
    }
}

pub(crate) fn rebuild_dynamic_fields_from_lines(
    box_widget: &GtkBox,
    overlay: &ToastOverlay,
    templates_state: &Rc<RefCell<Vec<StructuredPassLine>>>,
    rows_state: &Rc<RefCell<Vec<DynamicFieldRow>>>,
    structured_lines: &[(StructuredPassLine, Option<String>)],
) {
    clear_box_children(box_widget);
    templates_state.borrow_mut().clear();
    rows_state.borrow_mut().clear();

    let mut rows = Vec::new();
    let mut templates = Vec::new();

    for (line, value) in structured_lines.iter().cloned() {
        match line {
            StructuredPassLine::Field(template) => {
                let row = build_dynamic_field_row(
                    &template,
                    value.as_deref().unwrap_or_default(),
                    overlay,
                );
                box_widget.append(&row.widget());
                rows.push(row);
                templates.push(StructuredPassLine::Field(template));
            }
            StructuredPassLine::Username(template) => {
                templates.push(StructuredPassLine::Username(template));
            }
            StructuredPassLine::Otp(template) => {
                templates.push(StructuredPassLine::Otp(template));
            }
            StructuredPassLine::Preserved(line) => {
                templates.push(StructuredPassLine::Preserved(line));
            }
        }
    }

    box_widget.set_visible(!rows.is_empty());
    *templates_state.borrow_mut() = templates;
    *rows_state.borrow_mut() = rows;
}

fn build_dynamic_field_row(
    template: &DynamicFieldTemplate,
    value: &str,
    overlay: &ToastOverlay,
) -> DynamicFieldRow {
    if template.sensitive {
        let row = PasswordEntryRow::new();
        row.set_title(&template.title);
        row.set_text(value);
        apply_field_row_style(&row);
        let row_clone = row.clone();
        add_copy_suffix(&row, move || row_clone.text().to_string(), overlay);
        DynamicFieldRow::Secret(row)
    } else {
        let row = EntryRow::new();
        row.set_title(&template.title);
        row.set_text(value);
        apply_field_row_style(&row);
        let row_clone = row.clone();
        add_copy_suffix(&row, move || row_clone.text().to_string(), overlay);
        #[cfg(keycord_linux)]
        if is_url_field_key(&template.raw_key) {
            let row_clone = row.clone();
            add_open_url_suffix(&row, move || row_clone.text().to_string(), overlay);
        }
        DynamicFieldRow::Plain(row)
    }
}

fn apply_field_row_style<W: IsA<Widget>>(widget: &W) {
    widget.set_margin_start(15);
    widget.set_margin_end(15);
    widget.set_margin_bottom(6);
}
