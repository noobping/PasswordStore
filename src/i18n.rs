use std::path::{Path, PathBuf};

const DOMAIN: &str = env!("GETTEXT_DOMAIN");
const DEFAULT_LOCALEDIR: &str = env!("LOCALEDIR");
const AVAILABLE_LOCALES: &str = env!("AVAILABLE_LOCALES");

pub fn domain() -> &'static str {
    DOMAIN
}

pub fn init() {
    #[cfg(target_os = "linux")]
    linux::init();
}

pub fn gettext(message: &str) -> String {
    if message.is_empty() {
        return String::new();
    }

    #[cfg(target_os = "linux")]
    {
        linux::gettext(message)
    }

    #[cfg(not(target_os = "linux"))]
    {
        message.to_string()
    }
}

fn available_locales() -> impl Iterator<Item = &'static str> {
    AVAILABLE_LOCALES
        .split(':')
        .filter(|locale| !locale.is_empty())
}

fn default_locale_dir() -> PathBuf {
    PathBuf::from(DEFAULT_LOCALEDIR)
}

fn runtime_locale_dir_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(locale_dir) = std::env::var("KEYCORD_LOCALEDIR") {
        candidates.push(PathBuf::from(locale_dir));
    }

    candidates.push(PathBuf::from("/app/share/locale"));
    candidates.push(PathBuf::from("/usr/local/share/locale"));
    candidates.push(PathBuf::from("/usr/share/locale"));
    candidates.push(default_locale_dir());

    if let Some(data_dir) = dirs_next::data_dir() {
        candidates.push(data_dir.join("locale"));
    }

    candidates
}

fn has_domain_catalog(locale_dir: &Path) -> bool {
    available_locales().any(|locale| {
        locale_dir
            .join(locale)
            .join("LC_MESSAGES")
            .join(format!("{DOMAIN}.mo"))
            .exists()
    })
}

fn preferred_locale_dir() -> PathBuf {
    runtime_locale_dir_candidates()
        .into_iter()
        .find(|candidate| has_domain_catalog(candidate))
        .or_else(|| {
            runtime_locale_dir_candidates()
                .into_iter()
                .find(|candidate| candidate.exists())
        })
        .unwrap_or_else(default_locale_dir)
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{domain, preferred_locale_dir};
    use libc::{c_char, LC_ALL};
    use std::ffi::{CStr, CString};
    use std::sync::Once;

    static INIT: Once = Once::new();

    unsafe extern "C" {
        #[link_name = "bindtextdomain"]
        fn bindtextdomain_raw(domainname: *const c_char, dirname: *const c_char) -> *mut c_char;
        #[link_name = "bind_textdomain_codeset"]
        fn bind_textdomain_codeset_raw(
            domainname: *const c_char,
            codeset: *const c_char,
        ) -> *mut c_char;
        #[link_name = "textdomain"]
        fn textdomain_raw(domainname: *const c_char) -> *mut c_char;
        #[link_name = "gettext"]
        fn gettext_raw(msgid: *const c_char) -> *mut c_char;
        fn setlocale(category: libc::c_int, locale: *const c_char) -> *mut c_char;
    }

    pub fn init() {
        INIT.call_once(|| {
            let empty_locale = CString::new("").expect("CString::new failed for locale");
            let domain = CString::new(domain()).expect("CString::new failed for gettext domain");
            let locale_dir = preferred_locale_dir();
            let locale_dir =
                CString::new(locale_dir.to_string_lossy().as_bytes()).expect("Bad locale path");
            let codeset = CString::new("UTF-8").expect("CString::new failed for UTF-8");

            unsafe {
                setlocale(LC_ALL, empty_locale.as_ptr());
                bindtextdomain_raw(domain.as_ptr(), locale_dir.as_ptr());
                bind_textdomain_codeset_raw(domain.as_ptr(), codeset.as_ptr());
                textdomain_raw(domain.as_ptr());
            }
        });
    }

    pub fn gettext(message: &str) -> String {
        init();
        translate(message, |message| unsafe { gettext_raw(message) })
    }
    fn translate(message: &str, translate: impl FnOnce(*const c_char) -> *mut c_char) -> String {
        let Ok(message) = CString::new(message) else {
            return message.to_string();
        };

        unsafe {
            let translated = translate(message.as_ptr());
            if translated.is_null() {
                return message.to_string_lossy().into_owned();
            }

            CStr::from_ptr(translated).to_string_lossy().into_owned()
        }
    }
}
