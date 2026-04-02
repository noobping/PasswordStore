use crate::i18n::gettext;
use crate::logging::log_error;
use crate::store::recipients_page::{StoreRecipientsMode, StoreRecipientsPageState};
use crate::support::actions::activate_widget_action;
use crate::support::background::spawn_result_task_with_finalizer;
use crate::support::git::{
    add_store_git_remote, list_store_git_remotes, remove_store_git_remote, rename_store_git_remote,
    set_store_git_remote_url, store_git_repository_status, sync_store_repository, StoreGitHead,
    StoreGitRepositoryStatus,
};
use crate::support::runtime::{has_host_permission, supports_host_command_features};
use crate::support::ui::{
    append_action_row_with_button, append_info_row, clear_list_box, dialog_content_shell,
    dim_label_icon, flat_icon_button_with_tooltip, navigation_stack_contains_page,
    push_navigation_page_if_needed, reveal_navigation_page, visible_navigation_page_is,
};
use crate::window::append_optional_host_access_row;
use crate::window::navigation::{show_secondary_page_chrome, HasWindowChrome, APP_WINDOW_TITLE};
use adw::gio::{prelude::*, SimpleAction};
use adw::gtk::{Align, Box as GtkBox, Button, Image, Label, ListBox, Orientation};
use adw::prelude::*;
use adw::{
    ActionRow, ApplicationWindow, Dialog, EntryRow, NavigationPage, NavigationView,
    PreferencesGroup, PreferencesPage, StatusPage, Toast, ToastOverlay, WindowTitle,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone)]
pub struct StoreGitPageState {
    pub window: ApplicationWindow,
    pub nav: NavigationView,
    pub page: NavigationPage,
    pub remotes_list: ListBox,
    pub actions_list: ListBox,
    pub status_list: ListBox,
    pub overlay: ToastOverlay,
    pub back: Button,
    pub add: Button,
    pub find: Button,
    pub git: Button,
    pub store: Button,
    pub save: Button,
    pub raw: Button,
    pub win: WindowTitle,
    pub busy_page: NavigationPage,
    pub busy_status: StatusPage,
    pub current_store: Rc<RefCell<Option<String>>>,
}

impl StoreGitPageState {
    pub fn current_store(&self) -> Option<String> {
        self.current_store.borrow().clone()
    }
}

fn set_window_action_enabled(window: &ApplicationWindow, name: &str, enabled: bool) {
    let Some(action) = window.lookup_action(name) else {
        return;
    };
    let Ok(action) = action.downcast::<SimpleAction>() else {
        return;
    };
    action.set_enabled(enabled);
}

fn set_git_busy_actions_enabled(window: &ApplicationWindow, enabled: bool) {
    for action in [
        "context-save",
        "context-undo",
        "open-new-password",
        "toggle-find",
        "open-git",
        "open-raw-pass-file",
        "git-clone",
        "save-password",
        "save-store-recipients",
        "synchronize",
        "open-preferences",
        "open-tools",
        "toggle-hidden-and-duplicates",
    ] {
        set_window_action_enabled(window, action, enabled);
    }
}

fn begin_git_operation(state: &StoreGitPageState, title: &str) {
    set_git_busy_actions_enabled(&state.window, false);
    let chrome = state.window_chrome();
    show_secondary_page_chrome(&chrome, "Working", title, false);
    state.busy_status.set_title(&gettext(title));
    push_navigation_page_if_needed(&state.nav, &state.busy_page);
}

fn finish_git_operation(state: &StoreGitPageState) {
    set_git_busy_actions_enabled(&state.window, true);

    let current_page = state.nav.visible_page();
    let busy_visible = visible_navigation_page_is(&state.nav, &state.busy_page);
    let busy_in_stack = navigation_stack_contains_page(&state.nav, &state.busy_page);

    if busy_visible {
        state.nav.pop();
    } else if busy_in_stack {
        if let Some(current_page) = current_page.filter(|page| page != &state.busy_page) {
            let _ = state.nav.pop_to_page(&state.busy_page);
            let _ = state.nav.pop();
            state.nav.push(&current_page);
        }
    }

    if visible_navigation_page_is(&state.nav, &state.page) {
        sync_store_git_page_header(state);
    }
}

