pub mod config;
pub mod i18n;
pub mod buffer;
pub mod search;
pub mod syntax;
pub mod theme;
pub mod file_tree;
pub mod outline;
pub mod plugin;
pub mod component_plugin;
pub mod selfupdate;
pub mod session;
pub mod cli;
pub mod recent;
pub mod template;
pub mod gdrive;
pub mod single_instance;
pub mod fuzzy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // File
    New,
    NewFromTemplate(String),
    OpenTemplatesFolder,
    Open,
    OpenFolder,
    OpenGoogleDrive,
    FuzzyFinder,
    ReloadFile,
    OpenRecent(std::path::PathBuf),
    ClearRecent,
    Save,
    SaveAs,
    ExportConfig,
    ExportAll,
    ImportConfig,
    ToggleTrimTrailingWhitespace,
    ToggleEnsureFinalNewline,
    Close,
    Exit,

    // Edit
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Find,
    Replace,
    ProjectSearch,
    SelectAll,
    ToggleComment,
    ConvertTabsToSpaces,
    ConvertSpacesToTabs,
    FormatDocument,
    SortLines,
    PluginCommand(String),
    ManagePlugins,
    OpenPluginsFolder,

    // View
    GoToLine,
    ToggleLineNumbers,
    ToggleWordWrap,
    ToggleViMode,
    ToggleSidebar,
    ToggleFiles,
    ToggleOutline,
    RefreshFileTree,
    ReopenWithEncoding(Encoding),
    ConvertToEncoding(Encoding),
    SetLineEnding(LineEnding),
    SetTheme(String),
    SetSyntax(String),
    SetLanguage(String),

    // Help
    About,
    CheckForUpdates,

    // Settings / Preferences
    OpenSettings,

    NoOp,
}

impl Action {
    pub fn from_name(s: &str) -> Option<Self> {
        let normalized = s.trim().to_lowercase().replace([' ', '-'], "_");
        match normalized.as_str() {
            "new" => Some(Action::New),
            "open" => Some(Action::Open),
            "open_folder" => Some(Action::OpenFolder),
            "fuzzy_finder" | "quick_open" => Some(Action::FuzzyFinder),
            "project_search" | "find_in_files" => Some(Action::ProjectSearch),
            "save" => Some(Action::Save),
            "save_as" => Some(Action::SaveAs),
            "close" => Some(Action::Close),
            "exit" | "quit" => Some(Action::Exit),
            "undo" => Some(Action::Undo),
            "redo" => Some(Action::Redo),
            "cut" => Some(Action::Cut),
            "copy" => Some(Action::Copy),
            "paste" => Some(Action::Paste),
            "find" => Some(Action::Find),
            "replace" => Some(Action::Replace),
            "select_all" => Some(Action::SelectAll),
            "toggle_comment" => Some(Action::ToggleComment),
            "convert_tabs_to_spaces" => Some(Action::ConvertTabsToSpaces),
            "convert_spaces_to_tabs" => Some(Action::ConvertSpacesToTabs),
            "format_document" => Some(Action::FormatDocument),
            "sort_lines" => Some(Action::SortLines),
            "goto_line" | "go_to_line" => Some(Action::GoToLine),
            "toggle_sidebar" => Some(Action::ToggleSidebar),
            "toggle_vi_mode" => Some(Action::ToggleViMode),
            "toggle_line_numbers" => Some(Action::ToggleLineNumbers),
            "toggle_word_wrap" => Some(Action::ToggleWordWrap),
            "open_settings" | "settings" | "preferences" => Some(Action::OpenSettings),
            "about" => Some(Action::About),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    ShiftJis,
    EucJp,
    Iso2022Jp,
    Latin1,
}

impl Encoding {
    pub fn name(&self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf8Bom => "UTF-8 BOM",
            Encoding::Utf16Le => "UTF-16 LE",
            Encoding::Utf16Be => "UTF-16 BE",
            Encoding::ShiftJis => "Shift-JIS",
            Encoding::EucJp => "EUC-JP",
            Encoding::Iso2022Jp => "ISO-2022-JP",
            Encoding::Latin1 => "Latin-1",
        }
    }

    pub fn next(&self) -> Encoding {
        match self {
            Encoding::Utf8 => Encoding::Utf8Bom,
            Encoding::Utf8Bom => Encoding::Utf16Le,
            Encoding::Utf16Le => Encoding::Utf16Be,
            Encoding::Utf16Be => Encoding::ShiftJis,
            Encoding::ShiftJis => Encoding::EucJp,
            Encoding::EucJp => Encoding::Iso2022Jp,
            Encoding::Iso2022Jp => Encoding::Latin1,
            Encoding::Latin1 => Encoding::Utf8,
        }
    }

    pub const ALL: &'static [Encoding] = &[
        Encoding::Utf8,
        Encoding::Utf8Bom,
        Encoding::Utf16Le,
        Encoding::Utf16Be,
        Encoding::ShiftJis,
        Encoding::EucJp,
        Encoding::Iso2022Jp,
        Encoding::Latin1,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    Crlf,
    Cr,
}

impl LineEnding {
    pub fn name(&self) -> &'static str {
        match self {
            LineEnding::Lf => "LF",
            LineEnding::Crlf => "CRLF",
            LineEnding::Cr => "CR",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViMode {
    Normal,
    Insert,
    Visual,
    VisualLine,
    VisualBlock,
}

pub use config::{Config, export_backup, import_backup, BackupReport};
pub use i18n::I18n;
pub use buffer::{normalize_vi_char, normalize_vi_key, resolve_key_stroke};

pub mod vi_cmd;
pub use vi_cmd::{ExCommand, ExRange, parse_ex_command};
pub use ropey::Rope;
pub use ropey;

pub mod ime;
pub use ime::{is_cjk_ime_active, is_cjk_char, is_ssh_session};


