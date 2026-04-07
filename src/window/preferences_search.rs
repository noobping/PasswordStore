use adw::gtk::{ListBox, ListBoxRow, SearchEntry, Widget};
use adw::prelude::*;
use adw::{ActionRow, EntryRow, PreferencesGroup, PreferencesPage};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

type WidgetProvider = Rc<dyn Fn() -> Vec<Widget>>;

#[derive(Clone)]
pub struct SearchablePreferencesGroup {
    group: PreferencesGroup,
    content: SearchablePreferencesGroupContent,
}

#[derive(Clone)]
enum SearchablePreferencesGroupContent {
    Widgets(WidgetProvider),
    ListBox(ListBox),
}

impl SearchablePreferencesGroup {
    pub fn with_widgets(group: &PreferencesGroup, widgets: Vec<Widget>) -> Self {
        let widgets = Rc::new(widgets);
        Self {
            group: group.clone(),
            content: SearchablePreferencesGroupContent::Widgets(Rc::new(move || {
                widgets.iter().cloned().collect()
            })),
        }
    }

    pub fn with_tracked_widgets(
        group: &PreferencesGroup,
        tracked: Rc<RefCell<Vec<Widget>>>,
    ) -> Self {
        Self {
            group: group.clone(),
            content: SearchablePreferencesGroupContent::Widgets(Rc::new(move || {
                tracked.borrow().clone()
            })),
        }
    }

    pub fn with_list_box(group: &PreferencesGroup, list: &ListBox) -> Self {
        Self {
            group: group.clone(),
            content: SearchablePreferencesGroupContent::ListBox(list.clone()),
        }
    }
}

#[derive(Clone)]
pub struct PreferencesPageSearchState {
    _page: PreferencesPage,
    search_entry: SearchEntry,
    empty_group: Option<PreferencesGroup>,
    groups: Rc<Vec<SearchablePreferencesGroup>>,
    base_visibility: Rc<RefCell<HashMap<usize, bool>>>,
    tracked_widgets: Rc<RefCell<HashSet<usize>>>,
    configured_list_boxes: Rc<RefCell<HashSet<usize>>>,
    applying_search_visibility: Rc<Cell<bool>>,
}

impl PreferencesPageSearchState {
    pub fn new(
        page: &PreferencesPage,
        search_entry: &SearchEntry,
        empty_group: Option<&PreferencesGroup>,
        groups: Vec<SearchablePreferencesGroup>,
    ) -> Self {
        Self {
            _page: page.clone(),
            search_entry: search_entry.clone(),
            empty_group: empty_group.cloned(),
            groups: Rc::new(groups),
            base_visibility: Rc::new(RefCell::new(HashMap::new())),
            tracked_widgets: Rc::new(RefCell::new(HashSet::new())),
            configured_list_boxes: Rc::new(RefCell::new(HashSet::new())),
            applying_search_visibility: Rc::new(Cell::new(false)),
        }
    }

    pub fn connect_handlers(&self) {
        let state = self.clone();
        self.search_entry
            .connect_search_changed(move |_| state.sync());
    }

    pub fn sync(&self) {
        let query = normalize_search_query(self.search_entry.text().as_str());
        let query_active = !query.is_empty();
        let mut any_matches = false;

        for group in self.groups.iter() {
            self.ensure_widget_tracking(group.group.upcast_ref());
            match &group.content {
                SearchablePreferencesGroupContent::Widgets(widgets) => {
                    for widget in widgets() {
                        self.ensure_widget_tracking(&widget);
                    }
                    for list in descendant_list_boxes(group.group.upcast_ref()) {
                        self.ensure_list_box_filter(&group.group, &list);
                        self.ensure_list_box_children_tracking(&list);
                    }
                }
                SearchablePreferencesGroupContent::ListBox(list) => {
                    self.ensure_list_box_filter(&group.group, list);
                    self.ensure_list_box_children_tracking(list);
                }
            }
        }

        self.applying_search_visibility.set(true);
        for group in self.groups.iter() {
            any_matches |= self.apply_group(group, query_active, &query);
        }
        self.applying_search_visibility.set(false);

        if let Some(empty_group) = &self.empty_group {
            empty_group.set_visible(query_active && !any_matches);
        }
    }

