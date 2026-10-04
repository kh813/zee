use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use anyhow::{Result, Context};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub language: String,
    pub theme: String,
    pub line_numbers: bool,
    pub vi_mode: bool,
    pub word_wrap: bool,
    pub sidebar: bool,
    pub sidebar_position: String,
    pub tab_size: usize,
    pub expand_tab: bool,
    // GUI specific customization (CLI ignores font settings)
    pub font_family: Option<String>,
    pub font_size: f32,
    pub line_height: f32,
    pub ui_font_family: Option<String>,
    pub ui_font_size: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            theme: "terminal-default".to_string(),
            line_numbers: true,
            vi_mode: false,
            word_wrap: true,
            sidebar: false,
            sidebar_position: "right".to_string(),
            tab_size: 4,
            expand_tab: false,
            font_family: None,
            font_size: 12.0,
            line_height: 19.0,
            ui_font_family: None,
            ui_font_size: 12.5,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let mut config = Config::default();
        if let Some(path) = Config::config_file_path() {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(raw) = toml_span::parse(&content) {
                        if let Ok(loaded) = Config::deserialize_from_value(&raw) {
                            config = loaded;
                        }
                    }
                }
            }
        }
        config
    }

    fn deserialize_from_value(value: &toml_span::Value) -> Result<Self> {
        let mut config = Config::default();
        if let Some(table) = value.as_table() {
            for (k, v) in table {
                let key_str = k.name.as_ref();
                match key_str {
                    "language" => if let Some(s) = v.as_str() { config.language = s.to_string(); },
                    "theme" => if let Some(s) = v.as_str() { config.theme = s.to_string(); },
                    "line_numbers" => if let Some(b) = v.as_bool() { config.line_numbers = b; },
                    "vi_mode" => if let Some(b) = v.as_bool() { config.vi_mode = b; },
                    "word_wrap" => if let Some(b) = v.as_bool() { config.word_wrap = b; },
                    "sidebar" => if let Some(b) = v.as_bool() { config.sidebar = b; },
                    "sidebar_position" => if let Some(s) = v.as_str() {
                        let s = s.trim().to_lowercase();
                        if s == "left" || s == "right" {
                            config.sidebar_position = s;
                        }
                    },
                    "tab_size" => if let Some(i) = v.as_integer() { config.tab_size = i as usize; },
                    "expand_tab" => if let Some(b) = v.as_bool() { config.expand_tab = b; },
                    "font_family" => if let Some(s) = v.as_str() { config.font_family = Some(s.to_string()); },
                    "font_size" => {
                        if let Some(f) = v.as_float() {
                            config.font_size = f as f32;
                        } else if let Some(i) = v.as_integer() {
                            config.font_size = i as f32;
                        }
                    }
                    "line_height" => {
                        if let Some(f) = v.as_float() {
                            config.line_height = f as f32;
                        } else if let Some(i) = v.as_integer() {
                            config.line_height = i as f32;
                        }
                    }
                    "ui_font_family" => if let Some(s) = v.as_str() { config.ui_font_family = Some(s.to_string()); },
                    "ui_font_size" => {
                        if let Some(f) = v.as_float() {
                            config.ui_font_size = f as f32;
                        } else if let Some(i) = v.as_integer() {
                            config.ui_font_size = i as f32;
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(config)
    }

    pub fn config_dir() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            std::env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join("zee"))
        }
        #[cfg(not(target_os = "windows"))]
        {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config").join("zee"))
        }
    }

    pub fn config_file_path() -> Option<PathBuf> {
        Self::config_dir().map(|dir| dir.join("config.toml"))
    }

    pub fn themes_dir() -> Option<PathBuf> {
        Self::config_dir().map(|dir| dir.join("themes"))
    }

    pub fn syntax_dir() -> Option<PathBuf> {
        Self::config_dir().map(|dir| dir.join("syntax"))
    }

    pub fn write_key(key: &str, value: &str) -> Result<()> {
        let path = Self::config_file_path().context("Could not determine config path")?;
        let dir = path.parent().context("Could not determine config directory")?;
        
        if !dir.exists() {
            fs::create_dir_all(dir).context("Failed to create config directory")?;
        }

        let content = if path.exists() {
            fs::read_to_string(&path).context("Failed to read config file")?
        } else {
            "# zee configuration file\n\n".to_string()
        };

        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let mut found = false;
        for line in lines.iter_mut() {
            if line.trim().starts_with(key) && line.contains('=') {
                *line = format!("{} = {}", key, value);
                found = true;
                break;
            }
        }

        if !found {
            lines.push(format!("{} = {}", key, value));
        }

        fs::write(&path, lines.join("\n")).context("Failed to write config file")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialize_fonts_and_settings() {
        let toml_str = r#"
        language = "ja"
        theme = "catppuccin-mocha"
        line_numbers = false
        sidebar_position = "left"
        tab_size = 2
        expand_tab = false
        font_family = "Fira Code"
        font_size = 16.0
        line_height = 24.0
        ui_font_family = "Inter"
        ui_font_size = 13.5
        "#;
        let value = toml_span::parse(toml_str).unwrap();
        let config = Config::deserialize_from_value(&value).unwrap();
        assert_eq!(config.language, "ja");
        assert_eq!(config.theme, "catppuccin-mocha");
        assert!(!config.line_numbers);
        assert_eq!(config.sidebar_position, "left");
        assert_eq!(config.tab_size, 2);
        assert!(!config.expand_tab);
        assert_eq!(config.font_family, Some("Fira Code".to_string()));
        assert_eq!(config.font_size, 16.0);
        assert_eq!(config.line_height, 24.0);
        assert_eq!(config.ui_font_family, Some("Inter".to_string()));
        assert_eq!(config.ui_font_size, 13.5);

        assert_eq!(Config::default().sidebar_position, "right");
    }
}