fn append_status_row(list: &ListBox, title: &str, subtitle: &str, icon_name: &str) {
    let title = gettext(title);
    let row = ActionRow::builder()
        .title(&title)
        .subtitle(subtitle)
        .build();
    row.set_activatable(false);
    row.add_prefix(&dim_label_icon(icon_name));
    list.append(&row);
}

fn translated_branch_message(template: &str, branch: &str) -> String {
    gettext(template).replace("{branch}", branch)
}

fn translated_count_message(template: &str, count: usize) -> String {
    gettext(template).replace("{count}", &count.to_string())
}

fn append_translated_action_row_with_button(
    list: &ListBox,
    title: &str,
    subtitle: &str,
    icon_name: &str,
    action: impl Fn() + 'static,
) -> ActionRow {
    let row = ActionRow::builder().title(title).subtitle(subtitle).build();
    row.set_activatable(true);

    let icon = Image::from_icon_name(icon_name);
    row.add_suffix(&icon);
    list.append(&row);

    let action = Rc::new(action);
    let row_action = action.clone();
    row.connect_activated(move |_| row_action());

    row
}

fn repository_subtitle(status: &StoreGitRepositoryStatus) -> String {
    if !status.has_repository {
        return gettext("No Git repository yet. Add a remote to initialize one.");
    }
    if status.dirty && status.has_outgoing_commits && status.has_incoming_commits {
        return gettext(
            "Repository found. Local changes must be committed or discarded before sync, and local and remote commits are waiting to sync.",
        );
    }
    if status.dirty && status.has_outgoing_commits {
        return gettext(
            "Repository found. Local changes must be committed or discarded before sync, and local commits are waiting to sync.",
        );
    }
    if status.dirty && status.has_incoming_commits {
        return gettext(
            "Repository found. Local changes must be committed or discarded before sync, and remote commits are waiting to sync.",
        );
    }
    if status.dirty {
        return gettext(
            "Repository found. Local changes must be committed or discarded before sync.",
        );
    }

    match &status.head {
        StoreGitHead::Branch(_) if status.has_outgoing_commits && status.has_incoming_commits => {
            gettext("Repository found. Local and remote commits are waiting to sync.")
        }
        StoreGitHead::Branch(_) if status.has_outgoing_commits => {
            gettext("Repository found. Local commits are waiting to sync.")
        }
        StoreGitHead::Branch(_) if status.has_incoming_commits => {
            gettext("Repository found. Remote commits are waiting to sync.")
        }
        StoreGitHead::Branch(_) => gettext("Repository found and ready for remote management."),
        StoreGitHead::UnbornBranch(branch) => translated_branch_message(
            "Repository found. Create the first commit on '{branch}' before syncing.",
            branch,
        ),
        StoreGitHead::Detached => gettext("Repository found. Check out a branch before syncing."),
    }
}

fn branch_subtitle(status: &StoreGitRepositoryStatus) -> String {
    if !status.has_repository {
        return gettext("No branch yet.");
    }

    match &status.head {
        StoreGitHead::Branch(branch) => branch.clone(),
        StoreGitHead::UnbornBranch(branch) => {
            translated_branch_message("{branch} (no commits yet)", branch)
        }
        StoreGitHead::Detached => gettext("Detached HEAD"),
    }
}

fn remote_count_subtitle(status: &StoreGitRepositoryStatus) -> String {
    if status.has_outgoing_commits && status.has_incoming_commits {
        return gettext("Local and remote commits are waiting to sync.");
    }
    if status.has_outgoing_commits {
        return gettext("Local commits are waiting to sync.");
    }
    if status.has_incoming_commits {
        return gettext("Remote commits are waiting to sync.");
    }

    match status.remotes.len() {
        0 => gettext("No remotes configured."),
        1 => gettext("1 remote configured."),
        count => translated_count_message("{count} remotes configured.", count),
    }
}

