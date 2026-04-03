use super::is_writable;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn writability_probe_does_not_truncate_existing_perm_test_files() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("keycord-setup-writable-{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    let existing = dir.join(".perm_test");
    fs::write(&existing, "keep").expect("write marker");

    assert!(is_writable(&dir));
    assert_eq!(
        fs::read_to_string(&existing).expect("read marker"),
        "keep".to_string()
    );

    let _ = fs::remove_dir_all(dir);
}
