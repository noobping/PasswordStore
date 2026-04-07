use super::logic::{
    any_dirty, cached_download_matches_size, ReleaseAsset, ReleaseCandidate, SelectedRelease,
};
use super::platform;
use super::DirtyProbe;
use crate::i18n::gettext;
use crate::logging::{log_error, log_info};
use crate::support::background::spawn_worker;
use crate::support::object_data::{cloned_data, set_cloned_data};
use crate::support::ui::wrapped_dialog_body;
use adw::gio::SimpleAction;
use adw::glib::{self, WeakRef};
use adw::gtk::{Align, Box as GtkBox, Button, Label, Orientation, ProgressBar};
use adw::prelude::*;
use adw::{AlertDialog, Application, ApplicationWindow, Dialog, Toast, ToastOverlay};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::sync::Arc;
use std::time::Duration;

const UPDATE_CONTROLLER_KEY: &str = "app-updater-controller";
const GITHUB_API_ACCEPT: &str = "application/vnd.github+json";
const GITHUB_API_VERSION: &str = "2022-11-28";
const GITHUB_RELEASES_PER_PAGE: usize = 100;
const WORKER_POLL_INTERVAL_MS: u64 = 50;
const UPDATE_DIALOG_TITLE: &str = "Keycord Update";
const UPDATE_DIALOG_CONTENT_WIDTH: i32 = 560;
const UPDATE_DIALOG_CONTENT_HEIGHT: i32 = 320;
const UPDATE_CHECK_HEADING: &str = "Checking for updates";
const UPDATE_INSTALL_BUTTON_LABEL: &str = "Install Update";
const UPDATE_LATER_BUTTON_LABEL: &str = "Later";
const UPDATE_UP_TO_DATE_TOAST: &str = "Keycord is already up to date.";
const UPDATE_CHECK_FAILED_TOAST: &str = "Couldn't check for updates.";
const UPDATE_DOWNLOAD_FAILED_TOAST: &str = "Couldn't download the update.";
const UPDATE_CONFIRM_INSTALL_TITLE: &str = "Close Keycord to install the update?";
const UPDATE_CONFIRM_INSTALL_BODY: &str =
    "Installing the update will close Keycord. Unsaved changes will be lost.";

#[cfg(target_os = "windows")]
pub(crate) fn sanitize_filename(name: &str) -> String {
    let mut output = String::new();
    let mut chars = name.chars();
    while let Some(ch) = chars.next() {
        output.push(match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            _ => ch,
        });
    }
    output
}

pub(crate) fn repository_owner_and_name() -> Result<(&'static str, &'static str), String> {
    let repository = env!("CARGO_PKG_REPOSITORY");
    let path = repository
        .strip_prefix("https://github.com/")
        .or_else(|| repository.strip_prefix("http://github.com/"))
        .ok_or_else(|| format!("Unsupported repository URL for updates: {repository}"))?;
    let mut parts = path.split('/');
    let owner = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| format!("Missing owner in repository URL: {repository}"))?;
    let repo = parts
        .next()
        .filter(|part| !part.is_empty())
        .map(|part| part.trim_end_matches(".git"))
        .ok_or_else(|| format!("Missing repository name in repository URL: {repository}"))?;
    Ok((owner, repo))
}

pub fn register_app_actions(app: &Application) {
    if !platform::supports_updater() {
        return;
    }

    let action = SimpleAction::new("check-for-updates", None);
    let app_for_action = app.clone();
    action.connect_activate(move |_, _| {
        ensure_controller(&app_for_action).start_check(CheckMode::Manual);
    });
    app.add_action(&action);
}

pub fn register_window(
    app: &Application,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    has_unsaved_changes: DirtyProbe,
) {
    if !platform::supports_updater() {
        return;
    }

    ensure_controller(app).register_window(window, overlay, has_unsaved_changes);
}

pub fn after_window_presented(app: &Application, _window: &ApplicationWindow) {
    if !platform::supports_updater() {
        return;
    }

    ensure_controller(app).start_auto_check_once();
}

pub fn shutdown(app: &Application) {
    let Some(controller): Option<UpdaterController> = cloned_data(app, UPDATE_CONTROLLER_KEY)
    else {
        return;
    };

    controller.handle_shutdown();
}

#[derive(Clone)]
struct UpdaterController {
    inner: Rc<UpdaterControllerInner>,
}

