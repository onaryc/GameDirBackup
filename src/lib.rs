use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

use ignore::Walk;
use ignore::WalkBuilder;
use std::sync::Arc;
use std::sync::Mutex;

pub enum TraversalMode {
    Sequential,
    Parallel,
}

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct RootNode {
    pub files_nb: usize,
    pub dirs_nb: usize,
    pub nodes: Vec<FileNode>,
}

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub depth: usize,
    /// Chemin du dossier parent. Toujours `Some(...)` en pratique, puisque
    /// le dossier scanné lui-même n'est jamais transformé en `FileNode`
    /// (il est déjà représenté implicitement par `RootNode`).
    pub parent: Option<String>,
    /// Rempli uniquement après un appel à `to_tree`. Toujours vide dans le
    /// résultat brut de `build_file_node`.
    pub children: Vec<FileNode>,
}

/// Parcourt `path` et renvoie une liste **plate** de `FileNode` (chacun
/// connaît le chemin de son parent via `parent`, mais `children` reste
/// vide). Pour obtenir une arborescence imbriquée, appeler `to_tree` sur
/// le résultat.
pub fn build_file_node(
    path: &Path,
    traversal_mode: TraversalMode,
    force_canonical_path: bool,
) -> Result<RootNode, std::io::Error> {
    let path_str = if force_canonical_path {
        std::fs::canonicalize(path)?
            .into_os_string()
            .into_string()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "chemin non UTF-8"))?
    } else {
        path.to_string_lossy().into_owned()
    };

    let new_path = Path::new(&path_str);

    match traversal_mode {
        TraversalMode::Sequential => build_file_node_sequential(new_path),
        TraversalMode::Parallel => build_file_node_parallel(new_path),
    }
}

fn build_file_node_sequential(path: &Path) -> Result<RootNode, std::io::Error> {
    let mut root_node = RootNode {
        files_nb: 0,
        dirs_nb: 0,
        nodes: Vec::new(),
    };

    for result in Walk::new(path) {
        match result {
            Ok(entry) => {
                let is_directory = match entry.file_type() {
                    Some(ft) => ft.is_dir(),
                    None => false, // type indéterminable (ex: lien symbolique cassé)
                };

                if is_directory {
                    root_node.dirs_nb += 1
                } else {
                    root_node.files_nb += 1
                }

                // Le dossier scanné lui-même (depth 0) n'est pas un FileNode.
                if entry.depth() == 0 {
                    continue;
                }

                root_node.nodes.push(FileNode {
                    name: entry
                        .path()
                        .file_name()
                        .unwrap_or_else(|| path.as_os_str())
                        .to_string_lossy()
                        .into_owned(),
                    path: entry.path().to_string_lossy().into_owned(),
                    is_directory,
                    depth: entry.depth(),
                    parent: entry.path().parent().map(|p| p.to_string_lossy().into_owned()),
                    children: Vec::new(),
                });
            }
            Err(err) => eprintln!("ERROR: {}", err),
        }
    }

    Ok(root_node)
}

fn build_file_node_parallel(path: &Path) -> Result<RootNode, std::io::Error> {
    let nodes: Arc<Mutex<Vec<FileNode>>> = Arc::new(Mutex::new(vec![]));
    let files_nb = Arc::new(Mutex::new(0usize));
    let dirs_nb = Arc::new(Mutex::new(0usize));

    let walker = WalkBuilder::new(path)
        .threads(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4))
        .build_parallel();

    walker.run(|| {
        let nodes = nodes.clone();
        let files_nb = files_nb.clone();
        let dirs_nb = dirs_nb.clone();

        Box::new(move |result| {
            use ignore::WalkState::*;

            match result {
                Ok(entry) => {
                    let is_directory = match entry.file_type() {
                        Some(ft) => ft.is_dir(),
                        None => false,
                    };

                    if is_directory {
                        *dirs_nb.lock().unwrap() += 1
                    } else {
                        *files_nb.lock().unwrap() += 1
                    }

                    if entry.depth() != 0 {
                        nodes.lock().unwrap().push(FileNode {
                            name: entry
                                .path()
                                .file_name()
                                .unwrap_or_else(|| path.as_os_str())
                                .to_string_lossy()
                                .into_owned(),
                            path: entry.path().to_string_lossy().into_owned(),
                            is_directory,
                            depth: entry.depth(),
                            parent: entry.path().parent().map(|p| p.to_string_lossy().into_owned()),
                            children: Vec::new(),
                        });
                    }
                }
                Err(err) => eprintln!("ERROR: {}", err),
            }

            Continue
        })
    });

    let mut flat_nodes = Arc::try_unwrap(nodes).unwrap().into_inner().unwrap();
    // Ordre non garanti en parallèle : on trie pour un résultat déterministe.
    flat_nodes.sort_by(|a, b| {a.path.cmp(&b.path)});

    Ok(RootNode {
        files_nb: *files_nb.lock().unwrap(),
        dirs_nb: *dirs_nb.lock().unwrap(),
        nodes: flat_nodes,
    })
}

