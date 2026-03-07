use crate::item::OpenPassFile;
use adw::prelude::*;
use std::sync::{OnceLock, RwLock};

pub fn non_null_to_string_option<O: ObjectExt>(obj: &O, key: &str) -> Option<String> {
    non_null_to_string_result(unsafe { obj.data::<String>(key) }).ok()
}

fn non_null_to_string_result(label_opt: Option<std::ptr::NonNull<String>>) -> Result<String, ()> {
    if let Some(ptr) = label_opt {
        let s: &String = unsafe { ptr.as_ref() };
        Ok(s.clone())
    } else {
        Err(())
    }
}

fn opened_pass_file_state() -> &'static RwLock<Option<OpenPassFile>> {
    static OPENED_PASS_FILE: OnceLock<RwLock<Option<OpenPassFile>>> = OnceLock::new();
    OPENED_PASS_FILE.get_or_init(|| RwLock::new(None))
}

fn with_opened_pass_file_read<T>(f: impl FnOnce(Option<&OpenPassFile>) -> T) -> T {
    match opened_pass_file_state().read() {
        Ok(current) => f(current.as_ref()),
        Err(poisoned) => {
            let current = poisoned.into_inner();
            f(current.as_ref())
        }
    }
}

fn with_opened_pass_file_write<T>(f: impl FnOnce(&mut Option<OpenPassFile>) -> T) -> T {
    match opened_pass_file_state().write() {
        Ok(mut current) => f(&mut current),
        Err(poisoned) => {
            let mut current = poisoned.into_inner();
            f(&mut current)
        }
    }
}

pub fn set_opened_pass_file(pass_file: OpenPassFile) {
    with_opened_pass_file_write(|current| {
        *current = Some(pass_file);
    });
}

pub fn get_opened_pass_file() -> Option<OpenPassFile> {
    with_opened_pass_file_read(|current| current.cloned())
}

pub fn clear_opened_pass_file() {
    with_opened_pass_file_write(|current| {
        *current = None;
    });
}

pub fn is_opened_pass_file(pass_file: &OpenPassFile) -> bool {
    with_opened_pass_file_read(|current| current == Some(pass_file))
}

pub fn refresh_opened_pass_file_from_contents(
    pass_file: &OpenPassFile,
    contents: &str,
) -> Option<OpenPassFile> {
    with_opened_pass_file_write(|current| {
        let selected = current.as_mut()?;
        if selected != pass_file {
            return None;
        }

        selected.refresh_from_contents(contents);
        Some(selected.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::{
        clear_opened_pass_file, get_opened_pass_file, is_opened_pass_file,
        refresh_opened_pass_file_from_contents, set_opened_pass_file,
    };
    use crate::item::OpenPassFile;
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> &'static Mutex<()> {
        static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        TEST_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn opened_pass_file_state_round_trips() {
        let _guard = test_lock().lock().expect("test lock poisoned");
        clear_opened_pass_file();

        let pass_file = OpenPassFile::from_label("/tmp/store", "work/alice/github");
        set_opened_pass_file(pass_file.clone());

        assert_eq!(get_opened_pass_file(), Some(pass_file.clone()));
        assert!(is_opened_pass_file(&pass_file));

        clear_opened_pass_file();
    }

    #[test]
    fn late_updates_do_not_override_a_newer_selection() {
        let _guard = test_lock().lock().expect("test lock poisoned");
        clear_opened_pass_file();

        let first = OpenPassFile::from_label("/tmp/store", "work/alice/github");
        let second = OpenPassFile::from_label("/tmp/store", "work/bob/gitlab");
        set_opened_pass_file(second.clone());

        let refreshed =
            refresh_opened_pass_file_from_contents(&first, "secret\nusername: stale-user");
        assert_eq!(refreshed, None);
        assert_eq!(get_opened_pass_file(), Some(second));

        clear_opened_pass_file();
    }
}