struct UpdaterControllerInner {
    app: Application,
    registrations: RefCell<Vec<WindowRegistration>>,
    state: RefCell<UpdateState>,
    dialog: RefCell<Option<UpdateDialog>>,
    auto_check_started: Cell<bool>,
    next_run_id: Cell<u64>,
}

#[derive(Clone)]
struct WindowRegistration {
    window: WeakRef<ApplicationWindow>,
    overlay: WeakRef<ToastOverlay>,
    has_unsaved_changes: DirtyProbe,
}

#[derive(Clone)]
struct UpdateDialog {
    dialog: Dialog,
    heading: Label,
    body: Label,
    progress: ProgressBar,
    status: Label,
    install_button: Button,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DownloadedUpdate {
    pub(crate) path: PathBuf,
    pub(crate) size: u64,
    pub(crate) cleanup_dir: Option<PathBuf>,
}

#[derive(Clone)]
enum UpdateState {
    Idle,
    Checking {
        mode: CheckMode,
        run_id: u64,
    },
    Downloading {
        mode: CheckMode,
        run_id: u64,
        release: SelectedRelease,
        download: DownloadedUpdate,
        cancel: Arc<AtomicBool>,
        downloaded: u64,
    },
    Ready {
        release: SelectedRelease,
        download: DownloadedUpdate,
    },
    Installing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CheckMode {
    Automatic,
    Manual,
}

#[derive(Clone, Debug)]
enum WorkerMessage {
    CheckFinished {
        run_id: u64,
        mode: CheckMode,
        result: Result<Option<SelectedRelease>, String>,
    },
    DownloadProgress {
        run_id: u64,
        downloaded: u64,
    },
    DownloadReady {
        run_id: u64,
        download: DownloadedUpdate,
    },
    DownloadCancelled {
        run_id: u64,
    },
    DownloadFailed {
        run_id: u64,
        error: String,
    },
}

#[derive(Deserialize)]
struct GitHubReleaseResponse {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GitHubAssetResponse>,
}

#[derive(Deserialize)]
struct GitHubAssetResponse {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

impl UpdaterController {
    fn new(app: &Application) -> Self {
        Self {
            inner: Rc::new(UpdaterControllerInner {
                app: app.clone(),
                registrations: RefCell::new(Vec::new()),
                state: RefCell::new(UpdateState::Idle),
                dialog: RefCell::new(None),
                auto_check_started: Cell::new(false),
                next_run_id: Cell::new(0),
            }),
        }
    }

    fn register_window(
        &self,
        window: &ApplicationWindow,
        overlay: &ToastOverlay,
        has_unsaved_changes: DirtyProbe,
    ) {
        self.compact_registrations();
        self.inner
            .registrations
            .borrow_mut()
            .push(WindowRegistration {
                window: weak_ref(window),
                overlay: weak_ref(overlay),
                has_unsaved_changes,
            });
    }

    fn handle_shutdown(&self) {
        match self.inner.state.borrow().clone() {
            UpdateState::Downloading {
                cancel, download, ..
            } => {
                cancel.store(true, Ordering::Relaxed);
                platform::cleanup_download(&download);
            }
            UpdateState::Ready { download, .. } => platform::cleanup_download(&download),
            UpdateState::Idle | UpdateState::Checking { .. } | UpdateState::Installing => {}
        }
    }

    fn start_auto_check_once(&self) {
        if self.inner.auto_check_started.replace(true) {
            return;
        }

        self.start_check(CheckMode::Automatic);
    }