fn sync_allowed(status: &StoreGitRepositoryStatus) -> bool {
    has_host_permission()
        && status.has_repository
        && !status.remotes.is_empty()
        && !status.dirty
        && matches!(status.head, StoreGitHead::Branch(_))
}

fn sync_subtitle(status: &StoreGitRepositoryStatus) -> String {
    if !has_host_permission() {
        return gettext("Grant host access to fetch, merge, and push.");
    }
    if !status.has_repository {
        return gettext("Add a remote to initialize a Git repository first.");
    }
    if status.remotes.is_empty() {
        return gettext("Add at least one remote before syncing.");
    }
    if status.dirty && status.has_outgoing_commits && status.has_incoming_commits {
        return gettext(
            "Commit or discard local changes before syncing. Local and remote commits are also waiting to sync.",
        );
    }
    if status.dirty && status.has_outgoing_commits {
        return gettext(
            "Commit or discard local changes before syncing. Local commits are also waiting to sync.",
        );
    }
    if status.dirty && status.has_incoming_commits {
        return gettext(
            "Commit or discard local changes before syncing. Remote commits are also waiting to sync.",
        );
    }
    if status.dirty {
        return gettext("Commit or discard local changes before syncing.");
    }

    match &status.head {
        StoreGitHead::Branch(branch)
            if status.has_outgoing_commits && status.has_incoming_commits =>
        {
            translated_branch_message(
                "Local and remote commits are waiting to sync on '{branch}'.",
                branch,
            )
        }
        StoreGitHead::Branch(branch) if status.has_outgoing_commits => {
            translated_branch_message("Local commits are ready to push on '{branch}'.", branch)
        }
        StoreGitHead::Branch(branch) if status.has_incoming_commits => {
            translated_branch_message("Remote commits are ready to merge into '{branch}'.", branch)
        }
        StoreGitHead::Branch(branch) => translated_branch_message(
            "Fetch, merge, and push the current '{branch}' branch across all remotes.",
            branch,
        ),
        StoreGitHead::UnbornBranch(branch) => translated_branch_message(
            "Make an initial commit on '{branch}' before syncing.",
            branch,
        ),
        StoreGitHead::Detached => gettext("Check out a branch before syncing."),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoreGitRowState {
    subtitle: String,
    enabled: bool,
}

fn store_git_row_state(status: Result<StoreGitRepositoryStatus, String>) -> StoreGitRowState {
    match status {
        Ok(status) if sync_allowed(&status) => StoreGitRowState {
            subtitle: remote_count_subtitle(&status),
            enabled: true,
        },
        Ok(status) => StoreGitRowState {
            subtitle: sync_subtitle(&status),
            enabled: true,
        },
        Err(_) => StoreGitRowState {
            subtitle: gettext("Couldn't inspect Git remotes."),
            enabled: false,
        },
    }
}

fn store_git_row_state_for_store(store: &str) -> StoreGitRowState {
    store_git_row_state(store_git_repository_status(store))
}

fn next_available_remote_name(base: &str, existing_names: &[String]) -> String {
    if !existing_names
        .iter()
        .any(|name| name.eq_ignore_ascii_case(base))
    {
        return base.to_string();
    }

    let mut suffix = 2;
    loop {
        let candidate = format!("{base}-{suffix}");
        if !existing_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
        suffix += 1;
    }
}

fn suggested_remote_name_from_url(url: &str, existing_names: &[String]) -> Option<String> {
    (!url.trim().is_empty()).then(|| next_available_remote_name("origin", existing_names))
}

fn remote_name_exists(name: &str, existing_names: &[String]) -> bool {
    let name = name.trim();
    existing_names
        .iter()
        .any(|existing_name| existing_name.eq_ignore_ascii_case(name))
}

fn remote_url_exists(url: &str, existing_urls: &[String]) -> bool {
    let url = url.trim();
    existing_urls.iter().any(|existing_url| existing_url == url)
}

fn remote_dialog_error_message(
    name: &str,
    url: &str,
    existing_names: &[String],
    existing_urls: &[String],
) -> Option<&'static str> {
    if name.trim().is_empty() {
        return Some("Enter a remote name.");
    }
    if remote_name_exists(name, existing_names) {
        return Some("That remote name already exists.");
    }
    if url.trim().is_empty() {
        return Some("Enter a remote URL.");
    }
    if remote_url_exists(url, existing_urls) {
        return Some("That remote URL already exists.");
    }

    None
}

fn next_autofilled_remote_name(
    current_value: &str,
    previous_autofill: Option<&str>,
    suggestion: Option<String>,
) -> Option<String> {
    let current_value = current_value.trim();
    if !(current_value.is_empty() || previous_autofill == Some(current_value)) {
        return None;
    }

    Some(suggestion.unwrap_or_default())
}

struct RemoteDialogRequest<'a> {
    window: &'a ApplicationWindow,
    store: &'a str,
    title: &'a str,
    initial_name: &'a str,
    initial_url: &'a str,
    existing_names: Vec<String>,
    existing_urls: Vec<String>,
}

