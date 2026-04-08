use super::{
    next_generation, set_tool_row_enabled, set_tool_row_suffix_loading, ToolsPageState,
    AUDIT_EMPTY_BRANCH_SUBTITLE, AUDIT_EMPTY_BRANCH_TITLE, AUDIT_EMPTY_SELECTION_SUBTITLE,
    AUDIT_EMPTY_SELECTION_TITLE, AUDIT_ERROR_TITLE, AUDIT_FILTER_EMPTY_SUBTITLE,
    AUDIT_FILTER_EMPTY_TITLE, AUDIT_LOADING_COMMITS_SUBTITLE, AUDIT_LOADING_COMMITS_TITLE,
    AUDIT_LOADING_SUBTITLE, AUDIT_LOADING_TITLE, AUDIT_LOAD_MORE_SUBTITLE, AUDIT_LOAD_MORE_TITLE,
    AUDIT_NO_STORES_SUBTITLE, AUDIT_NO_STORES_TITLE, AUDIT_ROW_DISABLED_SUBTITLE,
    AUDIT_ROW_GIT_UNAVAILABLE_SUBTITLE, AUDIT_ROW_SUBTITLE, AUDIT_SEARCH_EMPTY_SUBTITLE,
    AUDIT_SEARCH_EMPTY_TITLE, AUDIT_SUBTITLE, AUDIT_TITLE,
};
use crate::i18n::gettext;
use crate::preferences::Preferences;
use crate::store::labels::{shortened_store_label_for_path, shortened_store_label_map};
use crate::support::background::spawn_result_task;
use crate::support::git::{
    audit_unverified_reason_message, discover_store_git_audit_catalog, git_command_available,
    has_git_repository, load_store_git_audit_commit_page, StoreGitAuditBranchRef,
    StoreGitAuditCatalog, StoreGitAuditCommit, StoreGitAuditCommitPage, StoreGitAuditPathChange,
    StoreGitAuditVerification, StoreGitAuditVerificationMode, StoreGitAuditVerificationState,
    STORE_GIT_AUDIT_PAGE_SIZE,
};
use crate::support::runtime::supports_audit_features;
use crate::support::ui::{reveal_navigation_page, visible_navigation_page_is};
use crate::window::navigation::{show_secondary_page_chrome, HasWindowChrome};
use adw::glib::WeakRef;
use adw::gtk::{
    Align, Box as GtkBox, CheckButton, Grid, Image, Label, Orientation, Spinner, Widget,
};
use adw::prelude::*;
use adw::{ActionRow, ExpanderRow, PreferencesGroup, Toast};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

#[derive(Default)]
pub(super) struct AuditToolState {
    generation: Cell<u64>,
    loading_catalog: Cell<bool>,
    catalog: RefCell<Option<StoreGitAuditCatalog>>,
    error: RefCell<Option<String>>,
    selected_stores: RefCell<Option<BTreeSet<String>>>,
    selected_branches: RefCell<Option<BTreeSet<String>>>,
    store_labels: RefCell<HashMap<String, String>>,
    branches: RefCell<BTreeMap<AuditBranchKey, Rc<AuditBranchState>>>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct AuditBranchKey {
    store_root: String,
    full_ref: String,
    branch_name: String,
}

#[derive(Default)]
struct AuditBranchState {
    generation: Cell<u64>,
    loading: Cell<bool>,
    loaded_once: Cell<bool>,
    expanded: Cell<bool>,
    has_more: Cell<bool>,
    next_page: Cell<usize>,
    error: RefCell<Option<String>>,
    current_row: RefCell<Option<WeakRef<ExpanderRow>>>,
    commits: RefCell<Vec<StoreGitAuditCommit>>,
    rows: RefCell<Vec<Widget>>,
}

impl ToolsPageState {
    pub(super) fn prepare_audit_page(&self) {
        if !supports_audit_features() || !git_command_available() {
            return;
        }

        let chrome = self.navigation.window_chrome();
        show_secondary_page_chrome(&chrome, AUDIT_TITLE, AUDIT_SUBTITLE, false);
        chrome.find.set_visible(true);
        reveal_navigation_page(&self.navigation.nav, &self.audit_page.page);

        if self.audit_page.audit.catalog.borrow().is_none()
            && !self.audit_page.audit.loading_catalog.get()
        {
            self.start_audit_catalog_load();
        } else {
            self.sync_audit_filter_button();
            self.render_audit_page();
        }
    }

    pub(super) fn sync_audit_tool_row(&self) {
        let supported = supports_audit_features();
        self.select_page.audit_row.set_visible(supported);
        if !supported {
            return;
        }

        let availability = audit_git_runtime_availability(&Preferences::new().store_roots());
        self.select_page
            .audit_row
            .set_subtitle(&localized_text(audit_row_subtitle(availability)));
        set_tool_row_enabled(
            &self.select_page.audit_row,
            matches!(availability, AuditGitRuntimeAvailability::Available),
        );
        set_tool_row_suffix_loading(
            &self.select_page.audit_suffix_stack,
            &self.select_page.audit_suffix_arrow,
            &self.select_page.audit_spinner,
            self.audit_tool_busy(),
        );
    }

    pub(super) fn clear_audit_transient_state(&self) {
        self.audit_page
            .audit
            .generation
            .set(next_generation(self.audit_page.audit.generation.get()));
        self.audit_page.audit.loading_catalog.set(false);
        *self.audit_page.audit.catalog.borrow_mut() = None;
        *self.audit_page.audit.error.borrow_mut() = None;
        self.audit_page.audit.store_labels.borrow_mut().clear();
        self.audit_page.audit.branches.borrow_mut().clear();
        clear_box_children(&self.audit_page.content);
        clear_box_children(&self.audit_page.filter_store_box);
        clear_box_children(&self.audit_page.filter_branch_box);
        self.reset_audit_view();
        self.set_audit_status(AUDIT_TITLE, AUDIT_NO_STORES_SUBTITLE);
        self.audit_page
            .stack
            .set_visible_child(&self.audit_page.audit_status);
        self.sync_audit_filter_button();
        self.sync_tool_rows();
    }

