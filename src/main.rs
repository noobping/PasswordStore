#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

#[cfg(all(
    target_os = "linux",
    any(
        feature = "setup",
        all(feature = "linux-updater", not(feature = "flatpak"))
    )
))]
mod setup;

mod backend;
mod clipboard;
mod fido2_recipient;
mod i18n;
#[cfg(feature = "logging")]
mod logging;
#[cfg(not(feature = "logging"))]
#[path = "logging/disabled.rs"]
mod logging;
mod password;
mod preferences;
mod private_key;
#[cfg(target_os = "linux")]
mod search_provider;
mod store;
mod support;
mod updater;
mod window;

use crate::i18n::gettext;
use crate::logging::{log_error, run_command_output, CommandLogOptions};
use crate::password::model::OpenPassFile;
use crate::preferences::Preferences;
use crate::support::hardening::apply_process_hardening;
use crate::support::object_data::{
    cloned_data, set_cloned_data, set_string_data, take_data, take_string_data,
};
use crate::support::runtime::handle_unsupported_host_command_invocation;
#[cfg(feature = "legacy-compat")]
use crate::support::startup::{
    fatal_startup_error, prompt_startup_recovery_dialog, show_startup_error_dialog,
    StartupRecoveryChoice,
};
#[cfg(feature = "platform-theme")]
use crate::support::theme::install_color_scheme_tracking;
use crate::window::navigation::APP_WINDOW_TITLE;

use adw::gio::SimpleAction;
use adw::gtk::{
    gdk::Display,
    gio::{resources_register_include, ApplicationFlags},
    glib::ExitCode,
    Builder, IconTheme, License, ShortcutsWindow,
};
use adw::prelude::*;
use adw::Application;
#[cfg(target_os = "windows")]
use dirs_next::cache_dir;
use std::ffi::OsString;
#[cfg(target_os = "windows")]
use std::fs;
#[cfg(target_os = "windows")]
use std::hash::{Hash, Hasher};
#[cfg(any(target_os = "windows", test))]
use std::path::Path;
#[cfg(target_os = "windows")]
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use winsafe::{self as w, co};

const APP_ID: &str = env!("APP_ID");
const RESOURCE_ID: &str = env!("RESOURCE_ID");
const ISSUE_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/issues");
const MAIN_WINDOW_ACTIVATING_KEY: &str = "main-window-activating";
const RIPASSO_VERSION: &str = env!("RIPASSO_VERSION");
const SEQUOIA_OPENPGP_VERSION: &str = env!("SEQUOIA_OPENPGP_VERSION");
const SHORTCUTS_UI: &str = include_str!("../data/shortcuts.ui");

