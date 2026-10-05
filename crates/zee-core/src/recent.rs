use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

pub const MAX_RECENT_FILES: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RecentFiles {
    pub files: Vec<PathBuf>,
}

impl RecentFiles {
    pub fn file_path() -> Option<PathBuf> {
        crate::config::Config::config_dir().map(|d| d.join("recent_files.json"))
    }

    pub fn load() -> Self {
        if let Some(path) = Self::file_path() {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(path) {
                    if let Ok(recent) = serde_json::from_str::<Self>(&content) {
                        return recent;
                    }
                }
            }
        }
        Self::default()
    }

    pub fn add<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref();
        let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.files.retain(|p| p != &normalized && p != path);
        self.files.insert(0, normalized);
        if self.files.len() > MAX_RECENT_FILES {
            self.files.truncate(MAX_RECENT_FILES);
        }
        let _ = self.save();
    }

    pub fn clear(&mut self) {
        self.files.clear();
        let _ = self.save();
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(path) = Self::file_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let data = serde_json::to_string_pretty(self)?;
            fs::write(path, data)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recent_files_add_and_dedup() {
        let mut recent = RecentFiles::default();
        let path1 = PathBuf::from("/tmp/test1.rs");
        let path2 = PathBuf::from("/tmp/test2.rs");

        recent.add(&path1);
        assert_eq!(recent.files.len(), 1);
        assert_eq!(recent.files[0], path1);

        recent.add(&path2);
        assert_eq!(recent.files.len(), 2);
        assert_eq!(recent.files[0], path2);
        assert_eq!(recent.files[1], path1);

        // Re-adding path1 moves it to front
        recent.add(&path1);
        assert_eq!(recent.files.len(), 2);
        assert_eq!(recent.files[0], path1);
        assert_eq!(recent.files[1], path2);

        recent.clear();
        assert!(recent.files.is_empty());
    }
}
