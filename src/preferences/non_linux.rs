use super::{BackendKind, Preferences};

impl Preferences {
    pub fn backend_kind(&self) -> BackendKind {
        BackendKind::Integrated
    }

    pub fn uses_integrated_backend(&self) -> bool {
        true
    }
}
