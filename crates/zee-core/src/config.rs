use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs::{self, File};
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
    #[serde(default)]
    pub show_hidden: bool,
    pub tab_size: usize,
    pub expand_tab: bool,
    pub trim_trailing_whitespace: bool,
    pub ensure_final_newline: bool,
    // GUI specific customization (CLI ignores font settings)
    pub font_family: Option<String>,
    pub font_size: f32,
    pub line_height: f32,
    pub ui_font_family: Option<String>,
    pub ui_font_size: f32,
    #[serde(default = "default_plugin_registries")]
    pub plugin_registries: Vec<String>,
    // Google Drive integration
    pub gdrive_client_id: Option<String>,
    pub gdrive_client_secret: Option<String>,
    // Update channel
    #[serde(default)]
    pub include_prerelease: bool,
}

pub fn default_plugin_registries() -> Vec<String> {
    vec!["https://github.com/kh813/zee-plugins".to_string()]
}

impl Default for Config {
    fn default() -> Self {
        Self {
            language: "auto".to_string(),
            theme: "terminal-default".to_string(),
            line_numbers: true,
            vi_mode: false,
            word_wrap: true,
            sidebar: false,
            sidebar_position: "left".to_string(),
            show_hidden: false,
            tab_size: 4,
            expand_tab: false,
            trim_trailing_whitespace: true,
            ensure_final_newline: true,
            font_family: None,
            font_size: 12.0,
            line_height: 19.0,
            ui_font_family: None,
            ui_font_size: 12.5,
            plugin_registries: default_plugin_registries(),
            gdrive_client_id: None,
            gdrive_client_secret: None,
            include_prerelease: false,
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
                    "trim_trailing_whitespace" => if let Some(b) = v.as_bool() { config.trim_trailing_whitespace = b; },
                    "ensure_final_newline" => if let Some(b) = v.as_bool() { config.ensure_final_newline = b; },
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
                    "plugin_registries" => {
                        if let Some(arr) = v.as_array() {
                            let mut list = Vec::new();
                            for item in arr {
                                if let Some(s) = item.as_str() {
                                    let s_trim = s.trim().to_string();
                                    if !s_trim.is_empty() {
                                        list.push(s_trim);
                                    }
                                }
                            }
                            if !list.is_empty() {
                                config.plugin_registries = list;
                            }
                        }
                    }
                    "gdrive_client_id" => {
                        if let Some(s) = v.as_str() {
                            let s_trim = s.trim();
                            config.gdrive_client_id = if s_trim.is_empty() { None } else { Some(s_trim.to_string()) };
                        }
                    }
                    "gdrive_client_secret" => {
                        if let Some(s) = v.as_str() {
                            let s_trim = s.trim();
                            config.gdrive_client_secret = if s_trim.is_empty() { None } else { Some(s_trim.to_string()) };
                        }
                    }
                    "include_prerelease" => {
                        if let Some(b) = v.as_bool() {
                            config.include_prerelease = b;
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

        // Ensure string values are quoted for valid TOML
        let formatted_val = match key {
            "language" | "theme" | "font_family" | "ui_font_family" | "sidebar_position" => {
                let trimmed = value.trim();
                if trimmed.starts_with('"') && trimmed.ends_with('"') {
                    trimmed.to_string()
                } else {
                    format!("\"{}\"", trimmed)
                }
            }
            _ => value.to_string(),
        };

        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let mut found = false;
        for line in lines.iter_mut() {
            if line.trim().starts_with(key) && line.contains('=') {
                *line = format!("{} = {}", key, formatted_val);
                found = true;
                break;
            }
        }

        if !found {
            lines.push(format!("{} = {}", key, formatted_val));
        }

        fs::write(&path, lines.join("\n")).context("Failed to write config file")?;
        Ok(())
    }

    pub fn save_plugin_registries(registries: &[String]) -> Result<()> {
        let items: Vec<String> = registries.iter().map(|s| format!("\"{}\"", s)).collect();
        let val = format!("[{}]", items.join(", "));
        Self::write_key("plugin_registries", &val)
    }

    pub fn save_show_hidden(show_hidden: bool) -> Result<()> {
        Self::write_key("show_hidden", &show_hidden.to_string())
    }

    pub fn save_gdrive_credentials(client_id: &str, client_secret: &str) -> Result<()> {
        Self::write_key("gdrive_client_id", &format!("\"{}\"", client_id.trim()))?;
        Self::write_key("gdrive_client_secret", &format!("\"{}\"", client_secret.trim()))?;
        Ok(())
    }

    pub fn add_plugin_registry(&mut self, url: &str) -> Result<()> {
        let trimmed = url.trim().to_string();
        if !trimmed.is_empty() && !self.plugin_registries.iter().any(|r| r.trim() == trimmed) {
            self.plugin_registries.push(trimmed);
            let _ = Self::save_plugin_registries(&self.plugin_registries);
        }
        Ok(())
    }

    pub fn remove_plugin_registry(&mut self, url: &str) -> Result<bool> {
        let trimmed = url.trim();
        let initial_len = self.plugin_registries.len();
        self.plugin_registries.retain(|r| {
            let r_trim = r.trim();
            r_trim != trimmed && !r_trim.contains(trimmed)
        });
        let removed = self.plugin_registries.len() < initial_len;
        if removed {
            let _ = Self::save_plugin_registries(&self.plugin_registries);
        }
        Ok(removed)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackupReport {
    pub config_copied: bool,
    pub themes_count: usize,
    pub syntax_count: usize,
    pub plugins_count: usize,
    pub total_files: usize,
}

fn collect_files_recursive(dir: &Path, base: &Path, files: &mut Vec<(PathBuf, String)>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, base, files)?;
        } else if path.is_file() {
            let rel = path.strip_prefix(base)?;
            let rel_str = rel.iter()
                .map(|p| p.to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            files.push((path, rel_str));
        }
    }
    Ok(())
}

pub fn export_backup_from_dir(config_dir: &Path, dest_path: &Path, include_plugins: bool) -> Result<BackupReport> {
    let mut files_to_pack: Vec<(PathBuf, String)> = Vec::new();
    let mut report = BackupReport::default();

    let config_file = config_dir.join("config.toml");
    if config_file.exists() {
        files_to_pack.push((config_file, "config.toml".to_string()));
        report.config_copied = true;
    }

    let themes_dir = config_dir.join("themes");
    let mut theme_files = Vec::new();
    collect_files_recursive(&themes_dir, config_dir, &mut theme_files)?;
    report.themes_count = theme_files.len();
    files_to_pack.extend(theme_files);

    let syntax_dir = config_dir.join("syntax");
    let mut syntax_files = Vec::new();
    collect_files_recursive(&syntax_dir, config_dir, &mut syntax_files)?;
    report.syntax_count = syntax_files.len();
    files_to_pack.extend(syntax_files);

    if include_plugins {
        let plugins_dir = config_dir.join("plugins");
        let mut plugin_files = Vec::new();
        collect_files_recursive(&plugins_dir, config_dir, &mut plugin_files)?;
        report.plugins_count = plugin_files.len();
        files_to_pack.extend(plugin_files);
    }

    report.total_files = files_to_pack.len();

    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let dest_name = dest_path.to_string_lossy().to_lowercase();
    if dest_name.ends_with(".tar.gz") || dest_name.ends_with(".tgz") {
        let tar_gz = File::create(dest_path)?;
        let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
        let mut tar = tar::Builder::new(enc);

        for (file_path, archive_name) in files_to_pack {
            let mut f = File::open(&file_path)?;
            tar.append_file(&archive_name, &mut f)?;
        }
        tar.finish()?;
    } else {
        let zip_file = File::create(dest_path)?;
        let mut zip = zip::ZipWriter::new(zip_file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for (file_path, archive_name) in files_to_pack {
            let mut f = File::open(&file_path)?;
            zip.start_file(&archive_name, options)?;
            std::io::copy(&mut f, &mut zip)?;
        }
        zip.finish()?;
    }

    Ok(report)
}

pub fn export_backup(dest_path: &Path, include_plugins: bool) -> Result<BackupReport> {
    let config_dir = Config::config_dir().context("Could not determine config directory")?;
    export_backup_from_dir(&config_dir, dest_path, include_plugins)
}

pub fn import_backup_to_dir(config_dir: &Path, src_path: &Path) -> Result<BackupReport> {
    if !src_path.exists() {
        return Err(anyhow::anyhow!("Source file does not exist: {:?}", src_path));
    }
    fs::create_dir_all(config_dir)?;

    let mut report = BackupReport::default();
    let src_str = src_path.to_string_lossy().to_lowercase();

    if src_str.ends_with(".toml") {
        let dest = config_dir.join("config.toml");
        fs::copy(src_path, dest)?;
        report.config_copied = true;
        report.total_files = 1;
        return Ok(report);
    }

    if src_str.ends_with(".tar.gz") || src_str.ends_with(".tgz") {
        let file = File::open(src_path)?;
        let dec = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(dec);

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();
            if path.iter().any(|c| c == ".." || c == "/") {
                continue;
            }
            let out_path = config_dir.join(&path);
            if let Some(p) = out_path.parent() {
                fs::create_dir_all(p)?;
            }
            if entry.header().entry_type().is_file() {
                entry.unpack(&out_path)?;
                let rel_str = path.to_string_lossy();
                if rel_str == "config.toml" {
                    report.config_copied = true;
                } else if rel_str.starts_with("themes") {
                    report.themes_count += 1;
                } else if rel_str.starts_with("syntax") {
                    report.syntax_count += 1;
                } else if rel_str.starts_with("plugins") {
                    report.plugins_count += 1;
                }
                report.total_files += 1;
            }
        }
        return Ok(report);
    }

    let file = File::open(src_path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    for i in 0..zip.len() {
        let mut zfile = zip.by_index(i)?;
        let raw_name = zfile.name().to_string();
        let path = PathBuf::from(&raw_name);
        if path.iter().any(|c| c == ".." || c == "/") {
            continue;
        }
        let out_path = config_dir.join(&path);
        if zfile.is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(p) = out_path.parent() {
                fs::create_dir_all(p)?;
            }
            let mut out = File::create(&out_path)?;
            std::io::copy(&mut zfile, &mut out)?;

            if raw_name == "config.toml" {
                report.config_copied = true;
            } else if raw_name.starts_with("themes") {
                report.themes_count += 1;
            } else if raw_name.starts_with("syntax") {
                report.syntax_count += 1;
            } else if raw_name.starts_with("plugins") {
                report.plugins_count += 1;
            }
            report.total_files += 1;
        }
    }

    Ok(report)
}

pub fn import_backup(src_path: &Path) -> Result<BackupReport> {
    let config_dir = Config::config_dir().context("Could not determine config directory")?;
    import_backup_to_dir(&config_dir, src_path)
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

        assert_eq!(Config::default().sidebar_position, "left");
        assert_eq!(Config::default().language, "auto");
    }

    #[test]
    fn test_export_and_import_backup_zip() {
        let temp_dir = std::env::temp_dir().join(format!("zee_test_backup_zip_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let src_config_dir = temp_dir.join("src_config");
        let dst_config_dir = temp_dir.join("dst_config");
        let zip_file = temp_dir.join("backup.zip");

        fs::create_dir_all(src_config_dir.join("themes")).unwrap();
        fs::create_dir_all(src_config_dir.join("plugins")).unwrap();
        fs::write(src_config_dir.join("config.toml"), "theme = \"nord\"\n").unwrap();
        fs::write(src_config_dir.join("themes").join("my_theme.toml"), "# theme\n").unwrap();
        fs::write(src_config_dir.join("plugins").join("my_plugin.wasm"), b"\0asm").unwrap();

        // Export with plugins
        let export_rep = export_backup_from_dir(&src_config_dir, &zip_file, true).unwrap();
        assert!(export_rep.config_copied);
        assert_eq!(export_rep.themes_count, 1);
        assert_eq!(export_rep.plugins_count, 1);
        assert_eq!(export_rep.total_files, 3);
        assert!(zip_file.exists());

        // Import
        let import_rep = import_backup_to_dir(&dst_config_dir, &zip_file).unwrap();
        assert!(import_rep.config_copied);
        assert_eq!(import_rep.themes_count, 1);
        assert_eq!(import_rep.plugins_count, 1);
        assert_eq!(import_rep.total_files, 3);

        assert!(dst_config_dir.join("config.toml").exists());
        assert_eq!(fs::read_to_string(dst_config_dir.join("config.toml")).unwrap(), "theme = \"nord\"\n");
        assert!(dst_config_dir.join("themes").join("my_theme.toml").exists());
        assert!(dst_config_dir.join("plugins").join("my_plugin.wasm").exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_export_and_import_backup_tar_gz() {
        let temp_dir = std::env::temp_dir().join(format!("zee_test_backup_tar_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let src_config_dir = temp_dir.join("src_config");
        let dst_config_dir = temp_dir.join("dst_config");
        let tar_file = temp_dir.join("backup.tar.gz");

        fs::create_dir_all(src_config_dir.join("syntax")).unwrap();
        fs::write(src_config_dir.join("config.toml"), "theme = \"solarized\"\n").unwrap();
        fs::write(src_config_dir.join("syntax").join("custom.syntax"), "syntax rules").unwrap();

        // Export without plugins
        let export_rep = export_backup_from_dir(&src_config_dir, &tar_file, false).unwrap();
        assert!(export_rep.config_copied);
        assert_eq!(export_rep.syntax_count, 1);
        assert_eq!(export_rep.plugins_count, 0);
        assert_eq!(export_rep.total_files, 2);
        assert!(tar_file.exists());

        // Import
        let import_rep = import_backup_to_dir(&dst_config_dir, &tar_file).unwrap();
        assert!(import_rep.config_copied);
        assert_eq!(import_rep.syntax_count, 1);
        assert_eq!(import_rep.total_files, 2);

        assert_eq!(fs::read_to_string(dst_config_dir.join("config.toml")).unwrap(), "theme = \"solarized\"\n");
        assert!(dst_config_dir.join("syntax").join("custom.syntax").exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_config_plugin_registries() {
        let toml_content = r#"
theme = "nord"
plugin_registries = [
    "https://github.com/kh813/zee-plugins",
    "https://github.com/other-user/zee-plugins"
]
"#;
        let raw = toml_span::parse(toml_content).unwrap();
        let mut config = Config::deserialize_from_value(&raw).unwrap();
        assert_eq!(config.plugin_registries.len(), 2);
        assert_eq!(config.plugin_registries[0], "https://github.com/kh813/zee-plugins");
        assert_eq!(config.plugin_registries[1], "https://github.com/other-user/zee-plugins");

        // Add registry
        let _ = config.add_plugin_registry("https://github.com/third-user/plugins");
        assert_eq!(config.plugin_registries.len(), 3);
        // Duplicate should not be added
        let _ = config.add_plugin_registry("https://github.com/third-user/plugins");
        assert_eq!(config.plugin_registries.len(), 3);

        // Remove registry
        let removed = config.remove_plugin_registry("third-user").unwrap();
        assert!(removed);
        assert_eq!(config.plugin_registries.len(), 2);
    }
}