fn main() -> ExitCode {
    let args = std::env::args_os().collect::<Vec<_>>();
    if handle_unsupported_host_command_invocation(&args) {
        return 126.into();
    }

    if let Some(code) = updater::handle_special_command(&args) {
        return code;
    }

    #[cfg(target_os = "linux")]
    if search_provider::is_search_provider_command(&args) {
        return search_provider::run();
    }

    i18n::init();
    if let Err(err) = apply_process_hardening() {
        log_error(format!("Failed to apply process hardening: {err}"));
    }
    #[cfg(target_os = "windows")]
    configure_windows_runtime_environment();
    if let Err(err) = resources_register_include!("compiled.gresource") {
        #[cfg(feature = "legacy-compat")]
        {
            return fatal_startup_error(APP_WINDOW_TITLE, "Failed to register resources.", err);
        }
        #[cfg(not(feature = "legacy-compat"))]
        {
            return nonlegacy_startup_error("Failed to register resources.", &err.to_string());
        }
    }

    if let Err(err) = adw::init() {
        #[cfg(feature = "legacy-compat")]
        {
            return fatal_startup_error(APP_WINDOW_TITLE, "Failed to initialize libadwaita.", err);
        }
        #[cfg(not(feature = "legacy-compat"))]
        {
            return nonlegacy_startup_error("Failed to initialize libadwaita.", &err.to_string());
        }
    }

    let Some(display) = Display::default() else {
        #[cfg(feature = "legacy-compat")]
        {
            return fatal_startup_error(
                APP_WINDOW_TITLE,
                "No display available.",
                "missing display",
            );
        }
        #[cfg(not(feature = "legacy-compat"))]
        {
            return nonlegacy_startup_error("No display available.", "missing display");
        }
    };
    #[cfg(feature = "platform-theme")]
    install_color_scheme_tracking(&display);
    let theme = IconTheme::for_display(&display);
    theme.add_resource_path(RESOURCE_ID);
    #[cfg(target_os = "windows")]
    add_windows_icon_search_path(&theme);

    match backend::prepare_startup() {
        Ok(backend::StartupPreparation::Ready) => {}
        #[cfg(feature = "legacy-compat")]
        Ok(backend::StartupPreparation::RecoveryRequired(recovery)) => {
            let choice = prompt_startup_recovery_dialog(APP_WINDOW_TITLE, recovery.detail());
            if choice == StartupRecoveryChoice::Quit {
                return 0.into();
            }
            if let Err(err) = backend::continue_after_startup_recovery(&recovery) {
                return fatal_startup_error(
                    APP_WINDOW_TITLE,
                    "Failed to recover incompatible managed private-key data.",
                    err,
                );
            }
        }
        Err(err) => {
            #[cfg(feature = "legacy-compat")]
            {
                return fatal_startup_error(
                    APP_WINDOW_TITLE,
                    "Failed to prepare managed private-key storage.",
                    err,
                );
            }
            #[cfg(not(feature = "legacy-compat"))]
            {
                return nonlegacy_startup_error(
                    "Failed to prepare managed private-key storage.",
                    &err,
                );
            }
        }
    }

    // Create the application
    let app = Application::builder()
        .application_id(APP_ID)
        .flags(ApplicationFlags::HANDLES_OPEN | ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    // keyboard shortcuts
    app.set_accels_for_action("app.about", &["F1"]);
    register_app_actions(&app);

    // When the desktop asks us to "open" something, just activate the app
    {
        app.connect_open(|app, _files, _hint| {
            app.activate();
        });
    }

    // Handle command-line arguments
    {
        app.connect_command_line(|app, cmd| {
            let args = cmd.arguments();
            if let Some(pass_file) = command_line_pass_file(&args) {
                set_cloned_data(app, "open-pass-file", pass_file);
            } else if let Some(query) = command_line_query(&args) {
                set_string_data(app, "query", query);
            }
            app.activate(); // continue normal startup path

            0.into()
        });
    }

    app.connect_shutdown(|_| {
        backend::clear_runtime_secret_state();
    });
    {
        let app_for_shutdown = app.clone();
        app.connect_shutdown(move |_| {
            updater::shutdown(&app_for_shutdown);
        });
    }

    // When the app is activated, create and show the main window
    app.connect_activate(|app| {
        let Some(_activation_guard) = MainWindowActivationGuard::acquire(app) else {
            return;
        };

        let query = take_string_data(app, "query");
        let pass_file = take_data(app, "open-pass-file");
        if let Some(window) = existing_main_window(app) {
            window::dispatch_main_window_command(&window, query, pass_file);
            window.present();
            return;
        }

        match window::create_main_window(app, query, pass_file) {
            Ok(win) => {
                win.present();
                updater::after_window_presented(app, &win);
            }
            Err(err) => {
                #[cfg(feature = "legacy-compat")]
                let _ =
                    fatal_startup_error(APP_WINDOW_TITLE, "Failed to build the main window.", err);
                #[cfg(not(feature = "legacy-compat"))]
                {
                    report_nonlegacy_startup_error("Failed to build the main window.", &err);
                }
                app.quit();
            }
        }
    });

    app.run()
}

struct MainWindowActivationGuard {
    app: Application,
}

impl MainWindowActivationGuard {
    fn acquire(app: &Application) -> Option<Self> {
        if cloned_data::<_, bool>(app, MAIN_WINDOW_ACTIVATING_KEY).unwrap_or(false) {
            return None;
        }

        set_cloned_data(app, MAIN_WINDOW_ACTIVATING_KEY, true);
        Some(Self { app: app.clone() })
    }
}

impl Drop for MainWindowActivationGuard {
    fn drop(&mut self) {
        set_cloned_data(&self.app, MAIN_WINDOW_ACTIVATING_KEY, false);
    }
}

fn existing_main_window(app: &Application) -> Option<adw::ApplicationWindow> {
    app.active_window()
        .and_then(|window| window.downcast::<adw::ApplicationWindow>().ok())
        .or_else(|| {
            app.windows()
                .into_iter()
                .find_map(|window| window.downcast::<adw::ApplicationWindow>().ok())
        })
}

fn command_line_pass_file(args: &[OsString]) -> Option<OpenPassFile> {
    if args.get(1).is_none_or(|arg| arg != "--open-entry") {
        return None;
    }

    let store_root = args.get(2)?.to_string_lossy().into_owned();
    let label = args.get(3)?.to_string_lossy().into_owned();
    if store_root.is_empty() || label.is_empty() {
        return None;
    }

    Some(OpenPassFile::from_label(store_root, label))
}

fn command_line_query(args: &[OsString]) -> Option<String> {
    if args.len() <= 1 || args.get(1).is_some_and(|arg| arg == "--open-entry") {
        return None;
    }

    args[1..]
        .join(&OsString::from(" "))
        .into_string()
        .ok()
        .filter(|query| !query.is_empty())
}

#[cfg(not(feature = "legacy-compat"))]
fn report_nonlegacy_startup_error(summary: &str, error: &str) {
    let detail = format!("{summary}\nerror: {error}");
    log_error(&detail);
    eprintln!("{APP_WINDOW_TITLE}: {detail}");
    #[cfg(target_os = "windows")]
    show_windows_startup_error_dialog(APP_WINDOW_TITLE, &detail);
}

#[cfg(not(feature = "legacy-compat"))]
fn nonlegacy_startup_error(summary: &str, error: &str) -> ExitCode {
    report_nonlegacy_startup_error(summary, error);
    1.into()
}

#[cfg(target_os = "windows")]
fn show_windows_startup_error_dialog(title: &str, body: &str) {
    let _ = w::HWND::GetDesktopWindow().MessageBox(body, title, co::MB::OK | co::MB::ICONERROR);
}

#[cfg(target_os = "windows")]
fn configure_windows_runtime_environment() {
    let Some(root) = windows_runtime_root() else {
        return;
    };

    set_windows_env_path_if_exists("GTK_EXE_PREFIX", &root);
    set_windows_env_path_if_exists("GTK_DATA_PREFIX", &root);

    let share = root.join("share");
    prepend_windows_env_path("XDG_DATA_DIRS", &share);
    prepend_windows_env_path("XDG_CONFIG_DIRS", &root.join("etc"));

    let schemas = share.join("glib-2.0").join("schemas");
    if schemas.join("gschemas.compiled").is_file() {
        set_windows_env_path_if_exists("GSETTINGS_SCHEMA_DIR", &schemas);
    }

    let pixbuf_root = root.join("lib").join("gdk-pixbuf-2.0").join("2.10.0");
    let pixbuf_modules = pixbuf_root.join("loaders");
    let pixbuf_cache = rewritten_windows_pixbuf_cache(&root, &pixbuf_root, &pixbuf_modules)
        .unwrap_or_else(|| pixbuf_root.join("loaders.cache"));
    if pixbuf_cache.is_file() {
        set_windows_env_path_if_exists("GDK_PIXBUF_MODULE_FILE", &pixbuf_cache);
    }
    prepend_windows_env_path("GDK_PIXBUF_MODULEDIR", &pixbuf_modules);
}

#[cfg(target_os = "windows")]
fn windows_runtime_root() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

#[cfg(target_os = "windows")]
fn set_windows_env_path_if_exists(name: &str, path: &Path) {
    if path.exists() {
        std::env::set_var(name, path);
    }
}

#[cfg(target_os = "windows")]
fn prepend_windows_env_path(name: &str, path: &Path) {
    if !path.exists() {
        return;
    }

    let mut paths = std::env::var_os(name)
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    if !paths.iter().any(|existing| existing == path) {
        paths.insert(0, path.to_path_buf());
    }
    if let Ok(joined) = std::env::join_paths(paths) {
        std::env::set_var(name, joined);
    }
}

#[cfg(target_os = "windows")]
fn rewritten_windows_pixbuf_cache(
    runtime_root: &Path,
    pixbuf_root: &Path,
    pixbuf_modules: &Path,
) -> Option<PathBuf> {
    let source_cache = pixbuf_root.join("loaders.cache");
    if !source_cache.is_file() || !pixbuf_modules.is_dir() {
        return None;
    }

    let source = fs::read_to_string(&source_cache).ok()?;
    let rewritten = rewrite_pixbuf_loader_cache(&source, pixbuf_modules);
    let output = windows_pixbuf_cache_output_path(runtime_root)?;
    let parent = output.parent()?;
    fs::create_dir_all(parent).ok()?;

    let should_write = fs::read_to_string(&output)
        .map(|existing| existing != rewritten)
        .unwrap_or(true);
    if should_write && fs::write(&output, rewritten).is_err() {
        return None;
    }

    Some(output)
}

#[cfg(target_os = "windows")]
fn windows_pixbuf_cache_output_path(runtime_root: &Path) -> Option<PathBuf> {
    let base = cache_dir().unwrap_or_else(std::env::temp_dir);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    runtime_root.hash(&mut hasher);
    let hash = hasher.finish();
    Some(
        base.join(APP_ID)
            .join("gdk-pixbuf")
            .join(format!("loaders-{hash:016x}.cache")),
    )
}

#[cfg(any(target_os = "windows", test))]
fn rewrite_pixbuf_loader_cache(source: &str, pixbuf_modules: &Path) -> String {
    let loader_dir = pixbuf_modules.display().to_string().replace('\\', "/");
    source
        .lines()
        .map(|line| {
            if line.starts_with("# LoaderDir = ") {
                return format!("# LoaderDir = {loader_dir}");
            }

            let Some(loader_name) = quoted_pixbuf_loader_name(line) else {
                return line.to_string();
            };

            let rewritten = pixbuf_modules
                .join(loader_name)
                .display()
                .to_string()
                .replace('\\', "/");
            format!("\"{rewritten}\"")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(any(target_os = "windows", test))]
fn quoted_pixbuf_loader_name(line: &str) -> Option<&str> {
    let inner = line.strip_prefix('"')?.strip_suffix('"')?;
    let name = inner.rsplit(['/', '\\']).next()?;
    let ext = name.rsplit('.').next()?;
    if name.is_empty() || !ext.eq_ignore_ascii_case("dll") {
        return None;
    }
    Some(name)
}

#[cfg(target_os = "windows")]
fn add_windows_icon_search_path(theme: &IconTheme) {
    if let Some(path) = windows_icon_search_path() {
        theme.add_search_path(path);
    }
}

#[cfg(target_os = "windows")]
fn windows_icon_search_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|dir| dir.join("share").join("icons")))
        .filter(|path| path.is_dir())
}

