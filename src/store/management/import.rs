use crate::i18n::gettext;
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::store::labels::shortened_store_labels;
use crate::support::background::spawn_result_task;
use crate::support::file_picker::{choose_local_file_path, choose_local_folder_path};
use crate::support::object_data::{
    cloned_data, non_null_to_string_option, set_cloned_data, set_string_data,
};
use crate::support::pass_import::{
    available_pass_import_sources, normalize_optional_text, run_pass_import, PassImportRequest,
};
use crate::support::runtime::supports_host_command_features;
use crate::support::ui::{
    connect_row_action, push_navigation_page_if_needed, visible_navigation_page_is,
};
use crate::window::navigation::{
    show_secondary_page_chrome, HasWindowChrome, WindowNavigationState,
};
use adw::gtk::{Button, Image, ListBox, ScrolledWindow, Stack};
use adw::prelude::*;
use adw::{
    ActionRow, ApplicationWindow, ComboRow, EntryRow, NavigationPage, StatusPage, Toast,
    ToastOverlay,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

const STORE_LIST_REFRESH_ID_KEY: &str = "store-list-refresh-id";
const STORE_IMPORT_PAGE_STATE_KEY: &str = "store-import-page-state";
const STORE_IMPORT_TITLE: &str = "Import passwords";
const STORE_IMPORT_SUBTITLE: &str = "Use pass import to import into an existing store.";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum PassImportSourceState {
    #[default]
    Checking,
    Unavailable,
    Available(Vec<String>),
}

impl PassImportSourceState {
    const fn is_available(&self) -> bool {
        matches!(self, Self::Available(sources) if !sources.is_empty())
    }

    fn sources(&self) -> Option<&[String]> {
        match self {
            Self::Available(sources) if !sources.is_empty() => Some(sources),
            Self::Checking | Self::Unavailable | Self::Available(_) => None,
        }
    }
}

#[derive(Clone)]
struct PassImportRowState {
    settings: Preferences,
    window: ApplicationWindow,
    overlay: ToastOverlay,
    row: ActionRow,
    source_state: Rc<RefCell<PassImportSourceState>>,
}

impl PassImportRowState {
    fn new(
        list: &ListBox,
        settings: &Preferences,
        window: &ApplicationWindow,
        overlay: &ToastOverlay,
        stores: &[String],
    ) -> Self {
        let row = ActionRow::builder()
            .title(gettext("Import passwords"))
            .build();
        let icon = Image::from_icon_name("go-next-symbolic");
        row.add_suffix(&icon);
        list.append(&row);

        let state = Self {
            settings: settings.clone(),
            window: window.clone(),
            overlay: overlay.clone(),
            row,
            source_state: Rc::new(RefCell::new(PassImportSourceState::Checking)),
        };
        state.sync(stores);

        let open_state = state.clone();
        connect_row_action(&state.row, move || open_state.open());

        state
    }

    fn open(&self) {
        let stores = self.settings.stores();
        let source_state = self.source_state.borrow();
        if !pass_import_row_enabled(
            self.settings.uses_host_command_backend(),
            &stores,
            &source_state,
        ) {
            return;
        }

        let Some(import_sources) = source_state.sources() else {
            return;
        };

        show_pass_import_page(&self.window, &stores, import_sources, &self.overlay);
    }

    fn set_source_state(&self, source_state: PassImportSourceState, stores: &[String]) {
        *self.source_state.borrow_mut() = source_state;
        self.sync(stores);
    }

    fn sync(&self, stores: &[String]) {
        sync_pass_import_row(
            &self.row,
            self.settings.uses_host_command_backend(),
            stores,
            &self.source_state.borrow(),
        );
    }
}

const fn pass_import_row_enabled(
    uses_host_command_backend: bool,
    stores: &[String],
    source_state: &PassImportSourceState,
) -> bool {
    uses_host_command_backend && !stores.is_empty() && source_state.is_available()
}

const fn pass_import_row_subtitle(
    uses_host_command_backend: bool,
    stores: &[String],
    source_state: &PassImportSourceState,
) -> &'static str {
    if !uses_host_command_backend {
        "Switch Backend to Host to use pass import."
    } else if stores.is_empty() {
        "Add a store to use pass import."
    } else if source_state.is_available() {
        "Use pass import with your custom pass command."
    } else if matches!(source_state, PassImportSourceState::Checking) {
        "Checking pass import availability."
    } else {
        "pass import is not available."
    }
}