    pub(super) fn audit_has_transient_state(&self) -> bool {
        self.audit_page.audit.loading_catalog.get()
            || self.audit_page.audit.catalog.borrow().is_some()
            || !self.audit_page.audit.branches.borrow().is_empty()
            || self.audit_page.audit.error.borrow().is_some()
    }

    pub(super) fn sync_audit_filter_button(&self) {
        let visible = supports_audit_features()
            && visible_navigation_page_is(&self.navigation.nav, &self.audit_page.page);
        self.audit_page.filter_button.set_visible(visible);
        self.audit_page.filter_button.set_sensitive(
            visible
                && !self.audit_page.audit.loading_catalog.get()
                && self.audit_page.audit.catalog.borrow().is_some(),
        );
        if !visible {
            self.audit_page.filter_popover.popdown();
        }
    }

    fn audit_tool_busy(&self) -> bool {
        self.audit_page.audit.loading_catalog.get()
            || self
                .audit_page
                .audit
                .branches
                .borrow()
                .values()
                .any(|branch| branch.loading.get())
    }

    fn start_audit_catalog_load(&self) {
        let generation = next_generation(self.audit_page.audit.generation.get());
        self.audit_page.audit.generation.set(generation);
        self.audit_page.audit.loading_catalog.set(true);
        *self.audit_page.audit.catalog.borrow_mut() = None;
        *self.audit_page.audit.error.borrow_mut() = None;
        self.audit_page.audit.store_labels.borrow_mut().clear();
        self.audit_page.audit.branches.borrow_mut().clear();
        self.render_audit_filter_controls();
        self.render_audit_page();

        let store_roots = Preferences::new().store_roots();
        let state_for_result = self.clone();
        let state_for_disconnect = self.clone();
        spawn_result_task(
            move || discover_store_git_audit_catalog(&store_roots),
            move |result| state_for_result.apply_audit_catalog(generation, result),
            move || state_for_disconnect.handle_audit_catalog_disconnect(generation),
        );
    }

    fn apply_audit_catalog(&self, generation: u64, result: Result<StoreGitAuditCatalog, String>) {
        if generation != self.audit_page.audit.generation.get() {
            return;
        }

        self.audit_page.audit.loading_catalog.set(false);
        match result {
            Ok(catalog) => {
                *self.audit_page.audit.error.borrow_mut() = None;
                self.audit_page
                    .audit
                    .store_labels
                    .replace(shortened_store_label_map(
                        &catalog
                            .stores
                            .iter()
                            .map(|store| store.store_root.clone())
                            .collect::<Vec<_>>(),
                    ));
                let available_stores = audit_available_store_ids(&catalog);
                let available_branches = audit_available_branch_names(&catalog);
                let selected_stores = self.audit_page.audit.selected_stores.borrow().clone();
                let selected_branches = self.audit_page.audit.selected_branches.borrow().clone();
                *self.audit_page.audit.selected_stores.borrow_mut() = Some(
                    reconciled_filter_selection(selected_stores.as_ref(), &available_stores),
                );
                *self.audit_page.audit.selected_branches.borrow_mut() = Some(
                    reconciled_filter_selection(selected_branches.as_ref(), &available_branches),
                );
                *self.audit_page.audit.catalog.borrow_mut() = Some(catalog);
            }
            Err(err) => {
                *self.audit_page.audit.catalog.borrow_mut() = None;
                *self.audit_page.audit.error.borrow_mut() = Some(err);
            }
        }

        self.render_audit_filter_controls();
        self.render_audit_page();
        self.sync_tool_rows();
    }

    fn handle_audit_catalog_disconnect(&self, generation: u64) {
        if generation != self.audit_page.audit.generation.get() {
            return;
        }

        self.audit_page.audit.loading_catalog.set(false);
        *self.audit_page.audit.catalog.borrow_mut() = None;
        *self.audit_page.audit.error.borrow_mut() = Some("Audit stopped unexpectedly.".to_string());
        self.render_audit_filter_controls();
        self.render_audit_page();
        self.sync_tool_rows();
    }

