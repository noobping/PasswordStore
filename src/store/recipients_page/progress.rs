use super::StoreRecipientsPageState;
use crate::backend::{StoreRecipientsSaveProgress, StoreRecipientsSaveStage};
use crate::fido2_recipient::is_fido2_recipient_string;
use crate::i18n::gettext;
use crate::support::ui::wrapped_dialog_body;
use adw::gtk::{Align, Box as GtkBox, Label, Orientation, ProgressBar};
use adw::prelude::*;
use adw::Dialog;

const REMOVE_FIDO2_SAVE_PROGRESS_TITLE: &str = "Removing Security Key";
const ADD_FIDO2_SAVE_PROGRESS_TITLE: &str = "Adding Security Key";
const UPDATE_FIDO2_SAVE_PROGRESS_TITLE: &str = "Updating Security Keys";

#[derive(Clone)]
pub(crate) struct StoreRecipientsSaveProgressDialogHandle {
    dialog: Dialog,
    status: Label,
    detail: Label,
    progress: ProgressBar,
}

impl StoreRecipientsSaveProgressDialogHandle {
    fn new(dialog: &Dialog, status: &Label, detail: &Label, progress: &ProgressBar) -> Self {
        Self {
            dialog: dialog.clone(),
            status: status.clone(),
            detail: detail.clone(),
            progress: progress.clone(),
        }
    }

    fn update(&self, progress: &StoreRecipientsSaveProgress) {
        self.status.set_label(&progress_status(progress));
        self.detail.set_label(&progress_detail(progress));
        self.progress.set_fraction(progress_fraction(progress));
    }

    fn force_close(&self) {
        self.dialog.force_close();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fido2SaveReason {
    Add,
    Remove,
    Update,
}

fn fido2_recipient_count(recipients: &[String]) -> usize {
    recipients
        .iter()
        .filter(|recipient| is_fido2_recipient_string(recipient))
        .count()
}

fn fido2_save_reason(
    saved_recipients: &[String],
    current_recipients: &[String],
) -> Fido2SaveReason {
    let saved_count = fido2_recipient_count(saved_recipients);
    let current_count = fido2_recipient_count(current_recipients);

    if current_count > saved_count {
        Fido2SaveReason::Add
    } else if current_count < saved_count {
        Fido2SaveReason::Remove
    } else {
        Fido2SaveReason::Update
    }
}

fn dialog_title(reason: Fido2SaveReason) -> &'static str {
    match reason {
        Fido2SaveReason::Add => ADD_FIDO2_SAVE_PROGRESS_TITLE,
        Fido2SaveReason::Remove => REMOVE_FIDO2_SAVE_PROGRESS_TITLE,
        Fido2SaveReason::Update => UPDATE_FIDO2_SAVE_PROGRESS_TITLE,
    }
}

fn dialog_description(reason: Fido2SaveReason) -> &'static str {
    match reason {
        Fido2SaveReason::Add => {
            "Keycord is updating every saved item so the new security key can unlock this store.\n\nKeep the security keys for this store connected. Touch a key when it starts blinking."
        }
        Fido2SaveReason::Remove => {
            "Keycord is updating every saved item so the removed security key no longer unlocks this store.\n\nKeep the security keys you still want to use connected. Touch a key when it starts blinking."
        }
        Fido2SaveReason::Update => {
            "Keycord is updating every saved item so the selected security keys stay in sync.\n\nKeep the security keys for this store connected. Touch a key when it starts blinking."
        }
    }
}

fn progress_status(progress: &StoreRecipientsSaveProgress) -> String {
    let template = match progress.stage {
        StoreRecipientsSaveStage::ReadingExistingItems => "Opening item {current} of {total}",
        StoreRecipientsSaveStage::WritingUpdatedItems => "Saving item {current} of {total}",
    };

    gettext(template)
        .replace("{current}", &progress.current_item.to_string())
        .replace("{total}", &progress.total_items.to_string())
}

fn progress_detail(progress: &StoreRecipientsSaveProgress) -> String {
    if progress.total_touches > 0 {
        return gettext("Touch {current} of {total} for this item when it starts blinking.")
            .replace("{current}", &progress.current_touch.to_string())
            .replace("{total}", &progress.total_touches.to_string());
    }

    gettext(match progress.stage {
        StoreRecipientsSaveStage::ReadingExistingItems => "Checking which keys can open this item.",
        StoreRecipientsSaveStage::WritingUpdatedItems => {
            "Updating this item with the selected keys."
        }
    })
}

fn progress_fraction(progress: &StoreRecipientsSaveProgress) -> f64 {
    if progress.total_items == 0 {
        return 0.0;
    }

    let stage_offset = match progress.stage {
        StoreRecipientsSaveStage::ReadingExistingItems => 0usize,
        StoreRecipientsSaveStage::WritingUpdatedItems => progress.total_items,
    };
    let completed_items = stage_offset + progress.current_item.saturating_sub(1);
    let item_fraction = if progress.total_touches > 0 && progress.current_touch > 0 {
        f64::from(progress.current_touch as u32) / f64::from(progress.total_touches as u32)
    } else {
        0.0
    };

    ((completed_items as f64) + item_fraction) / ((progress.total_items * 2) as f64)
}

pub(super) fn should_present_fido2_save_progress_dialog(
    saved_recipients: &[String],
    current_recipients: &[String],
) -> bool {
    saved_recipients != current_recipients
        && (fido2_recipient_count(saved_recipients) > 0
            || fido2_recipient_count(current_recipients) > 0)
}

