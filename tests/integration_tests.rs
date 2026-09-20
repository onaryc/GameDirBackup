use file_tree_json::{build_flat_tree, build_tree, TreeNode};
use std::fs;
use std::path::Path;
use std::fs::File;
use tempfile::tempdir;

#[test]
fn test_integration_tree_output_format() {
    let dir = tempdir().unwrap();
    File::create(dir.path().join("test.txt")).unwrap();

    let tree = build_tree(dir.path()).unwrap();
    let json = serde_json::to_string(&tree).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.get("name").is_some());
    assert!(parsed.get("path").is_some());
    assert!(parsed.get("is_directory").is_some());
    assert!(parsed.get("children").is_some());
}

#[test]
fn test_integration_flat_output_format() {
    let dir = tempdir().unwrap();
    File::create(dir.path().join("test.txt")).unwrap();

    let flat = build_flat_tree(dir.path()).unwrap();
    let json = serde_json::to_string(&flat).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_array());

    let first = &parsed[0];
    assert!(first.get("name").is_some());
    assert!(first.get("path").is_some());
    assert!(first.get("is_directory").is_some());
    assert!(first.get("parent").is_some());
}

#[test]
fn test_integration_complex_structure() {
    let dir = tempdir().unwrap();

    fs::create_dir(dir.path().join("src")).unwrap();
    fs::create_dir(dir.path().join("src/lib")).unwrap();
    File::create(dir.path().join("README.md")).unwrap();
    File::create(dir.path().join("src/main.rs")).unwrap();
    File::create(dir.path().join("src/lib/utils.rs")).unwrap();

    let tree = build_tree(dir.path()).unwrap();

    assert_eq!(tree.children.len(), 2);

    let src_dir = tree.children.iter().find(|n| n.name == "src").unwrap();
    assert_eq!(src_dir.children.len(), 2);

    let lib_dir = src_dir.children.iter().find(|n| n.name == "lib").unwrap();
    assert_eq!(lib_dir.children.len(), 1);
    assert_eq!(lib_dir.children[0].name, "utils.rs");

    let flat = build_flat_tree(dir.path()).unwrap();
    assert_eq!(flat.len(), 6);

    let readme = flat.iter().find(|n| n.name == "README.md").unwrap();
    assert_eq!(readme.parent, Some(dir.path().to_string_lossy().into_owned()));
}

#[test]
fn test_integration_target_directory_excluded() {
    let dir = tempdir().unwrap();

    fs::create_dir(dir.path().join("target")).unwrap();
    fs::create_dir(dir.path().join("target/debug")).unwrap();
    File::create(dir.path().join("target/debug/app")).unwrap();
    File::create(dir.path().join("main.rs")).unwrap();

    let tree = build_tree(dir.path()).unwrap();
    let flat = build_flat_tree(dir.path()).unwrap();

    let tree_has_target = tree.children.iter().any(|n| n.name == "target");
    let flat_has_target = flat.iter().any(|n| n.name == "target");

    assert!(!tree_has_target, "Tree mode should exclude target directory");
    assert!(!flat_has_target, "Flat mode should exclude target directory");
}

#[test]
fn test_integration_special_characters_in_filenames() {
    let dir = tempdir().unwrap();

    let special_filename = "test file with spaces.txt";
    File::create(dir.path().join(special_filename)).unwrap();

    let tree = build_tree(dir.path()).unwrap();
    let flat = build_flat_tree(dir.path()).unwrap();

    let tree_has_file = tree.children.iter().any(|n| n.name == special_filename);
    let flat_has_file = flat.iter().any(|n| n.name == special_filename);

    assert!(tree_has_file);
    assert!(flat_has_file);
}

#[test]
fn test_integration_deeply_nested_structure() {
    let dir = tempdir().unwrap();

    let deep_path = dir.path().join("a").join("b").join("c").join("d");
    fs::create_dir_all(&deep_path).unwrap();
    File::create(deep_path.join("deep.txt")).unwrap();

    let tree = build_tree(dir.path()).unwrap();
    let flat = build_flat_tree(dir.path()).unwrap();

    let mut current = &tree;
    for level in ["a", "b", "c", "d"] {
        current = current.children.iter().find(|n| n.name == level).unwrap();
    }
    assert_eq!(current.children[0].name, "deep.txt");

    let deep_file = flat.iter().find(|n| n.name == "deep.txt").unwrap();
    assert_eq!(deep_file.parent, Some(deep_path.to_string_lossy().into_owned()));
}