fn present_remote_dialog(
    request: RemoteDialogRequest<'_>,
    on_submit: impl Fn(String, String) -> Result<(), String> + 'static,
) {
    let existing_names = Rc::new(request.existing_names);
    let existing_urls = Rc::new(request.existing_urls);
    let name_row = EntryRow::new();
    name_row.set_title(&gettext("Remote name"));
    name_row.set_text(request.initial_name);
    let url_row = EntryRow::new();
    url_row.set_title(&gettext("Remote URL"));
    url_row.set_text(request.initial_url);
    url_row.set_show_apply_button(true);

    let syncing = Rc::new(Cell::new(false));
    let last_autofilled_name = Rc::new(RefCell::new(None::<String>));
    {
        let name_row = name_row.clone();
        let syncing = syncing.clone();
        let last_autofilled_name = last_autofilled_name.clone();
        let existing_names = existing_names.clone();
        url_row.connect_changed(move |row| {
            if syncing.get() {
                return;
            }

            let next_name = next_autofilled_remote_name(
                &name_row.text(),
                last_autofilled_name.borrow().as_deref(),
                suggested_remote_name_from_url(&row.text(), existing_names.as_slice()),
            );
            let Some(name) = next_name else {
                last_autofilled_name.borrow_mut().take();
                return;
            };

            let tracked_name = (!name.is_empty()).then_some(name.clone());
            syncing.set(true);
            name_row.set_text(&name);
            syncing.set(false);
            last_autofilled_name.replace(tracked_name);
        });
    }

    let group = PreferencesGroup::builder().build();
    group.add(&name_row);
    group.add(&url_row);

    let page = PreferencesPage::new();
    page.add(&group);

    let error_label = Label::new(None);
    error_label.set_halign(Align::Start);
    error_label.set_wrap(true);
    error_label.add_css_class("error");
    error_label.add_css_class("caption");
    error_label.set_margin_top(6);
    error_label.set_margin_start(18);
    error_label.set_margin_end(18);
    error_label.set_margin_bottom(18);
    error_label.set_visible(false);

    let content = GtkBox::new(Orientation::Vertical, 0);
    content.append(&page);
    content.append(&error_label);

    let dialog = Dialog::builder()
        .title(gettext(request.title))
        .content_height(280)
        .content_width(800)
        .follows_content_size(true)
        .child(&dialog_content_shell(
            request.title,
            Some(request.store),
            &content,
        ))
        .build();

    let dialog_for_submit = dialog.clone();
    let name_row_for_submit = name_row.clone();
    let existing_names_for_submit = existing_names.clone();
    let existing_urls_for_submit = existing_urls.clone();
    let error_label_for_submit = error_label.clone();
    url_row.connect_apply(move |row| {
        let name = name_row_for_submit.text().trim().to_string();
        let url = row.text().trim().to_string();
        if let Some(message) = remote_dialog_error_message(
            &name,
            &url,
            existing_names_for_submit.as_slice(),
            existing_urls_for_submit.as_slice(),
        ) {
            error_label_for_submit.set_label(&gettext(message));
            error_label_for_submit.set_visible(true);
            return;
        }
        error_label_for_submit.set_visible(false);

        match on_submit(name, url) {
            Ok(()) => {
                dialog_for_submit.close();
            }
            Err(err) => {
                log_error(format!("Git remote dialog failed: {err}"));
                error_label_for_submit.set_label(&gettext("Couldn't save that remote."));
                error_label_for_submit.set_visible(true);
            }
        }
    });

    {
        let error_label = error_label.clone();
        name_row.connect_changed(move |_| {
            error_label.set_visible(false);
        });
    }
    {
        let error_label = error_label.clone();
        url_row.connect_changed(move |_| {
            error_label.set_visible(false);
        });
    }

    dialog.present(Some(request.window));
}

