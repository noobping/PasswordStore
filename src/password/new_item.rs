use crate::preferences::Preferences;
use crate::support::actions::register_window_action;
use crate::support::ui::toggle_popover;
use adw::gtk::{DropDown, Popover, StringList, INVALID_LIST_POSITION};
use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct NewPasswordPopoverState {
    pub(crate) popover: Popover,
    pub(crate) store_row: ActionRow,
    pub(crate) store_dropdown: DropDown,
    pub(crate) store_roots: Rc<RefCell<Vec<String>>>,
}

fn available_store_roots() -> Vec<String> {
    Preferences::new().store_roots()
}

fn shortened_store_labels(stores: &[String]) -> Vec<String> {
    let path_segments = stores
        .iter()
        .map(|store| {
            Path::new(store)
                .components()
                .filter_map(|component| component.as_os_str().to_str())
                .filter(|segment| !segment.is_empty() && *segment != "/")
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let max_depth = path_segments.iter().map(Vec::len).max().unwrap_or_default();
    for depth in 1..=max_depth {
        let labels = path_segments
            .iter()
            .zip(stores)
            .map(|(segments, full_path)| {
                if segments.is_empty() {
                    return full_path.clone();
                }

                let start = segments.len().saturating_sub(depth);
                let suffix = segments[start..].join("/");
                if start == 0 {
                    suffix
                } else {
                    format!(".../{suffix}")
                }
            })
            .collect::<Vec<_>>();

        let mut unique = labels.clone();
        unique.sort();
        unique.dedup();
        if unique.len() == labels.len() {
            return labels;
        }
    }

    stores.to_vec()
}

fn resolve_selected_store(stores: &[String], selected: Option<&str>) -> Option<String> {
    selected
        .and_then(|selected| stores.iter().find(|store| store.as_str() == selected))
        .cloned()
        .or_else(|| stores.first().cloned())
}

fn selected_store_position(stores: &[String], selected: Option<&str>) -> u32 {
    resolve_selected_store(stores, selected)
        .and_then(|selected| stores.iter().position(|store| store == &selected))
        .map(|index| index as u32)
        .unwrap_or(INVALID_LIST_POSITION)
}

pub(crate) fn sync_new_password_store_selector(state: &NewPasswordPopoverState) {
    let stores = available_store_roots();
    let labels = shortened_store_labels(&stores);
    let selected = selected_new_password_store(state);
    *state.store_roots.borrow_mut() = stores.clone();
    state.store_row.set_visible(stores.len() > 1);

    let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    state
        .store_dropdown
        .set_model(Some(&StringList::new(&label_refs)));
    state
        .store_dropdown
        .set_selected(selected_store_position(&stores, selected.as_deref()));
}

pub(crate) fn selected_new_password_store(state: &NewPasswordPopoverState) -> Option<String> {
    let stores = state.store_roots.borrow();
    stores
        .get(state.store_dropdown.selected() as usize)
        .cloned()
        .or_else(|| stores.first().cloned())
}

pub(crate) fn register_open_new_password_action(
    window: &ApplicationWindow,
    state: &NewPasswordPopoverState,
) {
    let state = state.clone();
    register_window_action(window, "open-new-password", move || {
        if !state.popover.is_visible() {
            sync_new_password_store_selector(&state);
        }
        toggle_popover(&state.popover);
    });
}

#[cfg(test)]
mod tests {
    use super::{resolve_selected_store, selected_store_position, shortened_store_labels};
    use adw::gtk::INVALID_LIST_POSITION;

    #[test]
    fn store_labels_use_short_unique_suffixes() {
        let stores = vec![
            "/home/nick/.password-store".to_string(),
            "/home/nick/work/.password-store".to_string(),
        ];

        assert_eq!(
            shortened_store_labels(&stores),
            vec![
                ".../nick/.password-store".to_string(),
                ".../work/.password-store".to_string(),
            ]
        );
    }

    #[test]
    fn store_labels_fall_back_to_full_paths_when_needed() {
        let stores = vec!["/same".to_string(), "/same".to_string()];

        assert_eq!(shortened_store_labels(&stores), stores);
    }

    #[test]
    fn selected_store_uses_current_dropdown_index() {
        let stores = vec![
            "/home/nick/.password-store".to_string(),
            "/home/nick/work/.password-store".to_string(),
        ];

        assert_eq!(
            resolve_selected_store(&stores, Some("/home/nick/work/.password-store")),
            Some("/home/nick/work/.password-store".to_string())
        );
    }

    #[test]
    fn selected_store_position_falls_back_to_the_first_store() {
        let stores = vec![
            "/home/nick/.password-store".to_string(),
            "/home/nick/work/.password-store".to_string(),
        ];

        assert_eq!(selected_store_position(&stores, None), 0);
        assert_eq!(selected_store_position(&stores, Some("/missing/store")), 0);
        assert_eq!(selected_store_position(&[], None), INVALID_LIST_POSITION);
    }
}