    fn start_check(&self, mode: CheckMode) {
        self.compact_registrations();
        if !platform::supports_updater() {
            return;
        }

        {
            let mut state = self.inner.state.borrow_mut();
            match &mut *state {
                UpdateState::Idle => {}
                UpdateState::Checking {
                    mode: existing_mode,
                    ..
                } => {
                    if matches!(mode, CheckMode::Manual) {
                        *existing_mode = CheckMode::Manual;
                        self.present_checking_dialog();
                    }
                    return;
                }
                UpdateState::Downloading {
                    mode: existing_mode,
                    release,
                    downloaded,
                    ..
                } => {
                    if matches!(mode, CheckMode::Manual) {
                        *existing_mode = CheckMode::Manual;
                        self.present_download_dialog(release, *downloaded);
                    }
                    return;
                }
                UpdateState::Ready { release, download } => {
                    if cached_download_matches_release(download, &release.asset) {
                        if matches!(mode, CheckMode::Manual) {
                            self.present_ready_dialog(release);
                        }
                        return;
                    }

                    platform::cleanup_download(download);
                    *state = UpdateState::Idle;
                }
                UpdateState::Installing => return,
            }
        }

        let run_id = self.next_run_id();
        *self.inner.state.borrow_mut() = UpdateState::Checking { mode, run_id };
        if matches!(mode, CheckMode::Manual) {
            self.present_checking_dialog();
        }

        let (tx, rx) = mpsc::channel();
        if let Err(err) = spawn_worker("updater-check", move || {
            let result = fetch_update_release();
            let _ = tx.send(WorkerMessage::CheckFinished {
                run_id,
                mode,
                result,
            });
        }) {
            log_error(format!("Failed to spawn update check worker: {err}"));
            *self.inner.state.borrow_mut() = UpdateState::Idle;
            self.close_dialog();
            if matches!(mode, CheckMode::Manual) {
                self.show_toast(UPDATE_CHECK_FAILED_TOAST);
            }
            return;
        }

        let controller = self.clone();
        poll_worker(rx, move |message| controller.handle_worker_message(message));
    }

    fn handle_worker_message(&self, message: WorkerMessage) {
        match message {
            WorkerMessage::CheckFinished {
                run_id,
                mode,
                result,
            } => self.handle_check_finished(run_id, mode, result),
            WorkerMessage::DownloadProgress { run_id, downloaded } => {
                self.handle_download_progress(run_id, downloaded);
            }
            WorkerMessage::DownloadReady { run_id, download } => {
                self.handle_download_ready(run_id, download);
            }
            WorkerMessage::DownloadCancelled { run_id } => self.handle_download_cancelled(run_id),
            WorkerMessage::DownloadFailed { run_id, error } => {
                self.handle_download_failed(run_id, &error);
            }
        }
    }

    fn handle_check_finished(
        &self,
        run_id: u64,
        mode: CheckMode,
        result: Result<Option<SelectedRelease>, String>,
    ) {
        let state = self.inner.state.borrow().clone();
        let UpdateState::Checking {
            run_id: current_run_id,
            ..
        } = state
        else {
            return;
        };
        if current_run_id != run_id {
            return;
        }

        match result {
            Ok(Some(release)) => self.start_download(run_id, mode, release),
            Ok(None) => {
                *self.inner.state.borrow_mut() = UpdateState::Idle;
                self.close_dialog();
                if matches!(mode, CheckMode::Manual) {
                    self.show_toast(UPDATE_UP_TO_DATE_TOAST);
                }
            }
            Err(error) => {
                log_error(format!("Failed to check for updates: {error}"));
                *self.inner.state.borrow_mut() = UpdateState::Idle;
                self.close_dialog();
                if matches!(mode, CheckMode::Manual) {
                    self.show_toast(UPDATE_CHECK_FAILED_TOAST);
                }
            }
        }
    }

    fn start_download(&self, run_id: u64, mode: CheckMode, release: SelectedRelease) {
        if let Err(error) = validate_release_asset_digest(&release.asset) {
            log_error(format!(
                "Refusing to download update {}: {error}",
                release.version
            ));
            *self.inner.state.borrow_mut() = UpdateState::Idle;
            self.close_dialog();
            if matches!(mode, CheckMode::Manual) {
                self.show_toast(UPDATE_CHECK_FAILED_TOAST);
            }
            return;
        }

        let download = match platform::download_target(&release) {
            Ok(download) => download,
            Err(error) => {
                log_error(format!(
                    "Failed to prepare update download target for {}: {error}",
                    release.version
                ));
                *self.inner.state.borrow_mut() = UpdateState::Idle;
                self.close_dialog();
                if matches!(mode, CheckMode::Manual) {
                    self.show_toast(UPDATE_CHECK_FAILED_TOAST);
                }
                return;
            }
        };

        if cached_download_matches_release(&download, &release.asset) {
            log_info(format!(
                "Reusing cached update download for version {}.",
                release.version
            ));
            *self.inner.state.borrow_mut() = UpdateState::Ready {
                release: release.clone(),
                download,
            };
            self.present_ready_dialog(&release);
            return;
        }

        let cancel = Arc::new(AtomicBool::new(false));
        *self.inner.state.borrow_mut() = UpdateState::Downloading {
            mode,
            run_id,
            release: release.clone(),
            download: download.clone(),
            cancel: cancel.clone(),
            downloaded: 0,
        };
        self.present_download_dialog(&release, 0);

        let (tx, rx) = mpsc::channel();
        if let Err(err) = spawn_worker("updater-download", move || {
            let result = download_release_asset(run_id, &release, &download, &cancel, &tx);
            if let Some(message) = result {
                let _ = tx.send(message);
            }
        }) {
            log_error(format!("Failed to spawn update download worker: {err}"));
            *self.inner.state.borrow_mut() = UpdateState::Idle;
            self.close_dialog();
            if matches!(mode, CheckMode::Manual) {
                self.show_toast(UPDATE_DOWNLOAD_FAILED_TOAST);
            }
            return;
        }

        let controller = self.clone();
        poll_worker(rx, move |message| controller.handle_worker_message(message));
    }

