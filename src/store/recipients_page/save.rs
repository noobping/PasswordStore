use super::super::management::rebuild_stores_list;
use super::super::recipients::stores_with_preferred_first;
use super::{sync_store_recipients_page_header, StoreRecipientsPageState, StoreRecipientsRequest};
use crate::backend::{save_store_recipients, StoreRecipientsPrivateKeyRequirement};
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::private_key::git::prompt_private_key_unlock_for_store_git_commit_if_needed;
use crate::support::actions::{activate_widget_action, register_window_action};
use crate::support::background::spawn_result_task;
use adw::gtk::ListBox;
use adw::{ApplicationWindow, Toast, ToastOverlay};
use std::rc::Rc;

const fn should_reschedule_after_finish(
    save_queued: bool,
    include_dirty: bool,
    recipients_dirty: bool,
) -> bool {
    save_queued || (include_dirty && recipients_dirty)
}

fn finish_store_recipients_save(state: &StoreRecipientsPageState, include_dirty: bool) {
    state.save_in_flight.set(false);
    if should_reschedule_after_finish(
        state.save_queued.get(),
        include_dirty,
        state.recipients_are_dirty(),
    ) {
        state.save_queued.set(false);
        queue_store_recipients_autosave(state);
    }
}

fn maybe_prompt_store_recipients_git_unlock(
    overlay: &ToastOverlay,
    stores_list: &ListBox,
    state: &StoreRecipientsPageState,
    store_root: &str,
    recipients: &[String],
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    allow_git_unlock_prompt: bool,
) -> bool {
    if !allow_git_unlock_prompt || !Preferences::new().uses_integrated_backend() {
        return false;
    }

    let overlay_for_retry = overlay.clone();
    let stores_list_for_retry = stores_list.clone();
    let state_for_retry = state.clone();
    let after_unlock: Rc<dyn Fn()> = Rc::new(move || {
        save_store_recipients_async(
            &overlay_for_retry,
            &stores_list_for_retry,
            &state_for_retry,
            false,
        );
    });
    prompt_private_key_unlock_for_store_git_commit_if_needed(
        &state.platform.overlay,
        store_root,
        recipients,
        private_key_requirement,
        &after_unlock,
    )
}

fn save_store_recipients_async(
    overlay: &ToastOverlay,
    stores_list: &ListBox,
    state: &StoreRecipientsPageState,
    allow_git_unlock_prompt: bool,
) {
    let Some(request) = state.current_request() else {
        return;
    };

    let recipients = state.recipients.borrow().clone();
    let private_key_requirement = state.private_key_requirement.get();
    if recipients.is_empty() {
        return;
    }
    if !state.recipients_are_dirty() {
        state.save_queued.set(false);
        return;
    }
    if maybe_prompt_store_recipients_git_unlock(
        overlay,
        stores_list,
        state,
        &request.store,
        &recipients,
        private_key_requirement,
        allow_git_unlock_prompt,
    ) {
        return;
    }
    if state.save_in_flight.replace(true) {
        state.save_queued.set(true);
        return;
    }
    state.save_queued.set(false);

    let store_for_thread = request.store.clone();
    let recipients_for_save = recipients.clone();
    let overlay = overlay.clone();
    let stores_list = stores_list.clone();
    let state = state.clone();
    let overlay_for_disconnect = overlay.clone();
    let state_for_disconnect = state.clone();
    let mode_for_disconnect = request.mode;
    spawn_result_task(
        move || {
            save_store_recipients(
                &store_for_thread,
                &recipients_for_save,
                private_key_requirement,
            )
        },
        move |result| match result {
            Ok(()) => {
                let settings = Preferences::new();
                state.saved_recipients.borrow_mut().clone_from(&recipients);
                state
                    .saved_private_key_requirement
                    .set(private_key_requirement);
                let should_rebuild_store_list = if request.mode.creates_store() {
                    let stores = stores_with_preferred_first(&settings.stores(), &request.store);
                    if let Err(err) = settings.set_stores(stores) {
                        log_error(format!("Failed to save stores: {err}"));
                        overlay.add_toast(Toast::new("Store created, but it wasn't added."));
                        false
                    } else {
                        *state.request.borrow_mut() =
                            Some(StoreRecipientsRequest::edit(request.store.clone()));
                        sync_store_recipients_page_header(&state);
                        true
                    }
                } else {
                    true
                };

                if should_rebuild_store_list {
                    rebuild_stores_list(&stores_list, &settings, &state);
                }
                finish_store_recipients_save(&state, true);
            }
            Err(err) => {
                log_error(format!(
                    "Failed to save store recipients for '{}': {err}",
                    request.store
                ));
                finish_store_recipients_save(&state, false);
                overlay.add_toast(Toast::new(
                    err.toast_message(request.mode.save_failure_message()),
                ));
            }
        },
        move || {
            finish_store_recipients_save(&state_for_disconnect, false);
            overlay_for_disconnect
                .add_toast(Toast::new(mode_for_disconnect.save_failure_message()));
        },
    );
}

pub fn queue_store_recipients_autosave(state: &StoreRecipientsPageState) {
    if state.current_request().is_none()
        || state.recipients.borrow().is_empty()
        || !state.recipients_are_dirty()
    {
        return;
    }

    if state.save_in_flight.get() {
        state.save_queued.set(true);
    } else {
        activate_widget_action(&state.window, "win.save-store-recipients");
    }
}

pub fn register_store_recipients_save_action(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    stores_list: &ListBox,
    state: &StoreRecipientsPageState,
) {
    let overlay = overlay.clone();
    let stores_list = stores_list.clone();
    let state = state.clone();
    register_window_action(window, "save-store-recipients", move || {
        save_store_recipients_async(&overlay, &stores_list, &state, true);
    });
}

#[cfg(test)]
mod tests {
    use super::should_reschedule_after_finish;

    #[test]
    fn finish_reschedules_for_queued_or_still_dirty_changes() {
        assert!(should_reschedule_after_finish(true, false, false));
        assert!(should_reschedule_after_finish(false, true, true));
        assert!(!should_reschedule_after_finish(false, false, true));
        assert!(!should_reschedule_after_finish(false, true, false));
    }
}
