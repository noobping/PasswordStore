pub(crate) mod actions;
pub(crate) mod background;
#[cfg(not(feature = "flatpak"))]
pub(crate) mod git;
pub(crate) mod object_data;
pub(crate) mod ui;