const fn import_source_subtitle(source_path: Option<&str>) -> &'static str {
    if source_path.is_some() {
        ""
    } else {
        "Choose a file or folder if the importer needs one."
    }
}

fn next_store_list_refresh_id() -> String {
    static NEXT_STORE_LIST_REFRESH_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_STORE_LIST_REFRESH_ID
        .fetch_add(1, Ordering::Relaxed)
        .to_string()
}

fn stores_list_refresh_is_current(list: &ListBox, refresh_id: &str) -> bool {
    non_null_to_string_option(list, STORE_LIST_REFRESH_ID_KEY).as_deref() == Some(refresh_id)
}

#[derive(Clone)]
pub struct StoreImportPageState {
    pub window: ApplicationWindow,
    pub navigation: WindowNavigationState,
    pub overlay: ToastOverlay,
    pub page: NavigationPage,
    pub stack: Stack,
    pub form: ScrolledWindow,
    pub loading: StatusPage,
    pub store_dropdown: ComboRow,
    pub source_dropdown: ComboRow,
    pub source_path_row: ActionRow,
    pub source_file_button: Button,
    pub source_folder_button: Button,
    pub source_clear_button: Button,
    pub target_path_row: EntryRow,
    pub import_button: Button,
    pub store_roots: Rc<RefCell<Vec<String>>>,
    pub import_sources: Rc<RefCell<Vec<String>>>,
    pub source_path: Rc<RefCell<Option<String>>>,
    pub in_flight: Rc<Cell<bool>>,
}

impl StoreImportPageState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        window: &ApplicationWindow,
        navigation: &WindowNavigationState,
        overlay: &ToastOverlay,
        page: &NavigationPage,
        stack: &Stack,
        form: &ScrolledWindow,
        loading: &StatusPage,
        store_dropdown: &ComboRow,
        source_dropdown: &ComboRow,
        source_path_row: &ActionRow,
        source_file_button: &Button,
        source_folder_button: &Button,
        source_clear_button: &Button,
        target_path_row: &EntryRow,
        import_button: &Button,
    ) -> Self {
        Self {
            window: window.clone(),
            navigation: navigation.clone(),
            overlay: overlay.clone(),
            page: page.clone(),
            stack: stack.clone(),
            form: form.clone(),
            loading: loading.clone(),
            store_dropdown: store_dropdown.clone(),
            source_dropdown: source_dropdown.clone(),
            source_path_row: source_path_row.clone(),
            source_file_button: source_file_button.clone(),
            source_folder_button: source_folder_button.clone(),
            source_clear_button: source_clear_button.clone(),
            target_path_row: target_path_row.clone(),
            import_button: import_button.clone(),
            store_roots: Rc::new(RefCell::new(Vec::new())),
            import_sources: Rc::new(RefCell::new(Vec::new())),
            source_path: Rc::new(RefCell::new(None)),
            in_flight: Rc::new(Cell::new(false)),
        }
    }
}

fn set_store_import_loading(state: &StoreImportPageState, loading: bool) {
    state.in_flight.set(loading);
    let visible_child: &adw::gtk::Widget = if loading {
        state.loading.upcast_ref()
    } else {
        state.form.upcast_ref()
    };
    state.stack.set_visible_child(visible_child);
}

fn pop_store_import_page_if_visible(state: &StoreImportPageState) {
    if !visible_navigation_page_is(&state.navigation.nav, &state.page) {
        return;
    }

    state.navigation.nav.pop();
    let chrome = state.navigation.window_chrome();
    show_secondary_page_chrome(&chrome, "Tools", "Utilities and maintenance", false);
}

fn reset_store_import_form(state: &StoreImportPageState) {
    state.store_dropdown.set_selected(0);
    state.source_dropdown.set_selected(0);
    *state.source_path.borrow_mut() = None;
    state
        .source_path_row
        .set_subtitle(&gettext(import_source_subtitle(None)));
    state.target_path_row.set_text("");
}