    pub(crate) fn render_audit_page(&self) {
        self.sync_audit_filter_button();
        clear_box_children(&self.audit_page.content);
        let query = self.audit_search_query();

        if self.audit_page.audit.loading_catalog.get() {
            self.set_audit_status(AUDIT_LOADING_TITLE, AUDIT_LOADING_SUBTITLE);
            self.audit_page
                .stack
                .set_visible_child(&self.audit_page.audit_status);
            return;
        }

        if let Some(err) = self.audit_page.audit.error.borrow().clone() {
            self.set_audit_status_text(AUDIT_ERROR_TITLE, &err);
            self.audit_page
                .stack
                .set_visible_child(&self.audit_page.audit_status);
            return;
        }

        let Some(catalog) = self.audit_page.audit.catalog.borrow().clone() else {
            self.set_audit_status(AUDIT_TITLE, AUDIT_NO_STORES_SUBTITLE);
            self.audit_page
                .stack
                .set_visible_child(&self.audit_page.audit_status);
            return;
        };

        if catalog.stores.is_empty() {
            self.set_audit_status(AUDIT_NO_STORES_TITLE, AUDIT_NO_STORES_SUBTITLE);
            self.audit_page
                .stack
                .set_visible_child(&self.audit_page.audit_status);
            return;
        }

        let selected_stores = selected_filter_values(
            self.audit_page.audit.selected_stores.borrow().as_ref(),
            &audit_available_store_ids(&catalog),
        );
        let selected_branches = selected_filter_values(
            self.audit_page.audit.selected_branches.borrow().as_ref(),
            &audit_available_branch_names(&catalog),
        );
        if selected_stores.is_empty() || selected_branches.is_empty() {
            self.set_audit_status(AUDIT_EMPTY_SELECTION_TITLE, AUDIT_EMPTY_SELECTION_SUBTITLE);
            self.audit_page
                .stack
                .set_visible_child(&self.audit_page.audit_status);
            return;
        }

        let mut rendered_groups = 0;
        for store in &catalog.stores {
            if !selected_stores.contains(&store.store_root) {
                continue;
            }

            let store_label = shortened_store_label_for_path(
                &store.store_root,
                &self.audit_page.audit.store_labels.borrow(),
            );
            let visible_branches = store
                .branches
                .iter()
                .filter(|branch| {
                    selected_branches.contains(&branch.name)
                        && self.audit_branch_matches_query(store, &store_label, branch, &query)
                })
                .collect::<Vec<_>>();
            if visible_branches.is_empty() {
                continue;
            }

            let group = PreferencesGroup::builder()
                .title(gtk_safe_text(&store_label))
                .build();
            group.set_description(Some(&gtk_safe_text(&store.store_root)));

            for branch in visible_branches {
                group.add(&self.build_audit_branch_row(store, branch));
            }

            self.audit_page.content.append(&group);
            rendered_groups += 1;
        }

        if rendered_groups == 0 {
            self.set_audit_status(
                if query.is_empty() {
                    AUDIT_FILTER_EMPTY_TITLE
                } else {
                    AUDIT_SEARCH_EMPTY_TITLE
                },
                if query.is_empty() {
                    AUDIT_FILTER_EMPTY_SUBTITLE
                } else {
                    AUDIT_SEARCH_EMPTY_SUBTITLE
                },
            );
            self.audit_page
                .stack
                .set_visible_child(&self.audit_page.audit_status);
            return;
        }

        self.audit_page
            .stack
            .set_visible_child(&self.audit_page.scrolled);
    }

    fn render_audit_filter_controls(&self) {
        clear_box_children(&self.audit_page.filter_store_box);
        clear_box_children(&self.audit_page.filter_branch_box);

        let Some(catalog) = self.audit_page.audit.catalog.borrow().clone() else {
            self.sync_audit_filter_button();
            return;
        };

        let selected_stores = selected_filter_values(
            self.audit_page.audit.selected_stores.borrow().as_ref(),
            &audit_available_store_ids(&catalog),
        );
        let selected_branches = selected_filter_values(
            self.audit_page.audit.selected_branches.borrow().as_ref(),
            &audit_available_branch_names(&catalog),
        );

        for store in &catalog.stores {
            let store_root = store.store_root.clone();
            let label = shortened_store_label_for_path(
                &store_root,
                &self.audit_page.audit.store_labels.borrow(),
            );
            let toggle = build_audit_filter_toggle(&label, selected_stores.contains(&store_root));
            let state = self.clone();
            toggle.connect_toggled(move |toggle| {
                state.update_audit_store_filter(&store_root, toggle.is_active());
            });
            self.audit_page.filter_store_box.append(&toggle);
        }

        for branch_name in audit_available_branch_names(&catalog) {
            let toggle =
                build_audit_filter_toggle(&branch_name, selected_branches.contains(&branch_name));
            let state = self.clone();
            toggle.connect_toggled(move |toggle| {
                state.update_audit_branch_filter(&branch_name, toggle.is_active());
            });
            self.audit_page.filter_branch_box.append(&toggle);
        }

        self.sync_audit_filter_button();
    }

    fn update_audit_store_filter(&self, store_root: &str, active: bool) {
        let Some(catalog) = self.audit_page.audit.catalog.borrow().clone() else {
            return;
        };
        let available = audit_available_store_ids(&catalog);
        let mut selection = selected_filter_values(
            self.audit_page.audit.selected_stores.borrow().as_ref(),
            &available,
        );
        if active {
            selection.insert(store_root.to_string());
        } else {
            selection.remove(store_root);
        }
        *self.audit_page.audit.selected_stores.borrow_mut() = Some(selection);
        self.render_audit_page();
    }

    fn update_audit_branch_filter(&self, branch_name: &str, active: bool) {
        let Some(catalog) = self.audit_page.audit.catalog.borrow().clone() else {
            return;
        };
        let available = audit_available_branch_names(&catalog);
        let mut selection = selected_filter_values(
            self.audit_page.audit.selected_branches.borrow().as_ref(),
            &available,
        );
        if active {
            selection.insert(branch_name.to_string());
        } else {
            selection.remove(branch_name);
        }
        *self.audit_page.audit.selected_branches.borrow_mut() = Some(selection);
        self.render_audit_page();
    }

    fn audit_search_query(&self) -> String {
        audit_search_query(self.audit_page.search_entry.text().as_str())
    }

    fn reset_audit_view(&self) {
        self.audit_page.search_entry.set_visible(false);
        if self.audit_page.search_entry.text().is_empty() {
            return;
        }

        self.audit_page.search_entry.set_text("");
    }

    fn audit_branch_matches_query(
        &self,
        store: &crate::support::git::StoreGitAuditStore,
        store_label: &str,
        branch: &StoreGitAuditBranchRef,
        query: &str,
    ) -> bool {
        if query.is_empty()
            || audit_branch_context_matches_query(
                query,
                &store.store_root,
                store_label,
                &branch.name,
                &branch.full_ref,
            )
        {
            return true;
        }

        let key = AuditBranchKey {
            store_root: store.store_root.clone(),
            full_ref: branch.full_ref.clone(),
            branch_name: branch.name.clone(),
        };
        let Some(runtime) = self.existing_audit_branch_state(&key) else {
            return false;
        };

        let matches_loaded_commit = runtime
            .commits
            .borrow()
            .iter()
            .any(|commit| audit_commit_matches_query(commit, query));
        matches_loaded_commit
    }