    fn apply_group(
        &self,
        group: &SearchablePreferencesGroup,
        query_active: bool,
        query: &str,
    ) -> bool {
        let base_visible = self.base_visibility(group.group.upcast_ref());
        let group_matches = group_matches_query(&group.group, query);

        let any_child_visible = match &group.content {
            SearchablePreferencesGroupContent::Widgets(widgets) => {
                let list_boxes = descendant_list_boxes(group.group.upcast_ref());
                if list_boxes.is_empty() {
                    widgets().iter().any(|widget| {
                        let base_visible = self.base_visibility(widget);
                        base_visible
                            && (!query_active
                                || group_matches
                                || widget_matches_query(widget, query))
                    })
                } else {
                    for list in &list_boxes {
                        list.invalidate_filter();
                    }
                    list_boxes.iter().any(list_has_visible_rows)
                }
            }
            SearchablePreferencesGroupContent::ListBox(list) => {
                list.invalidate_filter();
                list_has_visible_rows(list)
            }
        };

        let visible = if query_active {
            base_visible && (group_matches || any_child_visible)
        } else {
            base_visible
        };
        if group.group.is_visible() != visible {
            group.group.set_visible(visible);
        }

        visible
    }

    fn ensure_widget_tracking(&self, widget: &Widget) {
        let key = widget_key(widget);
        if self.tracked_widgets.borrow().contains(&key) {
            return;
        }

        self.tracked_widgets.borrow_mut().insert(key);
        self.base_visibility
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| widget.is_visible());

        let state = self.clone();
        widget.connect_visible_notify(move |widget| {
            if state.applying_search_visibility.get() {
                return;
            }

            state
                .base_visibility
                .borrow_mut()
                .insert(widget_key(widget), widget.is_visible());
            if !state.search_entry.text().is_empty() {
                state.sync();
            }
        });
    }

    fn ensure_list_box_children_tracking(&self, list: &ListBox) {
        let mut child = list.first_child();
        while let Some(widget) = child {
            let next = widget.next_sibling();
            self.ensure_widget_tracking(&widget);
            child = next;
        }
    }

    fn ensure_list_box_filter(&self, group: &PreferencesGroup, list: &ListBox) {
        let key = widget_key(list.upcast_ref());
        if self.configured_list_boxes.borrow().contains(&key) {
            return;
        }

        self.configured_list_boxes.borrow_mut().insert(key);
        let search_entry = self.search_entry.clone();
        let group = group.clone();
        list.set_filter_func(move |row| {
            let query = normalize_search_query(search_entry.text().as_str());
            if query.is_empty() || group_matches_query(&group, &query) {
                return true;
            }
            list_box_row_matches_query(row, &query)
        });
    }

    fn base_visibility(&self, widget: &Widget) -> bool {
        let key = widget_key(widget);
        *self
            .base_visibility
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| widget.is_visible())
    }
}

fn normalize_search_query(query: &str) -> String {
    query.trim().to_lowercase()
}

fn widget_matches_query(widget: &Widget, query: &str) -> bool {
    widget_search_text(widget).contains(query)
}

fn widget_search_text(widget: &Widget) -> String {
    if let Some(row) = widget.downcast_ref::<ActionRow>() {
        return combined_row_text(row.title().as_str(), row.subtitle().as_deref());
    }
    if let Some(row) = widget.downcast_ref::<EntryRow>() {
        return row.title().to_lowercase();
    }

    String::new()
}

fn group_matches_query(group: &PreferencesGroup, query: &str) -> bool {
    combined_row_text(group.title().as_str(), group.description().as_deref()).contains(query)
}

