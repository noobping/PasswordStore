use super::super::management::rebuild_store_list;
use super::super::recipients::stores_with_preferred_first;
use super::{sync_store_recipients_page_header, StoreRecipientsPageState, StoreRecipientsRequest};
use crate::backend::{save_store_recipients, StoreRecipientsPrivateKeyRequirement};
use crate::logging::log_error;
use crate::preferences::Preferences;
#[cfg(keycord_flatpak)]
use crate::private_key::git::prompt_private_key_unlock_for_store_git_commit_if_needed;
use crate::support::actions::{activate_widget_action, register_window_action};
use crate::support::background::spawn_result_task;
use adw::gtk::ListBox;
use adw::{ApplicationWindow, Toast, ToastOverlay};
#[cfg(keycord_flatpak)]
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutosaveAction {
    Skip,
    Queue,
    Trigger,
}

fn autosave_action(
    has_request: bool,
    has_recipients: bool,
    is_dirty: bool,
    save_in_flight: bool,
) -> AutosaveAction {
    if !has_request || !has_recipients || !is_dirty {
        return AutosaveAction::Skip;
    }
    if save_in_flight {
        return AutosaveAction::Queue;
    }

    AutosaveAction::Trigger
}

fn should_reschedule_after_finish(
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

#[cfg(keycord_flatpak)]
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
    prompt_private_key_unlock_for_store_git_commit_if_needed(
        &state.platform.overlay,
        store_root,
        recipients,
        private_key_requirement,
        Rc::new(move || {
            save_store_recipients_async(
                &overlay_for_retry,
                &stores_list_for_retry,
                &state_for_retry,
                false,
            );
        }),
    )
}

#[cfg(keycord_standard_linux)]
fn maybe_prompt_store_recipients_git_unlock(
    _overlay: &ToastOverlay,
    _stores_list: &ListBox,
    _state: &StoreRecipientsPageState,
    _store_root: &str,
    _recipients: &[String],
    _private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    _allow_git_unlock_prompt: bool,
) -> bool {
    false
}

fn save_store_recipients_async(
    overlay: &ToastOverlay,
    stores_list: &ListBox,
    state: &StoreRecipientsPageState,
    _allow_git_unlock_prompt: bool,
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
        _allow_git_unlock_prompt,
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
    let request = request.clone();
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
                *state.saved_recipients.borrow_mut() = recipients.clone();
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
                    rebuild_store_list(&stores_list, &settings, &state.window, &overlay, &state);
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

pub(crate) fn queue_store_recipients_autosave(state: &StoreRecipientsPageState) {
    match autosave_action(
        state.current_request().is_some(),
        !state.recipients.borrow().is_empty(),
        state.recipients_are_dirty(),
        state.save_in_flight.get(),
    ) {
        AutosaveAction::Skip => {}
        AutosaveAction::Queue => state.save_queued.set(true),
        AutosaveAction::Trigger => {
            activate_widget_action(&state.window, "win.save-store-recipients")
        }
    }
}

pub(crate) fn register_store_recipients_save_action(
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
    use super::{autosave_action, should_reschedule_after_finish, AutosaveAction};

    #[test]
    fn autosave_skips_without_a_request_or_recipients_or_changes() {
        assert_eq!(
            autosave_action(false, true, true, false),
            AutosaveAction::Skip
        );
        assert_eq!(
            autosave_action(true, false, true, false),
            AutosaveAction::Skip
        );
        assert_eq!(
            autosave_action(true, true, false, false),
            AutosaveAction::Skip
        );
    }

    #[test]
    fn autosave_queues_while_a_save_is_in_flight() {
        assert_eq!(
            autosave_action(true, true, true, true),
            AutosaveAction::Queue
        );
    }

    #[test]
    fn autosave_triggers_only_when_it_can_save_now() {
        assert_eq!(
            autosave_action(true, true, true, false),
            AutosaveAction::Trigger
        );
    }

    #[test]
    fn finish_reschedules_for_queued_or_still_dirty_changes() {
        assert!(should_reschedule_after_finish(true, false, false));
        assert!(should_reschedule_after_finish(false, true, true));
        assert!(!should_reschedule_after_finish(false, false, true));
        assert!(!should_reschedule_after_finish(false, true, false));
    }
}
