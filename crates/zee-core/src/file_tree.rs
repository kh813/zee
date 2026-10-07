use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTreeNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_expanded: bool,
    pub children: Vec<FileTreeNode>,
}

#[derive(Debug, Clone)]
pub struct FlatFileItem {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_expanded: bool,
    pub has_children: bool,
    pub depth: usize,
}

pub fn user_root_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}

pub struct FileTree {
    pub root_path: PathBuf,
    pub root_node: FileTreeNode,
    pub show_hidden: bool,
}

impl FileTree {
    pub fn new<P: AsRef<Path>>(root_path: P, show_hidden: bool) -> Self {
        let root_path = root_path.as_ref().to_path_buf();
        let name = root_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| root_path.to_string_lossy().to_string());

        let mut root_node = FileTreeNode {
            name,
            path: root_path.clone(),
            is_dir: true,
            is_expanded: true,
            children: Vec::new(),
        };

        Self::populate_children(&mut root_node, show_hidden);

        Self {
            root_path,
            root_node,
            show_hidden,
        }
    }

    pub fn set_root<P: AsRef<Path>>(&mut self, root_path: P) {
        let root_path = root_path.as_ref().to_path_buf();
        let name = root_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| root_path.to_string_lossy().to_string());

        let mut root_node = FileTreeNode {
            name,
            path: root_path.clone(),
            is_dir: true,
            is_expanded: true,
            children: Vec::new(),
        };

        Self::populate_children(&mut root_node, self.show_hidden);

        self.root_path = root_path;
        self.root_node = root_node;
    }

    pub fn refresh(&mut self) {
        Self::refresh_node(&mut self.root_node, self.show_hidden);
    }

    pub fn set_show_hidden(&mut self, show_hidden: bool) {
        if self.show_hidden != show_hidden {
            self.show_hidden = show_hidden;
            self.refresh();
        }
    }

    pub fn toggle_show_hidden(&mut self) -> bool {
        self.show_hidden = !self.show_hidden;
        self.refresh();
        self.show_hidden
    }

    pub fn expanded_paths(&self) -> Vec<PathBuf> {
        let mut list = Vec::new();
        Self::collect_expanded(&self.root_node, &mut list);
        list
    }

    pub fn restore_expanded_paths(&mut self, list: &[PathBuf]) {
        Self::restore_expanded(&mut self.root_node, list, self.show_hidden);
    }

    fn refresh_node(node: &mut FileTreeNode, show_hidden: bool) {
        if node.is_dir {
            let mut existing_expanded: Vec<PathBuf> = Vec::new();
            Self::collect_expanded(node, &mut existing_expanded);

            Self::populate_children(node, show_hidden);
            Self::restore_expanded(node, &existing_expanded, show_hidden);
        }
    }

    fn collect_expanded(node: &FileTreeNode, list: &mut Vec<PathBuf>) {
        if node.is_dir && node.is_expanded {
            list.push(node.path.clone());
            for child in &node.children {
                Self::collect_expanded(child, list);
            }
        }
    }

    fn restore_expanded(node: &mut FileTreeNode, list: &[PathBuf], show_hidden: bool) {
        if node.is_dir {
            if list.contains(&node.path) {
                node.is_expanded = true;
                if node.children.is_empty() {
                    Self::populate_children(node, show_hidden);
                }
            }
            if node.is_expanded {
                for child in &mut node.children {
                    Self::restore_expanded(child, list, show_hidden);
                }
            }
        }
    }

    pub fn toggle_expand(&mut self, target_path: &Path) -> bool {
        Self::toggle_expand_node(&mut self.root_node, target_path, self.show_hidden)
    }

    pub fn ensure_expanded(&mut self, dir_path: &Path) {
        Self::ensure_expanded_node(&mut self.root_node, dir_path, self.show_hidden);
    }

    fn ensure_expanded_node(node: &mut FileTreeNode, dir_path: &Path, show_hidden: bool) -> bool {
        if node.path == dir_path {
            if node.is_dir {
                node.is_expanded = true;
                if node.children.is_empty() {
                    Self::populate_children(node, show_hidden);
                }
                return true;
            }
            return false;
        }

        if dir_path.starts_with(&node.path) && node.is_dir {
            node.is_expanded = true;
            if node.children.is_empty() {
                Self::populate_children(node, show_hidden);
            }
            for child in &mut node.children {
                if Self::ensure_expanded_node(child, dir_path, show_hidden) {
                    return true;
                }
            }
        }
        false
    }

    /// Create a new file in `parent_dir` with the given `filename`.
    pub fn create_file<P: AsRef<Path>>(&mut self, parent_dir: P, filename: &str) -> std::io::Result<PathBuf> {
        let filename = filename.trim();
        if filename.is_empty() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Filename cannot be empty"));
        }
        let mut parent = parent_dir.as_ref().to_path_buf();
        if parent.is_file() {
            if let Some(p) = parent.parent() {
                parent = p.to_path_buf();
            }
        }
        let target = parent.join(filename);
        if target.exists() {
            return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "File already exists"));
        }
        std::fs::File::create(&target)?;
        self.ensure_expanded(&parent);
        self.refresh();
        Ok(target)
    }

    /// Create a new folder in `parent_dir` with the given `folder_name`.
    pub fn create_folder<P: AsRef<Path>>(&mut self, parent_dir: P, folder_name: &str) -> std::io::Result<PathBuf> {
        let folder_name = folder_name.trim();
        if folder_name.is_empty() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Folder name cannot be empty"));
        }
        let mut parent = parent_dir.as_ref().to_path_buf();
        if parent.is_file() {
            if let Some(p) = parent.parent() {
                parent = p.to_path_buf();
            }
        }
        let target = parent.join(folder_name);
        if target.exists() {
            return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "Folder already exists"));
        }
        std::fs::create_dir_all(&target)?;
        self.ensure_expanded(&parent);
        self.refresh();
        Ok(target)
    }

    /// Rename a file or directory at `old_path` to `new_name`.
    pub fn rename_item<P: AsRef<Path>>(&mut self, old_path: P, new_name: &str) -> std::io::Result<PathBuf> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Name cannot be empty"));
        }
        let old = old_path.as_ref();
        let parent = old.parent().unwrap_or(old);
        let new_target = parent.join(new_name);
        if new_target.exists() {
            return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "Destination already exists"));
        }
        std::fs::rename(old, &new_target)?;
        self.refresh();
        Ok(new_target)
    }

    /// Delete a file or directory at `target_path`.
    pub fn delete_item<P: AsRef<Path>>(&mut self, target_path: P) -> std::io::Result<()> {
        let target = target_path.as_ref();
        if !target.exists() {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Target does not exist"));
        }
        if target.is_dir() {
            std::fs::remove_dir_all(target)?;
        } else {
            std::fs::remove_file(target)?;
        }
        self.refresh();
        Ok(())
    }

    fn toggle_expand_node(node: &mut FileTreeNode, target_path: &Path, show_hidden: bool) -> bool {
        if node.path == target_path {
            if node.is_dir {
                node.is_expanded = !node.is_expanded;
                if node.is_expanded && node.children.is_empty() {
                    Self::populate_children(node, show_hidden);
                }
                return true;
            }
            return false;
        }

        for child in &mut node.children {
            if Self::toggle_expand_node(child, target_path, show_hidden) {
                return true;
            }
        }
        false
    }

    fn populate_children(node: &mut FileTreeNode, show_hidden: bool) {
        if !node.is_dir {
            return;
        }

        let mut children = Vec::new();
        if let Ok(entries) = fs::read_dir(&node.path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                if !show_hidden && file_name.starts_with('.') {
                    continue;
                }

                let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);

                children.push(FileTreeNode {
                    name: file_name,
                    path,
                    is_dir,
                    is_expanded: false,
                    children: Vec::new(),
                });
            }
        }

        // Sort: directories first, then alphabetical (case-insensitive)
        children.sort_by(|a, b| {
            match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });

        node.children = children;
    }

    pub fn flatten(&self) -> Vec<FlatFileItem> {
        let mut items = Vec::new();
        Self::flatten_node(&self.root_node, 0, &mut items);
        items
    }

    fn flatten_node(node: &FileTreeNode, depth: usize, out: &mut Vec<FlatFileItem>) {
        let has_children = node.is_dir;
        out.push(FlatFileItem {
            name: node.name.clone(),
            path: node.path.clone(),
            is_dir: node.is_dir,
            is_expanded: node.is_expanded,
            has_children,
            depth,
        });

        if node.is_dir && node.is_expanded {
            for child in &node.children {
                Self::flatten_node(child, depth + 1, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_tree_creation_and_flatten() {
        let temp_dir = std::env::temp_dir().join("led_test_tree");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("sub_dir")).unwrap();
        fs::write(temp_dir.join("a.txt"), "hello").unwrap();
        fs::write(temp_dir.join("b.txt"), "world").unwrap();
        fs::write(temp_dir.join("sub_dir").join("c.txt"), "sub").unwrap();

        let mut tree = FileTree::new(&temp_dir, false);
        let flat = tree.flatten();
        // root + sub_dir + a.txt + b.txt (sub_dir is not expanded initially)
        assert_eq!(flat.len(), 4);
        assert!(flat[1].is_dir); // sub_dir sorted first
        assert_eq!(flat[1].name, "sub_dir");

        // Expand sub_dir
        tree.toggle_expand(&temp_dir.join("sub_dir"));
        let flat_expanded = tree.flatten();
        assert_eq!(flat_expanded.len(), 5);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_file_tree_expanded_paths_roundtrip() {
        let temp_dir = std::env::temp_dir().join("zee_test_expanded_paths");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("dir1").join("dir2")).unwrap();
        fs::write(temp_dir.join("dir1").join("dir2").join("file.txt"), "hello").unwrap();

        let mut tree = FileTree::new(&temp_dir, false);
        // Expand dir1
        tree.toggle_expand(&temp_dir.join("dir1"));
        // Expand dir2
        tree.toggle_expand(&temp_dir.join("dir1").join("dir2"));

        let expanded = tree.expanded_paths();
        assert!(expanded.contains(&temp_dir));
        assert!(expanded.contains(&temp_dir.join("dir1")));
        assert!(expanded.contains(&temp_dir.join("dir1").join("dir2")));

        // Create a new fresh tree on same dir
        let mut new_tree = FileTree::new(&temp_dir, false);
        assert_eq!(new_tree.flatten().len(), 2); // root + dir1 (collapsed)

        // Restore expanded paths
        new_tree.restore_expanded_paths(&expanded);
        assert_eq!(new_tree.flatten().len(), 4); // root + dir1 + dir2 + file.txt

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_file_tree_file_and_folder_operations() {
        let temp_dir = std::env::temp_dir().join("zee_test_tree_ops");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut tree = FileTree::new(&temp_dir, false);

        // 1. Create file
        let new_file = tree.create_file(&temp_dir, "new_doc.txt").unwrap();
        assert!(new_file.exists());
        let names: Vec<String> = tree.flatten().into_iter().map(|item| item.name).collect();
        assert!(names.contains(&"new_doc.txt".to_string()));

        // 2. Create folder
        let new_folder = tree.create_folder(&temp_dir, "src_dir").unwrap();
        assert!(new_folder.is_dir());
        let names: Vec<String> = tree.flatten().into_iter().map(|item| item.name).collect();
        assert!(names.contains(&"src_dir".to_string()));

        // 3. Create file inside folder
        let nested_file = tree.create_file(&new_folder, "mod.rs").unwrap();
        assert!(nested_file.exists());
        let names: Vec<String> = tree.flatten().into_iter().map(|item| item.name).collect();
        assert!(names.contains(&"mod.rs".to_string()));

        // 4. Rename file
        let renamed = tree.rename_item(&nested_file, "lib.rs").unwrap();
        assert!(renamed.exists());
        assert!(!nested_file.exists());
        let names: Vec<String> = tree.flatten().into_iter().map(|item| item.name).collect();
        assert!(names.contains(&"lib.rs".to_string()));
        assert!(!names.contains(&"mod.rs".to_string()));

        // 5. Delete file
        tree.delete_item(&renamed).unwrap();
        assert!(!renamed.exists());
        let names: Vec<String> = tree.flatten().into_iter().map(|item| item.name).collect();
        assert!(!names.contains(&"lib.rs".to_string()));

        // 6. Delete folder
        tree.delete_item(&new_folder).unwrap();
        assert!(!new_folder.exists());
        let names: Vec<String> = tree.flatten().into_iter().map(|item| item.name).collect();
        assert!(!names.contains(&"src_dir".to_string()));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_file_tree_refresh_detects_external_changes() {
        let temp_dir = std::env::temp_dir().join("zee_test_tree_refresh");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("sub")).unwrap();
        fs::write(temp_dir.join("initial.txt"), "hello").unwrap();

        let mut tree = FileTree::new(&temp_dir, false);
        tree.toggle_expand(&temp_dir.join("sub"));
        assert_eq!(tree.flatten().len(), 3); // root, sub, initial.txt

        // External change (e.g. via Finder): create external.txt and remove initial.txt
        fs::write(temp_dir.join("sub").join("external.txt"), "external").unwrap();
        fs::remove_file(temp_dir.join("initial.txt")).unwrap();

        // Before refresh, tree hasn't seen the change
        let names_before: Vec<String> = tree.flatten().into_iter().map(|i| i.name).collect();
        assert!(names_before.contains(&"initial.txt".to_string()));
        assert!(!names_before.contains(&"external.txt".to_string()));

        // Refresh
        tree.refresh();

        // After refresh, external change is reflected and sub remains expanded
        let names_after: Vec<String> = tree.flatten().into_iter().map(|i| i.name).collect();
        assert!(!names_after.contains(&"initial.txt".to_string()));
        assert!(names_after.contains(&"external.txt".to_string()));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_file_tree_toggle_show_hidden() {
        let temp_dir = std::env::temp_dir().join("zee_test_tree_hidden");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(temp_dir.join(".env"), "SECRET=1").unwrap();
        fs::write(temp_dir.join("main.rs"), "fn main() {}").unwrap();

        let mut tree = FileTree::new(&temp_dir, false);
        let names_hidden: Vec<String> = tree.flatten().into_iter().map(|i| i.name).collect();
        assert!(!names_hidden.contains(&".env".to_string()));
        assert!(names_hidden.contains(&"main.rs".to_string()));

        // Toggle show_hidden
        let new_state = tree.toggle_show_hidden();
        assert!(new_state);
        let names_visible: Vec<String> = tree.flatten().into_iter().map(|i| i.name).collect();
        assert!(names_visible.contains(&".env".to_string()));
        assert!(names_visible.contains(&"main.rs".to_string()));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
