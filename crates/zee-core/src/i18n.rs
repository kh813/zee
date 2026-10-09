use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;
use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageInfo {
    pub id: &'static str,
    pub name: &'static str,
}

pub const AVAILABLE_LANGUAGES: &[LanguageInfo] = &[
    LanguageInfo { id: "auto", name: "Auto" },
    LanguageInfo { id: "en", name: "English" },
    LanguageInfo { id: "ja", name: "日本語 (Japanese)" },
    LanguageInfo { id: "zh-CN", name: "简体中文 (Simplified Chinese)" },
    LanguageInfo { id: "zh-TW", name: "繁體中文 (Traditional Chinese)" },
    LanguageInfo { id: "ko", name: "한국어 (Korean)" },
    LanguageInfo { id: "es", name: "Español (Spanish)" },
    LanguageInfo { id: "fr", name: "Français (French)" },
    LanguageInfo { id: "de", name: "Deutsch (German)" },
    LanguageInfo { id: "it", name: "Italiano (Italian)" },
    LanguageInfo { id: "pt", name: "Português (Portuguese)" },
    LanguageInfo { id: "ru", name: "Русский (Russian)" },
];

#[cfg(target_os = "macos")]
fn get_macos_locale() -> Option<String> {
    if let Ok(output) = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn get_windows_locale() -> Option<String> {
    const LOCALE_NAME_MAX_LENGTH: usize = 85;
    let mut buffer = [0u16; LOCALE_NAME_MAX_LENGTH];
    extern "system" {
        fn GetUserDefaultLocaleName(lpLocaleName: *mut u16, cchLocaleName: i32) -> i32;
    }
    unsafe {
        let ret = GetUserDefaultLocaleName(buffer.as_mut_ptr(), LOCALE_NAME_MAX_LENGTH as i32);
        if ret > 0 {
            let len = (ret as usize).saturating_sub(1);
            String::from_utf16(&buffer[..len]).ok()
        } else {
            None
        }
    }
}

pub fn detect_system_locale() -> &'static str {
    static CACHED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CACHED.get_or_init(|| {
        for var in &["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var) {
                let val = val.trim();
                if !val.is_empty() && val != "C" && val != "POSIX" {
                    return val.to_string();
                }
            }
        }

        #[cfg(target_os = "macos")]
        if let Some(loc) = get_macos_locale() {
            return loc;
        }

        #[cfg(target_os = "windows")]
        if let Some(loc) = get_windows_locale() {
            return loc;
        }

        "en".to_string()
    })
}

#[derive(Clone)]
pub struct I18n {
    strings: HashMap<String, String>,
}

impl I18n {
    pub fn normalize_lang(lang: &str) -> String {
        let cleaned = lang.split('.').next().unwrap_or(lang).replace('_', "-");
        let lower = cleaned.to_lowercase();
        if lower.starts_with("ja") {
            "ja".to_string()
        } else if lower.starts_with("zh-tw") || lower.starts_with("zh-hk") || lower.starts_with("zh-hant") {
            "zh-TW".to_string()
        } else if lower.starts_with("zh") {
            "zh-CN".to_string()
        } else if lower.starts_with("ko") {
            "ko".to_string()
        } else if lower.starts_with("es") {
            "es".to_string()
        } else if lower.starts_with("fr") {
            "fr".to_string()
        } else if lower.starts_with("de") {
            "de".to_string()
        } else if lower.starts_with("it") {
            "it".to_string()
        } else if lower.starts_with("pt") {
            "pt".to_string()
        } else if lower.starts_with("ru") {
            "ru".to_string()
        } else {
            "en".to_string()
        }
    }

