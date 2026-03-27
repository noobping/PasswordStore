use crate::i18n::gettext;
use crate::logging::log_error;
#[cfg(target_os = "linux")]
use adw::gio;
#[cfg(target_os = "linux")]
use adw::gtk::{FileChooserAction, FileChooserNative, ResponseType};
use adw::prelude::*;
use adw::{ApplicationWindow, Toast, ToastOverlay};
#[cfg(target_os = "linux")]
use std::rc::Rc;

#[cfg(target_os = "windows")]
use winsafe::{self as w, co, prelude::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocalPathKind {
    File,
    Folder,
}

impl LocalPathKind {
    #[cfg(target_os = "windows")]
    const fn chooser_error_message(self) -> &'static str {
        match self {
            Self::File => "Couldn't open the file chooser.",
            Self::Folder => "Couldn't open the folder chooser.",
        }
    }

    const fn local_path_error_message(self) -> &'static str {
        match self {
            Self::File => "Choose a local file.",
            Self::Folder => "Choose a local folder.",
        }
    }
}

#[cfg(target_os = "linux")]
fn selected_local_path(
    file: &gio::File,
    kind: LocalPathKind,
    overlay: &ToastOverlay,
) -> Option<String> {
    let path = file.path().or_else(|| {
        log_error(format!(
            "The selected path is not available locally. {}",
            kind.local_path_error_message()
        ));
        overlay.add_toast(Toast::new(&gettext(kind.local_path_error_message())));
        None
    })?;

    Some(path.to_string_lossy().to_string())
}

#[cfg(target_os = "linux")]
fn choose_local_path_with_dialog(
    window: &ApplicationWindow,
    title: &str,
    accept_label: &str,
    kind: LocalPathKind,
    create_folders: bool,
    overlay: &ToastOverlay,
    on_selected: impl Fn(String) + 'static,
) {
    let dialog = FileChooserNative::new(
        Some(&gettext(title)),
        Some(window),
        match kind {
            LocalPathKind::File => FileChooserAction::Open,
            LocalPathKind::Folder => FileChooserAction::SelectFolder,
        },
        Some(&gettext(accept_label)),
        Some(&gettext("Cancel")),
    );
    if matches!(kind, LocalPathKind::Folder) {
        dialog.set_create_folders(create_folders);
    }

    let overlay = overlay.clone();
    let on_selected = Rc::new(on_selected);
    dialog.connect_response(move |dialog, response| {
        if response == ResponseType::Accept {
            let Some(file) = dialog.file() else {
                dialog.hide();
                return;
            };

            if let Some(path) = selected_local_path(&file, kind, &overlay) {
                on_selected(path);
            }
        }

        dialog.hide();
    });

    dialog.show();
}

#[cfg(target_os = "windows")]
fn choose_windows_path(
    title: &str,
    accept_label: &str,
    kind: LocalPathKind,
    create_folders: bool,
) -> Result<Option<String>, String> {
    let _com = w::CoInitializeEx(co::COINIT::APARTMENTTHREADED)
        .map_err(|err| format!("Failed to initialize COM for the file picker: {err}"))?;
    let dialog = w::CoCreateInstance::<w::IFileOpenDialog>(
        &co::CLSID::FileOpenDialog,
        None::<&w::IUnknown>,
        co::CLSCTX::INPROC_SERVER,
    )
    .map_err(|err| format!("Failed to create the Windows file picker: {err}"))?;

    let mut options = dialog
        .GetOptions()
        .map_err(|err| format!("Failed to read Windows file picker options: {err}"))?
        | co::FOS::FORCEFILESYSTEM;
    match kind {
        LocalPathKind::File => {
            options |= co::FOS::FILEMUSTEXIST;
        }
        LocalPathKind::Folder => {
            options |= co::FOS::PICKFOLDERS;
            if !create_folders {
                options |= co::FOS::PATHMUSTEXIST;
            }
        }
    }

    dialog
        .SetOptions(options)
        .map_err(|err| format!("Failed to configure Windows file picker options: {err}"))?;
    dialog
        .SetTitle(title)
        .map_err(|err| format!("Failed to set the Windows file picker title: {err}"))?;
    dialog
        .SetOkButtonLabel(accept_label)
        .map_err(|err| format!("Failed to set the Windows file picker button label: {err}"))?;

    let owner = w::HWND::GetDesktopWindow();
    let accepted = dialog
        .Show(&owner)
        .map_err(|err| format!("Failed to show the Windows file picker: {err}"))?;
    if !accepted {
        return Ok(None);
    }

    dialog
        .GetResult()
        .and_then(|item| item.GetDisplayName(co::SIGDN::FILESYSPATH))
        .map(Some)
        .map_err(|err| format!("Failed to read the selected Windows path: {err}"))
}