#[test]
fn test_integration_empty_file() {
    let dir = tempdir().unwrap();

    File::create(dir.path().join("empty.txt")).unwrap();

    let tree = build_tree(dir.path()).unwrap();
    let flat = build_flat_tree(dir.path()).unwrap();

    let empty_file_tree = tree.children.iter().find(|n| n.name == "empty.txt").unwrap();
    let empty_file_flat = flat.iter().find(|n| n.name == "empty.txt").unwrap();

    assert_eq!(empty_file_tree.is_directory, false);
    assert_eq!(empty_file_flat.is_directory, false);
}

#[test]
fn test_integration_symlink_handling() {
    let dir = tempdir().unwrap();

    File::create(dir.path().join("original.txt")).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink("original.txt", dir.path().join("link.txt")).unwrap();

        let tree = build_tree(dir.path()).unwrap();
        let flat = build_flat_tree(dir.path()).unwrap();

        let link_in_tree = tree.children.iter().any(|n| n.name == "link.txt");
        let link_in_flat = flat.iter().any(|n| n.name == "link.txt");

        assert!(link_in_tree, "Symlink should appear in tree");
        assert!(link_in_flat, "Symlink should appear in flat");
    }
}

#[test]
fn test_integration_consistency_between_modes() {
    let dir = tempdir().unwrap();

    fs::create_dir(dir.path().join("dir1")).unwrap();
    fs::create_dir(dir.path().join("dir1/dir2")).unwrap();
    File::create(dir.path().join("file1.txt")).unwrap();
    File::create(dir.path().join("dir1/file2.txt")).unwrap();
    File::create(dir.path().join("dir1/dir2/file3.txt")).unwrap();

    let tree = build_tree(dir.path()).unwrap();
    let flat = build_flat_tree(dir.path()).unwrap();

    let mut tree_paths = Vec::new();
    collect_paths(&tree, &mut tree_paths);

    let flat_paths: Vec<String> = flat.iter().map(|n| n.path.clone()).collect();

    assert_eq!(tree_paths.len(), flat_paths.len());

    for path in &tree_paths {
        assert!(flat_paths.contains(path), "Path {} missing in flat mode", path);
    }
}

fn collect_paths(node: &TreeNode, paths: &mut Vec<String>) {
    paths.push(node.path.clone());
    for child in &node.children {
        collect_paths(child, paths);
    }
}

#[test]
fn test_integration_test_data_directory() {
    let test_data_path = Path::new("tests/test_data");
    if !test_data_path.exists() {
        return;
    }

    let tree = build_tree(test_data_path).unwrap();
    let flat = build_flat_tree(test_data_path).unwrap();

    assert!(tree.children.len() > 0);
    assert!(flat.len() > 0);

    let tree_json = serde_json::to_string(&tree).unwrap();
    let flat_json = serde_json::to_string(&flat).unwrap();

    let tree_parsed: serde_json::Value = serde_json::from_str(&tree_json).unwrap();
    let flat_parsed: serde_json::Value = serde_json::from_str(&flat_json).unwrap();

    assert!(tree_parsed.is_object());
    assert!(flat_parsed.is_array());
}

#[test]
fn test_integration_large_directory_structure() {
    let dir = tempdir().unwrap();

    for i in 0..10 {
        fs::create_dir(dir.path().join(format!("dir_{}", i))).unwrap();
        for j in 0..5 {
            File::create(dir.path().join(format!("dir_{}", i)).join(format!("file_{}.txt", j))).unwrap();
        }
    }

    let tree = build_tree(dir.path()).unwrap();
    let flat = build_flat_tree(dir.path()).unwrap();

    assert_eq!(tree.children.len(), 10);
    assert_eq!(flat.len(), 61);

    for child in &tree.children {
        assert_eq!(child.children.len(), 5);
    }
}