    pub fn resolve_lang(lang: &str) -> String {
        let trimmed = lang.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
            Self::normalize_lang(detect_system_locale())
        } else {
            Self::normalize_lang(trimmed)
        }
    }

    pub fn load(lang: &str) -> Self {
        let norm = Self::resolve_lang(lang);
        let mut strings = match norm.as_str() {
            "ja" => Self::get_ja_defaults(),
            "zh-CN" => Self::get_zh_cn_defaults(),
            "zh-TW" => Self::get_zh_tw_defaults(),
            "ko" => Self::get_ko_defaults(),
            "es" => Self::get_es_defaults(),
            "fr" => Self::get_fr_defaults(),
            "de" => Self::get_de_defaults(),
            "it" => Self::get_it_defaults(),
            "pt" => Self::get_pt_defaults(),
            "ru" => Self::get_ru_defaults(),
            _ => Self::get_en_defaults(),
        };
        
        // Always load English as absolute base fallback
        if norm != "en" {
            let en = Self::get_en_defaults();
            for (k, v) in en {
                strings.entry(k).or_insert(v);
            }
        }

        if let Some(path) = Self::locale_file_path(&norm) {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(path) {
                    if let Ok(custom) = Self::parse_locale_toml(&content) {
                        for (k, v) in custom {
                            strings.insert(k, v);
                        }
                    }
                }
            }
        }
        
        Self { strings }
    }

    fn get_en_defaults() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("menu.zee.about".to_string(), "About zee".to_string());
        m.insert("menu.zee.quit".to_string(), "Quit zee".to_string());
        m.insert("menu.tabs".to_string(), "Tabs".to_string());
        m.insert("menu.tabs.next".to_string(), "Next Tab".to_string());
        m.insert("menu.tabs.prev".to_string(), "Previous Tab".to_string());

        m.insert("menu.file".to_string(), "File".to_string());
        m.insert("menu.file.new_tab".to_string(), "New Tab".to_string());
        m.insert("menu.file.new_window".to_string(), "New Window".to_string());
        m.insert("menu.file.new".to_string(), "New".to_string());
        m.insert("menu.file.new_from_template".to_string(), "New from Template…".to_string());
        m.insert("menu.file.open_templates_folder".to_string(), "Open Templates Folder".to_string());
        m.insert("menu.file.open".to_string(), "Open…".to_string());
        m.insert("menu.file.open_folder".to_string(), "Open Folder…".to_string());
        m.insert("menu.file.open_gdrive".to_string(), "Open from Google Drive…".to_string());
        m.insert("menu.file.open_recent".to_string(), "Open Recent".to_string());
        m.insert("menu.file.clear_recent".to_string(), "Clear Recent".to_string());
        m.insert("menu.file.no_recent".to_string(), "No Recent Files".to_string());
        m.insert("menu.file.reload".to_string(), "Reload File".to_string());
        m.insert("menu.file.save".to_string(), "Save".to_string());
        m.insert("menu.file.save_as".to_string(), "Save As…".to_string());
        m.insert("menu.file.trim_trailing_whitespace".to_string(), "Trim Trailing Whitespace on Save".to_string());
        m.insert("menu.file.ensure_final_newline".to_string(), "Ensure Final Newline on Save".to_string());
        m.insert("menu.file.close".to_string(), "Close".to_string());
        m.insert("menu.file.exit".to_string(), "Exit".to_string());

        m.insert("menu.edit".to_string(), "Edit".to_string());
        m.insert("menu.edit.undo".to_string(), "Undo".to_string());
        m.insert("menu.edit.redo".to_string(), "Redo".to_string());
        m.insert("menu.edit.cut".to_string(), "Cut".to_string());
        m.insert("menu.edit.copy".to_string(), "Copy".to_string());
        m.insert("menu.edit.paste".to_string(), "Paste".to_string());
        m.insert("menu.edit.find".to_string(), "Find…".to_string());
        m.insert("menu.edit.replace".to_string(), "Replace…".to_string());
        m.insert("menu.edit.select_all".to_string(), "Select All".to_string());
        m.insert("menu.edit.format_document".to_string(), "Format Document (Plugin)".to_string());
        m.insert("menu.edit.sort_lines".to_string(), "Sort Lines (Plugin)".to_string());
        m.insert("menu.edit.to_uppercase".to_string(), "Transform to UPPERCASE".to_string());
        m.insert("menu.edit.to_lowercase".to_string(), "Transform to lowercase".to_string());
        m.insert("menu.edit.to_snake_case".to_string(), "Transform to snake_case".to_string());
        m.insert("menu.edit.to_camel_case".to_string(), "Transform to camelCase".to_string());

        m.insert("menu.view".to_string(), "View".to_string());
        m.insert("menu.view.go_to_line".to_string(), "Go to Line…".to_string());
        m.insert("menu.view.line_numbers".to_string(), "Line Numbers".to_string());
        m.insert("menu.view.sidebar".to_string(), "Sidebar".to_string());
        m.insert("menu.view.word_wrap".to_string(), "Word Wrap".to_string());
        m.insert("menu.view.vi_mode".to_string(), "Vi Mode".to_string());
        m.insert("menu.view.encoding".to_string(), "Encoding".to_string());
        m.insert("menu.view.reopen_with_encoding".to_string(), "Reopen with Encoding".to_string());
        m.insert("menu.view.convert_to_encoding".to_string(), "Convert to Encoding".to_string());
        m.insert("menu.view.line_ending".to_string(), "Line Ending".to_string());
        m.insert("menu.view.theme".to_string(), "Theme".to_string());
        m.insert("menu.view.syntax".to_string(), "Syntax".to_string());
        m.insert("menu.view.language".to_string(), "Language".to_string());
        m.insert("menu.view.outline".to_string(), "Outline".to_string());

        m.insert("menu.help".to_string(), "Help".to_string());
        m.insert("menu.help.about".to_string(), "About".to_string());
        m.insert("menu.help.check_for_updates".to_string(), "Check for Updates…".to_string());
        m.insert("dialog.update.title".to_string(), "Check for Updates".to_string());
        m.insert("dialog.update.checking".to_string(), "Checking for updates...".to_string());
        m.insert("dialog.update.up_to_date".to_string(), "zee is up to date (v{version}).".to_string());
        m.insert("dialog.update.available".to_string(), "A new version (v{version}) is available. Would you like to update now?".to_string());
        m.insert("dialog.update.downloading".to_string(), "Downloading and applying update...".to_string());
        m.insert("dialog.update.success".to_string(), "Update installed! Restarting zee...".to_string());
        m.insert("dialog.update.failed".to_string(), "Update failed: {error}".to_string());
        m.insert("dialog.update.btn_update".to_string(), "Update Now".to_string());
        m.insert("dialog.update.btn_open_url".to_string(), "Open Release Page".to_string());

        m.insert("panel.find".to_string(), "Find:".to_string());
        m.insert("panel.replace".to_string(), "Replace:".to_string());
        m.insert("panel.prev".to_string(), "< Prev".to_string());
        m.insert("panel.next".to_string(), "> Next".to_string());
        m.insert("panel.replace_one".to_string(), "Replace".to_string());
        m.insert("panel.replace_all".to_string(), "Replace All".to_string());
        m.insert("panel.close".to_string(), "Close".to_string());
        m.insert("panel.match_case".to_string(), "Match Case".to_string());
        m.insert("panel.whole_word".to_string(), "Whole Word".to_string());
        m.insert("panel.use_regex".to_string(), "Use Regex".to_string());

        m.insert("status.no_name".to_string(), "[No Name]".to_string());
        m.insert("status.no_matches".to_string(), "No matches".to_string());
        m.insert("status.search_wrapped_top".to_string(), "Search wrapped to top".to_string());
        m.insert("status.search_wrapped_bottom".to_string(), "Search wrapped to bottom".to_string());
        m.insert("status.matches".to_string(), "{current} of {total} matches".to_string());
        m.insert("status.replaced_count".to_string(), "{n} replacement(s) made".to_string());
        m.insert("status.terminal_too_small".to_string(), "Terminal too small ({cols}x{rows}). Please resize.".to_string());
        m.insert("status.cursor".to_string(), "Ln {line}, Col {col}".to_string());
        m.insert("status.selection".to_string(), "{n} chars".to_string());

        m.insert("error".to_string(), "Error".to_string());
        m.insert("error.cannot_open_dir".to_string(), "Cannot open directory: {path}".to_string());
        m.insert("error.failed_to_open".to_string(), "Failed to open {path}: {error}".to_string());

        m.insert("dialog.ok".to_string(), "OK".to_string());
        m.insert("dialog.cancel".to_string(), "Cancel".to_string());
        m.insert("dialog.yes".to_string(), "Yes".to_string());
        m.insert("dialog.no".to_string(), "No".to_string());
        m.insert("dialog.save".to_string(), "Save".to_string());
        m.insert("dialog.dont_save".to_string(), "Don't Save".to_string());
        m.insert("dialog.discard_reopen".to_string(), "Discard & Reopen".to_string());
        m.insert("dialog.discard_reopen_prompt".to_string(), "Discard unsaved changes and reopen?".to_string());
        m.insert("dialog.reopen_file".to_string(), "Reopen File".to_string());
        m.insert("dialog.open_file".to_string(), "Open File…".to_string());
        m.insert("dialog.new_from_template".to_string(), "New from Template".to_string());
        m.insert("dialog.save_as".to_string(), "Save As…".to_string());
        m.insert("dialog.go_to_line".to_string(), "Go to Line".to_string());
        m.insert("dialog.about".to_string(), "About".to_string());
        m.insert("dialog.show_hidden".to_string(), "Show Hidden".to_string());
        m.insert("dialog.detect_encoding".to_string(), "Detect Encoding".to_string());
        m.insert("dialog.overwrite_prompt".to_string(), "File already exists. Overwrite?".to_string());
        m.insert("dialog.unsaved_changes_title".to_string(), "Unsaved Changes".to_string());
        m.insert("dialog.unsaved_changes".to_string(), "Unsaved changes in \"{filename}\".".to_string());

        m.insert("dialog.file_browser.name".to_string(), "Name".to_string());
        m.insert("dialog.file_browser.size".to_string(), "Size".to_string());
        m.insert("dialog.file_browser.modified".to_string(), "Modified".to_string());
        m.insert("dialog.file_browser.filename".to_string(), "File name".to_string());

        m.insert("sidebar.properties".to_string(), "File Properties".to_string());
        m.insert("sidebar.prop_file".to_string(), "File".to_string());
        m.insert("sidebar.prop_size".to_string(), "Size".to_string());
        m.insert("sidebar.prop_lines".to_string(), "Lines".to_string());
        m.insert("sidebar.prop_chars".to_string(), "Characters".to_string());
        m.insert("sidebar.prop_encoding".to_string(), "Encoding".to_string());
        m.insert("sidebar.prop_line_ending".to_string(), "Line Ending".to_string());
        m.insert("sidebar.no_file".to_string(), "(No active file)".to_string());
        m.insert("sidebar.no_files".to_string(), "(No files)".to_string());
        m.insert("sidebar.no_headings".to_string(), "(No headings in this file)".to_string());
        m.insert("sidebar.new_file".to_string(), "New File".to_string());
        m.insert("sidebar.new_folder".to_string(), "New Folder".to_string());
        m.insert("sidebar.rename".to_string(), "Rename".to_string());
        m.insert("sidebar.delete".to_string(), "Delete".to_string());
        m.insert("sidebar.refresh".to_string(), "Refresh".to_string());
        m.insert("sidebar.delete_confirm".to_string(), "Are you sure you want to delete this?".to_string());
        m.insert("sidebar.show_hidden".to_string(), "Show Hidden Files".to_string());
        m.insert("sidebar.hide_hidden".to_string(), "Hide Hidden Files".to_string());
        m.insert("dialog.file_name".to_string(), "File name:".to_string());
        m.insert("dialog.folder_name".to_string(), "Folder name:".to_string());
        m.insert("dialog.new_name".to_string(), "New name:".to_string());
        m.insert("menu.view.refresh_files".to_string(), "Refresh File Tree".to_string());

        m.insert("menu.view.syntax_plain".to_string(), "Plain Text".to_string());

        m.insert("menu.app.preferences".to_string(), "Preferences…".to_string());
        m.insert("menu.view.zoom_in".to_string(), "Zoom In".to_string());
        m.insert("menu.view.zoom_out".to_string(), "Zoom Out".to_string());
        m.insert("menu.view.reset_zoom".to_string(), "Reset Zoom".to_string());

        m.insert("dialog.settings.title".to_string(), "Preferences".to_string());
        m.insert("dialog.settings.theme".to_string(), "Theme".to_string());
        m.insert("dialog.settings.language".to_string(), "Language".to_string());
        m.insert("dialog.settings.language_auto".to_string(), "Auto (System)".to_string());
        m.insert("dialog.settings.font_family".to_string(), "Editor Font".to_string());
        m.insert("dialog.settings.font_system_default".to_string(), "System Default".to_string());
        m.insert("dialog.settings.font_size".to_string(), "Font Size".to_string());
        m.insert("dialog.settings.line_height".to_string(), "Line Height".to_string());
        m.insert("dialog.settings.ui_font_size".to_string(), "UI Font Size".to_string());
        m.insert("dialog.settings.tab_size".to_string(), "Tab Width".to_string());
        m.insert("dialog.settings.sidebar_position".to_string(), "Sidebar Position".to_string());
        m.insert("dialog.settings.sidebar_left".to_string(), "Left".to_string());
        m.insert("dialog.settings.sidebar_right".to_string(), "Right".to_string());
        m.insert("dialog.settings.include_prerelease".to_string(), "Receive preview (test) updates".to_string());
        m.insert("dialog.settings.reset_defaults".to_string(), "Reset Defaults".to_string());
        m.insert("dialog.settings.backup_section".to_string(), "Backup & Restore".to_string());
        m.insert("dialog.settings.export_config_only".to_string(), "Export Settings (Config Only)".to_string());
        m.insert("dialog.settings.export_all".to_string(), "Export All (Config + Plugins)".to_string());
        m.insert("dialog.settings.import_backup".to_string(), "Import Settings / Backup…".to_string());

        m.insert("menu.file.export_config".to_string(), "Export Settings…".to_string());
        m.insert("menu.file.import_config".to_string(), "Import Settings…".to_string());
        m.insert("menu.plugins".to_string(), "Plugins".to_string());
        m.insert("menu.plugins.manage".to_string(), "Manage Plugins…".to_string());
        m.insert("menu.plugins.open_folder".to_string(), "Open Plugins Folder".to_string());
        m.insert("menu.plugins.no_plugins".to_string(), "No Plugins Installed".to_string());

        m.insert("dialog.backup.export_title".to_string(), "Export Complete".to_string());
        m.insert("dialog.backup.export_success".to_string(), "Successfully exported {count} files to:\n{path}".to_string());
        m.insert("dialog.backup.import_title".to_string(), "Import Complete".to_string());
        m.insert("dialog.backup.import_success".to_string(), "Successfully restored configuration and plugins ({count} files).".to_string());
        m.insert("dialog.backup.error_title".to_string(), "Backup Operation Failed".to_string());

        m.insert("dialog.plugin.title".to_string(), "Plugin Manager".to_string());
        m.insert("dialog.plugin.tab_installed".to_string(), "Installed".to_string());
        m.insert("dialog.plugin.tab_registry".to_string(), "Online Registry".to_string());
        m.insert("dialog.plugin.install".to_string(), "Install Plugin…".to_string());
        m.insert("dialog.plugin.install_local".to_string(), "Install from File…".to_string());
        m.insert("dialog.plugin.install_online".to_string(), "Install".to_string());
        m.insert("dialog.plugin.loading".to_string(), "Loading plugins from registry…".to_string());
        m.insert("dialog.plugin.registry_empty".to_string(), "No plugins available in registry.".to_string());
        m.insert("dialog.plugin.open_dir".to_string(), "Open Plugins Folder".to_string());
        m.insert("dialog.plugin.uninstall".to_string(), "Uninstall".to_string());
        m.insert("dialog.plugin.installed".to_string(), "Installed Plugins".to_string());
        m.insert("dialog.plugin.empty".to_string(), "No plugins installed yet.".to_string());
        m.insert("dialog.plugin.active".to_string(), "Active".to_string());
        m.insert("dialog.plugin.repositories".to_string(), "Repositories:".to_string());
        m.insert("dialog.plugin.add_repo_btn".to_string(), "+ Add".to_string());
        m.insert("dialog.plugin.repo_placeholder".to_string(), "GitHub URL (e.g. user/plugins) or index.json URL".to_string());
        m.insert("dialog.plugin.repo_add".to_string(), "Add".to_string());
        m.insert("dialog.plugin.repo_cancel".to_string(), "Cancel".to_string());
        m.insert("dialog.plugin.search_placeholder".to_string(), "Search plugins by name, description, ID…".to_string());
        m.insert("dialog.plugin.no_search_results".to_string(), "No plugins match the search criteria.".to_string());
        m.insert("dialog.plugin.clear_search".to_string(), "Clear search".to_string());

        m.insert("about.version".to_string(), "Version".to_string());
        m.insert("about.license".to_string(), "License".to_string());

        m.insert("gdrive.title".to_string(), "Google Drive".to_string());
        m.insert("gdrive.connect".to_string(), "Connect Google Drive".to_string());
        m.insert("gdrive.connecting".to_string(), "Authenticating in browser...".to_string());
        m.insert("gdrive.connected".to_string(), "Connected".to_string());
        m.insert("gdrive.sign_out".to_string(), "Sign Out".to_string());
        m.insert("gdrive.syncing".to_string(), "Syncing with Google Drive...".to_string());
        m.insert("gdrive.synced".to_string(), "Google Drive Synced".to_string());
        m.insert("gdrive.sync_error".to_string(), "Google Drive Sync Failed".to_string());
        m.insert("gdrive.open".to_string(), "Open".to_string());
        m.insert("gdrive.cancel".to_string(), "Cancel".to_string());
        m.insert("gdrive.search_placeholder".to_string(), "Search Google Drive...".to_string());
        m.insert("gdrive.setup_title".to_string(), "Google Drive API Setup (Required)".to_string());
        m.insert("gdrive.setup_desc".to_string(), "To connect Zee directly to your Google Drive, please create a client key in Google Cloud Console:".to_string());
        m.insert("gdrive.step1".to_string(), "1. Open Google Cloud Console and create a new project (e.g. 'zee-GoogleDrive').".to_string());
        m.insert("gdrive.step2".to_string(), "2. Go to 'APIs & Services' -> 'Library', search for 'Google Drive API' and click 'Enable'.".to_string());
        m.insert("gdrive.step3".to_string(), "3. Go to 'Credentials' -> 'Create Credentials' -> 'OAuth client ID' and select 'Desktop app'.".to_string());
        m.insert("gdrive.step4".to_string(), "4. Copy the generated Client ID (and Secret) and paste them below:".to_string());
        m.insert("gdrive.open_gcp_btn".to_string(), "Open Google Cloud Console ↗".to_string());
        m.insert("gdrive.client_id_label".to_string(), "OAuth Client ID:".to_string());
        m.insert("gdrive.client_secret_label".to_string(), "OAuth Client Secret:".to_string());
        m.insert("gdrive.save_and_connect".to_string(), "Save & Connect".to_string());
        m.insert("gdrive.configure_api".to_string(), "Configure API Keys…".to_string());
        m.insert("gdrive.credentials_missing".to_string(), "Client ID is required to start authentication.".to_string());
        m.insert("gdrive.cancel_wait".to_string(), "Cancel Authentication".to_string());
        m
    }

    fn get_ja_defaults() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("menu.zee.about".to_string(), "zee について".to_string());
        m.insert("menu.zee.quit".to_string(), "zee を終了".to_string());
        m.insert("menu.tabs".to_string(), "タブ".to_string());
        m.insert("menu.tabs.next".to_string(), "次のタブ".to_string());
        m.insert("menu.tabs.prev".to_string(), "前のタブ".to_string());

        m.insert("menu.file".to_string(), "ファイル".to_string());
        m.insert("menu.file.new_tab".to_string(), "新規タブ".to_string());
        m.insert("menu.file.new_window".to_string(), "新規ウィンドウ".to_string());
        m.insert("menu.file.new".to_string(), "新規作成".to_string());
        m.insert("menu.file.new_from_template".to_string(), "テンプレートから新規作成…".to_string());
        m.insert("menu.file.open_templates_folder".to_string(), "テンプレートフォルダを開く".to_string());
        m.insert("menu.file.open".to_string(), "開く…".to_string());
        m.insert("menu.file.open_folder".to_string(), "フォルダを開く…".to_string());
        m.insert("menu.file.open_gdrive".to_string(), "Google ドライブから開く…".to_string());
        m.insert("menu.file.open_recent".to_string(), "最近開いたファイル".to_string());
        m.insert("menu.file.clear_recent".to_string(), "履歴を消去".to_string());
        m.insert("menu.file.no_recent".to_string(), "履歴なし".to_string());
        m.insert("menu.file.reload".to_string(), "ファイルを再読み込み".to_string());
        m.insert("menu.file.save".to_string(), "保存".to_string());
        m.insert("menu.file.save_as".to_string(), "名前を付けて保存…".to_string());
        m.insert("menu.file.trim_trailing_whitespace".to_string(), "保存時に行末の空白を削除".to_string());
        m.insert("menu.file.ensure_final_newline".to_string(), "保存時に末尾改行を付与".to_string());
        m.insert("menu.file.close".to_string(), "閉じる".to_string());
        m.insert("menu.file.exit".to_string(), "終了".to_string());

        m.insert("menu.edit".to_string(), "編集".to_string());
        m.insert("menu.edit.undo".to_string(), "元に戻す".to_string());
        m.insert("menu.edit.redo".to_string(), "やり直し".to_string());
        m.insert("menu.edit.cut".to_string(), "切り取り".to_string());
        m.insert("menu.edit.copy".to_string(), "コピー".to_string());
        m.insert("menu.edit.paste".to_string(), "貼り付け".to_string());
        m.insert("menu.edit.find".to_string(), "検索…".to_string());
        m.insert("menu.edit.replace".to_string(), "置換…".to_string());
        m.insert("menu.edit.select_all".to_string(), "すべて選択".to_string());
        m.insert("menu.edit.format_document".to_string(), "ドキュメント整形 (プラグイン)".to_string());
        m.insert("menu.edit.sort_lines".to_string(), "行の並び替え (プラグイン)".to_string());
        m.insert("menu.edit.to_uppercase".to_string(), "大文字に変換".to_string());
        m.insert("menu.edit.to_lowercase".to_string(), "小文字に変換".to_string());
        m.insert("menu.edit.to_snake_case".to_string(), "snake_caseに変換".to_string());
        m.insert("menu.edit.to_camel_case".to_string(), "camelCaseに変換".to_string());

        m.insert("menu.view".to_string(), "表示".to_string());
        m.insert("menu.view.go_to_line".to_string(), "行移動…".to_string());
        m.insert("menu.view.line_numbers".to_string(), "行番号".to_string());
        m.insert("menu.view.sidebar".to_string(), "サイドバー".to_string());
        m.insert("menu.view.word_wrap".to_string(), "右端で折り返す".to_string());
        m.insert("menu.view.vi_mode".to_string(), "Viモード".to_string());
        m.insert("menu.view.encoding".to_string(), "エンコード".to_string());
        m.insert("menu.view.reopen_with_encoding".to_string(), "エンコーディングを指定して再読み込み".to_string());
        m.insert("menu.view.convert_to_encoding".to_string(), "エンコーディングを変換".to_string());
        m.insert("menu.view.line_ending".to_string(), "改行コード".to_string());
        m.insert("menu.view.theme".to_string(), "テーマ".to_string());
        m.insert("menu.view.syntax".to_string(), "シンタックス".to_string());
        m.insert("menu.view.language".to_string(), "言語".to_string());
        m.insert("menu.view.outline".to_string(), "アウトライン".to_string());

        m.insert("menu.help".to_string(), "ヘルプ".to_string());
        m.insert("menu.help.about".to_string(), "このソフトについて".to_string());
        m.insert("menu.help.check_for_updates".to_string(), "アップデートを確認…".to_string());
        m.insert("dialog.update.title".to_string(), "アップデートの確認".to_string());
        m.insert("dialog.update.checking".to_string(), "最新バージョンを確認中...".to_string());
        m.insert("dialog.update.up_to_date".to_string(), "最新バージョン (v{version}) を使用しています。".to_string());
        m.insert("dialog.update.available".to_string(), "新しいバージョン (v{version}) が利用可能です。今すぐアップデートしますか？".to_string());
        m.insert("dialog.update.downloading".to_string(), "アップデートをダウンロードして適用中...".to_string());
        m.insert("dialog.update.success".to_string(), "アップデートが完了しました！ 再起動しています...".to_string());
        m.insert("dialog.update.failed".to_string(), "アップデートに失敗しました: {error}".to_string());
        m.insert("dialog.update.btn_update".to_string(), "今すぐアップデート".to_string());
        m.insert("dialog.update.btn_open_url".to_string(), "リリースを開く".to_string());

        m.insert("panel.find".to_string(), "検索:".to_string());
        m.insert("panel.replace".to_string(), "置換:".to_string());
        m.insert("panel.prev".to_string(), "前を検索".to_string());
        m.insert("panel.next".to_string(), "次を検索".to_string());
        m.insert("panel.replace_one".to_string(), "置換".to_string());
        m.insert("panel.replace_all".to_string(), "すべて置換".to_string());
        m.insert("panel.close".to_string(), "閉じる".to_string());
        m.insert("panel.match_case".to_string(), "大文字小文字を区別".to_string());
        m.insert("panel.whole_word".to_string(), "単語単位".to_string());
        m.insert("panel.use_regex".to_string(), "正規表現".to_string());

        m.insert("status.no_name".to_string(), "[無題]".to_string());
        m.insert("status.no_matches".to_string(), "見つかりません".to_string());
        m.insert("status.search_wrapped_top".to_string(), "最後まで検索したので先頭に戻りました".to_string());
        m.insert("status.search_wrapped_bottom".to_string(), "先頭まで検索したので最後に戻りました".to_string());
        m.insert("status.matches".to_string(), "{total} 個中 {current} 番目の一致".to_string());
        m.insert("status.replaced_count".to_string(), "{n} 箇所を置換しました".to_string());
        m.insert("status.terminal_too_small".to_string(), "ターミナルが小さすぎます ({cols}x{rows})。サイズを大きくしてください。".to_string());
        m.insert("status.cursor".to_string(), "{line} 行, {col} 列".to_string());
        m.insert("status.selection".to_string(), "{n} 文字選択".to_string());

        m.insert("error".to_string(), "エラー".to_string());
        m.insert("error.cannot_open_dir".to_string(), "ディレクトリは開けません: {path}".to_string());
        m.insert("error.failed_to_open".to_string(), "ファイルを読み込めませんでした {path}: {error}".to_string());

        m.insert("dialog.ok".to_string(), "OK".to_string());
        m.insert("dialog.cancel".to_string(), "キャンセル".to_string());
        m.insert("dialog.yes".to_string(), "はい".to_string());
        m.insert("dialog.no".to_string(), "いいえ".to_string());
        m.insert("dialog.save".to_string(), "保存".to_string());
        m.insert("dialog.dont_save".to_string(), "保存しない".to_string());
        m.insert("dialog.discard_reopen".to_string(), "破棄して再読み込み".to_string());
        m.insert("dialog.discard_reopen_prompt".to_string(), "変更を破棄して再読み込みしますか？".to_string());
        m.insert("dialog.reopen_file".to_string(), "再読み込み".to_string());
        m.insert("dialog.open_file".to_string(), "ファイルを開く…".to_string());
        m.insert("dialog.new_from_template".to_string(), "テンプレートから新規作成".to_string());
        m.insert("dialog.save_as".to_string(), "名前を付けて保存…".to_string());
        m.insert("dialog.go_to_line".to_string(), "行移動".to_string());
        m.insert("dialog.about".to_string(), "バージョン情報".to_string());
        m.insert("dialog.show_hidden".to_string(), "隠しファイルを表示".to_string());
        m.insert("dialog.detect_encoding".to_string(), "エンコードを自動判別".to_string());
        m.insert("dialog.overwrite_prompt".to_string(), "ファイルが既に存在します。上書きしますか？".to_string());
        m.insert("dialog.unsaved_changes_title".to_string(), "保存されていない変更".to_string());
        m.insert("dialog.unsaved_changes".to_string(), "\"{filename}\" は変更されています。保存しますか？".to_string());

        m.insert("dialog.file_browser.name".to_string(), "名前".to_string());
        m.insert("dialog.file_browser.size".to_string(), "サイズ".to_string());
        m.insert("dialog.file_browser.modified".to_string(), "更新日時".to_string());
        m.insert("dialog.file_browser.filename".to_string(), "ファイル名".to_string());

        m.insert("sidebar.properties".to_string(), "ファイル情報".to_string());
        m.insert("sidebar.prop_file".to_string(), "ファイル".to_string());
        m.insert("sidebar.prop_size".to_string(), "サイズ".to_string());
        m.insert("sidebar.prop_lines".to_string(), "行数".to_string());
        m.insert("sidebar.prop_chars".to_string(), "文字数".to_string());
        m.insert("sidebar.prop_encoding".to_string(), "エンコーディング".to_string());
        m.insert("sidebar.prop_line_ending".to_string(), "改行コード".to_string());
        m.insert("sidebar.no_file".to_string(), "（ファイルなし）".to_string());
        m.insert("sidebar.no_files".to_string(), "（ファイルなし）".to_string());
        m.insert("sidebar.no_headings".to_string(), "（見出しがありません）".to_string());
        m.insert("sidebar.new_file".to_string(), "新規ファイル".to_string());
        m.insert("sidebar.new_folder".to_string(), "新規フォルダ".to_string());
        m.insert("sidebar.rename".to_string(), "名前を変更".to_string());
        m.insert("sidebar.delete".to_string(), "削除".to_string());
        m.insert("sidebar.refresh".to_string(), "更新".to_string());
        m.insert("sidebar.delete_confirm".to_string(), "本当に削除しますか？".to_string());
        m.insert("sidebar.show_hidden".to_string(), "隠しファイルを表示".to_string());
        m.insert("sidebar.hide_hidden".to_string(), "隠しファイルを非表示".to_string());
        m.insert("dialog.file_name".to_string(), "ファイル名:".to_string());
        m.insert("dialog.folder_name".to_string(), "フォルダ名:".to_string());
        m.insert("dialog.new_name".to_string(), "新しい名前:".to_string());
        m.insert("menu.view.refresh_files".to_string(), "ファイルツリーを更新".to_string());

        m.insert("menu.view.syntax_plain".to_string(), "標準テキスト".to_string());

        m.insert("menu.app.preferences".to_string(), "設定…".to_string());
        m.insert("menu.view.zoom_in".to_string(), "拡大".to_string());
        m.insert("menu.view.zoom_out".to_string(), "縮小".to_string());
        m.insert("menu.view.reset_zoom".to_string(), "実際のサイズ".to_string());

        m.insert("dialog.settings.title".to_string(), "設定".to_string());
        m.insert("dialog.settings.theme".to_string(), "テーマ".to_string());
        m.insert("dialog.settings.language".to_string(), "言語".to_string());
        m.insert("dialog.settings.language_auto".to_string(), "自動（システム設定）".to_string());
        m.insert("dialog.settings.font_family".to_string(), "エディタフォント".to_string());
        m.insert("dialog.settings.font_system_default".to_string(), "システム既定".to_string());
        m.insert("dialog.settings.font_size".to_string(), "フォントサイズ".to_string());
        m.insert("dialog.settings.line_height".to_string(), "行の高さ".to_string());
        m.insert("dialog.settings.ui_font_size".to_string(), "UIフォントサイズ".to_string());
        m.insert("dialog.settings.tab_size".to_string(), "タブ幅".to_string());
        m.insert("dialog.settings.sidebar_position".to_string(), "サイドバーの位置".to_string());
        m.insert("dialog.settings.sidebar_left".to_string(), "左側".to_string());
        m.insert("dialog.settings.sidebar_right".to_string(), "右側".to_string());
        m.insert("dialog.settings.include_prerelease".to_string(), "プレビュー版（テスト版）の更新を受け取る".to_string());
        m.insert("dialog.settings.reset_defaults".to_string(), "初期値に戻す".to_string());
        m.insert("dialog.settings.backup_section".to_string(), "バックアップと復元".to_string());
        m.insert("dialog.settings.export_config_only".to_string(), "設定のみエクスポート".to_string());
        m.insert("dialog.settings.export_all".to_string(), "設定とプラグインをエクスポート".to_string());
        m.insert("dialog.settings.import_backup".to_string(), "設定 / バックアップをインポート…".to_string());

        m.insert("menu.file.export_config".to_string(), "設定のエクスポート…".to_string());
        m.insert("menu.file.import_config".to_string(), "設定のインポート…".to_string());
        m.insert("menu.plugins".to_string(), "プラグイン".to_string());
        m.insert("menu.plugins.manage".to_string(), "プラグインの管理…".to_string());
        m.insert("menu.plugins.open_folder".to_string(), "プラグインフォルダを開く".to_string());
        m.insert("menu.plugins.no_plugins".to_string(), "インストール済みのプラグインはありません".to_string());

        m.insert("dialog.backup.export_title".to_string(), "エクスポート完了".to_string());
        m.insert("dialog.backup.export_success".to_string(), "{count} 個のファイルを正常にエクスポートしました:\n{path}".to_string());
        m.insert("dialog.backup.import_title".to_string(), "インポート完了".to_string());
        m.insert("dialog.backup.import_success".to_string(), "設定およびプラグイン（{count} ファイル）を正常に復元しました。".to_string());
        m.insert("dialog.backup.error_title".to_string(), "バックアップ処理失敗".to_string());

        m.insert("dialog.plugin.title".to_string(), "プラグイン管理".to_string());
        m.insert("dialog.plugin.tab_installed".to_string(), "インストール済み".to_string());
        m.insert("dialog.plugin.tab_registry".to_string(), "オンライン (カタログ)".to_string());
        m.insert("dialog.plugin.install".to_string(), "プラグインをインストール…".to_string());
        m.insert("dialog.plugin.install_local".to_string(), "ファイルからインストール…".to_string());
        m.insert("dialog.plugin.install_online".to_string(), "インストール".to_string());
        m.insert("dialog.plugin.loading".to_string(), "zee-plugins からカタログを取得中…".to_string());
        m.insert("dialog.plugin.registry_empty".to_string(), "カタログにプラグインが見つかりません。".to_string());
        m.insert("dialog.plugin.open_dir".to_string(), "プラグインフォルダを開く".to_string());
        m.insert("dialog.plugin.uninstall".to_string(), "削除".to_string());
        m.insert("dialog.plugin.installed".to_string(), "インストール済みプラグイン".to_string());
        m.insert("dialog.plugin.empty".to_string(), "プラグインはまだインストールされていません。".to_string());
        m.insert("dialog.plugin.active".to_string(), "有効".to_string());
        m.insert("dialog.plugin.repositories".to_string(), "リポジトリ:".to_string());
        m.insert("dialog.plugin.add_repo_btn".to_string(), "+ 追加".to_string());
        m.insert("dialog.plugin.repo_placeholder".to_string(), "GitHub URL (例: user/plugins) または index.json URL".to_string());
        m.insert("dialog.plugin.repo_add".to_string(), "追加".to_string());
        m.insert("dialog.plugin.repo_cancel".to_string(), "キャンセル".to_string());
        m.insert("dialog.plugin.search_placeholder".to_string(), "プラグイン名、説明、IDで検索…".to_string());
        m.insert("dialog.plugin.no_search_results".to_string(), "検索条件に一致するプラグインは見つかりませんでした。".to_string());
        m.insert("dialog.plugin.clear_search".to_string(), "検索をクリア".to_string());

        m.insert("about.version".to_string(), "バージョン".to_string());
        m.insert("about.license".to_string(), "ライセンス".to_string());

        m.insert("gdrive.title".to_string(), "Google ドライブ".to_string());
        m.insert("gdrive.connect".to_string(), "Google ドライブに接続".to_string());
        m.insert("gdrive.connecting".to_string(), "ブラウザで認証待機中...".to_string());
        m.insert("gdrive.connected".to_string(), "接続中".to_string());
        m.insert("gdrive.sign_out".to_string(), "ログアウト".to_string());
        m.insert("gdrive.syncing".to_string(), "Google ドライブへ同期中...".to_string());
        m.insert("gdrive.synced".to_string(), "Google ドライブ同期完了".to_string());
        m.insert("gdrive.sync_error".to_string(), "Google ドライブ同期失敗".to_string());
        m.insert("gdrive.open".to_string(), "開く".to_string());
        m.insert("gdrive.cancel".to_string(), "キャンセル".to_string());
        m.insert("gdrive.search_placeholder".to_string(), "Google ドライブ内を検索...".to_string());
        m.insert("gdrive.setup_title".to_string(), "Google Drive API 設定 (初回のみ)".to_string());
        m.insert("gdrive.setup_desc".to_string(), "zeeからGoogleドライブへ安全に直接アクセスするために、Google Cloud Consoleでクライアントキーを作成してください:".to_string());
        m.insert("gdrive.step1".to_string(), "1. Google Cloud Console を開き、わかりやすい名前（例: zee-GoogleDrive）で新規プロジェクトを作成します。".to_string());
        m.insert("gdrive.step2".to_string(), "2. 「APIとサービス」→「ライブラリ」から「Google Drive API」を検索し、「有効にする」をクリックします。".to_string());
        m.insert("gdrive.step3".to_string(), "3. 「認証情報」→「認証情報を作成」→「OAuth クライアント ID」を選び、種類を「デスクトップ アプリ」にして作成します。".to_string());
        m.insert("gdrive.step4".to_string(), "4. 発行された クライアント ID（およびシークレット）をコピーし、下に入力して保存します。".to_string());
        m.insert("gdrive.open_gcp_btn".to_string(), "Google Cloud Console を開く ↗".to_string());
        m.insert("gdrive.client_id_label".to_string(), "OAuth クライアント ID:".to_string());
        m.insert("gdrive.client_secret_label".to_string(), "クライアント シークレット:".to_string());
        m.insert("gdrive.save_and_connect".to_string(), "保存して接続する".to_string());
        m.insert("gdrive.configure_api".to_string(), "API設定・キー変更…".to_string());
        m.insert("gdrive.credentials_missing".to_string(), "認証を開始するにはクライアント ID を入力してください。".to_string());
        m.insert("gdrive.cancel_wait".to_string(), "認証を中止".to_string());
        m
    }

    fn get_zh_cn_defaults() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("menu.file".to_string(), "文件".to_string());
        m.insert("menu.file.new_tab".to_string(), "新建标签页".to_string());
        m.insert("menu.file.new_window".to_string(), "新建窗口".to_string());
        m.insert("menu.file.new".to_string(), "新建".to_string());
        m.insert("menu.file.open".to_string(), "打开…".to_string());
        m.insert("menu.file.save".to_string(), "保存".to_string());
        m.insert("menu.file.save_as".to_string(), "另存为…".to_string());
        m.insert("menu.file.close".to_string(), "关闭".to_string());
        m.insert("menu.file.exit".to_string(), "退出".to_string());

        m.insert("menu.edit".to_string(), "编辑".to_string());
        m.insert("menu.edit.undo".to_string(), "撤销".to_string());
        m.insert("menu.edit.redo".to_string(), "重做".to_string());
        m.insert("menu.edit.cut".to_string(), "剪切".to_string());
        m.insert("menu.edit.copy".to_string(), "复制".to_string());
        m.insert("menu.edit.paste".to_string(), "粘贴".to_string());
        m.insert("menu.edit.find".to_string(), "查找…".to_string());
        m.insert("menu.edit.replace".to_string(), "替换…".to_string());
        m.insert("menu.edit.select_all".to_string(), "全选".to_string());

        m.insert("menu.view".to_string(), "视图".to_string());
        m.insert("menu.view.go_to_line".to_string(), "转到行…".to_string());
        m.insert("menu.view.line_numbers".to_string(), "行号".to_string());
        m.insert("menu.view.word_wrap".to_string(), "自动换行".to_string());
        m.insert("menu.view.vi_mode".to_string(), "Vi模式".to_string());
        m.insert("menu.view.encoding".to_string(), "编码".to_string());
        m.insert("menu.view.line_ending".to_string(), "换行符".to_string());
        m.insert("menu.view.theme".to_string(), "主题".to_string());
        m.insert("menu.view.syntax".to_string(), "语法高亮".to_string());

        m.insert("menu.help".to_string(), "帮助".to_string());
        m.insert("menu.help.about".to_string(), "关于".to_string());

        m.insert("panel.find".to_string(), "查找:".to_string());
        m.insert("panel.replace".to_string(), "替换:".to_string());
        m.insert("panel.prev".to_string(), "上一个".to_string());
        m.insert("panel.next".to_string(), "下一个".to_string());
        m.insert("panel.replace_one".to_string(), "替换".to_string());
        m.insert("panel.replace_all".to_string(), "全部替换".to_string());
        m.insert("panel.close".to_string(), "关闭".to_string());
        m.insert("panel.match_case".to_string(), "区分大小写".to_string());
        m.insert("panel.whole_word".to_string(), "全字匹配".to_string());
        m.insert("panel.use_regex".to_string(), "正则表达式".to_string());

        m.insert("status.no_name".to_string(), "[无标题]".to_string());
        m.insert("status.no_matches".to_string(), "未找到匹配项".to_string());
        m.insert("status.cursor".to_string(), "第 {line} 行, 第 {col} 列".to_string());
        m.insert("status.selection".to_string(), "已选择 {n} 个字符".to_string());

        m.insert("error".to_string(), "错误".to_string());
        m.insert("dialog.ok".to_string(), "确定".to_string());
        m.insert("dialog.cancel".to_string(), "取消".to_string());
        m.insert("dialog.yes".to_string(), "是".to_string());
        m.insert("dialog.no".to_string(), "否".to_string());
        m.insert("dialog.save".to_string(), "保存".to_string());
        m.insert("dialog.dont_save".to_string(), "不保存".to_string());
        m.insert("dialog.open_file".to_string(), "打开文件…".to_string());
        m.insert("dialog.save_as".to_string(), "另存为…".to_string());
        m.insert("dialog.go_to_line".to_string(), "转到行".to_string());
        m.insert("dialog.about".to_string(), "关于".to_string());
        m.insert("dialog.show_hidden".to_string(), "显示隐藏文件".to_string());
        m.insert("dialog.detect_encoding".to_string(), "自动检测编码".to_string());
        m.insert("dialog.file_browser.name".to_string(), "名称".to_string());
        m.insert("dialog.file_browser.size".to_string(), "大小".to_string());
        m.insert("dialog.file_browser.modified".to_string(), "修改时间".to_string());
        m.insert("dialog.file_browser.filename".to_string(), "文件名".to_string());
        m.insert("dialog.settings.title".to_string(), "偏好设置".to_string());
        m
    }

    fn get_zh_tw_defaults() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("menu.file".to_string(), "檔案".to_string());
        m.insert("menu.file.new_tab".to_string(), "新增分頁".to_string());
        m.insert("menu.file.new_window".to_string(), "開新視窗".to_string());
        m.insert("menu.file.new".to_string(), "新增".to_string());
        m.insert("menu.file.open".to_string(), "開啟…".to_string());
        m.insert("menu.file.save".to_string(), "儲存".to_string());
        m.insert("menu.file.save_as".to_string(), "另存為…".to_string());
        m.insert("menu.file.close".to_string(), "關閉".to_string());
        m.insert("menu.file.exit".to_string(), "結束".to_string());

        m.insert("menu.edit".to_string(), "編輯".to_string());
        m.insert("menu.edit.undo".to_string(), "復原".to_string());
        m.insert("menu.edit.redo".to_string(), "重做".to_string());
        m.insert("menu.edit.cut".to_string(), "剪下".to_string());
        m.insert("menu.edit.copy".to_string(), "複製".to_string());
        m.insert("menu.edit.paste".to_string(), "貼上".to_string());
        m.insert("menu.edit.find".to_string(), "尋找…".to_string());
        m.insert("menu.edit.replace".to_string(), "取代…".to_string());
        m.insert("menu.edit.select_all".to_string(), "全選".to_string());

        m.insert("menu.view".to_string(), "檢視".to_string());
        m.insert("menu.view.go_to_line".to_string(), "移至行…".to_string());
        m.insert("menu.view.line_numbers".to_string(), "行號".to_string());
        m.insert("menu.view.word_wrap".to_string(), "自動換行".to_string());
        m.insert("menu.view.vi_mode".to_string(), "Vi模式".to_string());
        m.insert("menu.view.encoding".to_string(), "編碼".to_string());
        m.insert("menu.view.line_ending".to_string(), "換行字元".to_string());
        m.insert("menu.view.theme".to_string(), "主題".to_string());
        m.insert("menu.view.syntax".to_string(), "語法標示".to_string());

        m.insert("menu.help".to_string(), "說明".to_string());
        m.insert("menu.help.about".to_string(), "關於".to_string());

        m.insert("panel.find".to_string(), "尋找:".to_string());
        m.insert("panel.replace".to_string(), "取代:".to_string());
        m.insert("panel.prev".to_string(), "上一個".to_string());
        m.insert("panel.next".to_string(), "下一個".to_string());
        m.insert("panel.replace_one".to_string(), "取代".to_string());
        m.insert("panel.replace_all".to_string(), "全部取代".to_string());
        m.insert("panel.close".to_string(), "關閉".to_string());

        m.insert("status.no_name".to_string(), "[未命名]".to_string());
        m.insert("status.cursor".to_string(), "第 {line} 行, 第 {col} 欄".to_string());
        m.insert("dialog.ok".to_string(), "確定".to_string());
        m.insert("dialog.cancel".to_string(), "取消".to_string());
        m.insert("dialog.save".to_string(), "儲存".to_string());
        m.insert("dialog.dont_save".to_string(), "不儲存".to_string());
        m.insert("dialog.open_file".to_string(), "開啟檔案…".to_string());
        m.insert("dialog.save_as".to_string(), "另存為…".to_string());
        m.insert("dialog.go_to_line".to_string(), "移至行".to_string());
        m.insert("dialog.file_browser.filename".to_string(), "檔案名稱".to_string());
        m.insert("dialog.settings.title".to_string(), "偏好設定".to_string());
        m
    }

    fn get_ko_defaults() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("menu.file".to_string(), "파일".to_string());
        m.insert("menu.file.new_tab".to_string(), "새 탭".to_string());
        m.insert("menu.file.new_window".to_string(), "새 창".to_string());
        m.insert("menu.file.new".to_string(), "새로 만들기".to_string());
        m.insert("menu.file.open".to_string(), "열기…".to_string());
        m.insert("menu.file.save".to_string(), "저장".to_string());
        m.insert("menu.file.save_as".to_string(), "다른 이름으로 저장…".to_string());
        m.insert("menu.file.close".to_string(), "닫기".to_string());
        m.insert("menu.file.exit".to_string(), "종료".to_string());

        m.insert("menu.edit".to_string(), "편집".to_string());
        m.insert("menu.edit.undo".to_string(), "실행 취소".to_string());
        m.insert("menu.edit.redo".to_string(), "다시 실행".to_string());
        m.insert("menu.edit.cut".to_string(), "잘라내기".to_string());
        m.insert("menu.edit.copy".to_string(), "복사".to_string());
        m.insert("menu.edit.paste".to_string(), "붙여넣기".to_string());
        m.insert("menu.edit.find".to_string(), "찾기…".to_string());
        m.insert("menu.edit.replace".to_string(), "바꾸기…".to_string());
        m.insert("menu.edit.select_all".to_string(), "모두 선택".to_string());

        m.insert("menu.view".to_string(), "보기".to_string());
        m.insert("menu.view.go_to_line".to_string(), "줄로 이동…".to_string());
        m.insert("menu.view.line_numbers".to_string(), "줄 번호".to_string());
        m.insert("menu.view.word_wrap".to_string(), "자동 줄 바꿈".to_string());
        m.insert("menu.view.vi_mode".to_string(), "Vi 모드".to_string());
        m.insert("menu.view.encoding".to_string(), "인코딩".to_string());
        m.insert("menu.view.line_ending".to_string(), "줄 바꿈".to_string());
        m.insert("menu.view.theme".to_string(), "테마".to_string());
        m.insert("menu.view.syntax".to_string(), "구문 강조".to_string());

        m.insert("menu.help".to_string(), "도움말".to_string());
        m.insert("menu.help.about".to_string(), "정보".to_string());

        m.insert("panel.find".to_string(), "찾기:".to_string());
        m.insert("panel.replace".to_string(), "바꾸기:".to_string());
        m.insert("panel.prev".to_string(), "이전".to_string());
        m.insert("panel.next".to_string(), "다음".to_string());
        m.insert("panel.replace_one".to_string(), "바꾸기".to_string());
        m.insert("panel.replace_all".to_string(), "모두 바꾸기".to_string());
        m.insert("panel.close".to_string(), "닫기".to_string());

        m.insert("status.no_name".to_string(), "[제목 없음]".to_string());
        m.insert("status.cursor".to_string(), "{line}행, {col}열".to_string());
        m.insert("dialog.ok".to_string(), "확인".to_string());
        m.insert("dialog.cancel".to_string(), "취소".to_string());
        m.insert("dialog.save".to_string(), "저장".to_string());
        m.insert("dialog.dont_save".to_string(), "저장 안 함".to_string());
        m.insert("dialog.open_file".to_string(), "파일 열기…".to_string());
        m.insert("dialog.save_as".to_string(), "다른 이름으로 저장…".to_string());
        m.insert("dialog.go_to_line".to_string(), "줄로 이동".to_string());
        m.insert("dialog.file_browser.filename".to_string(), "파일 이름".to_string());
        m.insert("dialog.settings.title".to_string(), "환경설정".to_string());
        m
    }

    fn get_es_defaults() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("menu.file".to_string(), "Archivo".to_string());
        m.insert("menu.file.new_tab".to_string(), "Nueva pestaña".to_string());
        m.insert("menu.file.new_window".to_string(), "Nueva ventana".to_string());
        m.insert("menu.file.new".to_string(), "Nuevo".to_string());
        m.insert("menu.file.open".to_string(), "Abrir…".to_string());
        m.insert("menu.file.save".to_string(), "Guardar".to_string());
        m.insert("menu.file.save_as".to_string(), "Guardar como…".to_string());
        m.insert("menu.file.close".to_string(), "Cerrar".to_string());
        m.insert("menu.file.exit".to_string(), "Salir".to_string());

        m.insert("menu.edit".to_string(), "Editar".to_string());
        m.insert("menu.edit.undo".to_string(), "Deshacer".to_string());
        m.insert("menu.edit.redo".to_string(), "Rehacer".to_string());
        m.insert("menu.edit.cut".to_string(), "Cortar".to_string());
        m.insert("menu.edit.copy".to_string(), "Copiar".to_string());
        m.insert("menu.edit.paste".to_string(), "Pegar".to_string());
        m.insert("menu.edit.find".to_string(), "Buscar…".to_string());
        m.insert("menu.edit.replace".to_string(), "Reemplazar…".to_string());
        m.insert("menu.edit.select_all".to_string(), "Seleccionar todo".to_string());

        m.insert("menu.view".to_string(), "Ver".to_string());
        m.insert("menu.view.go_to_line".to_string(), "Ir a la línea…".to_string());
        m.insert("menu.view.line_numbers".to_string(), "Números de línea".to_string());
        m.insert("menu.view.word_wrap".to_string(), "Ajuste de línea".to_string());
        m.insert("menu.view.vi_mode".to_string(), "Modo Vi".to_string());
        m.insert("menu.view.encoding".to_string(), "Codificación".to_string());
        m.insert("menu.view.line_ending".to_string(), "Fin de línea".to_string());
        m.insert("menu.view.theme".to_string(), "Tema".to_string());
        m.insert("menu.view.syntax".to_string(), "Sintaxis".to_string());

        m.insert("menu.help".to_string(), "Ayuda".to_string());
        m.insert("menu.help.about".to_string(), "Acerca de".to_string());

        m.insert("panel.find".to_string(), "Buscar:".to_string());
        m.insert("panel.replace".to_string(), "Reemplazar:".to_string());
        m.insert("panel.prev".to_string(), "Anterior".to_string());
        m.insert("panel.next".to_string(), "Siguiente".to_string());
        m.insert("panel.replace_one".to_string(), "Reemplazar".to_string());
        m.insert("panel.replace_all".to_string(), "Reemplazar todo".to_string());
        m.insert("panel.close".to_string(), "Cerrar".to_string());

        m.insert("status.no_name".to_string(), "[Sin nombre]".to_string());
        m.insert("status.cursor".to_string(), "Lín {line}, Col {col}".to_string());
        m.insert("dialog.ok".to_string(), "Aceptar".to_string());
        m.insert("dialog.cancel".to_string(), "Cancelar".to_string());
        m.insert("dialog.save".to_string(), "Guardar".to_string());
        m.insert("dialog.dont_save".to_string(), "No guardar".to_string());
        m.insert("dialog.open_file".to_string(), "Abrir archivo…".to_string());
        m.insert("dialog.save_as".to_string(), "Guardar como…".to_string());
        m.insert("dialog.file_browser.filename".to_string(), "Nombre de archivo".to_string());
        m.insert("dialog.settings.title".to_string(), "Preferencias".to_string());
        m
    }

    fn get_fr_defaults() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("menu.file".to_string(), "Fichier".to_string());
        m.insert("menu.file.new_tab".to_string(), "Nouvel onglet".to_string());
        m.insert("menu.file.new_window".to_string(), "Nouvelle fenêtre".to_string());
        m.insert("menu.file.new".to_string(), "Nouveau".to_string());
        m.insert("menu.file.open".to_string(), "Ouvrir…".to_string());
        m.insert("menu.file.save".to_string(), "Enregistrer".to_string());
        m.insert("menu.file.save_as".to_string(), "Enregistrer sous…".to_string());
        m.insert("menu.file.close".to_string(), "Fermer".to_string());
        m.insert("menu.file.exit".to_string(), "Quitter".to_string());

        m.insert("menu.edit".to_string(), "Édition".to_string());
        m.insert("menu.edit.undo".to_string(), "Annuler".to_string());
        m.insert("menu.edit.redo".to_string(), "Rétablir".to_string());
        m.insert("menu.edit.cut".to_string(), "Couper".to_string());
        m.insert("menu.edit.copy".to_string(), "Copier".to_string());
        m.insert("menu.edit.paste".to_string(), "Coller".to_string());
        m.insert("menu.edit.find".to_string(), "Rechercher…".to_string());
        m.insert("menu.edit.replace".to_string(), "Remplacer…".to_string());
        m.insert("menu.edit.select_all".to_string(), "Tout sélectionner".to_string());

        m.insert("menu.view".to_string(), "Affichage".to_string());
        m.insert("menu.view.go_to_line".to_string(), "Aller à la ligne…".to_string());
        m.insert("menu.view.line_numbers".to_string(), "Numéros de ligne".to_string());
        m.insert("menu.view.word_wrap".to_string(), "Retour à la ligne".to_string());
        m.insert("menu.view.vi_mode".to_string(), "Mode Vi".to_string());
        m.insert("menu.view.encoding".to_string(), "Encodage".to_string());
        m.insert("menu.view.line_ending".to_string(), "Fin de ligne".to_string());
        m.insert("menu.view.theme".to_string(), "Thème".to_string());
        m.insert("menu.view.syntax".to_string(), "Syntaxe".to_string());

        m.insert("menu.help".to_string(), "Aide".to_string());
        m.insert("menu.help.about".to_string(), "À propos".to_string());

        m.insert("panel.find".to_string(), "Rechercher:".to_string());
        m.insert("panel.replace".to_string(), "Remplacer:".to_string());
        m.insert("panel.prev".to_string(), "Précédent".to_string());
        m.insert("panel.next".to_string(), "Suivant".to_string());
        m.insert("panel.replace_one".to_string(), "Remplacer".to_string());
        m.insert("panel.replace_all".to_string(), "Tout remplacer".to_string());
        m.insert("panel.close".to_string(), "Fermer".to_string());

        m.insert("status.no_name".to_string(), "[Sans titre]".to_string());
        m.insert("status.cursor".to_string(), "Lig {line}, Col {col}".to_string());
        m.insert("dialog.ok".to_string(), "OK".to_string());
        m.insert("dialog.cancel".to_string(), "Annuler".to_string());
        m.insert("dialog.save".to_string(), "Enregistrer".to_string());
        m.insert("dialog.dont_save".to_string(), "Ne pas enregistrer".to_string());
        m.insert("dialog.open_file".to_string(), "Ouvrir un fichier…".to_string());
        m.insert("dialog.save_as".to_string(), "Enregistrer sous…".to_string());
        m.insert("dialog.file_browser.filename".to_string(), "Nom de fichier".to_string());
        m.insert("dialog.settings.title".to_string(), "Préférences".to_string());
        m
    }

    fn get_de_defaults() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("menu.file".to_string(), "Datei".to_string());
        m.insert("menu.file.new_tab".to_string(), "Neuer Tab".to_string());
        m.insert("menu.file.new_window".to_string(), "Neues Fenster".to_string());
        m.insert("menu.file.new".to_string(), "Neu".to_string());
        m.insert("menu.file.open".to_string(), "Öffnen…".to_string());
        m.insert("menu.file.save".to_string(), "Speichern".to_string());
        m.insert("menu.file.save_as".to_string(), "Speichern unter…".to_string());
        m.insert("menu.file.close".to_string(), "Schließen".to_string());
        m.insert("menu.file.exit".to_string(), "Beenden".to_string());

        m.insert("menu.edit".to_string(), "Bearbeiten".to_string());
        m.insert("menu.edit.undo".to_string(), "Rückgängig".to_string());
        m.insert("menu.edit.redo".to_string(), "Wiederholen".to_string());
        m.insert("menu.edit.cut".to_string(), "Ausschneiden".to_string());
        m.insert("menu.edit.copy".to_string(), "Kopieren".to_string());
        m.insert("menu.edit.paste".to_string(), "Einfügen".to_string());
        m.insert("menu.edit.find".to_string(), "Suchen…".to_string());
        m.insert("menu.edit.replace".to_string(), "Ersetzen…".to_string());
        m.insert("menu.edit.select_all".to_string(), "Alles auswählen".to_string());

        m.insert("menu.view".to_string(), "Ansicht".to_string());
        m.insert("menu.view.go_to_line".to_string(), "Gehe zu Zeile…".to_string());
        m.insert("menu.view.line_numbers".to_string(), "Zeilennummern".to_string());
        m.insert("menu.view.word_wrap".to_string(), "Zeilenumbruch".to_string());
        m.insert("menu.view.vi_mode".to_string(), "Vi-Modus".to_string());
        m.insert("menu.view.encoding".to_string(), "Kodierung".to_string());
        m.insert("menu.view.line_ending".to_string(), "Zeilenende".to_string());
        m.insert("menu.view.theme".to_string(), "Design".to_string());
        m.insert("menu.view.syntax".to_string(), "Syntax".to_string());

        m.insert("menu.help".to_string(), "Hilfe".to_string());
        m.insert("menu.help.about".to_string(), "Über".to_string());

        m.insert("panel.find".to_string(), "Suchen:".to_string());
        m.insert("panel.replace".to_string(), "Ersetzen:".to_string());
        m.insert("panel.prev".to_string(), "Vorherige".to_string());
        m.insert("panel.next".to_string(), "Nächste".to_string());
        m.insert("panel.replace_one".to_string(), "Ersetzen".to_string());
        m.insert("panel.replace_all".to_string(), "Alle ersetzen".to_string());
        m.insert("panel.close".to_string(), "Schließen".to_string());

        m.insert("status.no_name".to_string(), "[Unbenannt]".to_string());
        m.insert("status.cursor".to_string(), "Zl {line}, Sp {col}".to_string());
        m.insert("dialog.ok".to_string(), "OK".to_string());
        m.insert("dialog.cancel".to_string(), "Abbrechen".to_string());
        m.insert("dialog.save".to_string(), "Speichern".to_string());
        m.insert("dialog.dont_save".to_string(), "Nicht speichern".to_string());
        m.insert("dialog.open_file".to_string(), "Datei öffnen…".to_string());
        m.insert("dialog.save_as".to_string(), "Speichern unter…".to_string());
        m.insert("dialog.file_browser.filename".to_string(), "Dateiname".to_string());
        m.insert("dialog.settings.title".to_string(), "Einstellungen".to_string());
        m
    }

    fn get_it_defaults() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("menu.file".to_string(), "File".to_string());
        m.insert("menu.file.new_tab".to_string(), "Nuova scheda".to_string());
        m.insert("menu.file.new_window".to_string(), "Nuova finestra".to_string());
        m.insert("menu.file.new".to_string(), "Nuovo".to_string());
        m.insert("menu.file.open".to_string(), "Apri…".to_string());
        m.insert("menu.file.save".to_string(), "Salva".to_string());
        m.insert("menu.file.save_as".to_string(), "Salva con nome…".to_string());
        m.insert("menu.file.close".to_string(), "Chiudi".to_string());
        m.insert("menu.file.exit".to_string(), "Esci".to_string());

        m.insert("menu.edit".to_string(), "Modifica".to_string());
        m.insert("menu.edit.undo".to_string(), "Annulla".to_string());
        m.insert("menu.edit.redo".to_string(), "Ripeti".to_string());
        m.insert("menu.edit.cut".to_string(), "Taglia".to_string());
        m.insert("menu.edit.copy".to_string(), "Copia".to_string());
        m.insert("menu.edit.paste".to_string(), "Incolla".to_string());
        m.insert("menu.edit.find".to_string(), "Trova…".to_string());
        m.insert("menu.edit.replace".to_string(), "Sostituisci…".to_string());
        m.insert("menu.edit.select_all".to_string(), "Seleziona tutto".to_string());

        m.insert("menu.view".to_string(), "Visualizza".to_string());
        m.insert("menu.view.go_to_line".to_string(), "Vai alla riga…".to_string());
        m.insert("menu.view.line_numbers".to_string(), "Numeri di riga".to_string());
        m.insert("menu.view.word_wrap".to_string(), "A capo automatico".to_string());
        m.insert("menu.view.vi_mode".to_string(), "Modalità Vi".to_string());
        m.insert("menu.view.encoding".to_string(), "Codifica".to_string());
        m.insert("menu.view.line_ending".to_string(), "Fine riga".to_string());
        m.insert("menu.view.theme".to_string(), "Tema".to_string());
        m.insert("menu.view.syntax".to_string(), "Sintassi".to_string());

        m.insert("menu.help".to_string(), "Aiuto".to_string());
        m.insert("menu.help.about".to_string(), "Informazioni".to_string());

        m.insert("status.no_name".to_string(), "[Senza titolo]".to_string());
        m.insert("dialog.ok".to_string(), "OK".to_string());
        m.insert("dialog.cancel".to_string(), "Annulla".to_string());
        m.insert("dialog.save".to_string(), "Salva".to_string());
        m.insert("dialog.dont_save".to_string(), "Non salvare".to_string());
        m.insert("dialog.open_file".to_string(), "Apri file…".to_string());
        m.insert("dialog.save_as".to_string(), "Salva con nome…".to_string());
        m.insert("dialog.file_browser.filename".to_string(), "Nome file".to_string());
        m.insert("dialog.settings.title".to_string(), "Preferenze".to_string());
        m
    }

    fn get_pt_defaults() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("menu.file".to_string(), "Arquivo".to_string());
        m.insert("menu.file.new_tab".to_string(), "Nova aba".to_string());
        m.insert("menu.file.new_window".to_string(), "Nova janela".to_string());
        m.insert("menu.file.new".to_string(), "Novo".to_string());
        m.insert("menu.file.open".to_string(), "Abrir…".to_string());
        m.insert("menu.file.save".to_string(), "Salvar".to_string());
        m.insert("menu.file.save_as".to_string(), "Salvar como…".to_string());
        m.insert("menu.file.close".to_string(), "Fechar".to_string());
        m.insert("menu.file.exit".to_string(), "Sair".to_string());

        m.insert("menu.edit".to_string(), "Editar".to_string());
        m.insert("menu.edit.undo".to_string(), "Desfazer".to_string());
        m.insert("menu.edit.redo".to_string(), "Refazer".to_string());
        m.insert("menu.edit.cut".to_string(), "Recortar".to_string());
        m.insert("menu.edit.copy".to_string(), "Copiar".to_string());
        m.insert("menu.edit.paste".to_string(), "Colar".to_string());
        m.insert("menu.edit.find".to_string(), "Localizar…".to_string());
        m.insert("menu.edit.replace".to_string(), "Substituir…".to_string());
        m.insert("menu.edit.select_all".to_string(), "Selecionar tudo".to_string());

        m.insert("menu.view".to_string(), "Exibir".to_string());
        m.insert("menu.view.go_to_line".to_string(), "Ir para linha…".to_string());
        m.insert("menu.view.line_numbers".to_string(), "Números de linha".to_string());
        m.insert("menu.view.word_wrap".to_string(), "Quebra de linha".to_string());
        m.insert("menu.view.vi_mode".to_string(), "Modo Vi".to_string());
        m.insert("menu.view.encoding".to_string(), "Codificação".to_string());
        m.insert("menu.view.line_ending".to_string(), "Fim de linha".to_string());
        m.insert("menu.view.theme".to_string(), "Tema".to_string());
        m.insert("menu.view.syntax".to_string(), "Sintaxe".to_string());

        m.insert("menu.help".to_string(), "Ajuda".to_string());
        m.insert("menu.help.about".to_string(), "Sobre".to_string());

        m.insert("status.no_name".to_string(), "[Sem título]".to_string());
        m.insert("dialog.ok".to_string(), "OK".to_string());
        m.insert("dialog.cancel".to_string(), "Cancelar".to_string());
        m.insert("dialog.save".to_string(), "Salvar".to_string());
        m.insert("dialog.dont_save".to_string(), "Não salvar".to_string());
        m.insert("dialog.open_file".to_string(), "Abrir arquivo…".to_string());
        m.insert("dialog.save_as".to_string(), "Salvar como…".to_string());
        m.insert("dialog.file_browser.filename".to_string(), "Nome do arquivo".to_string());
        m.insert("dialog.settings.title".to_string(), "Preferências".to_string());
        m
    }

    fn get_ru_defaults() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("menu.file".to_string(), "Файл".to_string());
        m.insert("menu.file.new_tab".to_string(), "Новая вкладка".to_string());
        m.insert("menu.file.new_window".to_string(), "Новое окно".to_string());
        m.insert("menu.file.new".to_string(), "Создать".to_string());
        m.insert("menu.file.open".to_string(), "Открыть…".to_string());
        m.insert("menu.file.save".to_string(), "Сохранить".to_string());
        m.insert("menu.file.save_as".to_string(), "Сохранить как…".to_string());
        m.insert("menu.file.close".to_string(), "Закрыть".to_string());
        m.insert("menu.file.exit".to_string(), "Выход".to_string());

        m.insert("menu.edit".to_string(), "Правка".to_string());
        m.insert("menu.edit.undo".to_string(), "Отменить".to_string());
        m.insert("menu.edit.redo".to_string(), "Повторить".to_string());
        m.insert("menu.edit.cut".to_string(), "Вырезать".to_string());
        m.insert("menu.edit.copy".to_string(), "Копировать".to_string());
        m.insert("menu.edit.paste".to_string(), "Вставить".to_string());
        m.insert("menu.edit.find".to_string(), "Найти…".to_string());
        m.insert("menu.edit.replace".to_string(), "Заменить…".to_string());
        m.insert("menu.edit.select_all".to_string(), "Выделить все".to_string());

        m.insert("menu.view".to_string(), "Вид".to_string());
        m.insert("menu.view.go_to_line".to_string(), "Перейти к строке…".to_string());
        m.insert("menu.view.line_numbers".to_string(), "Номера строк".to_string());
        m.insert("menu.view.word_wrap".to_string(), "Перенос строк".to_string());
        m.insert("menu.view.vi_mode".to_string(), "Режим Vi".to_string());
        m.insert("menu.view.encoding".to_string(), "Кодировка".to_string());
        m.insert("menu.view.line_ending".to_string(), "Конец строки".to_string());
        m.insert("menu.view.theme".to_string(), "Тема".to_string());
        m.insert("menu.view.syntax".to_string(), "Синтаксис".to_string());

        m.insert("menu.help".to_string(), "Справка".to_string());
        m.insert("menu.help.about".to_string(), "О программе".to_string());

        m.insert("panel.find".to_string(), "Найти:".to_string());
        m.insert("panel.replace".to_string(), "Заменить:".to_string());
        m.insert("panel.prev".to_string(), "Назад".to_string());
        m.insert("panel.next".to_string(), "Далее".to_string());
        m.insert("panel.replace_one".to_string(), "Заменить".to_string());
        m.insert("panel.replace_all".to_string(), "Заменить все".to_string());
        m.insert("panel.close".to_string(), "Закрыть".to_string());

        m.insert("status.no_name".to_string(), "[Без имени]".to_string());
        m.insert("status.cursor".to_string(), "Стр {line}, Кол {col}".to_string());
        m.insert("dialog.ok".to_string(), "OK".to_string());
        m.insert("dialog.cancel".to_string(), "Отмена".to_string());
        m.insert("dialog.save".to_string(), "Сохранить".to_string());
        m.insert("dialog.dont_save".to_string(), "Не сохранять".to_string());
        m.insert("dialog.open_file".to_string(), "Открыть файл…".to_string());
        m.insert("dialog.save_as".to_string(), "Сохранить как…".to_string());
        m.insert("dialog.file_browser.filename".to_string(), "Имя файла".to_string());
        m.insert("dialog.settings.title".to_string(), "Настройки".to_string());
        m
    }

    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        self.strings.get(key).map(|s| s.as_str()).unwrap_or(key)
    }

    fn locale_file_path(lang: &str) -> Option<PathBuf> {
        Config::config_dir().map(|dir| dir.join("locales").join(format!("{}.toml", lang)))
    }

    fn parse_locale_toml(content: &str) -> anyhow::Result<HashMap<String, String>> {
        use toml_span::parse;
        let value = parse(content).map_err(|e| anyhow::anyhow!("failed to parse locale TOML: {:?}", e))?;
        let mut map = HashMap::new();
        if let Some(table) = value.as_table() {
            for (k, v) in table {
                Self::flatten_toml_value(&k.to_string(), v, &mut map);
            }
        }
        Ok(map)
    }

    fn flatten_toml_value(prefix: &str, value: &toml_span::Value, map: &mut HashMap<String, String>) {
        if let Some(s) = value.as_str() {
            map.insert(prefix.to_string(), s.to_string());
        } else if let Some(table) = value.as_table() {
            for (k, v) in table {
                let new_prefix = format!("{}.{}", prefix, k);
                Self::flatten_toml_value(&new_prefix, v, map);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_lang() {
        assert_eq!(I18n::normalize_lang("ja"), "ja");
        assert_eq!(I18n::normalize_lang("ja_JP.UTF-8"), "ja");
        assert_eq!(I18n::normalize_lang("ja-JP"), "ja");
        assert_eq!(I18n::normalize_lang("en_US.UTF-8"), "en");
        assert_eq!(I18n::normalize_lang("en"), "en");
        assert_eq!(I18n::normalize_lang("zh_CN"), "zh-CN");
        assert_eq!(I18n::normalize_lang("zh_TW"), "zh-TW");
        assert_eq!(I18n::normalize_lang("ko_KR"), "ko");
        assert_eq!(I18n::normalize_lang("es_ES"), "es");
        assert_eq!(I18n::normalize_lang("fr_FR"), "fr");
        assert_eq!(I18n::normalize_lang("de_DE"), "de");
        assert_eq!(I18n::normalize_lang("it_IT"), "it");
        assert_eq!(I18n::normalize_lang("pt_BR"), "pt");
        assert_eq!(I18n::normalize_lang("ru_RU"), "ru");
        assert_eq!(I18n::normalize_lang("unknown_lang"), "en");
    }

    #[test]
    fn test_resolve_lang() {
        assert_eq!(I18n::resolve_lang("ja"), "ja");
        assert_eq!(I18n::resolve_lang("en"), "en");
        let auto_resolved = I18n::resolve_lang("auto");
        assert!(!auto_resolved.is_empty());
        let empty_resolved = I18n::resolve_lang("");
        assert_eq!(empty_resolved, auto_resolved);
    }

    #[test]
    fn test_detect_system_locale_non_empty() {
        let loc = detect_system_locale();
        assert!(!loc.is_empty());
    }

    #[test]
    fn test_available_languages() {
        assert!(AVAILABLE_LANGUAGES.iter().any(|l| l.id == "auto"));
        assert!(AVAILABLE_LANGUAGES.iter().any(|l| l.id == "en"));
        assert!(AVAILABLE_LANGUAGES.iter().any(|l| l.id == "ja"));
        assert!(AVAILABLE_LANGUAGES.iter().any(|l| l.id == "zh-CN"));
    }

    #[test]
    fn test_japanese_translations_coverage() {
        let ja = I18n::load("ja");
        assert_eq!(ja.get("menu.file"), "ファイル");
        assert_eq!(ja.get("menu.edit"), "編集");
        assert_eq!(ja.get("menu.view"), "表示");
        assert_eq!(ja.get("menu.view.language"), "言語");
        assert_eq!(ja.get("menu.view.reopen_with_encoding"), "エンコーディングを指定して再読み込み");
        assert_eq!(ja.get("menu.view.convert_to_encoding"), "エンコーディングを変換");
        assert_eq!(ja.get("menu.tabs"), "タブ");
        assert_eq!(ja.get("menu.tabs.next"), "次のタブ");
        assert_eq!(ja.get("menu.tabs.prev"), "前のタブ");
        assert_eq!(ja.get("menu.zee.about"), "zee について");
        assert_eq!(ja.get("menu.zee.quit"), "zee を終了");
        assert_eq!(ja.get("dialog.settings.title"), "設定");
        assert_eq!(ja.get("dialog.settings.language"), "言語");
        assert_eq!(ja.get("dialog.settings.language_auto"), "自動（システム設定）");
        assert_eq!(ja.get("dialog.settings.theme"), "テーマ");
        assert_eq!(ja.get("dialog.settings.font_family"), "エディタフォント");
    }

    #[test]
    fn test_all_english_keys_have_japanese_translations() {
        let en_keys = I18n::get_en_defaults();
        let ja = I18n::load("ja");
        let mut missing_keys = Vec::new();

        for key in en_keys.keys() {
            let translated = ja.get(key);
            // If get(key) returns key itself, translation is missing
            if translated == key {
                missing_keys.push(key.clone());
            }
        }

        assert!(
            missing_keys.is_empty(),
            "The following keys lack Japanese translations: {:?}",
            missing_keys
        );
    }
}