fn register_app_actions(app: &Application) {
    updater::register_app_actions(app);

    let about_action = SimpleAction::new("about", None);
    let app_for_about = app.clone();
    about_action.connect_activate(move |_, _| {
        let about = build_about_dialog();
        let active_window = app_for_about.active_window();
        about.present(active_window.as_ref());
    });
    app.add_action(&about_action);

    let shortcuts_action = SimpleAction::new("shortcuts", None);
    let app_for_shortcuts = app.clone();
    shortcuts_action.connect_activate(move |_, _| match build_shortcuts_window() {
        Ok(shortcuts) => {
            if let Some(active_window) = app_for_shortcuts.active_window() {
                shortcuts.set_transient_for(Some(&active_window));
            }
            shortcuts.present();
        }
        Err(err) => {
            log_error(format!(
                "Failed to build the shortcuts window.\nerror: {err}"
            ));
            #[cfg(feature = "legacy-compat")]
            show_startup_error_dialog(
                APP_WINDOW_TITLE,
                &gettext("Couldn't open the shortcuts window."),
            );
        }
    });
    app.add_action(&shortcuts_action);
}

fn build_shortcuts_window() -> Result<ShortcutsWindow, String> {
    let builder = Builder::from_string(SHORTCUTS_UI);
    builder
        .object("shortcuts_window")
        .ok_or_else(|| "Failed to build shortcuts window.".to_string())
}