/// Transforme une liste plate de `FileNode` (telle que renvoyée par
/// `build_file_node`) en arborescence imbriquée : les nœuds de premier
/// niveau (enfants directs du dossier scanné) se retrouvent dans
/// `RootNode.nodes`, chacun avec ses propres enfants dans `children`.
///
/// Fonctionne quel que soit l'ordre des nœuds en entrée : les nœuds les
/// plus profonds sont rattachés à leur parent en premier, de sorte que
/// lorsqu'un nœud est à son tour déplacé chez son propre parent, ses
/// enfants lui sont déjà attachés.
pub fn to_tree(flat_root: &RootNode) -> RootNode {
    let mut by_path: HashMap<String, FileNode> = flat_root
        .nodes
        .iter()
        .cloned()
        .map(|node| (node.path.clone(), node))
        .collect();

    let mut order: Vec<String> = by_path.keys().cloned().collect();
    order.sort_by_key(|p| std::cmp::Reverse(by_path[p].depth));

    let mut top_level = Vec::new();

    for path in order {
        let Some(node) = by_path.remove(&path) else {
            continue;
        };

        match node.parent.as_ref().and_then(|pp| by_path.get_mut(pp)) {
            Some(parent) => parent.children.push(node),
            None => top_level.push(node), // parent = dossier scanné, absent de la map
        }
    }

    sort_tree(&mut top_level);

    RootNode {
        files_nb: flat_root.files_nb,
        dirs_nb: flat_root.dirs_nb,
        nodes: top_level,
    }
}

/// Trie récursivement l'arbre : dossiers avant fichiers, puis ordre alphabétique.
fn sort_tree(nodes: &mut Vec<FileNode>) {
    nodes.sort_by(|a, b| match (a.is_directory, b.is_directory) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });

    for node in nodes.iter_mut() {
        sort_tree(&mut node.children);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::tempdir;

    #[test]
    fn test_build_file_node_is_flat() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        File::create(dir.path().join("file1.txt")).unwrap();
        File::create(subdir.join("file2.txt")).unwrap();

        let result = build_file_node(dir.path(), TraversalMode::Sequential, false).unwrap();

        assert_eq!(result.files_nb, 2);
        assert_eq!(result.dirs_nb, 1);
        // 3 entrées à plat: file1.txt, subdir, subdir/file2.txt
        assert_eq!(result.nodes.len(), 3);
        assert!(result.nodes.iter().all(|n| n.children.is_empty()));

        let file2 = result.nodes.iter().find(|n| n.name == "file2.txt").unwrap();
        assert_eq!(file2.parent, Some(subdir.to_string_lossy().into_owned()));
    }

    #[test]
    fn test_to_tree_nested_structure() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        File::create(dir.path().join("file1.txt")).unwrap();
        File::create(subdir.join("file2.txt")).unwrap();

        let flat = build_file_node(dir.path(), TraversalMode::Sequential, false).unwrap();
        let tree = to_tree(&flat);

        assert_eq!(tree.files_nb, flat.files_nb);
        assert_eq!(tree.dirs_nb, flat.dirs_nb);
        assert_eq!(tree.nodes.len(), 2); // "file1.txt" et "subdir" au premier niveau

        let subdir_node = tree.nodes.iter().find(|n| n.name == "subdir").unwrap();
        assert!(subdir_node.is_directory);
        assert_eq!(subdir_node.children.len(), 1);
        assert_eq!(subdir_node.children[0].name, "file2.txt");
        assert!(!subdir_node.children[0].is_directory);
    }

    #[test]
    fn test_to_tree_deeply_nested() {
        let dir = tempdir().unwrap();
        let deep = dir.path().join("a").join("b").join("c");
        fs::create_dir_all(&deep).unwrap();
        File::create(deep.join("file.txt")).unwrap();

        let flat = build_file_node(dir.path(), TraversalMode::Sequential, false).unwrap();
        let tree = to_tree(&flat);

        let a = &tree.nodes[0];
        assert_eq!(a.name, "a");
        let b = &a.children[0];
        assert_eq!(b.name, "b");
        let c = &b.children[0];
        assert_eq!(c.name, "c");
        assert_eq!(c.children[0].name, "file.txt");
    }

    #[test]
    fn test_sequential_and_parallel_produce_same_flat_result() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();
        File::create(dir.path().join("a.txt")).unwrap();
        File::create(dir.path().join("subdir").join("b.txt")).unwrap();

        let seq = build_file_node(dir.path(), TraversalMode::Sequential, false).unwrap();
        let par = build_file_node(dir.path(), TraversalMode::Parallel, false).unwrap();

        assert_eq!(seq.files_nb, par.files_nb);
        assert_eq!(seq.dirs_nb, par.dirs_nb);
        assert_eq!(to_tree(&seq), to_tree(&par));
    }
}