fn combined_row_text(title: &str, subtitle: Option<&str>) -> String {
    match subtitle {
        Some(subtitle) => format!(
            "{}\n{}",
            title.to_ascii_lowercase(),
            subtitle.to_ascii_lowercase()
        ),
        None => title.to_ascii_lowercase(),
    }
}

fn widget_key(widget: &Widget) -> usize {
    widget.as_ptr() as usize
}

fn list_box_row_matches_query(row: &ListBoxRow, query: &str) -> bool {
    if let Some(action_row) = row.downcast_ref::<ActionRow>() {
        return combined_row_text(
            action_row.title().as_str(),
            action_row.subtitle().as_deref(),
        )
        .contains(query);
    }
    if let Some(entry_row) = row.downcast_ref::<EntryRow>() {
        return entry_row.title().to_lowercase().contains(query);
    }
    let Some(child) = row.child() else {
        return false;
    };
    widget_matches_query(&child, query)
}

fn list_has_visible_rows(list: &ListBox) -> bool {
    let mut index = 0;
    loop {
        let Some(row) = list.row_at_index(index) else {
            return false;
        };
        if row.is_visible() && row.is_child_visible() {
            return true;
        }
        index += 1;
    }
}

fn descendant_list_boxes(root: &Widget) -> Vec<ListBox> {
    let mut boxes = Vec::new();
    collect_descendant_list_boxes(root, &mut boxes);
    boxes
}

fn collect_descendant_list_boxes(widget: &Widget, boxes: &mut Vec<ListBox>) {
    if let Some(list) = widget.downcast_ref::<ListBox>() {
        boxes.push(list.clone());
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        let next = current.next_sibling();
        collect_descendant_list_boxes(&current, boxes);
        child = next;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        combined_row_text, normalize_search_query, PreferencesPageSearchState,
        SearchablePreferencesGroup,
    };
    use adw::gtk::SearchEntry;
    use adw::prelude::*;
    use adw::{ActionRow, PreferencesGroup, PreferencesPage};
    use std::sync::OnceLock;

    fn init_adw_for_tests() -> bool {
        static ADW_READY: OnceLock<bool> = OnceLock::new();
        *ADW_READY.get_or_init(|| adw::init().is_ok())
    }

    #[test]
    fn combined_row_text_lowercases_both_fields() {
        assert_eq!(
            combined_row_text("Git Remotes", Some("Manage store remotes")),
            "git remotes\nmanage store remotes".to_string()
        );
    }

    #[test]
    fn search_query_trims_and_lowercases() {
        assert_eq!(normalize_search_query("  Store Keys  "), "store keys");
    }

    #[test]
    fn explicit_group_search_filters_direct_action_rows() {
        if !init_adw_for_tests() {
            return;
        }

        let page = PreferencesPage::new();
        let search_entry = SearchEntry::new();
        let group = PreferencesGroup::builder()
            .title("Git remotes")
            .description("Manage store remotes")
            .build();
        let first_row = ActionRow::builder()
            .title("origin")
            .subtitle("ssh://example/origin.git")
            .build();
        let second_row = ActionRow::builder()
            .title("backup")
            .subtitle("ssh://example/backup.git")
            .build();
        group.add(&first_row);
        group.add(&second_row);
        page.add(&group);

        let search = PreferencesPageSearchState::new(
            &page,
            &search_entry,
            None,
            vec![SearchablePreferencesGroup::with_widgets(
                &group,
                vec![first_row.clone().upcast(), second_row.clone().upcast()],
            )],
        );

        search.sync();
        assert!(first_row.is_visible());
        assert!(second_row.is_visible());

        search_entry.set_text("origin");
        search.sync();
        assert!(group.is_visible());
        assert!(first_row.is_child_visible());
        assert!(!second_row.is_child_visible());

        search_entry.set_text("");
        search.sync();
        assert!(first_row.is_child_visible());
        assert!(second_row.is_child_visible());
    }
}