    fn build_audit_branch_row(
        &self,
        store: &crate::support::git::StoreGitAuditStore,
        branch: &StoreGitAuditBranchRef,
    ) -> ExpanderRow {
        let key = AuditBranchKey {
            store_root: store.store_root.clone(),
            full_ref: branch.full_ref.clone(),
            branch_name: branch.name.clone(),
        };
        let runtime = self.audit_branch_state(&key);

        let row = ExpanderRow::new();
        bind_audit_branch_row(&runtime, &row);
        row.set_use_markup(false);
        row.set_title(&gtk_safe_text(&branch.name));
        row.set_subtitle(&gtk_safe_text(&branch_row_subtitle(branch, &runtime)));
        row.set_enable_expansion(true);
        row.set_expanded(runtime.expanded.get());

        let state = self.clone();
        let key_for_toggle = key.clone();
        let branch_for_toggle = branch.clone();
        row.connect_notify_local(Some("expanded"), move |row, _| {
            let runtime = state.audit_branch_state(&key_for_toggle);
            runtime.expanded.set(row.is_expanded());
            row.set_subtitle(&gtk_safe_text(&branch_row_subtitle(
                &branch_for_toggle,
                &runtime,
            )));
            if branch_expansion_needs_initial_load(&runtime, row.is_expanded()) {
                state.load_audit_branch_if_needed(&key_for_toggle);
            } else if row.is_expanded() {
                state.populate_audit_branch_row(row, &key_for_toggle, &runtime);
            } else {
                clear_audit_branch_row_contents(row, &runtime);
            }
        });

        if runtime.expanded.get() {
            self.populate_audit_branch_row(&row, &key, &runtime);
        }

        row
    }

    fn populate_audit_branch_row(
        &self,
        row: &ExpanderRow,
        key: &AuditBranchKey,
        runtime: &Rc<AuditBranchState>,
    ) {
        if !audit_branch_row_is_current(row, runtime) {
            return;
        }

        clear_audit_branch_row_contents(row, runtime);

        if let Some(err) = runtime.error.borrow().clone() {
            let error_row = build_info_text_row(AUDIT_ERROR_TITLE, &err);
            add_audit_branch_row_child(row, runtime, &error_row);
        }

        let search_query = self.audit_search_query();
        let store_label = shortened_store_label_for_path(
            &key.store_root,
            &self.audit_page.audit.store_labels.borrow(),
        );
        let show_all_commits = search_query.is_empty()
            || audit_branch_context_matches_query(
                &search_query,
                &key.store_root,
                &store_label,
                &key.branch_name,
                &key.full_ref,
            );
        let commits = runtime
            .commits
            .borrow()
            .iter()
            .filter(|commit| show_all_commits || audit_commit_matches_query(commit, &search_query))
            .cloned()
            .collect::<Vec<_>>();
        if commits.is_empty() {
            if runtime.loading.get() {
                let loading_row = build_loading_branch_row();
                add_audit_branch_row_child(row, runtime, &loading_row);
                return;
            }

            if runtime.loaded_once.get() {
                let empty_row =
                    build_info_row(AUDIT_EMPTY_BRANCH_TITLE, AUDIT_EMPTY_BRANCH_SUBTITLE);
                add_audit_branch_row_child(row, runtime, &empty_row);
                return;
            }
        }

        for commit in commits {
            let commit_row = self.build_commit_row(&commit);
            add_audit_branch_row_child(row, runtime, &commit_row);
        }

        if runtime.loading.get() {
            let loading_row = build_loading_branch_row();
            add_audit_branch_row_child(row, runtime, &loading_row);
            return;
        }

        if runtime.has_more.get() {
            let load_more = build_action_row(
                AUDIT_LOAD_MORE_TITLE,
                &gettext(AUDIT_LOAD_MORE_SUBTITLE)
                    .replace("{count}", &STORE_GIT_AUDIT_PAGE_SIZE.to_string()),
                true,
            );
            load_more.add_suffix(&Image::from_icon_name("go-next-symbolic"));
            let state = self.clone();
            let key = key.clone();
            load_more.connect_activated(move |_| state.load_more_audit_branch(&key));
            add_audit_branch_row_child(row, runtime, &load_more);
        }
    }

    fn build_commit_row(&self, commit: &StoreGitAuditCommit) -> ExpanderRow {
        let row = ExpanderRow::new();
        row.set_use_markup(false);
        row.set_title(&gtk_safe_text(&commit.subject));
        row.set_subtitle(&gtk_safe_text(&commit_summary_subtitle(commit)));
        row.set_enable_expansion(true);
        row.add_row(&build_commit_details_widget(commit));

        row
    }

    fn load_audit_branch_if_needed(&self, key: &AuditBranchKey) {
        let runtime = self.audit_branch_state(key);
        if runtime.loading.get() || runtime.loaded_once.get() {
            return;
        }

        self.start_audit_branch_load(key, 0, false);
    }

    fn load_more_audit_branch(&self, key: &AuditBranchKey) {
        let runtime = self.audit_branch_state(key);
        if runtime.loading.get() || !runtime.has_more.get() {
            return;
        }

        self.start_audit_branch_load(key, runtime.next_page.get(), true);
    }