    fn handle_download_progress(&self, run_id: u64, downloaded: u64) {
        let mut state = self.inner.state.borrow_mut();
        let UpdateState::Downloading {
            run_id: current_run_id,
            release,
            downloaded: current_downloaded,
            ..
        } = &mut *state
        else {
            return;
        };
        if *current_run_id != run_id {
            return;
        }

        *current_downloaded = downloaded;
        self.update_download_dialog(release, downloaded);
    }

    fn handle_download_ready(&self, run_id: u64, download: DownloadedUpdate) {
        let state = self.inner.state.borrow().clone();
        let UpdateState::Downloading {
            run_id: current_run_id,
            release,
            ..
        } = state
        else {
            return;
        };
        if current_run_id != run_id {
            return;
        }

        *self.inner.state.borrow_mut() = UpdateState::Ready {
            release: release.clone(),
            download,
        };
        self.present_ready_dialog(&release);
    }

    fn handle_download_cancelled(&self, run_id: u64) {
        let state = self.inner.state.borrow().clone();
        let UpdateState::Downloading {
            run_id: current_run_id,
            download,
            ..
        } = state
        else {
            return;
        };
        if current_run_id != run_id {
            return;
        }

        platform::cleanup_download(&download);
        *self.inner.state.borrow_mut() = UpdateState::Idle;
    }

    fn handle_download_failed(&self, run_id: u64, error: &str) {
        let state = self.inner.state.borrow().clone();
        let UpdateState::Downloading {
            run_id: current_run_id,
            mode,
            download,
            ..
        } = state
        else {
            return;
        };
        if current_run_id != run_id {
            return;
        }

        log_error(format!("Failed to download the update: {error}"));
        platform::cleanup_download(&download);
        *self.inner.state.borrow_mut() = UpdateState::Idle;
        let had_dialog = self.inner.dialog.borrow().is_some();
        self.close_dialog();
        if matches!(mode, CheckMode::Manual) || had_dialog {
            self.show_toast(UPDATE_DOWNLOAD_FAILED_TOAST);
        }
    }

    fn present_checking_dialog(&self) {
        let dialog = self.ensure_dialog();
        dialog.heading.set_label(&gettext(UPDATE_CHECK_HEADING));
        dialog
            .body
            .set_label(&gettext(platform::update_check_body()));
        dialog.progress.set_visible(false);
        dialog.progress.set_fraction(0.0);
        dialog.status.set_label("");
        dialog.status.set_visible(false);
        dialog.install_button.set_sensitive(false);
        self.present_dialog(&dialog);
    }

    fn present_download_dialog(&self, release: &SelectedRelease, downloaded: u64) {
        let dialog = self.ensure_dialog();
        update_dialog_release_copy(&dialog, release);
        dialog.progress.set_visible(true);
        dialog.status.set_visible(true);
        dialog.install_button.set_sensitive(false);
        self.update_download_dialog(release, downloaded);
        self.present_dialog(&dialog);
    }

    fn update_download_dialog(&self, release: &SelectedRelease, downloaded: u64) {
        let Some(dialog) = self.inner.dialog.borrow().clone() else {
            return;
        };

        let total = release.asset.size;
        if total > 0 {
            let fraction = (downloaded.min(total) as f64) / (total as f64);
            dialog.progress.set_fraction(fraction);
        } else {
            dialog.progress.pulse();
        }

        dialog
            .status
            .set_label(&download_status_label(downloaded, total));
    }

