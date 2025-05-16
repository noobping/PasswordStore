use gtk::glib::MainContext;
use std::thread;

pub fn run<F, R>(work: impl FnOnce() -> R + Send + 'static, update_ui: F)
where
    R: Send + 'static,
    F: FnOnce(R) + Send + 'static,
{
    thread::spawn(move || MainContext::default().invoke(move || update_ui(work())));
}
