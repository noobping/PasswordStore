use super::{
    append_loading_rows, collect_loaded_entry_requests, next_generation, FieldValueRequest,
    ToolsPageState, WEAK_PASSWORDS_EMPTY_SUBTITLE, WEAK_PASSWORDS_EMPTY_TITLE,
    WEAK_PASSWORDS_FILTER_EMPTY_SUBTITLE, WEAK_PASSWORDS_FILTER_EMPTY_TITLE,
    WEAK_PASSWORDS_LOADING_SUBTITLE, WEAK_PASSWORDS_LOADING_TITLE, WEAK_PASSWORDS_SUBTITLE,
    WEAK_PASSWORDS_TITLE,
};
use crate::backend::read_password_line;
use crate::password::model::OpenPassFile;
use crate::password::page::open_password_entry_page;
use crate::password::strength::weak_password_reason;
use crate::support::background::spawn_result_task;
use crate::support::ui::{
    append_action_row_with_button, append_info_row, clear_list_box, reveal_navigation_page,
};
use crate::window::navigation::{show_secondary_page_chrome, HasWindowChrome};
use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Default)]
pub(super) struct WeakPasswordToolState {
    pub(super) generation: Cell<u64>,
    pub(super) in_flight: Cell<bool>,
    pub(super) tool_busy: Cell<bool>,
    pub(super) results: RefCell<Option<Vec<WeakPasswordFinding>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WeakPasswordFinding {
    root: String,
    label: String,
    normalized_label: String,
    reason: String,
    normalized_reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WeakPasswordBatch {
    generation: u64,
    results: Vec<WeakPasswordFinding>,
}

impl ToolsPageState {
    pub(super) fn prepare_weak_passwords_browser(&self) {
        if self.tools_are_busy() {
            return;
        }

        self.invalidate_stale_tool_cache();
        self.start_weak_passwords_scan(true);
    }

    pub(super) fn refresh_weak_passwords_browser_if_needed(&self) {
        if self.tools_are_busy()
            || self.weak_password_page.weak_passwords.in_flight.get()
            || self
                .weak_password_page
                .weak_passwords
                .results
                .borrow()
                .is_some()
        {
            return;
        }

        self.start_weak_passwords_scan(false);
    }

    fn start_weak_passwords_scan(&self, reset_search: bool) {
        if reset_search {
            self.reset_weak_passwords_view();
        }
        self.set_weak_passwords_tool_busy(true);
        let requests = collect_loaded_entry_requests(&self.root_list);
        let generation = next_generation(self.weak_password_page.weak_passwords.generation.get());
        self.weak_password_page
            .weak_passwords
            .generation
            .set(generation);
        self.weak_password_page.weak_passwords.in_flight.set(true);
        *self.weak_password_page.weak_passwords.results.borrow_mut() = None;
        self.render_weak_passwords_list();
        self.show_weak_passwords_browser_page();

        self.unlock_tool_keys_if_needed(
            requests,
            Rc::new({
                let state = self.clone();
                move |requests| {
                    state.open_weak_passwords_browser_with_requests(generation, requests)
                }
            }),
            Rc::new({
                let state = self.clone();
                move || state.handle_weak_password_disconnect(generation)
            }),
        );
    }

    fn show_weak_passwords_browser_page(&self) {
        let chrome = self.navigation.window_chrome();
        show_secondary_page_chrome(
            &chrome,
            WEAK_PASSWORDS_TITLE,
            WEAK_PASSWORDS_SUBTITLE,
            false,
        );
        reveal_navigation_page(&self.navigation.nav, &self.weak_password_page.page);
    }

    fn open_weak_passwords_browser_with_requests(
        &self,
        generation: u64,
        requests: Vec<FieldValueRequest>,
    ) {
        if generation != self.weak_password_page.weak_passwords.generation.get() {
            return;
        }

        if requests.is_empty() {
            self.apply_weak_password_batch(WeakPasswordBatch {
                generation,
                results: Vec::new(),
            });
            return;
        }

        let state_for_result = self.clone();
        let state_for_disconnect = self.clone();
        spawn_result_task(
            move || build_weak_password_batch(generation, requests),
            move |batch| state_for_result.apply_weak_password_batch(batch),
            move || state_for_disconnect.handle_weak_password_disconnect(generation),
        );
    }

    fn apply_weak_password_batch(&self, batch: WeakPasswordBatch) {
        if batch.generation != self.weak_password_page.weak_passwords.generation.get() {
            return;
        }

        self.weak_password_page.weak_passwords.in_flight.set(false);
        self.set_weak_passwords_tool_busy(false);
        *self.weak_password_page.weak_passwords.results.borrow_mut() = Some(batch.results);
        self.render_weak_passwords_list();
    }

    fn handle_weak_password_disconnect(&self, generation: u64) {
        if generation != self.weak_password_page.weak_passwords.generation.get() {
            return;
        }

        self.weak_password_page.weak_passwords.in_flight.set(false);
        self.set_weak_passwords_tool_busy(false);
        self.render_weak_passwords_list();
    }

    pub(super) fn render_weak_passwords_list(&self) {
        clear_list_box(&self.weak_password_page.list);

        if self.weak_password_page.weak_passwords.in_flight.get() {
            append_loading_rows(
                &self.weak_password_page.list,
                WEAK_PASSWORDS_LOADING_TITLE,
                WEAK_PASSWORDS_LOADING_SUBTITLE,
            );
            return;
        }

        let Some(results) = self
            .weak_password_page
            .weak_passwords
            .results
            .borrow()
            .clone()
        else {
            append_info_row(
                &self.weak_password_page.list,
                WEAK_PASSWORDS_EMPTY_TITLE,
                WEAK_PASSWORDS_EMPTY_SUBTITLE,
            );
            return;
        };

        let query = self.weak_password_page.search_entry.text();
        let query = query.as_str().trim().to_lowercase();
        let results = results
            .into_iter()
            .filter(|result| {
                query.is_empty()
                    || result.normalized_label.contains(&query)
                    || result.normalized_reason.contains(&query)
            })
            .collect::<Vec<_>>();

        if results.is_empty() {
            append_info_row(
                &self.weak_password_page.list,
                if query.is_empty() {
                    WEAK_PASSWORDS_EMPTY_TITLE
                } else {
                    WEAK_PASSWORDS_FILTER_EMPTY_TITLE
                },
                if query.is_empty() {
                    WEAK_PASSWORDS_EMPTY_SUBTITLE
                } else {
                    WEAK_PASSWORDS_FILTER_EMPTY_SUBTITLE
                },
            );
            return;
        }

        for result in results {
            let state = self.clone();
            let root = result.root.clone();
            let label = result.label.clone();
            append_action_row_with_button(
                &self.weak_password_page.list,
                &result.label,
                &result.reason,
                "go-next-symbolic",
                move || state.open_weak_password_entry(&root, &label),
            );
        }
    }

    fn open_weak_password_entry(&self, root: &str, label: &str) {
        self.mark_weak_passwords_stale();
        open_password_entry_page(
            &self.password_page,
            OpenPassFile::from_label(root, label),
            true,
        );
    }

    pub(super) fn reset_weak_passwords_view(&self) {
        if !self.weak_password_page.search_entry.text().is_empty() {
            self.weak_password_page.search_entry.set_text("");
        }
    }

    fn mark_weak_passwords_stale(&self) {
        self.weak_password_page
            .weak_passwords
            .generation
            .set(next_generation(
                self.weak_password_page.weak_passwords.generation.get(),
            ));
        self.weak_password_page.weak_passwords.in_flight.set(false);
        *self.weak_password_page.weak_passwords.results.borrow_mut() = None;
    }

    pub(super) fn clear_weak_passwords_cache(&self) {
        self.mark_weak_passwords_stale();
        self.set_weak_passwords_tool_busy(false);
        self.reset_weak_passwords_view();
    }
}

fn build_weak_password_batch(
    generation: u64,
    requests: Vec<FieldValueRequest>,
) -> WeakPasswordBatch {
    let results = requests
        .into_iter()
        .filter_map(|request| {
            let password = read_password_line(&request.root, &request.label).ok()?;
            let reason = weak_password_reason(&password)?;
            Some(WeakPasswordFinding {
                root: request.root,
                label: request.label.to_string(),
                normalized_label: request.label.to_lowercase(),
                normalized_reason: reason.to_lowercase(),
                reason,
            })
        })
        .collect();

    WeakPasswordBatch {
        generation,
        results,
    }
}