fn update_store_git_remote(
    store: &str,
    current_name: &str,
    next_name: &str,
    next_url: &str,
) -> Result<(), String> {
    let name_changed = current_name != next_name;
    let current_url = list_store_git_remotes(store)?
        .into_iter()
        .find(|remote| remote.name == current_name)
        .map(|remote| remote.url)
        .unwrap_or_default();
    let url_changed = current_url != next_url;

    if !name_changed && !url_changed {
        return Ok(());
    }
    if name_changed {
        rename_store_git_remote(store, current_name, next_name)?;
    }
    if url_changed {
        if let Err(err) = set_store_git_remote_url(store, next_name, next_url) {
            if name_changed {
                let _ = rename_store_git_remote(store, next_name, current_name);
            }
            return Err(err);
        }
    }

    Ok(())
}

fn sync_related_views(state: &StoreGitPageState) {
    activate_widget_action(&state.window, "win.reload-store-recipients-list");
    activate_widget_action(&state.window, "win.reload-password-list");
}

fn append_remote_row(
    state: &StoreGitPageState,
    store: &str,
    name: &str,
    url: &str,
    existing_names: Vec<String>,
    existing_urls: Vec<String>,
) {
    let row = ActionRow::builder().title(name).subtitle(url).build();
    row.set_activatable(false);
    row.add_prefix(&dim_label_icon("git-symbolic"));

    let edit_button = flat_icon_button_with_tooltip("edit-symbolic", "Edit remote");
    row.add_suffix(&edit_button);

    let delete_button = flat_icon_button_with_tooltip("user-trash-symbolic", "Remove remote");
    row.add_suffix(&delete_button);

    state.remotes_list.append(&row);

    let store_for_edit = store.to_string();
    let state_for_edit = state.clone();
    let current_name = name.to_string();
    let current_url = url.to_string();
    edit_button.connect_clicked(move |_| {
        let state_for_submit = state_for_edit.clone();
        let store_for_submit = store_for_edit.clone();
        let current_name_for_submit = current_name.clone();
        present_remote_dialog(
            RemoteDialogRequest {
                window: &state_for_edit.window,
                store: &store_for_edit,
                title: "Edit remote",
                initial_name: &current_name,
                initial_url: &current_url,
                existing_names: existing_names.clone(),
                existing_urls: existing_urls.clone(),
            },
            move |next_name, next_url| {
                update_store_git_remote(
                    &store_for_submit,
                    &current_name_for_submit,
                    &next_name,
                    &next_url,
                )?;
                rebuild_store_git_page(&state_for_submit);
                sync_related_views(&state_for_submit);
                state_for_submit
                    .overlay
                    .add_toast(Toast::new(&gettext("Remote updated.")));
                Ok(())
            },
        );
    });

    let store_for_delete = store.to_string();
    let state_for_delete = state.clone();
    let name_for_delete = name.to_string();
    delete_button.connect_clicked(move |_| {
        match remove_store_git_remote(&store_for_delete, &name_for_delete) {
            Ok(()) => {
                rebuild_store_git_page(&state_for_delete);
                sync_related_views(&state_for_delete);
                state_for_delete
                    .overlay
                    .add_toast(Toast::new(&gettext("Remote removed.")));
            }
            Err(err) => {
                log_error(format!(
                    "Failed to remove Git remote '{name_for_delete}' from '{store_for_delete}': {err}"
                ));
                state_for_delete
                    .overlay
                    .add_toast(Toast::new(&gettext("Couldn't remove that remote.")));
            }
        }
    });
}