    fn present_ready_dialog(&self, release: &SelectedRelease) {
        let dialog = self.ensure_dialog();
        update_dialog_release_copy(&dialog, release);
        dialog.progress.set_visible(true);
        dialog.progress.set_fraction(1.0);
        dialog.status.set_visible(true);
        dialog.status.set_label(&gettext(platform::ready_status()));
        dialog.install_button.set_sensitive(true);
        self.present_dialog(&dialog);
    }

    fn ensure_dialog(&self) -> UpdateDialog {
        if let Some(dialog) = self.inner.dialog.borrow().clone() {
            return dialog;
        }

        let dialog = build_update_dialog(self);
        *self.inner.dialog.borrow_mut() = Some(dialog.clone());
        self.present_dialog(&dialog);
        dialog
    }

    fn present_dialog(&self, dialog: &UpdateDialog) {
        if let Some(parent) = self.preferred_window() {
            dialog.dialog.present(Some(&parent));
        }
    }

    fn close_dialog(&self) {
        let dialog = self.inner.dialog.borrow_mut().take();
        if let Some(dialog) = dialog {
            dialog.dialog.force_close();
        }
    }

    fn handle_dialog_closed(&self) {
        self.inner.dialog.borrow_mut().take();

        let state = self.inner.state.borrow().clone();
        let UpdateState::Downloading {
            cancel, download, ..
        } = state
        else {
            return;
        };

        cancel.store(true, Ordering::Relaxed);
        platform::cleanup_download(&download);
        *self.inner.state.borrow_mut() = UpdateState::Idle;
    }

    fn begin_install_flow(&self) {
        let state = self.inner.state.borrow().clone();
        let UpdateState::Ready { release, download } = state else {
            return;
        };

        if !cached_download_matches_release(&download, &release.asset) {
            platform::cleanup_download(&download);
            *self.inner.state.borrow_mut() = UpdateState::Idle;
            self.start_check(CheckMode::Manual);
            return;
        }

        if self.any_window_has_unsaved_changes() {
            self.present_install_confirmation();
            return;
        }

        self.launch_update(&download);
    }

    fn present_install_confirmation(&self) {
        let Some(window) = self.preferred_window() else {
            return;
        };

        let dialog = AlertDialog::builder()
            .heading(&gettext(UPDATE_CONFIRM_INSTALL_TITLE))
            .body(&gettext(UPDATE_CONFIRM_INSTALL_BODY))
            .build();
        let cancel = gettext("Cancel");
        let install = gettext(UPDATE_INSTALL_BUTTON_LABEL);
        dialog.add_responses(&[("cancel", cancel.as_str()), ("install", install.as_str())]);
        dialog.set_close_response("cancel");
        dialog.set_default_response(Some("install"));

        let controller_weak = Rc::downgrade(&self.inner);
        dialog.connect_response(None, move |_, response| {
            if response != "install" {
                return;
            }

            let Some(inner) = controller_weak.upgrade() else {
                return;
            };
            let controller = UpdaterController { inner };
            let download = match controller.inner.state.borrow().clone() {
                UpdateState::Ready { download, .. } => download,
                _ => return,
            };
            controller.launch_update(&download);
        });
        dialog.present(Some(&window));
    }

    fn launch_update(&self, download: &DownloadedUpdate) {
        match platform::launch_update(download) {
            Ok(()) => {
                *self.inner.state.borrow_mut() = UpdateState::Installing;
                self.close_dialog();
                self.inner.app.quit();
            }
            Err(error) => {
                log_error(format!("Failed to start update install: {error}"));
                self.show_toast(platform::install_failed_toast());
            }
        }
    }

    fn any_window_has_unsaved_changes(&self) -> bool {
        self.compact_registrations();
        let flags = self
            .inner
            .registrations
            .borrow()
            .iter()
            .map(|registration| (registration.has_unsaved_changes)())
            .collect::<Vec<_>>();
        any_dirty(flags)
    }

    fn preferred_window(&self) -> Option<ApplicationWindow> {
        self.active_window().or_else(|| {
            self.inner
                .registrations
                .borrow()
                .iter()
                .find_map(|registration| registration.window.upgrade())
        })
    }

    fn active_window(&self) -> Option<ApplicationWindow> {
        self.inner
            .app
            .active_window()
            .and_then(|window| window.downcast::<ApplicationWindow>().ok())
    }

    fn show_toast(&self, message: &str) {
        let Some(overlay) = self.preferred_overlay() else {
            return;
        };
        overlay.add_toast(Toast::new(&gettext(message)));
    }

