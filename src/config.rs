pub const APP_ID: &str = concat!("dev.noobping.", env!("CARGO_PKG_NAME"));

#[cfg(feature = "setup")]
pub const RESOURCE_ID: &str = concat!("/dev/noobping/", env!("CARGO_PKG_NAME"));