pub fn rebuild_store_git_page(state: &StoreGitPageState) {
    clear_list_box(&state.remotes_list);
    clear_list_box(&state.actions_list);
    clear_list_box(&state.status_list);

    let Some(store) = state.current_store() else {
        append_info_row(
            &state.remotes_list,
            "No password store",
            "Open a store first.",
        );
        return;
    };

    match store_git_repository_status(&store) {
        Ok(status) => {
            let existing_remote_names = status
                .remotes
                .iter()
                .map(|remote| remote.name.clone())
                .collect::<Vec<_>>();
            let existing_remote_urls = status
                .remotes
                .iter()
                .map(|remote| remote.url.clone())
                .collect::<Vec<_>>();
            if status.remotes.is_empty() {
                append_status_row(
                    &state.remotes_list,
                    "Repository",
                    &repository_subtitle(&status),
                    "git-symbolic",
                );
            } else {
                for remote in &status.remotes {
                    append_remote_row(
                        state,
                        &store,
                        &remote.name,
                        &remote.url,
                        existing_remote_names
                            .iter()
                            .filter(|existing_name| {
                                !existing_name.eq_ignore_ascii_case(&remote.name)
                            })
                            .cloned()
                            .collect(),
                        status
                            .remotes
                            .iter()
                            .filter(|existing_remote| existing_remote.name != remote.name)
                            .map(|existing_remote| existing_remote.url.clone())
                            .collect(),
                    );
                }
            }

            let add_state = state.clone();
            let store_for_add = store.clone();
            let add_row = append_action_row_with_button(
                &state.actions_list,
                "Add remote",
                "Add a Git remote for this store.",
                "list-add-symbolic",
                move || {
                    let state_for_submit = add_state.clone();
                    let store_for_submit = store_for_add.clone();
                    present_remote_dialog(
                        RemoteDialogRequest {
                            window: &add_state.window,
                            store: &store_for_add,
                            title: "Add remote",
                            initial_name: "",
                            initial_url: "",
                            existing_names: existing_remote_names.clone(),
                            existing_urls: existing_remote_urls.clone(),
                        },
                        move |name, url| {
                            add_store_git_remote(&store_for_submit, &name, &url)?;
                            rebuild_store_git_page(&state_for_submit);
                            sync_related_views(&state_for_submit);
                            state_for_submit
                                .overlay
                                .add_toast(Toast::new(&gettext("Remote added.")));
                            Ok(())
                        },
                    );
                },
            );
            add_row.set_sensitive(has_host_permission());
            add_row.set_activatable(has_host_permission());

            append_optional_host_access_row(&state.status_list, &state.overlay);

            let sync_state = state.clone();
            let store_for_sync = store.clone();
            let sync_row = append_translated_action_row_with_button(
                &state.status_list,
                &gettext("Sync now"),
                &sync_subtitle(&status),
                "view-refresh-symbolic",
                move || {
                    let current_status = match store_git_repository_status(&store_for_sync) {
                        Ok(status) => status,
                        Err(err) => {
                            log_error(format!(
                                "Failed to inspect Git state before syncing '{store_for_sync}': {err}"
                            ));
                            sync_state
                                .overlay
                                .add_toast(Toast::new(&gettext("Couldn't inspect Git remotes.")));
                            rebuild_store_git_page(&sync_state);
                            return;
                        }
                    };
                    if !sync_allowed(&current_status) {
                        sync_state
                            .overlay
                            .add_toast(Toast::new(&sync_subtitle(&current_status)));
                        rebuild_store_git_page(&sync_state);
                        return;
                    }

                    begin_git_operation(&sync_state, "Syncing store");

                    let state_for_finalize = sync_state.clone();
                    let state_for_result = sync_state.clone();
                    let state_for_disconnect = sync_state.clone();
                    let store_for_worker = store_for_sync.clone();
                    let store_for_result = store_for_sync.clone();
                    spawn_result_task_with_finalizer(
                        move || sync_store_repository(&store_for_worker),
                        move || {
                            finish_git_operation(&state_for_finalize);
                            rebuild_store_git_page(&state_for_finalize);
                            sync_related_views(&state_for_finalize);
                        },
                        move |result| match result {
                            Ok(()) => {
                                state_for_result
                                    .overlay
                                    .add_toast(Toast::new(&gettext("Store synced.")));
                            }
                            Err(err) => {
                                log_error(format!(
                                    "Failed to sync password store '{store_for_result}': {err}"
                                ));
                                state_for_result
                                    .overlay
                                    .add_toast(Toast::new(&gettext("Couldn't sync store.")));
                            }
                        },
                        move || {
                            state_for_disconnect.overlay.add_toast(Toast::new(&gettext(
                                "Store sync stopped unexpectedly.",
                            )));
                        },
                    );
                },
            );
            sync_row.set_sensitive(sync_allowed(&status));
            sync_row.set_activatable(sync_allowed(&status));

            append_status_row(
                &state.status_list,
                "Branch",
                &branch_subtitle(&status),
                "object-select-symbolic",
            );
        }
        Err(err) => {
            log_error(format!("Failed to inspect Git state for '{store}': {err}"));
            append_info_row(
                &state.remotes_list,
                "Couldn't inspect Git state",
                "Check the logs for details.",
            );
        }
    }
}