    fn preferred_overlay(&self) -> Option<ToastOverlay> {
        self.compact_registrations();
        let active_window = self.active_window();

        if let Some(active_window) = active_window {
            if let Some(overlay) =
                self.inner
                    .registrations
                    .borrow()
                    .iter()
                    .find_map(|registration| {
                        let window = registration.window.upgrade()?;
                        if window == active_window {
                            registration.overlay.upgrade()
                        } else {
                            None
                        }
                    })
            {
                return Some(overlay);
            }
        }

        self.inner
            .registrations
            .borrow()
            .iter()
            .find_map(|registration| registration.overlay.upgrade())
    }

    fn compact_registrations(&self) {
        self.inner
            .registrations
            .borrow_mut()
            .retain(|registration| {
                registration.window.upgrade().is_some() && registration.overlay.upgrade().is_some()
            });
    }

    fn next_run_id(&self) -> u64 {
        let next = self.inner.next_run_id.get().saturating_add(1);
        self.inner.next_run_id.set(next);
        next
    }
}

fn ensure_controller(app: &Application) -> UpdaterController {
    if let Some(controller) = cloned_data(app, UPDATE_CONTROLLER_KEY) {
        return controller;
    }

    let controller = UpdaterController::new(app);
    set_cloned_data(app, UPDATE_CONTROLLER_KEY, controller.clone());
    controller
}

fn build_update_dialog(controller: &UpdaterController) -> UpdateDialog {
    let heading = Label::new(None);
    heading.set_halign(Align::Start);
    heading.set_xalign(0.0);
    heading.set_wrap(true);
    heading.add_css_class("title-3");

    let body = Label::new(None);
    body.set_halign(Align::Start);
    body.set_xalign(0.0);
    body.set_wrap(true);
    body.add_css_class("dim-label");

    let progress = ProgressBar::new();
    progress.set_hexpand(true);
    progress.set_visible(false);

    let status = Label::new(None);
    status.set_halign(Align::Start);
    status.set_xalign(0.0);
    status.set_wrap(true);
    status.add_css_class("caption");
    status.add_css_class("dim-label");
    status.set_visible(false);

    let later_button = Button::with_label(&gettext(UPDATE_LATER_BUTTON_LABEL));
    let install_button = Button::with_label(&gettext(UPDATE_INSTALL_BUTTON_LABEL));
    install_button.add_css_class("suggested-action");
    install_button.set_sensitive(false);

    let buttons = GtkBox::new(Orientation::Horizontal, 12);
    buttons.set_halign(Align::End);
    buttons.append(&later_button);
    buttons.append(&install_button);

    let content = GtkBox::new(Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.append(&heading);
    content.append(&body);
    content.append(&progress);
    content.append(&status);
    content.append(&buttons);

    let dialog = Dialog::builder()
        .title(&gettext(UPDATE_DIALOG_TITLE))
        .content_width(UPDATE_DIALOG_CONTENT_WIDTH)
        .content_height(UPDATE_DIALOG_CONTENT_HEIGHT)
        .follows_content_size(true)
        .child(&wrapped_dialog_body(&content))
        .build();

    let dialog_for_later = dialog.clone();
    later_button.connect_clicked(move |_| {
        dialog_for_later.force_close();
    });

    let controller_weak = Rc::downgrade(&controller.inner);
    install_button.connect_clicked(move |_| {
        let Some(inner) = controller_weak.upgrade() else {
            return;
        };
        UpdaterController { inner }.begin_install_flow();
    });

    let controller_weak = Rc::downgrade(&controller.inner);
    dialog.connect_closed(move |_| {
        let Some(inner) = controller_weak.upgrade() else {
            return;
        };
        UpdaterController { inner }.handle_dialog_closed();
    });

    UpdateDialog {
        dialog,
        heading,
        body,
        progress,
        status,
        install_button,
    }
}

fn update_dialog_release_copy(dialog: &UpdateDialog, release: &SelectedRelease) {
    dialog.heading.set_label(&gettext("Update available"));
    dialog.body.set_label(&format!(
        "{}\n{}\n{}",
        gettext(platform::update_available_description()),
        format!(
            "{} {}",
            gettext("Current version:"),
            env!("CARGO_PKG_VERSION")
        ),
        format!("{} {}", gettext("Available version:"), release.version),
    ));
}

fn download_release_asset(
    run_id: u64,
    release: &SelectedRelease,
    download: &DownloadedUpdate,
    cancel: &Arc<AtomicBool>,
    tx: &mpsc::Sender<WorkerMessage>,
) -> Option<WorkerMessage> {
    match perform_download(release, download, cancel, tx, run_id) {
        Ok(()) => Some(WorkerMessage::DownloadReady {
            run_id,
            download: download.clone(),
        }),
        Err(DownloadFailure::Cancelled) => Some(WorkerMessage::DownloadCancelled { run_id }),
        Err(DownloadFailure::Error(error)) => Some(WorkerMessage::DownloadFailed { run_id, error }),
    }
}

fn perform_download(
    release: &SelectedRelease,
    download: &DownloadedUpdate,
    cancel: &Arc<AtomicBool>,
    tx: &mpsc::Sender<WorkerMessage>,
    run_id: u64,
) -> Result<(), DownloadFailure> {
    let Some(parent) = download.path.parent() else {
        return Err(DownloadFailure::Error(
            "Update download path has no parent directory.".to_string(),
        ));
    };
    fs::create_dir_all(parent).map_err(download_fs_error("create update download directory"))?;

    if download.path.exists() && !cached_download_matches_release(download, &release.asset) {
        fs::remove_file(&download.path).map_err(download_fs_error("remove stale update file"))?;
    }

    let temp_path = download.path.with_extension("download");
    if temp_path.exists() {
        fs::remove_file(&temp_path)
            .map_err(download_fs_error("remove stale partial update file"))?;
    }

    let mut response = asset_download_client()?
        .get(&release.asset.browser_download_url)
        .send()
        .map_err(download_http_error("send release asset request"))?
        .error_for_status()
        .map_err(download_http_error("download release asset"))?;

    let mut file =
        File::create(&temp_path).map_err(download_fs_error("create partial update file"))?;
    let mut downloaded = 0u64;
    let mut buffer = [0u8; 64 * 1024];

    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = fs::remove_file(&temp_path);
            return Err(DownloadFailure::Cancelled);
        }

        let read = response
            .read(&mut buffer)
            .map_err(download_io_error("read update bytes"))?;
        if read == 0 {
            break;
        }

        file.write_all(&buffer[..read])
            .map_err(download_io_error("write update bytes"))?;
        downloaded = downloaded.saturating_add(read as u64);
        let _ = tx.send(WorkerMessage::DownloadProgress { run_id, downloaded });
    }

