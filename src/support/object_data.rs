use adw::prelude::*;

pub fn set_cloned_data<O: ObjectExt, T: Clone + 'static>(obj: &O, key: &str, value: T) {
    unsafe {
        obj.set_data(key, value);
    }
}

pub fn cloned_data<O: ObjectExt, T: Clone + 'static>(obj: &O, key: &str) -> Option<T> {
    unsafe { obj.data::<T>(key) }.map(|ptr| unsafe { ptr.as_ref() }.clone())
}

pub fn set_string_data<O: ObjectExt>(obj: &O, key: &str, value: String) {
    set_cloned_data(obj, key, value);
}

pub fn non_null_to_string_option<O: ObjectExt>(obj: &O, key: &str) -> Option<String> {
    cloned_data(obj, key)
}
