use std::fs;
use tempfile::TempDir;
use zpro_code::modules::update_manager::UpdateManager;

fn create_test_manager() -> (UpdateManager, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("updates.log");
    let manager = UpdateManager::new(log_path);
    (manager, temp_dir)
}

#[test]
fn test_update_manager_initialization() {
    let (manager, _temp) = create_test_manager();
    // UpdateManager is private, but we can test it initializes without panicking
    // and verify it doesn't crash on creation
    drop(manager);
}

#[test]
fn test_logging_creates_file() {
    let (manager, _temp) = create_test_manager();
    manager.log_update("TEST", "test message").unwrap();

    let log_content = fs::read_to_string(&_temp.path().join("updates.log")).unwrap();
    assert!(log_content.contains("TEST"));
    assert!(log_content.contains("test message"));
}

#[test]
fn test_logging_appends_multiple_entries() {
    let (manager, _temp) = create_test_manager();
    manager.log_update("ENTRY1", "first").unwrap();
    manager.log_update("ENTRY2", "second").unwrap();

    let log_content = fs::read_to_string(&_temp.path().join("updates.log")).unwrap();
    assert!(log_content.contains("ENTRY1"));
    assert!(log_content.contains("ENTRY2"));

    // Verify order: first entry should appear before second entry
    let entry1_pos = log_content.find("ENTRY1").unwrap();
    let entry2_pos = log_content.find("ENTRY2").unwrap();
    assert!(entry1_pos < entry2_pos);
}

#[test]
fn test_logging_creates_parent_directories() {
    let temp_dir = TempDir::new().unwrap();
    let nested_path = temp_dir
        .path()
        .join("logs")
        .join("nested")
        .join("updates.log");

    let manager = UpdateManager::new(nested_path.clone());
    manager.log_update("TEST", "test").unwrap();

    assert!(nested_path.exists());
    let content = fs::read_to_string(&nested_path).unwrap();
    assert!(content.contains("TEST"));
}

#[test]
fn test_logging_with_special_characters() {
    let (manager, _temp) = create_test_manager();
    let special_message = "Message with: special@chars!#$%^&*()";

    manager.log_update("SPECIAL", special_message).unwrap();

    let log_content = fs::read_to_string(&_temp.path().join("updates.log")).unwrap();
    assert!(log_content.contains(special_message));
}
