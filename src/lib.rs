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

/// Construit une arborescence de fichiers/dossiers en mode Tree (imbriqué)
pub fn build_tree(path: &Path) -> Result<TreeNode, std::io::Error> {
    let name = path.file_name()
        .unwrap_or_else(|| path.as_os_str())
        .to_string_lossy()
        .into_owned();

    let path_str = path.to_string_lossy().into_owned();

    // Exclure le dossier target/
    if path.file_name().map_or(false, |n| n == "target") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Skipping target directory"
        ));
    }

    let mut node = TreeNode {
        name,
        path: path_str.clone(),
        is_directory: path.is_dir(),
        children: Vec::new(),
    };

    if node.is_directory {
        let entries = fs::read_dir(path)?;
        for entry in entries {
            let entry = entry?;
            let child_path = entry.path();

            // Exclure le dossier target/
            if child_path.file_name().map_or(false, |n| n == "target") {
                continue;
            }

            match build_tree(&child_path) {
                Ok(child) => node.children.push(child),
                Err(_) => continue, // Ignorer les erreurs (comme target/)
            }
        }
        // Trier les enfants alphabétiquement
        node.children.sort_by(|a, b| a.name.cmp(&b.name));
    }

    Ok(node)
}

/// Convertit une arborescence TreeNode en une liste plate de FlatNode
pub fn tree_to_flat(tree: &TreeNode) -> Vec<FlatNode> {
    let mut flat_nodes = Vec::new();
    build_flat_from_tree(tree, None, &mut flat_nodes);
    flat_nodes
}

fn build_flat_from_tree(node: &TreeNode, parent_path: Option<String>, flat_nodes: &mut Vec<FlatNode>) {
    let flat_node = FlatNode {
        name: node.name.clone(),
        path: node.path.clone(),
        is_directory: node.is_directory,
        parent: parent_path.clone(),
    };
    flat_nodes.push(flat_node);

    if node.is_directory {
        for child in &node.children {
            build_flat_from_tree(child, Some(node.path.clone()), flat_nodes);
        }
    }
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
    fn test_tree_to_flat() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        File::create(dir.path().join("file1.txt")).unwrap();
        File::create(subdir.join("file2.txt")).unwrap();

        let tree = build_tree(dir.path()).unwrap();
        let flat = tree_to_flat(&tree);

        assert_eq!(flat.len(), 4); // root, file1.txt, subdir, file2.txt

        // Vérifier que la racine a parent = None
        let root_flat = flat.iter().find(|n| n.path == dir.path().to_string_lossy()).unwrap();
        assert_eq!(root_flat.parent, None);

        // Vérifier que file1.txt a pour parent la racine
        let file1 = flat.iter().find(|n| n.name == "file1.txt").unwrap();
        assert_eq!(file1.parent, Some(dir.path().to_string_lossy().into_owned()));
        assert_eq!(file1.is_directory, false);

        // Vérifier que subdir a pour parent la racine
        let subdir_flat = flat.iter().find(|n| n.name == "subdir").unwrap();
        assert_eq!(subdir_flat.parent, Some(dir.path().to_string_lossy().into_owned()));
        assert_eq!(subdir_flat.is_directory, true);

        // Vérifier que file2.txt a pour parent subdir
        let file2 = flat.iter().find(|n| n.name == "file2.txt").unwrap();
        assert_eq!(file2.parent, Some(subdir.to_string_lossy().into_owned()));
    }

    #[test]
    fn test_build_tree_excludes_target() {
        let dir = tempdir().unwrap();
        let target_dir = dir.path().join("target");
        fs::create_dir(&target_dir).unwrap();
        File::create(target_dir.join("debug")).unwrap();

        let result = build_tree(dir.path()).unwrap();

        // Vérifier que target n'est pas dans les enfants
        assert!(!result.children.iter().any(|n| n.name == "target"));
    }

    #[test]
    fn test_tree_and_flat_consistency() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();
        File::create(dir.path().join("file1.txt")).unwrap();
        File::create(dir.path().join("subdir").join("file2.txt")).unwrap();

        let tree = build_tree(dir.path()).unwrap();
        let flat = tree_to_flat(&tree);

        let all_flat_paths: Vec<String> = flat.iter().map(|n| n.path.clone()).collect();

        let mut all_tree_paths = Vec::new();
        collect_tree_paths(&tree, &mut all_tree_paths);

        assert_eq!(all_flat_paths, all_tree_paths);
    }

    fn collect_tree_paths(node: &TreeNode, paths: &mut Vec<String>) {
        paths.push(node.path.clone());
        for child in &node.children {
            collect_tree_paths(child, paths);
        }
    }

    // Tests de régression
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
    fn test_regression_flat_structure() {
        let test_data_path = Path::new("tests/test_data");
        if !test_data_path.exists() {
            return;
        }

        let tree = build_tree(test_data_path).unwrap();
        let flat = tree_to_flat(&tree);

        assert!(flat.len() > 0);

        let root = flat.iter().find(|n| n.path == "tests/test_data").unwrap();
        assert_eq!(root.is_directory, true);
        assert_eq!(root.parent, None);

        let file1 = flat.iter().find(|n| n.name == "file1.txt").unwrap();
        assert_eq!(file1.is_directory, false);
        assert_eq!(file1.parent, Some("tests/test_data".to_string()));

        let deep_dir = flat.iter().find(|n| n.path == "tests/test_data/deep").unwrap();
        assert_eq!(deep_dir.is_directory, true);
        assert_eq!(deep_dir.parent, Some("tests/test_data".to_string()));
    }

    #[test]
    fn test_regression_alphabetical_order() {
        let test_data_path = Path::new("tests/test_data");
        if !test_data_path.exists() {
            return;
        }

        let tree = build_tree(test_data_path).unwrap();

        let file_names: Vec<String> = tree.children.iter()
            .filter(|n| !n.is_directory)
            .map(|n| n.name.clone())
            .collect();

        assert_eq!(file_names, vec!["file1.txt", "file2.txt"]);

        let dir_names: Vec<String> = tree.children.iter()
            .filter(|n| n.is_directory)
            .map(|n| n.name.clone())
            .collect();

        assert_eq!(dir_names, vec!["deep"]);
    }
}