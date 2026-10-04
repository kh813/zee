use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;
use crate::config::Config;

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

    pub fn load(lang: &str) -> Self {
        let norm = Self::normalize_lang(lang);
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

        if let Some(path) = Self::locale_file_path(lang) {
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
        m.insert("menu.file".to_string(), "File".to_string());
        m.insert("menu.file.new_tab".to_string(), "New Tab".to_string());
        m.insert("menu.file.new_window".to_string(), "New Window".to_string());
        m.insert("menu.file.new".to_string(), "New".to_string());
        m.insert("menu.file.open".to_string(), "Open…".to_string());
        m.insert("menu.file.open_folder".to_string(), "Open Folder…".to_string());
        m.insert("menu.file.save".to_string(), "Save".to_string());
        m.insert("menu.file.save_as".to_string(), "Save As…".to_string());
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
        m.insert("menu.view.line_ending".to_string(), "Line Ending".to_string());
        m.insert("menu.view.theme".to_string(), "Theme".to_string());
        m.insert("menu.view.syntax".to_string(), "Syntax".to_string());

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

        m.insert("menu.view.syntax_plain".to_string(), "Plain Text".to_string());

        m.insert("menu.app.preferences".to_string(), "Preferences…".to_string());
        m.insert("menu.view.zoom_in".to_string(), "Zoom In".to_string());
        m.insert("menu.view.zoom_out".to_string(), "Zoom Out".to_string());
        m.insert("menu.view.reset_zoom".to_string(), "Reset Zoom".to_string());

        m.insert("dialog.settings.title".to_string(), "Preferences".to_string());
        m.insert("dialog.settings.theme".to_string(), "Theme".to_string());
        m.insert("dialog.settings.font_family".to_string(), "Editor Font".to_string());
        m.insert("dialog.settings.font_size".to_string(), "Font Size".to_string());
        m.insert("dialog.settings.line_height".to_string(), "Line Height".to_string());
        m.insert("dialog.settings.ui_font_size".to_string(), "UI Font Size".to_string());
        m.insert("dialog.settings.tab_size".to_string(), "Tab Width".to_string());
        m.insert("dialog.settings.sidebar_position".to_string(), "Sidebar Position".to_string());
        m.insert("dialog.settings.sidebar_left".to_string(), "Left".to_string());
        m.insert("dialog.settings.sidebar_right".to_string(), "Right".to_string());
        m.insert("dialog.settings.reset_defaults".to_string(), "Reset Defaults".to_string());

        m.insert("about.version".to_string(), "Version".to_string());
        m.insert("about.license".to_string(), "License".to_string());
        m
    }

    fn get_ja_defaults() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("menu.file".to_string(), "ファイル".to_string());
        m.insert("menu.file.new_tab".to_string(), "新規タブ".to_string());
        m.insert("menu.file.new_window".to_string(), "新規ウィンドウ".to_string());
        m.insert("menu.file.new".to_string(), "新規作成".to_string());
        m.insert("menu.file.open".to_string(), "開く…".to_string());
        m.insert("menu.file.open_folder".to_string(), "フォルダを開く…".to_string());
        m.insert("menu.file.save".to_string(), "保存".to_string());
        m.insert("menu.file.save_as".to_string(), "名前を付けて保存…".to_string());
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
        m.insert("menu.view.line_ending".to_string(), "改行コード".to_string());
        m.insert("menu.view.theme".to_string(), "テーマ".to_string());
        m.insert("menu.view.syntax".to_string(), "シンタックス".to_string());

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

        m.insert("menu.view.syntax_plain".to_string(), "標準テキスト".to_string());

        m.insert("menu.app.preferences".to_string(), "設定…".to_string());
        m.insert("menu.view.zoom_in".to_string(), "拡大".to_string());
        m.insert("menu.view.zoom_out".to_string(), "縮小".to_string());
        m.insert("menu.view.reset_zoom".to_string(), "実際のサイズ".to_string());

        m.insert("dialog.settings.title".to_string(), "設定".to_string());
        m.insert("dialog.settings.theme".to_string(), "テーマ".to_string());
        m.insert("dialog.settings.font_family".to_string(), "エディタフォント".to_string());
        m.insert("dialog.settings.font_size".to_string(), "フォントサイズ".to_string());
        m.insert("dialog.settings.line_height".to_string(), "行の高さ".to_string());
        m.insert("dialog.settings.ui_font_size".to_string(), "UIフォントサイズ".to_string());
        m.insert("dialog.settings.tab_size".to_string(), "タブ幅".to_string());
        m.insert("dialog.settings.sidebar_position".to_string(), "サイドバーの位置".to_string());
        m.insert("dialog.settings.sidebar_left".to_string(), "左側".to_string());
        m.insert("dialog.settings.sidebar_right".to_string(), "右側".to_string());
        m.insert("dialog.settings.reset_defaults".to_string(), "初期値に戻す".to_string());

        m.insert("about.version".to_string(), "バージョン".to_string());
        m.insert("about.license".to_string(), "ライセンス".to_string());
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
