use super::recipients::store_uses_fido2_recipients;
use std::collections::HashMap;

#[derive(Default)]
pub struct StoreSupportCache {
    uses_fido2_by_root: HashMap<String, bool>,
}

impl StoreSupportCache {
    pub fn supports_password_read_tools(&mut self, store_root: &str) -> bool {
        !self.uses_fido2_recipients(store_root)
    }

    pub fn supports_advanced_search(
        &mut self,
        store_root: &str,
        uses_advanced_features: bool,
    ) -> bool {
        !uses_advanced_features || self.supports_password_read_tools(store_root)
    }

    fn uses_fido2_recipients(&mut self, store_root: &str) -> bool {
        if let Some(uses_fido2) = self.uses_fido2_by_root.get(store_root) {
            return *uses_fido2;
        }

        let uses_fido2 = store_uses_fido2_recipients(store_root);
        self.uses_fido2_by_root
            .insert(store_root.to_string(), uses_fido2);
        uses_fido2
    }
}

#[cfg(test)]
mod tests {
    use super::StoreSupportCache;

    #[test]
    fn advanced_search_without_advanced_features_always_stays_available() {
        let mut cache = StoreSupportCache::default();
        assert!(cache.supports_advanced_search("/does/not/matter", false));
    }
}
