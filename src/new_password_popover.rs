use crate::preferences::Preferences;
use adw::gio::SimpleAction;
use adw::prelude::*;
use adw::{ApplicationWindow, EntryRow};
use adw::gtk::{Box as GtkBox, DropDown, Popover, StringList};
use std::path::Path;

#[derive(Clone)]
pub(crate) struct NewPasswordPopoverState {
    pub(crate) popover: Popover,
    pub(crate) path_entry: EntryRow,
    pub(crate) store_box: GtkBox,
    pub(crate) store_dropdown: DropDown,
}

fn available_store_roots() -> Vec<String> {
    Preferences::new().stores()
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

    let max_depth = path_segments
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or_default();
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

pub(crate) fn sync_new_password_store_dropdown(box_widget: &GtkBox, dropdown: &DropDown) {
    let stores = available_store_roots();
    let labels = shortened_store_labels(&stores);
    let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    dropdown.set_model(Some(&StringList::new(&label_refs)));
    box_widget.set_visible(stores.len() > 1);
    if !stores.is_empty() {
        dropdown.set_selected(dropdown.selected().min(stores.len() as u32 - 1));
    }
}

pub(crate) fn selected_new_password_store(dropdown: &DropDown) -> Option<String> {
    let stores = available_store_roots();
    if stores.len() <= 1 {
        return stores.into_iter().next();
    }

    stores
        .get(dropdown.selected() as usize)
        .cloned()
        .or_else(|| stores.into_iter().next())
}

pub(crate) fn register_open_new_password_action(
    window: &ApplicationWindow,
    state: &NewPasswordPopoverState,
) {
    let state = state.clone();
    let action = SimpleAction::new("open-new-password", None);
    action.connect_activate(move |_, _| {
        if state.popover.is_visible() {
            state.popover.popdown();
        } else {
            sync_new_password_store_dropdown(&state.store_box, &state.store_dropdown);
            state.popover.popup();
            state.path_entry.grab_focus();
        }
    });
    window.add_action(&action);
}

#[cfg(test)]
mod tests {
    use super::shortened_store_labels;

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
}