pub fn sync_store_git_page_header(state: &StoreGitPageState) {
    let Some(store) = state.current_store() else {
        state.page.set_title(&gettext("Git remotes"));
        let chrome = state.window_chrome();
        show_secondary_page_chrome(&chrome, "Git remotes", APP_WINDOW_TITLE, false);
        return;
    };

    let chrome = state.window_chrome();
    show_secondary_page_chrome(&chrome, "Git remotes", &store, false);
    state.page.set_title(&gettext("Git remotes"));
}

pub fn show_store_git_page(state: &StoreGitPageState, store: impl Into<String>) {
    if !supports_host_command_features() {
        return;
    }

    *state.current_store.borrow_mut() = Some(store.into());
    rebuild_store_git_page(state);
    sync_store_git_page_header(state);
    let _ = reveal_navigation_page(&state.nav, &state.page);
}

pub fn rebuild_store_recipients_git_row(state: &StoreRecipientsPageState) {
    clear_list_box(&state.platform.git_list);
    if !supports_host_command_features() {
        state.platform.git_group.set_visible(false);
        return;
    }
    let Some(request) = state.current_request() else {
        state.platform.git_group.set_visible(false);
        return;
    };

    let visible = matches!(
        request.mode,
        StoreRecipientsMode::Edit | StoreRecipientsMode::Create
    );
    state.platform.git_group.set_visible(visible);
    if !visible {
        return;
    }

    let store = request.store.clone();
    let row_state = store_git_row_state_for_store(&store);
    let git_page = state.platform.store_git_page.clone();
    let row = append_translated_action_row_with_button(
        &state.platform.git_list,
        &gettext("Git remotes"),
        &row_state.subtitle,
        "go-next-symbolic",
        move || {
            show_store_git_page(&git_page, store.clone());
        },
    );
    row.add_prefix(&dim_label_icon("git-symbolic"));
    row.set_sensitive(row_state.enabled);
    row.set_activatable(row_state.enabled);
}

#[cfg(test)]
mod tests {
    use super::{
        next_autofilled_remote_name, next_available_remote_name, remote_count_subtitle,
        remote_dialog_error_message, remote_name_exists, remote_url_exists, store_git_row_state,
        suggested_remote_name_from_url, StoreGitHead, StoreGitRepositoryStatus,
    };
    use crate::i18n::gettext;
    use crate::support::git::GitRemote;

    #[test]
    fn git_row_is_disabled_when_git_state_cannot_be_inspected() {
        let state = store_git_row_state(Err("boom".to_string()));

        assert_eq!(state.subtitle, gettext("Couldn't inspect Git remotes."));
        assert!(!state.enabled);
    }

