use super::super::management::rebuild_store_list;
use super::super::recipients::stores_with_preferred_first;
use super::{sync_store_recipients_page_header, StoreRecipientsPageState, StoreRecipientsRequest};
use crate::backend::save_store_recipients;
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::support::actions::register_window_action;
use crate::support::background::spawn_result_task;
use crate::window::messages::with_logs_hint;
use adw::gtk::ListBox;
use adw::{ApplicationWindow, Toast, ToastOverlay};

fn can_autosave_store_recipients(state: &StoreRecipientsPageState) -> bool {
    state.current_request().is_some()
        && !state.recipients.borrow().is_empty()
        && state.recipients_are_dirty()
}

fn finish_store_recipients_save(state: &StoreRecipientsPageState, include_dirty: bool) {
    state.save_in_flight.set(false);
    if state.save_queued.get() || (include_dirty && state.recipients_are_dirty()) {
        state.save_queued.set(false);
        queue_store_recipients_autosave(state);
    }
}

fn save_store_recipients_async(
    overlay: &ToastOverlay,
    stores_list: &ListBox,
    state: &StoreRecipientsPageState,
) {
    let Some(request) = state.current_request() else {
        return;
    };

    let recipients = state.recipients.borrow().clone();
    if recipients.is_empty() {
        return;
    }
    if !state.recipients_are_dirty() {
        state.save_queued.set(false);
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
        move || save_store_recipients(&store_for_thread, &recipients_for_save),
        move |result| match result {
            Ok(()) => {
                let settings = Preferences::new();
                *state.saved_recipients.borrow_mut() = recipients.clone();
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
            Err(message) => {
                log_error(format!(
                    "Failed to save store recipients for '{}': {message}",
                    request.store
                ));
                let message = with_logs_hint(request.mode.save_failure_message());
                finish_store_recipients_save(&state, false);
                overlay.add_toast(Toast::new(&message));
            }
        },
        move || {
            let message = with_logs_hint(mode_for_disconnect.save_failure_message());
            finish_store_recipients_save(&state_for_disconnect, false);
            overlay_for_disconnect.add_toast(Toast::new(&message));
        },
    );
}

pub(crate) fn queue_store_recipients_autosave(state: &StoreRecipientsPageState) {
    if !can_autosave_store_recipients(state) {
        return;
    }
    if state.save_in_flight.get() {
        state.save_queued.set(true);
        return;
    }

    let _ =
        adw::prelude::WidgetExt::activate_action(&state.window, "win.save-store-recipients", None);
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
        save_store_recipients_async(&overlay, &stores_list, &state);
    });
}
