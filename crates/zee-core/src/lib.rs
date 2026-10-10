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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // File
    New,
    NewFromTemplate(String),
    OpenTemplatesFolder,
    Open,
    OpenFolder,
    OpenGoogleDrive,
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
    SelectAll,
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

pub mod ime;
pub use ime::{is_cjk_ime_active, is_cjk_char, is_ssh_session};


