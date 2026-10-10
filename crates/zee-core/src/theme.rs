use serde::{Deserialize, Serialize, Deserializer, Serializer};
use std::path::Path;
use std::fs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Rgb(u8, u8, u8),
    Ansi(u8),
}

impl Color {
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some(Color::Rgb(r, g, b))
        } else if hex.len() >= 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        } else {
            None
        }
    }

    pub fn from_css_string(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.starts_with('#') {
            return Self::from_hex(s);
        }
        if s.starts_with("ansi(") && s.ends_with(')') {
            let inner = s[5..s.len()-1].trim();
            if let Ok(i) = inner.parse::<u8>() {
                return Some(Color::Ansi(i));
            }
            let idx = match inner.to_lowercase().as_str() {
                "black" => Some(0),
                "red" => Some(1),
                "green" => Some(2),
                "yellow" => Some(3),
                "blue" => Some(4),
                "magenta" => Some(5),
                "cyan" => Some(6),
                "white" => Some(7),
                "bright_black" | "bright-black" | "gray" | "grey" => Some(8),
                "bright_red" | "bright-red" => Some(9),
                "bright_green" | "bright-green" => Some(10),
                "bright_yellow" | "bright-yellow" => Some(11),
                "bright_blue" | "bright-blue" => Some(12),
                "bright_magenta" | "bright-magenta" => Some(13),
                "bright_cyan" | "bright-cyan" => Some(14),
                "bright_white" | "bright-white" => Some(15),
                _ => None,
            };
            if let Some(i) = idx {
                return Some(Color::Ansi(i));
            }
            return None;
        }
        if (s.starts_with("rgb(") || s.starts_with("rgba(")) && s.ends_with(')') {
            let start = s.find('(')? + 1;
            let inner = &s[start..s.len()-1];
            let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
            if parts.len() >= 3 {
                let r = parts[0].parse::<u8>().ok()?;
                let g = parts[1].parse::<u8>().ok()?;
                let b = parts[2].parse::<u8>().ok()?;
                return Some(Color::Rgb(r, g, b));
            }
        }
        match s.to_lowercase().as_str() {
            "black" => Some(Color::Rgb(0, 0, 0)),
            "white" => Some(Color::Rgb(255, 255, 255)),
            "red" => Some(Color::Rgb(255, 0, 0)),
            "green" => Some(Color::Rgb(0, 255, 0)),
            "blue" => Some(Color::Rgb(0, 0, 255)),
            "yellow" => Some(Color::Rgb(255, 255, 0)),
            "cyan" => Some(Color::Rgb(0, 255, 255)),
            "magenta" => Some(Color::Rgb(255, 0, 255)),
            "gray" | "grey" => Some(Color::Rgb(128, 128, 128)),
            _ => None,
        }
    }

    pub fn to_hex(&self) -> String {
        match self {
            Color::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
            Color::Ansi(i) => format!("ansi({})", i),
        }
    }

    pub fn to_ansi256(&self) -> u8 {
        match self {
            Color::Ansi(i) => *i,
            Color::Rgb(r, g, b) => {
                let r = *r;
                let g = *g;
                let b = *b;
                let cube = [0, 95, 135, 175, 215, 255];
                let mut best_dist = f64::MAX;
                let mut best_idx = 16u8;

                for (i, &cr) in cube.iter().enumerate() {
                    for (j, &cg) in cube.iter().enumerate() {
                        for (k, &cb) in cube.iter().enumerate() {
                            let dr = (r as f64) - (cr as f64);
                            let dg = (g as f64) - (cg as f64);
                            let db = (b as f64) - (cb as f64);
                            let dist = dr * dr * 0.299 + dg * dg * 0.587 + db * db * 0.114;
                            if dist < best_dist {
                                best_dist = dist;
                                best_idx = (16 + 36 * i + 6 * j + k) as u8;
                            }
                        }
                    }
                }

                for g_idx in 0..24 {
                    let gv = 8 + g_idx * 10;
                    let dr = (r as f64) - (gv as f64);
                    let dg = (g as f64) - (gv as f64);
                    let db = (b as f64) - (gv as f64);
                    let dist = dr * dr * 0.299 + dg * dg * 0.587 + db * db * 0.114;
                    if dist < best_dist {
                        best_dist = dist;
                        best_idx = (232 + g_idx) as u8;
                    }
                }

                best_idx
            }
        }
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Color::from_css_string(&s).ok_or_else(|| serde::de::Error::custom(format!("invalid color: {}", s)))
    }
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub meta: ThemeMeta,
    pub editor: EditorColors,
    pub ui: UiColors,
    pub syntax: SyntaxColors,
}