    fn start_audit_branch_load(&self, key: &AuditBranchKey, page: usize, append: bool) {
        let runtime = self.audit_branch_state(key);
        let generation = next_generation(runtime.generation.get());
        runtime.generation.set(generation);
        runtime.loading.set(true);
        if !append {
            runtime.loaded_once.set(false);
            runtime.has_more.set(false);
            runtime.next_page.set(0);
            runtime.commits.borrow_mut().clear();
        }
        *runtime.error.borrow_mut() = None;
        self.render_audit_page();
        self.sync_tool_rows();

        let store_root = key.store_root.clone();
        let full_ref = key.full_ref.clone();
        let use_history = Preferences::new().audit_use_commit_history_recipients();
        let state_for_result = self.clone();
        let state_for_disconnect = self.clone();
        let key_for_result = key.clone();
        let key_for_disconnect = key.clone();
        spawn_result_task(
            move || load_store_git_audit_commit_page(&store_root, &full_ref, use_history, page),
            move |result| {
                state_for_result.apply_audit_branch_page(
                    &key_for_result,
                    generation,
                    append,
                    result,
                )
            },
            move || {
                state_for_disconnect.handle_audit_branch_disconnect(&key_for_disconnect, generation)
            },
        );
    }

    fn apply_audit_branch_page(
        &self,
        key: &AuditBranchKey,
        generation: u64,
        append: bool,
        result: Result<StoreGitAuditCommitPage, String>,
    ) {
        let Some(runtime) = self.existing_audit_branch_state(key) else {
            return;
        };
        if generation != runtime.generation.get() {
            return;
        }

        runtime.loading.set(false);
        match result {
            Ok(page) => {
                runtime.loaded_once.set(true);
                runtime.has_more.set(page.has_more);
                runtime.next_page.set(page.next_page);
                *runtime.error.borrow_mut() = None;
                if append {
                    runtime.commits.borrow_mut().extend(page.commits);
                } else {
                    *runtime.commits.borrow_mut() = page.commits;
                }
            }
            Err(err) => {
                *runtime.error.borrow_mut() = Some(err.clone());
                self.overlay
                    .add_toast(Toast::new(&gettext("Couldn't load commits.")));
            }
        }

        self.render_audit_page();
        self.sync_tool_rows();
    }

    fn handle_audit_branch_disconnect(&self, key: &AuditBranchKey, generation: u64) {
        let Some(runtime) = self.existing_audit_branch_state(key) else {
            return;
        };
        if generation != runtime.generation.get() {
            return;
        }

        runtime.loading.set(false);
        *runtime.error.borrow_mut() = Some("Commit loading stopped unexpectedly.".to_string());
        self.render_audit_page();
        self.sync_tool_rows();
    }

    fn audit_branch_state(&self, key: &AuditBranchKey) -> Rc<AuditBranchState> {
        if let Some(existing) = self.audit_page.audit.branches.borrow().get(key).cloned() {
            return existing;
        }

        let state = Rc::new(AuditBranchState::default());
        self.audit_page
            .audit
            .branches
            .borrow_mut()
            .insert(key.clone(), state.clone());
        state
    }

    fn existing_audit_branch_state(&self, key: &AuditBranchKey) -> Option<Rc<AuditBranchState>> {
        self.audit_page.audit.branches.borrow().get(key).cloned()
    }

    fn set_audit_status(&self, title: &str, description: &str) {
        self.audit_page
            .audit_status
            .set_title(&localized_text(title));
        self.audit_page
            .audit_status
            .set_description(Some(&localized_text(description)));
    }

