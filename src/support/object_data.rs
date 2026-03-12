use adw::prelude::*;

pub(crate) fn set_string_data<O: ObjectExt>(obj: &O, key: &str, value: String) {
    unsafe {
        obj.set_data(key, value);
    }
}

pub(crate) fn non_null_to_string_option<O: ObjectExt>(obj: &O, key: &str) -> Option<String> {
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
