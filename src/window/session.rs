use crate::password::model::OpenPassFile;
use crate::password::undo::UndoAction;
use crate::support::object_data::{cloned_data, set_cloned_data};
use adw::gtk::Widget;
use adw::prelude::*;
use adw::ApplicationWindow;
use std::cell::RefCell;
use std::rc::Rc;

const WINDOW_SESSION_STATE_KEY: &str = "window-session-state";
const MAX_UNDO_ACTIONS: usize = 32;

#[derive(Clone, Default)]
pub struct WindowSessionState {
    opened_pass_file: Rc<RefCell<Option<OpenPassFile>>>,
    undo_stack: Rc<RefCell<Vec<UndoAction>>>,
}

impl WindowSessionState {
    pub fn set_opened_pass_file(&self, pass_file: OpenPassFile) {
        *self.opened_pass_file.borrow_mut() = Some(pass_file);
    }

    pub fn get_opened_pass_file(&self) -> Option<OpenPassFile> {
        self.opened_pass_file.borrow().clone()
    }

    pub fn clear_opened_pass_file(&self) {
        *self.opened_pass_file.borrow_mut() = None;
    }

    pub fn is_opened_pass_file(&self, pass_file: &OpenPassFile) -> bool {
        self.opened_pass_file.borrow().as_ref() == Some(pass_file)
    }

    pub fn refresh_opened_pass_file_from_contents(
        &self,
        pass_file: &OpenPassFile,
        contents: &str,
    ) -> Option<OpenPassFile> {
        let mut opened_pass_file = self.opened_pass_file.borrow_mut();
        let selected = opened_pass_file.as_mut()?;
        if selected != pass_file {
            return None;
        }

        selected.refresh_from_contents(contents);
        Some(selected.clone())
    }

    pub fn push_undo_action(&self, action: UndoAction) {
        let mut undo_stack = self.undo_stack.borrow_mut();
        undo_stack.push(action);
        if undo_stack.len() > MAX_UNDO_ACTIONS {
            let drain_len = undo_stack.len() - MAX_UNDO_ACTIONS;
            undo_stack.drain(0..drain_len);
        }
    }

    pub fn pop_undo_action(&self) -> Option<UndoAction> {
        self.undo_stack.borrow_mut().pop()
    }

    #[cfg(test)]
    pub fn has_undo_actions(&self) -> bool {
        !self.undo_stack.borrow().is_empty()
    }

    #[cfg(test)]
    pub fn clear_undo_actions(&self) {
        self.undo_stack.borrow_mut().clear();
    }
}

pub fn initialize_window_session(window: &ApplicationWindow) -> WindowSessionState {
    let session = WindowSessionState::default();
    set_cloned_data(window, WINDOW_SESSION_STATE_KEY, session.clone());
    session
}

pub fn window_session(window: &ApplicationWindow) -> WindowSessionState {
    cloned_data(window, WINDOW_SESSION_STATE_KEY)
        .expect("window session should be initialized before use")
}

pub fn window_session_for_widget(widget: &impl IsA<Widget>) -> WindowSessionState {
    let window = widget
        .root()
        .and_then(|root| root.downcast::<ApplicationWindow>().ok())
        .expect("window session requires the widget to belong to an application window");
    window_session(&window)
}

#[cfg(test)]
mod tests {
    use super::WindowSessionState;
    use crate::password::model::OpenPassFile;
    use crate::password::undo::UndoAction;
    use crate::preferences::UsernameFallbackMode;

    #[test]
    fn window_sessions_keep_opened_pass_files_separate() {
        let first = WindowSessionState::default();
        let second = WindowSessionState::default();

        let first_pass_file = OpenPassFile::from_label_with_mode(
            "/tmp/first",
            "work/alice/github",
            UsernameFallbackMode::Folder,
        );
        let second_pass_file = OpenPassFile::from_label_with_mode(
            "/tmp/second",
            "work/bob/gitlab",
            UsernameFallbackMode::Folder,
        );

        first.set_opened_pass_file(first_pass_file.clone());
        second.set_opened_pass_file(second_pass_file.clone());

        assert_eq!(first.get_opened_pass_file(), Some(first_pass_file));
        assert_eq!(second.get_opened_pass_file(), Some(second_pass_file));
    }

    #[test]
    fn window_sessions_keep_undo_stacks_separate() {
        let first = WindowSessionState::default();
        let second = WindowSessionState::default();

        first.push_undo_action(UndoAction::RenameEntry {
            store: "/tmp/first".to_string(),
            old_label: "work/alice/github".to_string(),
            new_label: "work/alice/gitlab".to_string(),
        });
        second.push_undo_action(UndoAction::RenameEntry {
            store: "/tmp/second".to_string(),
            old_label: "work/bob/gitlab".to_string(),
            new_label: "work/bob/github".to_string(),
        });

        assert!(first.has_undo_actions());
        assert!(second.has_undo_actions());
        assert_eq!(
            first.pop_undo_action(),
            Some(UndoAction::RenameEntry {
                store: "/tmp/first".to_string(),
                old_label: "work/alice/github".to_string(),
                new_label: "work/alice/gitlab".to_string(),
            })
        );
        assert_eq!(
            second.pop_undo_action(),
            Some(UndoAction::RenameEntry {
                store: "/tmp/second".to_string(),
                old_label: "work/bob/gitlab".to_string(),
                new_label: "work/bob/github".to_string(),
            })
        );
    }
}