    fn set_audit_status_text(&self, title: &str, description: &str) {
        self.audit_page
            .audit_status
            .set_title(&localized_text(title));
        self.audit_page
            .audit_status
            .set_description(Some(&gtk_safe_text(description)));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuditGitRuntimeAvailability {
    Available,
    GitUnavailable,
    NoGitStores,
}

fn audit_git_runtime_availability(stores: &[String]) -> AuditGitRuntimeAvailability {
    if !git_command_available() {
        return AuditGitRuntimeAvailability::GitUnavailable;
    }
    if stores.iter().any(|store| has_git_repository(store)) {
        AuditGitRuntimeAvailability::Available
    } else {
        AuditGitRuntimeAvailability::NoGitStores
    }
}

const fn audit_row_subtitle(availability: AuditGitRuntimeAvailability) -> &'static str {
    match availability {
        AuditGitRuntimeAvailability::Available => AUDIT_ROW_SUBTITLE,
        AuditGitRuntimeAvailability::GitUnavailable => AUDIT_ROW_GIT_UNAVAILABLE_SUBTITLE,
        AuditGitRuntimeAvailability::NoGitStores => AUDIT_ROW_DISABLED_SUBTITLE,
    }
}

fn audit_available_store_ids(catalog: &StoreGitAuditCatalog) -> BTreeSet<String> {
    catalog
        .stores
        .iter()
        .map(|store| store.store_root.clone())
        .collect()
}

fn audit_available_branch_names(catalog: &StoreGitAuditCatalog) -> BTreeSet<String> {
    catalog
        .stores
        .iter()
        .flat_map(|store| store.branches.iter().map(|branch| branch.name.clone()))
        .collect()
}

fn reconciled_filter_selection(
    existing: Option<&BTreeSet<String>>,
    available: &BTreeSet<String>,
) -> BTreeSet<String> {
    existing.map_or_else(
        || available.clone(),
        |existing| {
            existing
                .iter()
                .filter(|value| available.contains(*value))
                .cloned()
                .collect()
        },
    )
}

fn selected_filter_values(
    existing: Option<&BTreeSet<String>>,
    available: &BTreeSet<String>,
) -> BTreeSet<String> {
    reconciled_filter_selection(existing, available)
}

fn audit_search_query(text: &str) -> String {
    text.trim().to_lowercase()
}

fn audit_text_matches_query(text: &str, query: &str) -> bool {
    query.is_empty() || text.to_lowercase().contains(query)
}

fn audit_branch_context_matches_query(
    query: &str,
    store_root: &str,
    store_label: &str,
    branch_name: &str,
    full_ref: &str,
) -> bool {
    audit_text_matches_query(store_label, query)
        || audit_text_matches_query(store_root, query)
        || audit_text_matches_query(branch_name, query)
        || audit_text_matches_query(full_ref, query)
}

fn branch_expansion_needs_initial_load(runtime: &AuditBranchState, expanded: bool) -> bool {
    expanded && !runtime.loading.get() && !runtime.loaded_once.get()
}

fn branch_row_subtitle(branch: &StoreGitAuditBranchRef, runtime: &AuditBranchState) -> String {
    let scope = if branch.remote {
        gettext("Remote-tracking branch")
    } else {
        gettext("Local branch")
    };
    if runtime.loading.get() {
        return format!("{scope} · {}", gettext(AUDIT_LOADING_COMMITS_TITLE));
    }
    if let Some(err) = runtime.error.borrow().clone() {
        return format!("{scope} · {err}");
    }
    if runtime.loaded_once.get() {
        let count = runtime.commits.borrow().len();
        let loaded = gettext("{count} commits loaded").replace("{count}", &count.to_string());
        return format!("{scope} · {loaded}");
    }

    scope
}

fn commit_summary_subtitle(commit: &StoreGitAuditCommit) -> String {
    format!(
        "{} · {} · {}",
        commit.short_oid,
        commit.committed_at,
        verification_summary(&commit.verification)
    )
}

fn verification_state_summary(verification: &StoreGitAuditVerification) -> String {
    match verification.state {
        StoreGitAuditVerificationState::Verified => gettext("Verified"),
        StoreGitAuditVerificationState::Unverified => gettext("Unverified"),
    }
}

fn verification_summary(verification: &StoreGitAuditVerification) -> String {
    match verification.reason {
        Some(reason) => format!(
            "{}: {}",
            verification_state_summary(verification),
            audit_unverified_reason_message(reason)
        ),
        None => verification_state_summary(verification),
    }
}

fn verification_mode_summary(verification: &StoreGitAuditVerification) -> String {
    match verification.mode {
        StoreGitAuditVerificationMode::BranchTipRecipients => {
            gettext("Current branch-tip recipients")
        }
        StoreGitAuditVerificationMode::CommitHistoryRecipients => {
            gettext("Recipients from the audited commit")
        }
    }
}

fn changed_path_detail(change: &StoreGitAuditPathChange) -> String {
    format!("{} ({})", change.path, change.status)
}

fn audit_commit_matches_query(commit: &StoreGitAuditCommit, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }

    audit_text_matches_query(&commit.oid, query)
        || audit_text_matches_query(&commit.short_oid, query)
        || audit_text_matches_query(&commit.subject, query)
        || audit_text_matches_query(&commit.author, query)
        || audit_text_matches_query(&commit.authored_at, query)
        || audit_text_matches_query(&commit.committer, query)
        || audit_text_matches_query(&commit.committed_at, query)
        || audit_text_matches_query(&commit.message, query)
        || audit_text_matches_query(&verification_summary(&commit.verification), query)
        || audit_text_matches_query(&verification_mode_summary(&commit.verification), query)
        || commit
            .changed_paths
            .iter()
            .any(|change| audit_text_matches_query(&changed_path_detail(change), query))
}

fn trimmed_multiline_text(text: &str) -> String {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        gettext("Empty")
    } else {
        trimmed.to_string()
    }
}

fn clear_box_children(container: &GtkBox) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn bind_audit_branch_row(runtime: &AuditBranchState, row: &ExpanderRow) {
    *runtime.current_row.borrow_mut() = Some(row.downgrade());
    runtime.rows.borrow_mut().clear();
}

fn audit_branch_row_is_current(row: &ExpanderRow, runtime: &AuditBranchState) -> bool {
    runtime
        .current_row
        .borrow()
        .as_ref()
        .and_then(WeakRef::upgrade)
        .is_some_and(|current| current == row.clone())
}

fn add_audit_branch_row_child<W>(row: &ExpanderRow, runtime: &AuditBranchState, child: &W)
where
    W: IsA<Widget> + Clone,
{
    if !audit_branch_row_is_current(row, runtime) {
        return;
    }

    row.add_row(child);
    runtime
        .rows
        .borrow_mut()
        .push(child.clone().upcast::<Widget>());
}

fn clear_audit_branch_row_contents(row: &ExpanderRow, runtime: &AuditBranchState) {
    if !audit_branch_row_is_current(row, runtime) {
        return;
    }

    for child in runtime.rows.borrow_mut().drain(..) {
        row.remove(&child);
    }
}

fn build_info_row(title: &str, subtitle: &str) -> ActionRow {
    build_action_row(title, &localized_text(subtitle), false)
}

fn build_info_text_row(title: &str, subtitle: &str) -> ActionRow {
    build_action_row(title, subtitle, false)
}

fn build_loading_branch_row() -> ActionRow {
    let row = build_info_row(AUDIT_LOADING_COMMITS_TITLE, AUDIT_LOADING_COMMITS_SUBTITLE);
    let spinner = Spinner::builder().spinning(true).build();
    row.add_suffix(&spinner);
    row
}

fn build_action_row(title: &str, subtitle: &str, activatable: bool) -> ActionRow {
    let row = ActionRow::builder()
        .title(localized_text(title))
        .subtitle(gtk_safe_text(subtitle))
        .use_markup(false)
        .build();
    row.set_use_markup(false);
    row.set_activatable(activatable);
    row.set_sensitive(activatable);
    row
}

