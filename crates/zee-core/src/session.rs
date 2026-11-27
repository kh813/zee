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
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowSession {
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
    #[serde(default)]
    pub window_state: Option<crate::window_state::WindowState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    #[serde(default)]
    pub window_state: Option<crate::window_state::WindowState>,
    #[serde(default)]
    pub extra_windows: Vec<WindowSession>,
}

fn default_sidebar_tab() -> String {
    "files".to_string()
}

impl UpdateSession {
    /// Construct a multi-window session from a list of per-window snapshots.
    /// The first window populates the legacy top-level fields for full backwards compatibility,
    /// and any remaining windows are stored in `extra_windows`.
    pub fn from_windows(mut windows: Vec<WindowSession>) -> Self {
        if windows.is_empty() {
            return Self {
                files: vec![],
                active_index: 0,
                root_folder: None,
                expanded_folders: vec![],
                sidebar_visible: false,
                sidebar_tab: default_sidebar_tab(),
                window_state: None,
                extra_windows: vec![],
            };
        }
        let first = windows.remove(0);
        Self {
            files: first.files,
            active_index: first.active_index,
            root_folder: first.root_folder,
            expanded_folders: first.expanded_folders,
            sidebar_visible: first.sidebar_visible,
            sidebar_tab: first.sidebar_tab,
            window_state: first.window_state,
            extra_windows: windows,
        }
    }

    /// Return all windows represented by this session (primary window followed by `extra_windows`).
    pub fn all_windows(&self) -> Vec<WindowSession> {
        let mut out = Vec::with_capacity(1 + self.extra_windows.len());
        out.push(WindowSession {
            files: self.files.clone(),
            active_index: self.active_index,
            root_folder: self.root_folder.clone(),
            expanded_folders: self.expanded_folders.clone(),
            sidebar_visible: self.sidebar_visible,
            sidebar_tab: self.sidebar_tab.clone(),
            window_state: self.window_state.clone(),
        });
        out.extend(self.extra_windows.iter().cloned());
        out
    }

    pub fn session_file_path() -> Option<PathBuf> {
        crate::config::Config::config_dir().map(|d| d.join("update_session.json"))
    }

    pub fn gui_session_file_path() -> Option<PathBuf> {
        crate::config::Config::config_dir().map(|d| d.join("gui_session.json"))
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

    pub fn save_gui_session(&self) -> Result<()> {
        if let Some(path) = Self::gui_session_file_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let data = serde_json::to_string_pretty(self)
                .context("Failed to serialize GUI session")?;
            fs::write(&path, data)
                .with_context(|| format!("Failed to write GUI session to {:?}", path))?;
        }
        Ok(())
    }

    pub fn load_gui_session() -> Option<Self> {
        let path = Self::gui_session_file_path()?;
        if !path.exists() {
            return None;
        }
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn clear_gui_session() {
        if let Some(path) = Self::gui_session_file_path() {
            let _ = fs::remove_file(path);
        }
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

    pub fn exists() -> bool {
        Self::session_file_path().map(|p| p.exists()).unwrap_or(false)
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
                    pinned: true,
                },
                SessionEditorState {
                    path: None,
                    cursor: 5,
                    scroll_row: 0,
                    is_modified: true,
                    unsaved_content: Some("Unsaved draft".to_string()),
                    pinned: false,
                },
            ],
            active_index: 1,
            root_folder: Some(PathBuf::from("/tmp/my_project")),
            expanded_folders: vec![PathBuf::from("/tmp/my_project/src")],
            sidebar_visible: true,
            sidebar_tab: "outline".to_string(),
            window_state: Some(crate::window_state::WindowState {
                x: 100.0,
                y: 150.0,
                width: 1200.0,
                height: 800.0,
                is_maximized: false,
            }),
            extra_windows: vec![WindowSession {
                files: vec![SessionEditorState {
                    path: Some(PathBuf::from("/tmp/bar.md")),
                    cursor: 12,
                    scroll_row: 2,
                    is_modified: false,
                    unsaved_content: None,
                    pinned: false,
                }],
                active_index: 0,
                root_folder: Some(PathBuf::from("/tmp/other")),
                expanded_folders: vec![],
                sidebar_visible: false,
                sidebar_tab: "files".to_string(),
                window_state: Some(crate::window_state::WindowState {
                    x: 400.0,
                    y: 200.0,
                    width: 1000.0,
                    height: 700.0,
                    is_maximized: false,
                }),
            }],
        };

        let serialized = serde_json::to_string(&session).unwrap();
        let deserialized: UpdateSession = serde_json::from_str(&serialized).unwrap();
        assert_eq!(session, deserialized);
        assert_eq!(deserialized.all_windows().len(), 2);
        assert!(deserialized.all_windows()[0].files[0].pinned);
        assert_eq!(deserialized.all_windows()[1].files[0].path, Some(PathBuf::from("/tmp/bar.md")));
    }

    #[test]
    fn test_update_session_exists_lifecycle() {
        let _lock = crate::config::TEST_MUTEX.lock().unwrap();
        // Ensure starting clean
        UpdateSession::clear();
        assert!(!UpdateSession::exists());

        let session = UpdateSession {
            files: vec![],
            active_index: 0,
            root_folder: None,
            expanded_folders: vec![],
            sidebar_visible: false,
            sidebar_tab: "files".to_string(),
            window_state: None,
            extra_windows: vec![],
        };
        session.save().expect("Save session should succeed");
        assert!(UpdateSession::exists());

        let loaded = UpdateSession::load_and_clear();
        assert!(loaded.is_some());
        assert!(!UpdateSession::exists());
    }

    #[test]
    fn test_multi_window_gui_session_from_windows_roundtrip() {
        let w1 = WindowSession {
            files: vec![SessionEditorState {
                path: Some(PathBuf::from("/tmp/win1.rs")),
                cursor: 10,
                scroll_row: 1,
                is_modified: false,
                unsaved_content: None,
                pinned: true,
            }],
            active_index: 0,
            root_folder: Some(PathBuf::from("/tmp/p1")),
            expanded_folders: vec![],
            sidebar_visible: true,
            sidebar_tab: "files".to_string(),
            window_state: None,
        };
        let w2 = WindowSession {
            files: vec![SessionEditorState {
                path: Some(PathBuf::from("/tmp/win2.md")),
                cursor: 20,
                scroll_row: 3,
                is_modified: true,
                unsaved_content: Some("# draft".to_string()),
                pinned: false,
            }],
            active_index: 0,
            root_folder: Some(PathBuf::from("/tmp/p2")),
            expanded_folders: vec![],
            sidebar_visible: false,
            sidebar_tab: "outline".to_string(),
            window_state: None,
        };
        let session = UpdateSession::from_windows(vec![w1.clone(), w2.clone()]);
        assert_eq!(session.files, w1.files);
        assert_eq!(session.extra_windows.len(), 1);
        assert_eq!(session.all_windows(), vec![w1, w2]);
    }
}