fn build_about_dialog() -> adw::AboutDialog {
    let application_name = gettext(APP_WINDOW_TITLE);
    let authors: Vec<_> = env!("CARGO_PKG_AUTHORS").split(':').collect();
    let developer_name = authors
        .first()
        .map(|author| author_display_name(author.trim()))
        .unwrap_or(application_name.as_str());
    let about = adw::AboutDialog::builder()
        .application_name(&application_name)
        .application_icon(APP_ID)
        .version(env!("CARGO_PKG_VERSION"))
        .developer_name(developer_name)
        .developers(&authors[..])
        .comments(about_comments(&application_name))
        .translator_credits(gettext("Translated by Nick."))
        .license_type(License::Gpl30Only)
        .website(env!("CARGO_PKG_HOMEPAGE"))
        .issue_url(ISSUE_URL)
        .support_url(ISSUE_URL)
        .build();
    about.add_link(&gettext("Repository"), env!("CARGO_PKG_REPOSITORY"));
    about
}

fn author_display_name(author: &str) -> &str {
    author.split_once(" <").map_or(author, |(name, _)| name)
}

fn about_comments(project: &str) -> String {
    let comments = gettext(option_env!("CARGO_PKG_DESCRIPTION").unwrap_or(""));
    let settings = Preferences::new();
    let backend_details = if settings.uses_integrated_backend() {
        format!(
            "{} {RIPASSO_VERSION}\n{} {SEQUOIA_OPENPGP_VERSION}",
            gettext("backend: ripasso"),
            gettext("sequoia-openpgp")
        )
    } else {
        get_pass_version(&settings).map_or_else(
            || gettext("backend: host"),
            |version| format!("{}\n{version}", gettext("backend: host")),
        )
    };

    if comments.is_empty() {
        backend_details
    } else {
        format!("{project}: {comments}\n\n{backend_details}")
    }
}

