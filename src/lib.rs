use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct FlatNode {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub parent: Option<String>,
}

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct TreeNode {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub children: Vec<TreeNode>,
}

pub fn build_flat_tree(path: &Path) -> Result<Vec<FlatNode>, std::io::Error> {
    let mut nodes = Vec::new();
    build_flat_tree_recursive(path, None, &mut nodes)?;
    nodes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(nodes)
}

fn build_flat_tree_recursive(
    path: &Path,
    parent_path: Option<String>,
    nodes: &mut Vec<FlatNode>,
) -> Result<(), std::io::Error> {
    let name = path.file_name()
        .unwrap_or_else(|| path.as_os_str())
        .to_string_lossy()
        .into_owned();

    let path_str = path.to_string_lossy().into_owned();

    if path.file_name().map_or(false, |n| n == "target") {
        return Ok(());
    }

    let node = FlatNode {
        name,
        path: path_str.clone(),
        is_directory: path.is_dir(),
        parent: parent_path,
    };

    nodes.push(node);

    if path.is_dir() {
        let entries = fs::read_dir(path)?;
        for entry in entries {
            let entry = entry?;
            build_flat_tree_recursive(&entry.path(), Some(path_str.clone()), nodes)?;
        }
    }

    Ok(())
}

pub fn build_tree(path: &Path) -> Result<TreeNode, std::io::Error> {
    let name = path.file_name()
        .unwrap_or_else(|| path.as_os_str())
        .to_string_lossy()
        .into_owned();

    let mut node = TreeNode {
        name,
        path: path.to_string_lossy().into_owned(),
        is_directory: path.is_dir(),
        children: Vec::new(),
    };

    if node.is_directory {
        let entries = fs::read_dir(path)?;
        for entry in entries {
            let entry = entry?;
            let child_path = entry.path();
            if child_path.file_name().map_or(false, |n| n == "target") {
                continue;
            }
            let child = build_tree(&child_path)?;
            node.children.push(child);
        }
        node.children.sort_by(|a, b| a.name.cmp(&b.name));
    }

    Ok(node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_build_tree_empty_directory() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        let result = build_tree(path).unwrap();

        assert_eq!(result.name, path.file_name().unwrap().to_string_lossy());
        assert_eq!(result.is_directory, true);
        assert_eq!(result.children.len(), 0);
    }

    #[test]
    fn test_build_tree_single_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        let result = build_tree(dir.path()).unwrap();

        assert_eq!(result.is_directory, true);
        assert_eq!(result.children.len(), 1);
        assert_eq!(result.children[0].name, "test.txt");
        assert_eq!(result.children[0].is_directory, false);
    }

    #[test]
    fn test_build_tree_nested_structure() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        File::create(subdir.join("file.txt")).unwrap();

        let result = build_tree(dir.path()).unwrap();

        assert_eq!(result.is_directory, true);
        assert_eq!(result.children.len(), 1);
        assert_eq!(result.children[0].name, "subdir");
        assert_eq!(result.children[0].is_directory, true);
        assert_eq!(result.children[0].children.len(), 1);
        assert_eq!(result.children[0].children[0].name, "file.txt");
    }

    #[test]
    fn test_build_tree_multiple_files() {
        let dir = tempdir().unwrap();
        File::create(dir.path().join("a.txt")).unwrap();
        File::create(dir.path().join("b.txt")).unwrap();
        File::create(dir.path().join("c.txt")).unwrap();

        let result = build_tree(dir.path()).unwrap();

        assert_eq!(result.children.len(), 3);
        assert_eq!(result.children[0].name, "a.txt");
        assert_eq!(result.children[1].name, "b.txt");
        assert_eq!(result.children[2].name, "c.txt");
    }

    #[test]
    fn test_build_flat_tree_empty_directory() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        let result = build_flat_tree(path).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].is_directory, true);
        assert_eq!(result[0].parent, None);
    }

    #[test]
    fn test_build_flat_tree_single_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        let result = build_flat_tree(dir.path()).unwrap();

        assert_eq!(result.len(), 2);

        let dir_node = result.iter().find(|n| n.is_directory).unwrap();
        let file_node = result.iter().find(|n| !n.is_directory).unwrap();

        assert_eq!(dir_node.parent, None);
        assert_eq!(file_node.parent, Some(dir.path().to_string_lossy().into_owned()));
    }

    #[test]
    fn test_build_flat_tree_nested_structure() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        File::create(subdir.join("file.txt")).unwrap();

        let result = build_flat_tree(dir.path()).unwrap();

        assert_eq!(result.len(), 3);

        let dir_node = result.iter().find(|n| n.path == dir.path().to_string_lossy()).unwrap();
        let subdir_node = result.iter().find(|n| n.name == "subdir").unwrap();
        let file_node = result.iter().find(|n| n.name == "file.txt").unwrap();

        assert_eq!(dir_node.parent, None);
        assert_eq!(subdir_node.parent, Some(dir.path().to_string_lossy().into_owned()));
        assert_eq!(file_node.parent, Some(subdir.to_string_lossy().into_owned()));
    }

    #[test]
    fn test_build_flat_tree_excludes_target() {
        let dir = tempdir().unwrap();
        let target_dir = dir.path().join("target");
        fs::create_dir(&target_dir).unwrap();
        File::create(target_dir.join("debug")).unwrap();

        let result = build_flat_tree(dir.path()).unwrap();

        assert!(!result.iter().any(|n| n.name == "target"));
        assert!(!result.iter().any(|n| n.name == "debug"));
    }

    #[test]
    fn test_build_tree_excludes_target() {
        let dir = tempdir().unwrap();
        let target_dir = dir.path().join("target");
        fs::create_dir(&target_dir).unwrap();
        File::create(target_dir.join("debug")).unwrap();

        let result = build_tree(dir.path()).unwrap();

        assert!(!result.children.iter().any(|n| n.name == "target"));
    }

    #[test]
    fn test_tree_and_flat_consistency() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();
        File::create(dir.path().join("file1.txt")).unwrap();
        File::create(dir.path().join("subdir").join("file2.txt")).unwrap();

        let tree = build_tree(dir.path()).unwrap();
        let flat = build_flat_tree(dir.path()).unwrap();

        let mut all_flat_paths: Vec<String> = flat.iter().map(|n| n.path.clone()).collect();
        all_flat_paths.sort();

        let mut all_tree_paths = Vec::new();
        collect_tree_paths(&tree, &mut all_tree_paths);
        all_tree_paths.sort();

        assert_eq!(all_flat_paths, all_tree_paths);
    }

    fn collect_tree_paths(node: &TreeNode, paths: &mut Vec<String>) {
        paths.push(node.path.clone());
        for child in &node.children {
            collect_tree_paths(child, paths);
        }
    }

    // Regression tests using the test_data directory
    #[test]
    fn test_regression_flat_structure() {
        let test_data_path = Path::new("tests/test_data");
        if !test_data_path.exists() {
            return;
        }

        let result = build_flat_tree(test_data_path).unwrap();

        assert!(result.len() > 0);

        let root = result.iter().find(|n| n.path == "tests/test_data").unwrap();
        assert_eq!(root.is_directory, true);
        assert_eq!(root.parent, None);

        let file1 = result.iter().find(|n| n.name == "file1.txt").unwrap();
        assert_eq!(file1.is_directory, false);
        assert_eq!(file1.parent, Some("tests/test_data".to_string()));

        let deep_dir = result.iter().find(|n| n.path == "tests/test_data/deep").unwrap();
        assert_eq!(deep_dir.is_directory, true);
        assert_eq!(deep_dir.parent, Some("tests/test_data".to_string()));
    }

    #[test]
    fn test_regression_tree_structure() {
        let test_data_path = Path::new("tests/test_data");
        if !test_data_path.exists() {
            return;
        }

        let result = build_tree(test_data_path).unwrap();

        assert_eq!(result.name, "test_data");
        assert_eq!(result.is_directory, true);

        let file_nodes: Vec<&TreeNode> = result.children.iter()
            .filter(|n| !n.is_directory)
            .collect();
        assert_eq!(file_nodes.len(), 2);

        let deep_dir = result.children.iter()
            .find(|n| n.name == "deep")
            .unwrap();
        assert_eq!(deep_dir.is_directory, true);
        assert_eq!(deep_dir.children.len(), 2);

        let nested_dir = deep_dir.children.iter()
            .find(|n| n.name == "nested")
            .unwrap();
        assert_eq!(nested_dir.is_directory, true);
        assert_eq!(nested_dir.children.len(), 1);
        assert_eq!(nested_dir.children[0].name, "file4.txt");
    }

    #[test]
    fn test_regression_alphabetical_order() {
        let test_data_path = Path::new("tests/test_data");
        if !test_data_path.exists() {
            return;
        }

        let result = build_tree(test_data_path).unwrap();

        let file_names: Vec<String> = result.children.iter()
            .filter(|n| !n.is_directory)
            .map(|n| n.name.clone())
            .collect();

        assert_eq!(file_names, vec!["file1.txt", "file2.txt"]);

        let dir_names: Vec<String> = result.children.iter()
            .filter(|n| n.is_directory)
            .map(|n| n.name.clone())
            .collect();

        assert_eq!(dir_names, vec!["deep"]);
    }

    #[test]
    fn test_regression_flat_alphabetical_order() {
        let test_data_path = Path::new("tests/test_data");
        if !test_data_path.exists() {
            return;
        }

        let result = build_flat_tree(test_data_path).unwrap();

        let paths: Vec<String> = result.iter().map(|n| n.path.clone()).collect();

        for i in 1..paths.len() {
            assert!(paths[i-1] <= paths[i],
                "Paths not sorted: {} > {}", paths[i-1], paths[i]);
        }
    }
}