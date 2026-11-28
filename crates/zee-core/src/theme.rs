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

    pub fn ansi256_to_rgb(idx: u8) -> (u8, u8, u8) {
        if idx < 16 {
            let table = [
                (0, 0, 0), (170, 0, 0), (0, 170, 0), (170, 85, 0),
                (0, 0, 170), (170, 0, 170), (0, 170, 170), (170, 170, 170),
                (85, 85, 85), (255, 85, 85), (85, 255, 85), (255, 255, 85),
                (85, 85, 255), (255, 85, 255), (85, 255, 255), (255, 255, 255),
            ];
            return table[idx as usize];
        }
        if idx >= 232 {
            let v = (idx - 232) * 10 + 8;
            return (v, v, v);
        }
        let idx = idx - 16;
        let r = (idx / 36) % 6;
        let g = (idx / 6) % 6;
        let b = idx % 6;
        let conv = |v: u8| if v == 0 { 0 } else { v * 40 + 55 };
        (conv(r), conv(g), conv(b))
    }

    pub fn to_rgb_tuple(&self) -> (u8, u8, u8) {
        match *self {
            Color::Rgb(r, g, b) => (r, g, b),
            Color::Ansi(i) => Self::ansi256_to_rgb(i),
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

pub fn is_default_theme(name: &str) -> bool {
    let s = name.trim().to_lowercase().replace([' ', '_'], "-");
    s == "os-default" || s == "terminal-default" || s == "default" || s == "system" || s.is_empty()
}

pub fn is_same_theme(a: &str, b: &str) -> bool {
    let norm_a = a.trim().to_lowercase().replace([' ', '_'], "-");
    let norm_b = b.trim().to_lowercase().replace([' ', '_'], "-");
    if norm_a == norm_b {
        return true;
    }
    is_default_theme(&norm_a) && is_default_theme(&norm_b)
}

impl Theme {
    pub fn builtins() -> Vec<Self> {
        let mut themes = vec![Self::terminal_default()];
        let files = [
            include_str!("../../../assets/themes/dark.toml"),
            include_str!("../../../assets/themes/light.toml"),
            include_str!("../../../assets/themes/tokyo-night.toml"),
            include_str!("../../../assets/themes/catppuccin-mocha.toml"),
            include_str!("../../../assets/themes/catppuccin-latte.toml"),
            include_str!("../../../assets/themes/solarized-dark.toml"),
            include_str!("../../../assets/themes/solarized-light.toml"),
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

        if is_default_theme(&target) {
            return Some(Self::terminal_default());
        }

        if target == "dark" || target_slug == "dark" {
            if let Some(t) = all.iter().find(|t| t.meta.name.to_lowercase() == "dark") {
                return Some(t.clone());
            }
            if let Some(t) = all.iter().find(|t| {
                let n = t.meta.name.to_lowercase();
                n == "tokyo night" || n == "tokyo-night"
            }) {
                return Some(t.clone());
            }
        }

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
                cursor: Color::Ansi(252),
                vi_normal_cursor: Some(Color::Ansi(117)),
                selection: Color::Ansi(238),
                line_number: Color::Ansi(246),
                current_line: None,
            },
            ui: UiColors {
                menu_bar_bg: Color::Ansi(235),
                menu_bar_fg: Color::Ansi(253),
                menu_item_active_bg: Color::Ansi(24),
                menu_item_active_fg: Color::Ansi(15),
                tab_bar_bg: Color::Ansi(233),
                tab_active_bg: Color::Ansi(239),
                tab_active_fg: Color::Ansi(15),
                tab_inactive_bg: Color::Ansi(236),
                tab_inactive_fg: Color::Ansi(247),
                status_bar_bg: Color::Ansi(235),
                status_bar_fg: Color::Ansi(252),
                panel_bg: Color::Ansi(235),
                panel_fg: Color::Ansi(252),
                panel_error_fg: Color::Ansi(9),
                dialog_bg: Color::Ansi(236),
                dialog_border: Color::Ansi(75),
                button_active_bg: Color::Ansi(24),
                button_active_fg: Color::Ansi(15),
            },
            syntax: SyntaxColors {
                keyword: Some(Color::Ansi(13)),    // Bright Magenta
                type_name: Some(Color::Ansi(14)),  // Bright Cyan
                function: Some(Color::Ansi(75)),   // Sky Blue
                string: Some(Color::Ansi(10)),     // Bright Green
                number: Some(Color::Ansi(11)),     // Bright Yellow
                comment: Some(Color::Ansi(246)),   // Medium-Light Gray
                operator: Some(Color::Ansi(14)),   // Bright Cyan
                punctuation: Some(Color::Ansi(7)), // White
                constant: Some(Color::Ansi(11)),
                attribute: Some(Color::Ansi(13)),
                error: Some(Color::Ansi(9)),       // Bright Red
                heading1: Some(Color::Ansi(10)),   // Bright Green
                heading2: Some(Color::Ansi(75)),   // Sky Blue
                heading3: Some(Color::Ansi(9)),    // Bright Red
                heading4: Some(Color::Ansi(11)),   // Bright Yellow / Gold
                heading5: Some(Color::Ansi(246)),  // Medium-Light Gray
                heading6: Some(Color::Ansi(13)),   // Bright Magenta / Violet
                inline_code: Some(Color::Ansi(13)), // Bright Magenta / Pink
                code_block: Some(Color::Ansi(14)),  // Cyan / Deep Navy contrast
                bold: None,
                link: None,
                list_marker: Some(Color::Ansi(246)), // Medium-Light Gray
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
    #[serde(default)]
    pub vi_normal_cursor: Option<Color>,
    pub selection: Color,
    pub line_number: Color,
    pub current_line: Option<Color>,
}

impl EditorColors {
    /// Returns the cursor color to use in Vi Normal/Visual (Command) mode,
    /// ensuring it is visibly distinct from the Insert mode cursor (`self.cursor`)
    /// while maintaining high WCAG contrast against `self.background`.
    pub fn vi_command_cursor(&self) -> Color {
        if let Some(c) = self.vi_normal_cursor {
            return c;
        }
        let (br, bg, bb) = self.background.to_rgb_tuple();
        let is_light_bg = (br as u32 * 299 + bg as u32 * 587 + bb as u32 * 114) > 140_000;
        let (cr, cg, cb) = self.cursor.to_rgb_tuple();
        if is_light_bg {
            let candidate = Color::Rgb(0, 71, 143);
            if (cr as i32 - 0).abs() + (cg as i32 - 71).abs() + (cb as i32 - 143).abs() < 60 {
                Color::Rgb(30, 41, 59)
            } else {
                candidate
            }
        } else {
            let candidate = Color::Rgb(125, 207, 255);
            if (cr as i32 - 125).abs() + (cg as i32 - 207).abs() + (cb as i32 - 255).abs() < 60 {
                Color::Rgb(245, 224, 220)
            } else {
                candidate
            }
        }
    }
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
    #[serde(default)]
    pub heading1: Option<Color>,
    #[serde(default)]
    pub heading2: Option<Color>,
    #[serde(default)]
    pub heading3: Option<Color>,
    #[serde(default)]
    pub heading4: Option<Color>,
    #[serde(default)]
    pub heading5: Option<Color>,
    #[serde(default)]
    pub heading6: Option<Color>,
    #[serde(default)]
    pub inline_code: Option<Color>,
    #[serde(default)]
    pub code_block: Option<Color>,
    #[serde(default)]
    pub bold: Option<Color>,
    #[serde(default)]
    pub link: Option<Color>,
    #[serde(default)]
    pub list_marker: Option<Color>,
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
                vi_normal_cursor: Some(Color::Rgb(125, 207, 255)),
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
                heading1: Some(Color::Rgb(122, 162, 247)),
                heading2: Some(Color::Rgb(187, 154, 247)),
                heading3: Some(Color::Rgb(255, 51, 85)),
                heading4: Some(Color::Rgb(241, 207, 84)),
                heading5: Some(Color::Rgb(86, 95, 137)),
                heading6: Some(Color::Rgb(157, 124, 216)),
                inline_code: Some(Color::Rgb(245, 194, 231)),
                code_block: Some(Color::Rgb(0x93, 0xc5, 0xfd)),
                bold: None,
                link: None,
                list_marker: Some(Color::Rgb(86, 95, 137)),
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
    fn test_light_theme_markdown_heading_colors() {
        let light = Theme::builtins()
            .into_iter()
            .find(|t| t.meta.name == "Light")
            .expect("Light theme should exist");
        assert_eq!(light.syntax.heading1, Some(Color::Rgb(0x15, 0x80, 0x3d))); // #15803d
        assert_eq!(light.syntax.heading2, Some(Color::Rgb(0x1d, 0x4e, 0xd8))); // #1d4ed8
        assert_eq!(light.syntax.heading3, Some(Color::Rgb(0xcf, 0x6f, 0x34))); // #cf6f34
        assert_eq!(light.syntax.heading4, Some(Color::Rgb(0x7c, 0x3a, 0xed))); // #7c3aed
        assert_eq!(light.syntax.heading5, Some(Color::Rgb(0x4d, 0x7c, 0x0f))); // #4d7c0f
        assert_eq!(light.syntax.heading6, Some(Color::Rgb(0x73, 0x82, 0x96))); // #738296
        assert_eq!(light.syntax.code_block, Some(Color::Rgb(0x1e, 0x40, 0xaf))); // #1e40af
        assert_eq!(light.syntax.bold, Some(Color::Rgb(0xea, 0x58, 0x0c)));     // #ea580c

        // All Markdown heading levels + bold + inline_code + code_block + link must be pairwise distinct
        let md_colors = [
            ("heading1", light.syntax.heading1.unwrap()),
            ("heading2", light.syntax.heading2.unwrap()),
            ("heading3", light.syntax.heading3.unwrap()),
            ("heading4", light.syntax.heading4.unwrap()),
            ("heading5", light.syntax.heading5.unwrap()),
            ("heading6", light.syntax.heading6.unwrap()),
            ("bold", light.syntax.bold.unwrap()),
            ("inline_code", light.syntax.inline_code.unwrap()),
            ("code_block", light.syntax.code_block.unwrap()),
            ("link", light.syntax.link.unwrap()),
        ];
        for i in 0..md_colors.len() {
            for j in (i + 1)..md_colors.len() {
                assert_ne!(
                    md_colors[i].1, md_colors[j].1,
                    "Light theme Markdown tokens '{}' and '{}' must have distinct colors",
                    md_colors[i].0, md_colors[j].0
                );
            }
        }

        // Dark and Tokyo Night themes must use soft sky blue (#93c5fd) for code_block, distinct from H6 and H2
        for dark_name in ["Dark", "Tokyo Night"] {
            let dark = Theme::builtins()
                .into_iter()
                .find(|t| t.meta.name == dark_name)
                .unwrap_or_else(|| panic!("{dark_name} theme should exist"));
            assert_eq!(dark.syntax.code_block, Some(Color::Rgb(0x93, 0xc5, 0xfd))); // #93c5fd
            assert_ne!(dark.syntax.code_block, dark.syntax.heading6);
            assert_ne!(dark.syntax.code_block, dark.syntax.heading2);
        }
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

    #[test]
    fn test_builtin_themes_contrast_ratios() {
        fn ansi256_to_rgb(idx: u8) -> (u8, u8, u8) {
            if idx < 16 {
                let table = [
                    (0, 0, 0), (170, 0, 0), (0, 170, 0), (170, 85, 0),
                    (0, 0, 170), (170, 0, 170), (0, 170, 170), (170, 170, 170),
                    (85, 85, 85), (255, 85, 85), (85, 255, 85), (255, 255, 85),
                    (85, 85, 255), (255, 85, 255), (85, 255, 255), (255, 255, 255),
                ];
                return table[idx as usize];
            }
            if idx >= 232 {
                let v = (idx - 232) * 10 + 8;
                return (v, v, v);
            }
            let idx = idx - 16;
            let r = (idx / 36) % 6;
            let g = (idx / 6) % 6;
            let b = idx % 6;
            let conv = |v: u8| if v == 0 { 0 } else { v * 40 + 55 };
            (conv(r), conv(g), conv(b))
        }

        fn color_to_rgb(c: Color, use_256: bool) -> (u8, u8, u8) {
            if use_256 {
                return ansi256_to_rgb(c.to_ansi256());
            }
            match c {
                Color::Rgb(r, g, b) => (r, g, b),
                Color::Ansi(i) => ansi256_to_rgb(i),
            }
        }

        fn relative_luminance((r, g, b): (u8, u8, u8)) -> f64 {
            let chan = |v: u8| {
                let s = (v as f64) / 255.0;
                if s <= 0.03928 {
                    s / 12.92
                } else {
                    ((s + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * chan(r) + 0.7152 * chan(g) + 0.0722 * chan(b)
        }

        fn contrast_ratio(c1: (u8, u8, u8), c2: (u8, u8, u8)) -> f64 {
            let l1 = relative_luminance(c1);
            let l2 = relative_luminance(c2);
            let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
            (lighter + 0.05) / (darker + 0.05)
        }

        for theme in Theme::builtins() {
            for use_256 in [false, true] {
                let bg = color_to_rgb(theme.editor.background, use_256);
                let cl = color_to_rgb(theme.editor.current_line.unwrap_or(theme.editor.background), use_256);
                let fg = color_to_rgb(theme.editor.foreground, use_256);
                let cur = color_to_rgb(theme.editor.cursor, use_256);
                let vi_cur = color_to_rgb(theme.editor.vi_command_cursor(), use_256);
                let sel = color_to_rgb(theme.editor.selection, use_256);
                let ln = color_to_rgb(theme.editor.line_number, use_256);

                let check = |label: &str, c1: (u8, u8, u8), c2: (u8, u8, u8), min_ratio: f64| {
                    let ratio = contrast_ratio(c1, c2);
                    assert!(
                        ratio >= min_ratio,
                        "Built-in theme '{}' (256-col: {}) failed contrast check for {}: {:.2}:1 < {:.2}:1 ({:?} vs {:?})",
                        theme.meta.name, use_256, label, ratio, min_ratio, c1, c2
                    );
                };

                // Editor core colors & Vi Insert / Command mode cursors
                check("editor.foreground vs background", fg, bg, 3.5);
                check("editor.foreground vs current_line", fg, cl, 3.5);
                check("editor.foreground vs selection", fg, sel, 3.0);
                check("editor.line_number vs background", ln, bg, 2.5);
                check("editor.line_number vs current_line", ln, cl, 2.5);
                check("editor.cursor vs background (Insert cursor)", cur, bg, 5.0);
                check("editor.cursor vs current_line (Insert cursor)", cur, cl, 5.0);
                check("editor.vi_command_cursor vs background (Vi block cursor)", vi_cur, bg, 5.0);
                check("editor.vi_command_cursor vs current_line (Vi block cursor)", vi_cur, cl, 5.0);
                check("editor.background (cursor inverted text) vs vi_command_cursor", bg, vi_cur, 5.0);
                assert_ne!(
                    cur, vi_cur,
                    "Built-in theme '{}' (256-col: {}) must have distinct cursor colors for Vi Command mode ({:?}) vs Insert mode ({:?})",
                    theme.meta.name, use_256, vi_cur, cur
                );

                // UI colors
                check("ui.menu_bar_fg vs menu_bar_bg", color_to_rgb(theme.ui.menu_bar_fg, use_256), color_to_rgb(theme.ui.menu_bar_bg, use_256), 3.5);
                check("ui.menu_item_active_fg vs menu_item_active_bg", color_to_rgb(theme.ui.menu_item_active_fg, use_256), color_to_rgb(theme.ui.menu_item_active_bg, use_256), 3.0);
                check("ui.tab_active_fg vs tab_active_bg", color_to_rgb(theme.ui.tab_active_fg, use_256), color_to_rgb(theme.ui.tab_active_bg, use_256), 3.5);
                check("ui.tab_inactive_fg vs tab_inactive_bg", color_to_rgb(theme.ui.tab_inactive_fg, use_256), color_to_rgb(theme.ui.tab_inactive_bg, use_256), 2.5);
                check("ui.status_bar_fg vs status_bar_bg", color_to_rgb(theme.ui.status_bar_fg, use_256), color_to_rgb(theme.ui.status_bar_bg, use_256), 3.5);
                check("ui.panel_fg vs panel_bg", color_to_rgb(theme.ui.panel_fg, use_256), color_to_rgb(theme.ui.panel_bg, use_256), 3.0);
                check("ui.panel_error_fg vs panel_bg", color_to_rgb(theme.ui.panel_error_fg, use_256), color_to_rgb(theme.ui.panel_bg, use_256), 2.5);
                check("ui.panel_fg vs dialog_bg", color_to_rgb(theme.ui.panel_fg, use_256), color_to_rgb(theme.ui.dialog_bg, use_256), 3.0);
                check("editor.line_number vs dialog_bg", ln, color_to_rgb(theme.ui.dialog_bg, use_256), 2.5);
                check("ui.button_active_fg vs button_active_bg", color_to_rgb(theme.ui.button_active_fg, use_256), color_to_rgb(theme.ui.button_active_bg, use_256), 3.0);

                // Syntax tokens
                let syn_tokens: [(&str, Option<Color>); 21] = [
                    ("syntax.keyword", theme.syntax.keyword),
                    ("syntax.type_name", theme.syntax.type_name),
                    ("syntax.function", theme.syntax.function),
                    ("syntax.string", theme.syntax.string),
                    ("syntax.number", theme.syntax.number),
                    ("syntax.comment", theme.syntax.comment),
                    ("syntax.operator", theme.syntax.operator),
                    ("syntax.punctuation", theme.syntax.punctuation),
                    ("syntax.constant", theme.syntax.constant),
                    ("syntax.attribute", theme.syntax.attribute),
                    ("syntax.error", theme.syntax.error),
                    ("syntax.heading1", theme.syntax.heading1),
                    ("syntax.heading2", theme.syntax.heading2),
                    ("syntax.heading3", theme.syntax.heading3),
                    ("syntax.heading4", theme.syntax.heading4),
                    ("syntax.heading5", theme.syntax.heading5),
                    ("syntax.heading6", theme.syntax.heading6),
                    ("syntax.inline_code", theme.syntax.inline_code),
                    ("syntax.code_block", theme.syntax.code_block),
                    ("syntax.bold", theme.syntax.bold),
                    ("syntax.list_marker", theme.syntax.list_marker),
                ];
                for (name, opt_col) in syn_tokens {
                    if let Some(col) = opt_col {
                        let sc = color_to_rgb(col, use_256);
                        check(&format!("{} vs background", name), sc, bg, 2.5);
                        check(&format!("{} vs current_line", name), sc, cl, 2.5);
                    }
                }
            }
        }
    }
}