fn build_commit_details_widget(commit: &StoreGitAuditCommit) -> GtkBox {
    let details = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(18)
        .margin_top(8)
        .margin_bottom(14)
        .margin_start(14)
        .margin_end(14)
        .hexpand(true)
        .build();

    let metadata = Grid::builder()
        .column_spacing(18)
        .row_spacing(10)
        .hexpand(true)
        .build();
    metadata.add_css_class("audit-commit-details");

    append_commit_detail_grid_row(&metadata, 0, "Commit", &commit.oid, true);
    append_commit_detail_grid_row(&metadata, 1, "Author", &commit.author, false);
    append_commit_detail_grid_row(&metadata, 2, "Authored", &commit.authored_at, true);
    append_commit_detail_grid_row(&metadata, 3, "Committer", &commit.committer, false);
    append_commit_detail_grid_row(&metadata, 4, "Committed", &commit.committed_at, true);
    append_commit_detail_grid_row(
        &metadata,
        5,
        "Verification",
        &verification_state_summary(&commit.verification),
        false,
    );
    if let Some(reason) = commit.verification.reason {
        append_commit_detail_grid_row(
            &metadata,
            6,
            "Reason",
            audit_unverified_reason_message(reason),
            false,
        );
    }
    append_commit_detail_grid_row(
        &metadata,
        7,
        "Recipient source",
        &verification_mode_summary(&commit.verification),
        false,
    );
    if commit.verification.used_commit_history_fallback {
        append_commit_detail_grid_row(
            &metadata,
            8,
            "Historical fallback",
            "Enabled for this result",
            false,
        );
    }
    details.append(&metadata);

    details.append(&build_commit_detail_section(
        "Message",
        &trimmed_multiline_text(&commit.message),
        false,
    ));

    let changed_paths = if commit.changed_paths.is_empty() {
        "No changed paths".to_string()
    } else {
        commit
            .changed_paths
            .iter()
            .map(changed_path_detail)
            .collect::<Vec<_>>()
            .join("\n")
    };
    details.append(&build_commit_detail_section(
        "Changed paths",
        &changed_paths,
        true,
    ));

    details
}

fn append_commit_detail_grid_row(grid: &Grid, row: i32, title: &str, value: &str, monospace: bool) {
    let title_label = Label::new(None);
    title_label.set_use_markup(false);
    title_label.set_text(&localized_text(title));
    title_label.set_halign(Align::Start);
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.add_css_class("caption");
    title_label.add_css_class("dim-label");

    let value_label = Label::new(None);
    value_label.set_use_markup(false);
    value_label.set_text(&gtk_safe_text(value));
    value_label.set_halign(Align::Start);
    value_label.set_xalign(0.0);
    value_label.set_wrap(true);
    value_label.set_selectable(true);
    value_label.set_hexpand(true);
    if monospace {
        value_label.add_css_class("monospace");
    }

    grid.attach(&title_label, 0, row, 1, 1);
    grid.attach(&value_label, 1, row, 1, 1);
}

fn build_commit_detail_section(title: &str, value: &str, monospace: bool) -> GtkBox {
    let section = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(6)
        .hexpand(true)
        .build();

    let title_label = Label::new(None);
    title_label.set_use_markup(false);
    title_label.set_text(&localized_text(title));
    title_label.set_halign(Align::Start);
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.add_css_class("caption");
    title_label.add_css_class("dim-label");

    let value_label = Label::new(None);
    value_label.set_use_markup(false);
    value_label.set_text(&gtk_safe_text(value));
    value_label.set_halign(Align::Start);
    value_label.set_xalign(0.0);
    value_label.set_wrap(true);
    value_label.set_selectable(true);
    value_label.set_hexpand(true);
    if monospace {
        value_label.add_css_class("monospace");
    }

    section.append(&title_label);
    section.append(&value_label);
    section
}

fn build_audit_filter_toggle(label: &str, active: bool) -> CheckButton {
    CheckButton::builder()
        .label(gtk_safe_text(label))
        .active(active)
        .hexpand(true)
        .build()
}

fn localized_text(text: &str) -> String {
    gtk_safe_text(&gettext(text))
}

fn gtk_safe_text(text: &str) -> String {
    text.replace('\0', "\u{FFFD}")
}

#[cfg(test)]
mod tests {
    use super::{
        audit_available_branch_names, audit_available_store_ids,
        audit_branch_context_matches_query, audit_commit_matches_query, audit_search_query,
        branch_expansion_needs_initial_load, commit_summary_subtitle, gtk_safe_text,
        localized_text, reconciled_filter_selection, verification_state_summary,
        verification_summary, AuditBranchState,
    };
    use crate::support::git::{
        StoreGitAuditBranchRef, StoreGitAuditCatalog, StoreGitAuditCommit, StoreGitAuditStore,
        StoreGitAuditUnverifiedReason, StoreGitAuditVerification, StoreGitAuditVerificationMode,
        StoreGitAuditVerificationState,
    };
    use std::collections::BTreeSet;

    fn test_catalog() -> StoreGitAuditCatalog {
        StoreGitAuditCatalog {
            stores: vec![
                StoreGitAuditStore {
                    store_root: "/stores/work".to_string(),
                    branches: vec![
                        StoreGitAuditBranchRef {
                            full_ref: "refs/heads/main".to_string(),
                            name: "main".to_string(),
                            remote: false,
                        },
                        StoreGitAuditBranchRef {
                            full_ref: "refs/remotes/origin/main".to_string(),
                            name: "origin/main".to_string(),
                            remote: true,
                        },
                    ],
                },
                StoreGitAuditStore {
                    store_root: "/stores/home".to_string(),
                    branches: vec![StoreGitAuditBranchRef {
                        full_ref: "refs/heads/main".to_string(),
                        name: "main".to_string(),
                        remote: false,
                    }],
                },
            ],
        }
    }

