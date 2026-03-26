use super::model::OpenPassFile;
use crate::window::session::window_session_for_widget;
#[cfg(test)]
use crate::window::session::WindowSessionState;
use adw::gtk::Widget;
use adw::prelude::*;

pub fn set_opened_pass_file(widget: &impl IsA<Widget>, pass_file: OpenPassFile) {
    window_session_for_widget(widget).set_opened_pass_file(pass_file);
}

pub fn get_opened_pass_file(widget: &impl IsA<Widget>) -> Option<OpenPassFile> {
    window_session_for_widget(widget).get_opened_pass_file()
}

pub fn clear_opened_pass_file(widget: &impl IsA<Widget>) {
    window_session_for_widget(widget).clear_opened_pass_file();
}

pub fn is_opened_pass_file(widget: &impl IsA<Widget>, pass_file: &OpenPassFile) -> bool {
    window_session_for_widget(widget).is_opened_pass_file(pass_file)
}

pub fn refresh_opened_pass_file_from_contents(
    widget: &impl IsA<Widget>,
    pass_file: &OpenPassFile,
    contents: &str,
) -> Option<OpenPassFile> {
    window_session_for_widget(widget).refresh_opened_pass_file_from_contents(pass_file, contents)
}

#[cfg(test)]
mod tests {
    use super::WindowSessionState;
    use crate::password::model::OpenPassFile;
    use crate::preferences::UsernameFallbackMode;

    #[test]
    fn late_updates_do_not_override_a_newer_selection_in_the_same_window() {
        let session = WindowSessionState::default();

        let first = OpenPassFile::from_label_with_mode(
            "/tmp/store",
            "work/alice/github",
            UsernameFallbackMode::Folder,
        );
        let second = OpenPassFile::from_label_with_mode(
            "/tmp/store",
            "work/bob/gitlab",
            UsernameFallbackMode::Folder,
        );

        session.set_opened_pass_file(second.clone());

        let refreshed =
            session.refresh_opened_pass_file_from_contents(&first, "secret\nusername: stale-user");
        assert_eq!(refreshed, None);
        assert_eq!(session.get_opened_pass_file(), Some(second));
    }

    #[test]
    fn late_updates_only_change_the_window_that_started_the_open() {
        let first_session = WindowSessionState::default();
        let second_session = WindowSessionState::default();

        let first = OpenPassFile::from_label_with_mode(
            "/tmp/first",
            "work/alice/github",
            UsernameFallbackMode::Folder,
        );
        let second = OpenPassFile::from_label_with_mode(
            "/tmp/second",
            "work/bob/gitlab",
            UsernameFallbackMode::Folder,
        );

        first_session.set_opened_pass_file(first.clone());
        second_session.set_opened_pass_file(second.clone());

        let refreshed = first_session
            .refresh_opened_pass_file_from_contents(&first, "secret\nusername: alice@example.com");

        assert_eq!(
            refreshed.as_ref().and_then(OpenPassFile::username),
            Some("alice@example.com")
        );
        assert_eq!(
            first_session
                .get_opened_pass_file()
                .as_ref()
                .and_then(OpenPassFile::username),
            Some("alice@example.com")
        );
        assert_eq!(second_session.get_opened_pass_file(), Some(second));
    }
}
