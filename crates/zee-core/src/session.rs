use std::fs;
use std::path::PathBuf;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionEditorState {
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub cursor: usize,
    #[serde(default)]
    pub scroll_row: usize,
    #[serde(default)]
    pub is_modified: bool,
    #[serde(default)]
    pub unsaved_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateSession {
    pub files: Vec<SessionEditorState>,
    #[serde(default)]
    pub active_index: usize,
    #[serde(default)]
    pub root_folder: Option<PathBuf>,
    #[serde(default)]
    pub expanded_folders: Vec<PathBuf>,
    #[serde(default)]
    pub sidebar_visible: bool,
    #[serde(default = "default_sidebar_tab")]
    pub sidebar_tab: String,
}

fn default_sidebar_tab() -> String {
    "files".to_string()
}

impl UpdateSession {
    pub fn session_file_path() -> Option<PathBuf> {
        crate::config::Config::config_dir().map(|d| d.join("update_session.json"))
    }

    pub fn save(&self) -> Result<()> {
        if let Some(path) = Self::session_file_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let data = serde_json::to_string_pretty(self)
                .context("Failed to serialize update session")?;
            fs::write(&path, data)
                .with_context(|| format!("Failed to write update session to {:?}", path))?;
        }
        Ok(())
    }

    pub fn load_and_clear() -> Option<Self> {
        let path = Self::session_file_path()?;
        if !path.exists() {
            return None;
        }
        let content = fs::read_to_string(&path).ok()?;
        let _ = fs::remove_file(&path);
        serde_json::from_str(&content).ok()
    }

    pub fn clear() {
        if let Some(path) = Self::session_file_path() {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_session_serialization_defaults() {
        let json = r#"{
            "files": [
                {
                    "path": "/tmp/test.rs",
                    "cursor": 42,
                    "scroll_row": 5
                }
            ]
        }"#;

        let session: UpdateSession = serde_json::from_str(json).expect("Deserialization should succeed");
        assert_eq!(session.files.len(), 1);
        assert_eq!(session.files[0].path, Some(PathBuf::from("/tmp/test.rs")));
        assert_eq!(session.files[0].cursor, 42);
        assert_eq!(session.files[0].scroll_row, 5);
        assert_eq!(session.files[0].is_modified, false);
        assert_eq!(session.files[0].unsaved_content, None);
        assert_eq!(session.active_index, 0);
        assert_eq!(session.root_folder, None);
        assert_eq!(session.expanded_folders, Vec::<PathBuf>::new());
        assert_eq!(session.sidebar_visible, false);
        assert_eq!(session.sidebar_tab, "files");
    }

    #[test]
    fn test_update_session_roundtrip_json() {
        let session = UpdateSession {
            files: vec![
                SessionEditorState {
                    path: Some(PathBuf::from("/tmp/foo.rs")),
                    cursor: 100,
                    scroll_row: 10,
                    is_modified: false,
                    unsaved_content: None,
                },
                SessionEditorState {
                    path: None,
                    cursor: 5,
                    scroll_row: 0,
                    is_modified: true,
                    unsaved_content: Some("Unsaved draft".to_string()),
                },
            ],
            active_index: 1,
            root_folder: Some(PathBuf::from("/tmp/my_project")),
            expanded_folders: vec![PathBuf::from("/tmp/my_project/src")],
            sidebar_visible: true,
            sidebar_tab: "outline".to_string(),
        };

        let serialized = serde_json::to_string(&session).unwrap();
        let deserialized: UpdateSession = serde_json::from_str(&serialized).unwrap();
        assert_eq!(session, deserialized);
    }
}