    #[test]
    fn filter_selection_defaults_to_all_available_values() {
        let available = BTreeSet::from(["main".to_string(), "origin/main".to_string()]);

        assert_eq!(reconciled_filter_selection(None, &available), available);
    }

    #[test]
    fn filter_selection_preserves_explicit_empty_choice() {
        let available = BTreeSet::from(["main".to_string(), "origin/main".to_string()]);
        let selected = BTreeSet::new();

        assert_eq!(
            reconciled_filter_selection(Some(&selected), &available),
            BTreeSet::new()
        );
    }

    #[test]
    fn available_filters_cover_all_stores_and_branch_names() {
        let catalog = test_catalog();

        assert_eq!(
            audit_available_store_ids(&catalog),
            BTreeSet::from(["/stores/home".to_string(), "/stores/work".to_string(),])
        );
        assert_eq!(
            audit_available_branch_names(&catalog),
            BTreeSet::from(["main".to_string(), "origin/main".to_string()])
        );
    }

    #[test]
    fn gtk_safe_text_replaces_interior_nuls() {
        assert_eq!(gtk_safe_text("main\0branch"), "main\u{FFFD}branch");
    }

    #[test]
    fn localized_text_preserves_plain_static_strings() {
        let key = "audit-ui-localized-text-regression-key";
        assert_eq!(localized_text(key), key);
    }

    #[test]
    fn branch_expansion_only_loads_the_first_time() {
        let runtime = AuditBranchState::default();

        assert!(branch_expansion_needs_initial_load(&runtime, true));

        runtime.loaded_once.set(true);
        assert!(!branch_expansion_needs_initial_load(&runtime, true));

        runtime.loaded_once.set(false);
        runtime.loading.set(true);
        assert!(!branch_expansion_needs_initial_load(&runtime, true));
        assert!(!branch_expansion_needs_initial_load(&runtime, false));
    }

    #[test]
    fn commit_summary_subtitle_omits_author_identity() {
        let commit = StoreGitAuditCommit {
            oid: "0123456789abcdef".to_string(),
            short_oid: "0123456".to_string(),
            subject: "Fix audit row rendering".to_string(),
            author: "nick <noobping@users.noreply.github.com>".to_string(),
            authored_at: "2026-04-07T01:00:00+02:00".to_string(),
            committer: "nick <noobping@users.noreply.github.com>".to_string(),
            committed_at: "2026-04-07T01:01:00+02:00".to_string(),
            message: "message".to_string(),
            changed_paths: Vec::new(),
            verification: StoreGitAuditVerification {
                state: StoreGitAuditVerificationState::Unverified,
                mode: StoreGitAuditVerificationMode::BranchTipRecipients,
                used_commit_history_fallback: false,
                reason: None,
                signer_fingerprint: None,
                signer_label: None,
            },
        };

        let subtitle = commit_summary_subtitle(&commit);

        assert!(!subtitle.contains(&commit.author));
        assert!(subtitle.contains(&commit.short_oid));
        assert!(subtitle.contains(&commit.committed_at));
    }

    #[test]
    fn verification_state_summary_omits_reason_details() {
        let verification = StoreGitAuditVerification {
            state: StoreGitAuditVerificationState::Unverified,
            mode: StoreGitAuditVerificationMode::BranchTipRecipients,
            used_commit_history_fallback: false,
            reason: Some(StoreGitAuditUnverifiedReason::NoSignature),
            signer_fingerprint: None,
            signer_label: None,
        };

        assert_eq!(verification_state_summary(&verification), "Unverified");
        assert_eq!(
            verification_summary(&verification),
            "Unverified: No signature"
        );
    }

    #[test]
    fn audit_search_query_trims_and_lowercases() {
        assert_eq!(audit_search_query("  HeLLo World  "), "hello world");
    }

    #[test]
    fn audit_branch_context_query_matches_store_and_branch_text() {
        assert!(audit_branch_context_matches_query(
            "origin/main",
            "/stores/work",
            "Work",
            "origin/main",
            "refs/remotes/origin/main"
        ));
        assert!(audit_branch_context_matches_query(
            "stores/work",
            "/stores/work",
            "Work",
            "main",
            "refs/heads/main"
        ));
    }

    #[test]
    fn audit_commit_query_matches_commit_details() {
        let commit = StoreGitAuditCommit {
            oid: "0123456789abcdef".to_string(),
            short_oid: "0123456".to_string(),
            subject: "Fix audit search".to_string(),
            author: "nick <noobping@users.noreply.github.com>".to_string(),
            authored_at: "2026-04-07T01:00:00+02:00".to_string(),
            committer: "nick <noobping@users.noreply.github.com>".to_string(),
            committed_at: "2026-04-07T01:01:00+02:00".to_string(),
            message: "Include changed path filtering".to_string(),
            changed_paths: vec![crate::support::git::StoreGitAuditPathChange {
                status: "M".to_string(),
                path: "src/window/tools/audit.rs".to_string(),
            }],
            verification: StoreGitAuditVerification {
                state: StoreGitAuditVerificationState::Unverified,
                mode: StoreGitAuditVerificationMode::BranchTipRecipients,
                used_commit_history_fallback: false,
                reason: Some(StoreGitAuditUnverifiedReason::NoSignature),
                signer_fingerprint: None,
                signer_label: None,
            },
        };

        assert!(audit_commit_matches_query(&commit, "audit search"));
        assert!(audit_commit_matches_query(&commit, "no signature"));
        assert!(audit_commit_matches_query(
            &commit,
            "src/window/tools/audit.rs"
        ));
        assert!(!audit_commit_matches_query(&commit, "totally missing"));
    }
}
