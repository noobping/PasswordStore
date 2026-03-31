use super::StoreRecipientsPageState;
use crate::backend::StoreRecipientsError;
use crate::fido2_recipient::is_fido2_recipient_string;
use crate::i18n::gettext;
use crate::support::actions::activate_widget_action;
use crate::support::ui::wrapped_dialog_body;
use adw::gtk::{Align, Box as GtkBox, Button, Label, Orientation};
use adw::prelude::*;
use adw::Dialog;
use std::{cell::Cell, rc::Rc};

const ADDITIONAL_FIDO2_SAVE_GUIDE_TITLE: &str = "Add Security Key";
const ADDITIONAL_FIDO2_SAVE_GUIDE_NEXT_LABEL: &str = "Next";
const ADDITIONAL_FIDO2_SAVE_GUIDE_STEP_COUNT: usize = 3;
const ADDITIONAL_FIDO2_SAVE_GUIDE_STEP_TITLES: [&str; ADDITIONAL_FIDO2_SAVE_GUIDE_STEP_COUNT] =
    ["Step 1 of 3", "Step 2 of 3", "Step 3 of 3"];
const ADDITIONAL_FIDO2_SAVE_GUIDE_STEP_BODIES: [&str; ADDITIONAL_FIDO2_SAVE_GUIDE_STEP_COUNT] = [
    "Unplug the new security key.",
    "Plug in a security key that already works with this store. Then click Next.",
    "Touch the FIDO2 security key when it starts blinking.",
];

fn step_description(step_index: usize) -> String {
    format!(
        "{}\n\n{}",
        gettext(ADDITIONAL_FIDO2_SAVE_GUIDE_STEP_TITLES[step_index]),
        gettext(ADDITIONAL_FIDO2_SAVE_GUIDE_STEP_BODIES[step_index]),
    )
}

pub(super) fn saved_fido2_recipient_exists(saved_recipients: &[String]) -> bool {
    saved_recipients
        .iter()
        .any(|recipient| is_fido2_recipient_string(recipient))
}

fn fido2_recipient_count(recipients: &[String]) -> usize {
    recipients
        .iter()
        .filter(|recipient| is_fido2_recipient_string(recipient))
        .count()
}

pub(super) fn needs_additional_fido2_save_guidance(
    saved_recipients: &[String],
    current_recipients: &[String],
    error: &StoreRecipientsError,
) -> bool {
    matches!(error, StoreRecipientsError::IncompatiblePrivateKey(_))
        && fido2_recipient_count(saved_recipients) > 0
        && fido2_recipient_count(current_recipients) > fido2_recipient_count(saved_recipients)
}

pub(super) fn close_additional_fido2_save_guidance_dialog(state: &StoreRecipientsPageState) {
    let dialog = state.additional_fido2_save_guide_dialog.borrow_mut().take();
    if let Some(dialog) = dialog {
        dialog.force_close();
    }
}

pub(super) fn present_additional_fido2_save_guidance_dialog(state: &StoreRecipientsPageState) {
    close_additional_fido2_save_guidance_dialog(state);

    let body = Label::new(Some(&step_description(0)));
    body.set_wrap(true);
    body.set_xalign(0.0);
    body.set_halign(Align::Fill);

    let close_button = Button::with_label(&gettext("Not Now"));
    close_button.add_css_class("flat");

    let next_button = Button::with_label(&gettext(ADDITIONAL_FIDO2_SAVE_GUIDE_NEXT_LABEL));
    next_button.add_css_class("suggested-action");

    let button_row = GtkBox::new(Orientation::Horizontal, 12);
    button_row.set_halign(Align::End);
    button_row.append(&close_button);
    button_row.append(&next_button);

    let content = GtkBox::new(Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.append(&body);
    content.append(&button_row);

    let dialog = Dialog::builder()
        .title(gettext(ADDITIONAL_FIDO2_SAVE_GUIDE_TITLE))
        .content_width(640)
        .content_height(320)
        .follows_content_size(true)
        .child(&wrapped_dialog_body(&content))
        .build();

    let window = state.window.clone();
    let dialog_for_close = dialog.clone();
    close_button.connect_clicked(move |_| {
        dialog_for_close.close();
    });

    *state.additional_fido2_save_guide_dialog.borrow_mut() = Some(dialog.clone());

    let tracked_dialog = state.additional_fido2_save_guide_dialog.clone();
    let dialog_for_closed = dialog.clone();
    dialog.connect_closed(move |_| {
        let should_clear = tracked_dialog
            .borrow()
            .as_ref()
            .is_some_and(|tracked| tracked == &dialog_for_closed);
        if should_clear {
            tracked_dialog.borrow_mut().take();
        }
    });

    let step_index = Rc::new(Cell::new(0usize));
    let step_index_for_next = step_index.clone();
    let body_for_next = body.clone();
    let state = state.clone();
    next_button.connect_clicked(move |button| {
        let current = step_index_for_next.get();
        if current == 1 {
            activate_widget_action(&state.window, "win.save-store-recipients");
        }

        let next = current + 1;
        if next >= ADDITIONAL_FIDO2_SAVE_GUIDE_STEP_COUNT {
            return;
        }

        step_index_for_next.set(next);
        body_for_next.set_label(&step_description(next));
        button.set_visible(next + 1 < ADDITIONAL_FIDO2_SAVE_GUIDE_STEP_COUNT);
    });
    dialog.present(Some(&window));
}

#[cfg(test)]
mod tests {
    use super::{needs_additional_fido2_save_guidance, saved_fido2_recipient_exists};
    use crate::backend::StoreRecipientsError;

    #[test]
    fn saved_fido2_recipient_detection_matches_store_type() {
        assert!(!saved_fido2_recipient_exists(&[]));
        assert!(!saved_fido2_recipient_exists(&[
            "alice@example.com".to_string()
        ]));
        assert!(saved_fido2_recipient_exists(&[
            "keycord-fido2-recipient-v1=0123456789abcdef0123456789abcdef01234567:4669727374:63726564"
                .to_string(),
        ]));
    }

    #[test]
    fn additional_fido2_save_guidance_is_only_used_for_extra_fido2_recipients() {
        let saved = vec![
            "keycord-fido2-recipient-v1=0123456789abcdef0123456789abcdef01234567:4669727374:637265642d31"
                .to_string(),
        ];
        let current = vec![
            saved[0].clone(),
            "keycord-fido2-recipient-v1=89abcdef0123456789abcdef0123456789abcdef:5365636f6e64:637265642d32"
                .to_string(),
        ];

        assert!(needs_additional_fido2_save_guidance(
            &saved,
            &current,
            &StoreRecipientsError::IncompatiblePrivateKey(
                "The available private keys cannot decrypt this item.".to_string()
            ),
        ));
        assert!(!needs_additional_fido2_save_guidance(
            &saved,
            &saved,
            &StoreRecipientsError::IncompatiblePrivateKey(
                "The available private keys cannot decrypt this item.".to_string()
            ),
        ));
    }
}