fn sync_store_import_models(
    state: &StoreImportPageState,
    stores: &[String],
    import_sources: &[String],
) {
    let store_labels = shortened_store_labels(stores);
    let store_label_refs = store_labels.iter().map(String::as_str).collect::<Vec<_>>();
    let store_model = adw::gtk::StringList::new(&store_label_refs);
    state.store_dropdown.set_model(Some(&store_model));
    *state.store_roots.borrow_mut() = stores.to_vec();

    let source_refs = import_sources
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let source_model = adw::gtk::StringList::new(&source_refs);
    state.source_dropdown.set_model(Some(&source_model));
    *state.import_sources.borrow_mut() = import_sources.to_vec();
}

fn show_pass_import_page(
    window: &ApplicationWindow,
    stores: &[String],
    import_sources: &[String],
    overlay: &ToastOverlay,
) {
    let Some(state) = cloned_data::<_, StoreImportPageState>(window, STORE_IMPORT_PAGE_STATE_KEY)
    else {
        log_error("Store import page state was not initialized.".to_string());
        overlay.add_toast(Toast::new(&gettext("Couldn't open the import page.")));
        return;
    };

    let chrome = state.navigation.window_chrome();
    show_secondary_page_chrome(&chrome, STORE_IMPORT_TITLE, STORE_IMPORT_SUBTITLE, false);
    push_navigation_page_if_needed(&state.navigation.nav, &state.page);

    if state.in_flight.get() {
        set_store_import_loading(&state, true);
        return;
    }

    sync_store_import_models(&state, stores, import_sources);
    reset_store_import_form(&state);
    set_store_import_loading(&state, false);
    state.store_dropdown.grab_focus();
}

pub fn initialize_store_import_page(state: &StoreImportPageState) {
    set_cloned_data(&state.window, STORE_IMPORT_PAGE_STATE_KEY, state.clone());

    {
        let state = state.clone();
        state.source_file_button.connect_clicked(move |_| {
            let source_path = state.source_path.clone();
            let source_path_row = state.source_path_row.clone();
            choose_local_file_path(
                &state.window,
                "Choose import source file",
                "Select",
                &state.overlay,
                move |path| {
                    *source_path.borrow_mut() = Some(path.clone());
                    source_path_row.set_subtitle(&path);
                },
            );
        });
    }

    {
        let state = state.clone();
        state.source_folder_button.connect_clicked(move |_| {
            let source_path = state.source_path.clone();
            let source_path_row = state.source_path_row.clone();
            choose_local_folder_path(
                &state.window,
                "Choose import source folder",
                "Select",
                false,
                &state.overlay,
                move |path| {
                    *source_path.borrow_mut() = Some(path.clone());
                    source_path_row.set_subtitle(&path);
                },
            );
        });
    }

    {
        let source_path = state.source_path.clone();
        let source_path_row = state.source_path_row.clone();
        state.source_clear_button.connect_clicked(move |_| {
            *source_path.borrow_mut() = None;
            source_path_row.set_subtitle(&gettext(import_source_subtitle(None)));
        });
    }

    {
        let state = state.clone();
        let import_button = state.import_button.clone();
        import_button.connect_clicked(move |_| {
            let store_roots = state.store_roots.borrow();
            let Some(store_root) = store_roots
                .get(state.store_dropdown.selected() as usize)
                .cloned()
            else {
                state
                    .overlay
                    .add_toast(Toast::new(&gettext("Choose a store.")));
                return;
            };

            let import_sources = state.import_sources.borrow();
            let Some(source) = import_sources
                .get(state.source_dropdown.selected() as usize)
                .cloned()
            else {
                state
                    .overlay
                    .add_toast(Toast::new(&gettext("Choose an importer.")));
                return;
            };

            let request = PassImportRequest {
                store_root,
                source,
                source_path: state.source_path.borrow().clone(),
                target_path: normalize_optional_text(&state.target_path_row.text()),
            };
            start_pass_import(&state, request);
        });
    }
}

fn finish_pass_import(
    state: &StoreImportPageState,
    result: Result<(), String>,
    request: &PassImportRequest,
) {
    set_store_import_loading(state, false);

    match result {
        Ok(()) => {
            reset_store_import_form(state);
            pop_store_import_page_if_visible(state);
            state
                .overlay
                .add_toast(Toast::new(&gettext("Passwords imported.")));
        }
        Err(err) => {
            log_error(format!(
                "Failed to import passwords into '{}' from '{}': {err}",
                request.store_root, request.source
            ));
            state
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't import passwords.")));
        }
    }
}