impl Theme {
    pub fn builtins() -> Vec<Self> {
        let mut themes = vec![Self::terminal_default()];
        let files = [
            include_str!("../../../assets/themes/tokyo-night.toml"),
            include_str!("../../../assets/themes/catppuccin-mocha.toml"),
            include_str!("../../../assets/themes/catppuccin-latte.toml"),
            include_str!("../../../assets/themes/solarized-dark.toml"),
            include_str!("../../../assets/themes/solarized-light.toml"),
            include_str!("../../../assets/themes/light.toml"),
        ];

        for s in files {
            if let Ok(theme) = toml::from_str(s) {
                themes.push(theme);
            }
        }
        themes
    }

    pub fn load_all() -> Vec<Self> {
        let mut themes = Self::builtins();

        if let Some(themes_dir) = crate::config::Config::themes_dir() {
            if themes_dir.exists() {
                if let Ok(entries) = fs::read_dir(themes_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("toml") {
                            if let Ok(content) = fs::read_to_string(&path) {
                                if let Ok(mut custom_theme) = toml::from_str::<Theme>(&content) {
                                    if custom_theme.meta.name.is_empty() {
                                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                            custom_theme.meta.name = stem.to_string();
                                        }
                                    }
                                    // If user custom theme has same name as a built-in, override it
                                    if let Some(pos) = themes.iter().position(|t| t.meta.name == custom_theme.meta.name) {
                                        themes[pos] = custom_theme;
                                    } else {
                                        themes.push(custom_theme);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        themes
    }

    pub fn find_by_name(name: &str) -> Option<Self> {
        let all = Self::load_all();
        let target = name.trim().to_lowercase();
        let target_slug = target.replace([' ', '_'], "-");

        all.into_iter().find(|t| {
            let t_name = t.meta.name.to_lowercase();
            let t_slug = t_name.replace([' ', '_'], "-");
            t_name == target || t_slug == target_slug
        })
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let theme: Theme = toml::from_str(&content)?;
        Ok(theme)
    }

    pub fn terminal_default() -> Self {
        Self {
            meta: ThemeMeta {
                name: "Terminal Default".to_string(),
                author: Some("zee contributors".to_string()),
                version: Some("1.0".to_string()),
            },
            editor: EditorColors {
                background: Color::Rgb(0, 0, 0), // Special: will be Reset in TUI
                foreground: Color::Rgb(255, 255, 255), // Special: will be Reset in TUI
                cursor: Color::Rgb(255, 255, 255),
                selection: Color::Ansi(8),
                line_number: Color::Ansi(8),
                current_line: None,
            },
            ui: UiColors {
                menu_bar_bg: Color::Ansi(27),
                menu_bar_fg: Color::Ansi(231),
                menu_item_active_bg: Color::Ansi(21),
                menu_item_active_fg: Color::Ansi(231),
                tab_bar_bg: Color::Ansi(233),
                tab_active_bg: Color::Ansi(239),
                tab_active_fg: Color::Ansi(15),
                tab_inactive_bg: Color::Ansi(236),
                tab_inactive_fg: Color::Ansi(247),
                status_bar_bg: Color::Ansi(235),
                status_bar_fg: Color::Ansi(252),
                panel_bg: Color::Ansi(235),
                panel_fg: Color::Ansi(252),
                panel_error_fg: Color::Ansi(1),
                dialog_bg: Color::Ansi(236),
                dialog_border: Color::Ansi(75),
                button_active_bg: Color::Ansi(24),
                button_active_fg: Color::Ansi(15),
            },
            syntax: SyntaxColors {
                keyword: Some(Color::Ansi(5)),     // Magenta
                type_name: Some(Color::Ansi(3)),   // Yellow
                function: Some(Color::Ansi(4)),    // Blue
                string: Some(Color::Ansi(2)),      // Green
                number: Some(Color::Ansi(11)),     // Bright Yellow
                comment: Some(Color::Ansi(8)),     // Bright Black (Gray)
                operator: Some(Color::Ansi(6)),    // Cyan
                punctuation: Some(Color::Ansi(7)), // White
                constant: Some(Color::Ansi(11)),
                attribute: Some(Color::Ansi(5)),
                error: Some(Color::Ansi(1)),       // Red
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeMeta {
    pub name: String,
    pub author: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorColors {
    pub background: Color,
    pub foreground: Color,
    pub cursor: Color,
    pub selection: Color,
    pub line_number: Color,
    pub current_line: Option<Color>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiColors {
    pub menu_bar_bg: Color,
    pub menu_bar_fg: Color,
    pub menu_item_active_bg: Color,
    pub menu_item_active_fg: Color,
    pub tab_bar_bg: Color,
    pub tab_active_bg: Color,
    pub tab_active_fg: Color,
    pub tab_inactive_bg: Color,
    pub tab_inactive_fg: Color,
    pub status_bar_bg: Color,
    pub status_bar_fg: Color,
    pub panel_bg: Color,
    pub panel_fg: Color,
    pub panel_error_fg: Color,
    pub dialog_bg: Color,
    pub dialog_border: Color,
    pub button_active_bg: Color,
    pub button_active_fg: Color,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxColors {
    pub keyword: Option<Color>,
    pub type_name: Option<Color>,
    pub function: Option<Color>,
    pub string: Option<Color>,
    pub number: Option<Color>,
    pub comment: Option<Color>,
    pub operator: Option<Color>,
    pub punctuation: Option<Color>,
    pub constant: Option<Color>,
    pub attribute: Option<Color>,
    pub error: Option<Color>,
}

impl Default for Theme {
    fn default() -> Self {
        // Tokyo Night inspired default
        Self {
            meta: ThemeMeta {
                name: "Tokyo Night".to_string(),
                author: Some("zee contributors".to_string()),
                version: Some("1.0".to_string()),
            },
            editor: EditorColors {
                background: Color::Rgb(26, 27, 38),
                foreground: Color::Rgb(192, 202, 245),
                cursor: Color::Rgb(192, 202, 245),
                selection: Color::Rgb(40, 52, 87),
                line_number: Color::Rgb(120, 124, 153),
                current_line: Some(Color::Rgb(30, 32, 48)),
            },
            ui: UiColors {
                menu_bar_bg: Color::Rgb(22, 22, 30),
                menu_bar_fg: Color::Rgb(192, 202, 245),
                menu_item_active_bg: Color::Rgb(122, 162, 247),
                menu_item_active_fg: Color::Rgb(21, 22, 30),
                tab_bar_bg: Color::Rgb(19, 19, 26),
                tab_active_bg: Color::Rgb(36, 40, 59),
                tab_active_fg: Color::Rgb(255, 255, 255),
                tab_inactive_bg: Color::Rgb(26, 27, 38),
                tab_inactive_fg: Color::Rgb(120, 124, 153),
                status_bar_bg: Color::Rgb(22, 22, 30),
                status_bar_fg: Color::Rgb(192, 202, 245),
                panel_bg: Color::Rgb(22, 22, 30),
                panel_fg: Color::Rgb(192, 202, 245),
                panel_error_fg: Color::Rgb(247, 118, 142),
                dialog_bg: Color::Rgb(30, 32, 48),
                dialog_border: Color::Rgb(122, 162, 247),
                button_active_bg: Color::Rgb(122, 162, 247),
                button_active_fg: Color::Rgb(26, 27, 38),
            },
            syntax: SyntaxColors {
                keyword: Some(Color::Rgb(187, 154, 247)),
                type_name: Some(Color::Rgb(42, 195, 222)),
                function: Some(Color::Rgb(122, 162, 247)),
                string: Some(Color::Rgb(158, 206, 106)),
                number: Some(Color::Rgb(255, 158, 100)),
                comment: Some(Color::Rgb(86, 95, 137)),
                operator: Some(Color::Rgb(137, 221, 255)),
                punctuation: Some(Color::Rgb(192, 202, 245)),
                constant: Some(Color::Rgb(255, 158, 100)),
                attribute: Some(Color::Rgb(187, 154, 247)),
                error: Some(Color::Rgb(247, 118, 142)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_from_css_and_hex() {
        assert_eq!(Color::from_css_string("#fff"), Some(Color::Rgb(255, 255, 255)));
        assert_eq!(Color::from_css_string("#1a1b26"), Some(Color::Rgb(26, 27, 38)));
        assert_eq!(Color::from_css_string("#1a1b2680"), Some(Color::Rgb(26, 27, 38)));
        assert_eq!(Color::from_css_string("rgb(255, 128, 0)"), Some(Color::Rgb(255, 128, 0)));
        assert_eq!(Color::from_css_string("rgba(255, 128, 0, 0.5)"), Some(Color::Rgb(255, 128, 0)));
        assert_eq!(Color::from_css_string("ansi(15)"), Some(Color::Ansi(15)));
        assert_eq!(Color::from_css_string("ansi(red)"), Some(Color::Ansi(1)));
        assert_eq!(Color::from_css_string("black"), Some(Color::Rgb(0, 0, 0)));
    }

    #[test]
    fn test_theme_builtins_and_finding() {
        let builtins = Theme::builtins();
        assert!(!builtins.is_empty());
        let tokyo = Theme::find_by_name("tokyo-night");
        assert!(tokyo.is_some());
        assert_eq!(tokyo.unwrap().meta.name, "Tokyo Night");
    }

    #[test]
    fn test_theme_from_toml() {
        let toml_str = r##"
        [meta]
        name = "My Custom Theme"
        author = "Test"
        version = "1.0"

        [editor]
        background = "#123456"
        foreground = "rgb(200, 200, 200)"
        cursor = "#ff0000"
        selection = "#334455"
        line_number = "ansi(8)"
        current_line = "rgba(255, 255, 255, 0.1)"

        [ui]
        menu_bar_bg = "#111"
        menu_bar_fg = "#eee"
        menu_item_active_bg = "#333"
        menu_item_active_fg = "#fff"
        tab_bar_bg = "#111"
        tab_active_bg = "#222"
        tab_active_fg = "#fff"
        tab_inactive_bg = "#111"
        tab_inactive_fg = "#888"
        status_bar_bg = "#111"
        status_bar_fg = "#eee"
        panel_bg = "#111"
        panel_fg = "#eee"
        panel_error_fg = "#f00"
        dialog_bg = "#222"
        dialog_border = "#555"
        button_active_bg = "#333"
        button_active_fg = "#fff"

        [syntax]
        keyword = "#bb9af7"
        "##;
        let theme: Result<Theme, _> = toml::from_str(toml_str);
        assert!(theme.is_ok());
        let theme = theme.unwrap();
        assert_eq!(theme.meta.name, "My Custom Theme");
        assert_eq!(theme.editor.background, Color::Rgb(18, 52, 86));
        assert_eq!(theme.editor.foreground, Color::Rgb(200, 200, 200));
        assert_eq!(theme.editor.line_number, Color::Ansi(8));
        assert_eq!(theme.syntax.keyword, Some(Color::Rgb(187, 154, 247)));
    }
}