pub fn choose_local_file_path(
    window: &ApplicationWindow,
    title: &str,
    accept_label: &str,
    overlay: &ToastOverlay,
    on_selected: impl Fn(String) + 'static,
) {
    #[cfg(target_os = "linux")]
    choose_local_path_with_dialog(
        window,
        title,
        accept_label,
        LocalPathKind::File,
        false,
        overlay,
        on_selected,
    );

    #[cfg(target_os = "windows")]
    {
        let _ = window;
        match choose_windows_path(title, accept_label, LocalPathKind::File, false) {
            Ok(Some(path)) => on_selected(path),
            Ok(None) => {}
            Err(err) => {
                log_error(err);
                overlay.add_toast(Toast::new(&gettext(
                    LocalPathKind::File.chooser_error_message(),
                )));
            }
        }
    }
}

pub fn choose_local_folder_path(
    window: &ApplicationWindow,
    title: &str,
    accept_label: &str,
    create_folders: bool,
    overlay: &ToastOverlay,
    on_selected: impl Fn(String) + 'static,
) {
    #[cfg(target_os = "linux")]
    choose_local_path_with_dialog(
        window,
        title,
        accept_label,
        LocalPathKind::Folder,
        create_folders,
        overlay,
        on_selected,
    );

    #[cfg(target_os = "windows")]
    {
        let _ = window;
        match choose_windows_path(title, accept_label, LocalPathKind::Folder, create_folders) {
            Ok(Some(path)) => on_selected(path),
            Ok(None) => {}
            Err(err) => {
                log_error(err);
                overlay.add_toast(Toast::new(&gettext(
                    LocalPathKind::Folder.chooser_error_message(),
                )));
            }
        }
    }
}

#[cfg(target_os = "linux")]
pub fn choose_file_bytes(
    window: &ApplicationWindow,
    title: &str,
    accept_label: &str,
    overlay: &ToastOverlay,
    log_context: &'static str,
    read_error_message: &'static str,
    on_selected: impl Fn(Vec<u8>) + 'static,
) {
    let dialog = FileChooserNative::new(
        Some(&gettext(title)),
        Some(window),
        FileChooserAction::Open,
        Some(&gettext(accept_label)),
        Some(&gettext("Cancel")),
    );
    let overlay = overlay.clone();
    let on_selected = Rc::new(on_selected);
    dialog.connect_response(move |dialog, response| {
        if response != ResponseType::Accept {
            dialog.hide();
            return;
        }

        let Some(file) = dialog.file() else {
            dialog.hide();
            return;
        };

        match file.load_bytes(None::<&gio::Cancellable>) {
            Ok((bytes, _)) => on_selected(bytes.as_ref().to_vec()),
            Err(err) => {
                log_error(format!("{log_context}: {err}"));
                overlay.add_toast(Toast::new(&gettext(read_error_message)));
            }
        }

        dialog.hide();
    });

    dialog.show();
}

#[cfg(target_os = "windows")]
pub fn choose_file_bytes(
    window: &ApplicationWindow,
    title: &str,
    accept_label: &str,
    overlay: &ToastOverlay,
    log_context: &'static str,
    read_error_message: &'static str,
    on_selected: impl Fn(Vec<u8>) + 'static,
) {
    let _ = window;
    match choose_windows_path(title, accept_label, LocalPathKind::File, false) {
        Ok(Some(path)) => match std::fs::read(&path) {
            Ok(bytes) => on_selected(bytes),
            Err(err) => {
                log_error(format!("{log_context}: {err}"));
                overlay.add_toast(Toast::new(&gettext(read_error_message)));
            }
        },
        Ok(None) => {}
        Err(err) => {
            log_error(err);
            overlay.add_toast(Toast::new(&gettext(
                LocalPathKind::File.chooser_error_message(),
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LocalPathKind;

    #[test]
    fn local_path_messages_match_the_selection_kind() {
        assert_eq!(
            LocalPathKind::File.chooser_error_message(),
            "Couldn't open the file chooser."
        );
        assert_eq!(
            LocalPathKind::Folder.chooser_error_message(),
            "Couldn't open the folder chooser."
        );
        assert_eq!(
            LocalPathKind::File.local_path_error_message(),
            "Choose a local file."
        );
        assert_eq!(
            LocalPathKind::Folder.local_path_error_message(),
            "Choose a local folder."
        );
    }
}
