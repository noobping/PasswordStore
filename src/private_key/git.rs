use crate::backend::{
    git_commit_private_key_requiring_unlock_for_entry,
    git_commit_private_key_requiring_unlock_for_store_recipients,
};
use crate::logging::log_error;
use crate::private_key::unlock::prompt_private_key_unlock_for_action;
use crate::support::runtime::git_network_operations_available;
use adw::ToastOverlay;
use std::rc::Rc;

fn prompt_private_key_unlock_for_git_commit_if_needed(
    overlay: &ToastOverlay,
    fingerprint: Result<Option<String>, String>,
    context: &str,
    after_unlock: Rc<dyn Fn()>,
) -> bool {
    if !git_network_operations_available() {
        return false;
    }

    match fingerprint {
        Ok(Some(fingerprint)) => {
            prompt_private_key_unlock_for_action(overlay, fingerprint, after_unlock);
            true
        }
        Ok(None) => false,
        Err(err) => {
            log_error(format!(
                "Failed to resolve the private key needed to sign the Git commit for {context}: {err}"
            ));
            false
        }
    }
}

pub(crate) fn prompt_private_key_unlock_for_entry_git_commit_if_needed(
    overlay: &ToastOverlay,
    store_root: &str,
    label: &str,
    after_unlock: Rc<dyn Fn()>,
) -> bool {
    prompt_private_key_unlock_for_git_commit_if_needed(
        overlay,
        git_commit_private_key_requiring_unlock_for_entry(store_root, label),
        label,
        after_unlock,
    )
}

pub(crate) fn prompt_private_key_unlock_for_store_git_commit_if_needed(
    overlay: &ToastOverlay,
    store_root: &str,
    recipients: &[String],
    after_unlock: Rc<dyn Fn()>,
) -> bool {
    prompt_private_key_unlock_for_git_commit_if_needed(
        overlay,
        git_commit_private_key_requiring_unlock_for_store_recipients(store_root, recipients),
        store_root,
        after_unlock,
    )
}
