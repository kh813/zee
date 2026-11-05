use std::fs;
use std::path::PathBuf;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowState {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub is_maximized: bool,
}

impl WindowState {
    pub fn new(x: f32, y: f32, width: f32, height: f32, is_maximized: bool) -> Self {
        Self {
            x,
            y,
            width,
            height,
            is_maximized,
        }
    }

    pub fn file_path() -> Option<PathBuf> {
        crate::config::Config::config_dir().map(|d| d.join("window_state.json"))
    }

    pub fn save(&self) -> Result<()> {
        if let Some(path) = Self::file_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let data = serde_json::to_string_pretty(self)
                .context("Failed to serialize window state")?;
            fs::write(&path, data)
                .with_context(|| format!("Failed to write window state to {:?}", path))?;
        }
        Ok(())
    }

    pub fn load() -> Option<Self> {
        let path = Self::file_path()?;
        if !path.exists() {
            return None;
        }
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn clear() {
        if let Some(path) = Self::file_path() {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_state_serialization_roundtrip() {
        let state = WindowState {
            x: 120.0,
            y: 80.0,
            width: 1280.0,
            height: 900.0,
            is_maximized: true,
        };

        let json = serde_json::to_string_pretty(&state).expect("serialize should succeed");
        let deserialized: WindowState = serde_json::from_str(&json).expect("deserialize should succeed");

        assert_eq!(state, deserialized);
    }

    #[test]
    fn test_window_state_deserialization_default_maximized() {
        let json = r#"{
            "x": 100.0,
            "y": 50.0,
            "width": 1024.0,
            "height": 768.0
        }"#;

        let deserialized: WindowState = serde_json::from_str(json).expect("deserialize should succeed");
        assert_eq!(deserialized.x, 100.0);
        assert_eq!(deserialized.y, 50.0);
        assert_eq!(deserialized.width, 1024.0);
        assert_eq!(deserialized.height, 768.0);
        assert_eq!(deserialized.is_maximized, false);
    }
}