    file.flush()
        .map_err(download_io_error("flush partial update file"))?;
    drop(file);

    if cancel.load(Ordering::Relaxed) {
        let _ = fs::remove_file(&temp_path);
        return Err(DownloadFailure::Cancelled);
    }

    if download.size > 0 && downloaded != download.size {
        let _ = fs::remove_file(&temp_path);
        return Err(DownloadFailure::Error(format!(
            "Update size mismatch after download (expected {}, got {}).",
            download.size, downloaded
        )));
    }

    if let Err(error) = validate_downloaded_update(&temp_path, download, &release.asset) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    fs::rename(&temp_path, &download.path).map_err(download_fs_error("finalize update file"))?;
    Ok(())
}

fn fetch_update_release() -> Result<Option<SelectedRelease>, String> {
    let (owner, repo) = repository_owner_and_name()?;
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/releases?per_page={GITHUB_RELEASES_PER_PAGE}"
    );

    let releases = github_http_client()?
        .get(url)
        .send()
        .map_err(http_error("send GitHub release request"))?
        .error_for_status()
        .map_err(http_error("read GitHub release response"))?
        .json::<Vec<GitHubReleaseResponse>>()
        .map_err(http_error("decode GitHub release response"))?;

    let releases = releases
        .into_iter()
        .map(|release| ReleaseCandidate {
            tag_name: release.tag_name,
            draft: release.draft,
            prerelease: release.prerelease,
            assets: release
                .assets
                .into_iter()
                .map(|asset| ReleaseAsset {
                    name: asset.name,
                    browser_download_url: asset.browser_download_url,
                    size: asset.size,
                    sha256_digest: asset.digest,
                })
                .collect(),
        })
        .collect::<Vec<_>>();

    platform::select_update_release(env!("CARGO_PKG_VERSION"), &releases)
}

fn github_http_client() -> Result<Client, String> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static(GITHUB_API_ACCEPT));
    headers.insert(
        "X-GitHub-Api-Version",
        HeaderValue::from_static(GITHUB_API_VERSION),
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        )),
    );

    Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(http_error("build GitHub client"))
}

