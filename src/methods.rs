use adw::prelude::*;

pub fn non_null_to_string_option<O: ObjectExt>(obj: &O, key: &str) -> Option<String> {
    non_null_to_string_result(unsafe { obj.data::<String>(key) }).ok()
}

fn non_null_to_string_result(label_opt: Option<std::ptr::NonNull<String>>) -> Result<String, ()> {
    if let Some(ptr) = label_opt {
        // SAFETY: caller must guarantee the pointer is valid and points to a valid String
        let s: &String = unsafe { ptr.as_ref() };
        Ok(s.clone())
    } else {
        Err(())
    }
}