pub(super) fn close_fido2_save_progress_dialog(state: &StoreRecipientsPageState) {
    let dialog = state.fido2_save_progress_dialog.borrow_mut().take();
    if let Some(dialog) = dialog {
        dialog.force_close();
    }
}

pub(super) fn present_fido2_save_progress_dialog(
    state: &StoreRecipientsPageState,
    saved_recipients: &[String],
    current_recipients: &[String],
) {
    close_fido2_save_progress_dialog(state);

    let reason = fido2_save_reason(saved_recipients, current_recipients);
    let intro = Label::new(Some(&gettext(dialog_description(reason))));
    intro.set_wrap(true);
    intro.set_xalign(0.0);
    intro.set_halign(Align::Fill);

    let status = Label::new(Some(&gettext("Preparing saved items")));
    status.set_xalign(0.0);
    status.set_halign(Align::Fill);
    status.add_css_class("heading");

    let detail = Label::new(Some(&gettext(
        "This can take a while because every saved item has to be opened and saved again.",
    )));
    detail.set_wrap(true);
    detail.set_xalign(0.0);
    detail.set_halign(Align::Fill);
    detail.add_css_class("dim-label");

    let progress = ProgressBar::new();
    progress.set_fraction(0.0);
    progress.set_show_text(false);

    let content = GtkBox::new(Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.append(&intro);
    content.append(&status);
    content.append(&detail);
    content.append(&progress);

    let dialog = Dialog::builder()
        .title(gettext(dialog_title(reason)))
        .content_width(640)
        .content_height(280)
        .follows_content_size(true)
        .child(&wrapped_dialog_body(&content))
        .build();
    dialog.set_can_close(false);

    let handle = StoreRecipientsSaveProgressDialogHandle::new(&dialog, &status, &detail, &progress);
    *state.fido2_save_progress_dialog.borrow_mut() = Some(handle);

    let tracked_dialog = state.fido2_save_progress_dialog.clone();
    let dialog_for_closed = dialog.clone();
    dialog.connect_closed(move |_| {
        let should_clear = tracked_dialog
            .borrow()
            .as_ref()
            .is_some_and(|tracked| tracked.dialog == dialog_for_closed);
        if should_clear {
            tracked_dialog.borrow_mut().take();
        }
    });

    dialog.present(Some(&state.window));
}

pub(super) fn update_fido2_save_progress_dialog(
    state: &StoreRecipientsPageState,
    progress: &StoreRecipientsSaveProgress,
) {
    if let Some(dialog) = state.fido2_save_progress_dialog.borrow().as_ref() {
        dialog.update(progress);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        fido2_save_reason, progress_detail, progress_fraction, progress_status,
        should_present_fido2_save_progress_dialog, Fido2SaveReason,
    };
    use crate::backend::{StoreRecipientsSaveProgress, StoreRecipientsSaveStage};

    #[test]
    fn save_progress_dialog_is_only_used_when_fido2_recipients_change() {
        let standard = vec!["alice@example.com".to_string()];
        let first_fido2 = vec![
            "keycord-fido2-recipient-v1=0123456789abcdef0123456789abcdef01234567:4669727374:63726564"
                .to_string(),
        ];
        let second_fido2 = vec![
            first_fido2[0].clone(),
            "keycord-fido2-recipient-v1=89abcdef0123456789abcdef0123456789abcdef:5365636f6e64:637265642d32"
                .to_string(),
        ];

        assert!(!should_present_fido2_save_progress_dialog(
            &standard, &standard
        ));
        assert!(should_present_fido2_save_progress_dialog(
            &first_fido2,
            &second_fido2
        ));
        assert!(should_present_fido2_save_progress_dialog(
            &second_fido2,
            &first_fido2
        ));
    }

    #[test]
    fn save_progress_reason_matches_add_remove_and_update() {
        let first_fido2 = vec![
            "keycord-fido2-recipient-v1=0123456789abcdef0123456789abcdef01234567:4669727374:63726564"
                .to_string(),
        ];
        let second_fido2 = vec![
            first_fido2[0].clone(),
            "keycord-fido2-recipient-v1=89abcdef0123456789abcdef0123456789abcdef:5365636f6e64:637265642d32"
                .to_string(),
        ];

        assert_eq!(
            fido2_save_reason(&first_fido2, &second_fido2),
            Fido2SaveReason::Add
        );
        assert_eq!(
            fido2_save_reason(&second_fido2, &first_fido2),
            Fido2SaveReason::Remove
        );
        assert_eq!(
            fido2_save_reason(&first_fido2, &first_fido2),
            Fido2SaveReason::Update
        );
    }

    #[test]
    fn save_progress_copy_and_fraction_follow_stage_and_touches() {
        let progress = StoreRecipientsSaveProgress {
            stage: StoreRecipientsSaveStage::WritingUpdatedItems,
            current_item: 2,
            total_items: 4,
            current_touch: 1,
            total_touches: 2,
        };

        assert_eq!(progress_status(&progress), "Saving item 2 of 4");
        assert_eq!(
            progress_detail(&progress),
            "Touch 1 of 2 for this item when it starts blinking."
        );
        assert!(progress_fraction(&progress) > 0.5);
        assert!(progress_fraction(&progress) < 1.0);
    }
}
