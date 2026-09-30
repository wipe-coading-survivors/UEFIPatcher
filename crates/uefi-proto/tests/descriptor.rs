use uefi_proto::descriptor;

#[test]
fn pool_contains_all_service_methods() {
    let names = descriptor::method_names();
    assert!(names.contains(&"ImageNodesList".to_string()));
    assert!(names.contains(&"HiiUnlock".to_string()));
    assert!(names.contains(&"NvarSet".to_string()));
    assert!(names.contains(&"ImageSnapshotRestore".to_string()));
    assert!(names.contains(&"ImageUpload".to_string()));
    assert!(names.contains(&"HiiGetValue".to_string()));
    assert_eq!(names.len(), 41);
}

#[test]
fn method_input_output_resolvable() {
    let m = descriptor::method("ImageNodesList").unwrap();
    assert_eq!(m.name(), "ImageNodesList");
    assert_eq!(m.input().full_name(), "engine.ImageNodesListRequest");
    assert_eq!(m.output().full_name(), "engine.ImageNodesResponse");
}

#[test]
fn unknown_method_is_none() {
    assert!(descriptor::method("NoSuchMethod").is_none());
}