    #[test]
    fn git_row_stays_enabled_when_git_state_is_available() {
        let status = StoreGitRepositoryStatus {
            has_repository: true,
            head: StoreGitHead::Branch("main".to_string()),
            dirty: false,
            has_outgoing_commits: false,
            has_incoming_commits: false,
            remotes: vec![GitRemote {
                name: "origin".to_string(),
                url: "ssh://example.test/repo.git".to_string(),
            }],
        };

        let state = store_git_row_state(Ok(status.clone()));

        assert_eq!(state.subtitle, remote_count_subtitle(&status));
        assert!(state.enabled);
    }

    #[test]
    fn remote_name_autofill_suggests_origin_for_non_empty_urls() {
        assert_eq!(
            suggested_remote_name_from_url("ssh://git@example.test/repo.git", &[]),
            Some("origin".to_string())
        );
        assert_eq!(suggested_remote_name_from_url("", &[]), None);
    }

    #[test]
    fn remote_name_autofill_only_updates_empty_or_last_autofilled_values() {
        assert_eq!(
            next_autofilled_remote_name(
                "",
                None,
                suggested_remote_name_from_url("ssh://git@example.test/repo.git", &[]),
            ),
            Some("origin".to_string())
        );
        assert_eq!(
            next_autofilled_remote_name(
                "origin",
                Some("origin"),
                suggested_remote_name_from_url("ssh://git@example.test/other.git", &[]),
            ),
            Some("origin".to_string())
        );
        assert_eq!(
            next_autofilled_remote_name(
                "upstream",
                Some("origin"),
                suggested_remote_name_from_url("ssh://git@example.test/repo.git", &[]),
            ),
            None
        );
    }

    #[test]
    fn remote_name_autofill_uses_the_next_available_origin_name() {
        let existing = vec!["origin".to_string(), "origin-2".to_string()];

        assert_eq!(next_available_remote_name("origin", &existing), "origin-3");
        assert_eq!(
            suggested_remote_name_from_url("ssh://git@example.test/repo.git", &existing),
            Some("origin-3".to_string())
        );
    }

    #[test]
    fn remote_name_validation_rejects_existing_names_case_insensitively() {
        let existing = vec!["origin".to_string(), "upstream".to_string()];

        assert!(remote_name_exists("origin", &existing));
        assert!(remote_name_exists("ORIGIN", &existing));
        assert!(!remote_name_exists("origin-2", &existing));
    }

    #[test]
    fn remote_url_validation_rejects_existing_urls() {
        let existing = vec![
            "ssh://git@example.test/repo.git".to_string(),
            "https://example.test/repo.git".to_string(),
        ];

        assert!(remote_url_exists(
            "ssh://git@example.test/repo.git",
            &existing
        ));
        assert!(remote_url_exists(
            " ssh://git@example.test/repo.git ",
            &existing
        ));
        assert!(!remote_url_exists(
            "ssh://git@example.test/other.git",
            &existing
        ));
    }

    #[test]
    fn remote_dialog_validation_reports_the_first_relevant_error() {
        let existing_names = vec!["origin".to_string()];
        let existing_urls = vec!["ssh://git@example.test/repo.git".to_string()];

        assert_eq!(
            remote_dialog_error_message("", "", &existing_names, &existing_urls),
            Some("Enter a remote name.")
        );
        assert_eq!(
            remote_dialog_error_message("origin", "", &existing_names, &existing_urls),
            Some("That remote name already exists.")
        );
        assert_eq!(
            remote_dialog_error_message("upstream", "", &existing_names, &existing_urls),
            Some("Enter a remote URL.")
        );
        assert_eq!(
            remote_dialog_error_message(
                "upstream",
                "ssh://git@example.test/repo.git",
                &existing_names,
                &existing_urls,
            ),
            Some("That remote URL already exists.")
        );
        assert_eq!(
            remote_dialog_error_message(
                "upstream",
                "ssh://git@example.test/other.git",
                &existing_names,
                &existing_urls,
            ),
            None
        );
    }
}