fn asset_download_client() -> Result<Client, DownloadFailure> {
    Client::builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(download_http_error("build download client"))
}

fn cached_download_matches_release(download: &DownloadedUpdate, asset: &ReleaseAsset) -> bool {
    validate_update_file(&download.path, download.size, asset).is_ok()
}

fn validate_downloaded_update(
    path: &Path,
    download: &DownloadedUpdate,
    asset: &ReleaseAsset,
) -> Result<(), DownloadFailure> {
    validate_update_file(path, download.size, asset).map_err(DownloadFailure::Error)
}

fn validate_update_file(
    path: &Path,
    expected_size: u64,
    asset: &ReleaseAsset,
) -> Result<(), String> {
    if !cached_download_matches_size(path, expected_size) {
        return Err(format!("Update size mismatch for '{}'.", path.display()));
    }

    let expected_digest = parse_release_sha256_digest(asset)?;
    let actual_digest = sha256_file_hex(path)
        .map_err(|error| format!("Failed to hash update file '{}': {error}", path.display()))?;
    if !actual_digest.eq_ignore_ascii_case(expected_digest) {
        return Err(format!("Update SHA-256 mismatch for '{}'.", path.display()));
    }

    Ok(())
}

fn validate_release_asset_digest(asset: &ReleaseAsset) -> Result<(), String> {
    parse_release_sha256_digest(asset).map(|_| ())
}

fn parse_release_sha256_digest(asset: &ReleaseAsset) -> Result<&str, String> {
    let digest = asset.sha256_digest.as_deref().ok_or_else(|| {
        format!(
            "Release asset '{}' is missing a GitHub SHA-256 digest.",
            asset.name
        )
    })?;
    let digest = digest.trim();
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return Err(format!(
            "Release asset '{}' has an unsupported digest format.",
            asset.name
        ));
    };

    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "Release asset '{}' has an invalid SHA-256 digest.",
            asset.name
        ));
    }

    Ok(hex)
}

fn sha256_file_hex(path: &Path) -> Result<String, std::io::Error> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    Ok(hex)
}

fn download_status_label(downloaded: u64, total: u64) -> String {
    if total == 0 {
        return format!("{} {}", gettext("Downloaded"), format_bytes(downloaded));
    }

    let percentage = ((downloaded.min(total) as f64) / (total as f64)) * 100.0;
    format!(
        "{} of {} ({percentage:.0}%)",
        format_bytes(downloaded),
        format_bytes(total),
    )
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;

    if bytes as f64 >= MIB {
        format!("{:.1} MiB", (bytes as f64) / MIB)
    } else if bytes as f64 >= KIB {
        format!("{:.1} KiB", (bytes as f64) / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn weak_ref<T: glib::object::IsA<glib::Object>>(object: &T) -> WeakRef<T> {
    let weak = WeakRef::new();
    weak.set(Some(object));
    weak
}

fn poll_worker<T: Send + 'static>(
    rx: mpsc::Receiver<T>,
    mut handle_message: impl FnMut(T) + 'static,
) {
    glib::timeout_add_local(Duration::from_millis(WORKER_POLL_INTERVAL_MS), move || {
        loop {
            match rx.try_recv() {
                Ok(message) => handle_message(message),
                Err(TryRecvError::Empty) => return glib::ControlFlow::Continue,
                Err(TryRecvError::Disconnected) => return glib::ControlFlow::Break,
            }
        }
    });
}

enum DownloadFailure {
    Cancelled,
    Error(String),
}

fn http_error(context: &'static str) -> impl FnOnce(reqwest::Error) -> String {
    move |error| format!("Failed to {context}: {error}")
}

fn download_http_error(context: &'static str) -> impl FnOnce(reqwest::Error) -> DownloadFailure {
    move |error| DownloadFailure::Error(format!("Failed to {context}: {error}"))
}

fn download_fs_error(context: &'static str) -> impl FnOnce(std::io::Error) -> DownloadFailure {
    move |error| DownloadFailure::Error(format!("Failed to {context}: {error}"))
}

fn download_io_error(context: &'static str) -> impl FnOnce(std::io::Error) -> DownloadFailure {
    move |error| DownloadFailure::Error(format!("Failed to {context}: {error}"))
}