fn get_pass_version(settings: &Preferences) -> Option<String> {
    let mut cmd = settings.command();
    cmd.arg("--version");
    let output =
        run_command_output(&mut cmd, "Read pass version", CommandLogOptions::DEFAULT).ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<String> = stdout
        .lines()
        .map(str::trim)
        .map(|line| line.trim_matches('='))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        command_line_pass_file, command_line_query, quoted_pixbuf_loader_name,
        rewrite_pixbuf_loader_cache,
    };
    use std::ffi::OsString;
    use std::path::Path;

    #[test]
    fn open_entry_command_line_is_parsed() {
        let args = vec![
            OsString::from("keycord"),
            OsString::from("--open-entry"),
            OsString::from("/tmp/store"),
            OsString::from("work/alice/github"),
        ];

        let pass_file = command_line_pass_file(&args).expect("expected pass file");
        assert_eq!(pass_file.store_path(), "/tmp/store");
        assert_eq!(pass_file.label(), "work/alice/github".to_string());
        assert_eq!(command_line_query(&args), None);
    }

    #[test]
    fn free_form_arguments_become_a_query() {
        let args = vec![
            OsString::from("keycord"),
            OsString::from("find"),
            OsString::from("otp"),
            OsString::from("and"),
            OsString::from("user"),
            OsString::from("alice"),
        ];

        assert_eq!(
            command_line_query(&args),
            Some("find otp and user alice".to_string())
        );
        assert!(command_line_pass_file(&args).is_none());
    }

    #[test]
    fn pixbuf_loader_cache_rewrite_uses_runtime_loader_dir() {
        let source = concat!(
            "# LoaderDir = C:/tools/msys64/mingw64/lib/gdk-pixbuf-2.0/2.10.0/loaders\n",
            "\"C:/tools/msys64/mingw64/lib/gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-svg.dll\"\n",
            "\"svg\" 6 \"gdk-pixbuf\" \"Scalable Vector Graphics\" \"LGPL\""
        );
        let modules = Path::new(
            r"C:\Users\nick\AppData\Local\Programs\Keycord\lib\gdk-pixbuf-2.0\2.10.0\loaders",
        );

        let rewritten = rewrite_pixbuf_loader_cache(source, modules);

        assert!(rewritten.contains(
            "# LoaderDir = C:/Users/nick/AppData/Local/Programs/Keycord/lib/gdk-pixbuf-2.0/2.10.0/loaders"
        ));
        assert!(rewritten.contains(
            "\"C:/Users/nick/AppData/Local/Programs/Keycord/lib/gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-svg.dll\""
        ));
        assert!(
            rewritten.contains("\"svg\" 6 \"gdk-pixbuf\" \"Scalable Vector Graphics\" \"LGPL\"")
        );
    }

    #[test]
    fn pixbuf_loader_name_only_matches_loader_path_lines() {
        assert_eq!(
            quoted_pixbuf_loader_name("\"C:/msys64/libpixbufloader-svg.dll\""),
            Some("libpixbufloader-svg.dll")
        );
        assert_eq!(quoted_pixbuf_loader_name("\"svg\" 6 \"gdk-pixbuf\""), None);
        assert_eq!(quoted_pixbuf_loader_name("# LoaderDir = C:/tmp"), None);
    }
}
