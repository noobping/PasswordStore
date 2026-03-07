#[allow(dead_code)]
#[cfg(debug_assertions)]
pub const APP_ID: &str = concat!("io.github.noobping.", env!("CARGO_PKG_NAME"), ".develop");

#[allow(dead_code)]
#[cfg(not(debug_assertions))]
pub const APP_ID: &str = concat!("io.github.noobping.", env!("CARGO_PKG_NAME"));

pub const RESOURCE_ID: &str = concat!("/io/github/noobping/", env!("CARGO_PKG_NAME"));