fn start_pass_import(state: &StoreImportPageState, request: PassImportRequest) {
    set_store_import_loading(state, true);
    let state = state.clone();
    let state_for_disconnect = state.clone();
    let store_for_error = request.store_root.clone();
    let source_for_error = request.source.clone();
    let request_for_result = request.clone();
    spawn_result_task(
        move || run_pass_import(&request),
        move |result| {
            finish_pass_import(&state, result, &request_for_result);
        },
        move || {
            set_store_import_loading(&state_for_disconnect, false);
            log_error(format!(
                "Pass import worker disconnected unexpectedly while importing into '{store_for_error}' from '{source_for_error}'."
            ));
            state_for_disconnect
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't import passwords.")));
        },
    );
}

fn sync_pass_import_row(
    row: &ActionRow,
    uses_host_command_backend: bool,
    stores: &[String],
    source_state: &PassImportSourceState,
) {
    let enabled = pass_import_row_enabled(uses_host_command_backend, stores, source_state);
    row.set_subtitle(&gettext(pass_import_row_subtitle(
        uses_host_command_backend,
        stores,
        source_state,
    )));
    row.set_activatable(enabled);
    row.set_sensitive(enabled);
}

pub fn schedule_store_import_row(
    list: &ListBox,
    settings: &Preferences,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
) {
    if !supports_host_command_features() {
        return;
    }

    let refresh_id = next_store_list_refresh_id();
    set_string_data(list, STORE_LIST_REFRESH_ID_KEY, refresh_id.clone());

    let stores = settings.stores();
    let row_state = PassImportRowState::new(list, settings, window, overlay, &stores);

    let list_for_result = list.clone();
    let stores_for_result = stores;
    let refresh_id_for_result = refresh_id;
    let row_state_for_result = row_state;
    spawn_result_task(
        available_pass_import_sources,
        move |result| {
            if !stores_list_refresh_is_current(&list_for_result, &refresh_id_for_result) {
                return;
            }

            let source_state = result.map_or(
                PassImportSourceState::Unavailable,
                PassImportSourceState::Available,
            );
            row_state_for_result.set_source_state(source_state, &stores_for_result);
        },
        move || {},
    );
}

#[cfg(test)]
mod tests {
    use super::{pass_import_row_enabled, pass_import_row_subtitle, PassImportSourceState};

    #[test]
    fn pass_import_row_is_enabled_only_when_every_requirement_is_met() {
        assert!(!pass_import_row_enabled(
            false,
            &["/tmp/store".to_string()],
            &PassImportSourceState::Available(vec!["bitwarden".to_string()]),
        ));
        assert!(!pass_import_row_enabled(
            true,
            &[],
            &PassImportSourceState::Available(vec!["bitwarden".to_string()]),
        ));
        assert!(!pass_import_row_enabled(
            true,
            &["/tmp/store".to_string()],
            &PassImportSourceState::Unavailable,
        ));
        assert!(pass_import_row_enabled(
            true,
            &["/tmp/store".to_string()],
            &PassImportSourceState::Available(vec!["bitwarden".to_string()]),
        ));
    }

    #[test]
    fn pass_import_row_subtitle_matches_the_current_requirement() {
        assert_eq!(
            pass_import_row_subtitle(
                false,
                &["/tmp/store".to_string()],
                &PassImportSourceState::Available(vec!["bitwarden".to_string()]),
            ),
            "Switch Backend to Host to use pass import."
        );
        assert_eq!(
            pass_import_row_subtitle(true, &[], &PassImportSourceState::Checking),
            "Add a store to use pass import."
        );
        assert_eq!(
            pass_import_row_subtitle(
                true,
                &["/tmp/store".to_string()],
                &PassImportSourceState::Checking,
            ),
            "Checking pass import availability."
        );
        assert_eq!(
            pass_import_row_subtitle(
                true,
                &["/tmp/store".to_string()],
                &PassImportSourceState::Unavailable,
            ),
            "pass import is not available."
        );
    }
}
