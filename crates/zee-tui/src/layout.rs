pub struct Layout {
    pub width: u16,
    pub height: u16,
    pub menu_height: u16,
    pub tab_height: u16,
    pub panel_height: u16,
    pub status_height: u16,
    pub cmdline_height: u16,
    pub sidebar_width: u16,
    pub gutter_width: u16,
    pub menu_bar_items: Vec<(String, u16, u16)>, // (label, col_start, col_end)
    pub tab_rects: Vec<(usize, u16, u16)>,       // (tab_index, col_start, col_end)
    #[allow(dead_code)]
    pub tab_scroll: usize,
}

impl Layout {
    pub fn new(width: u16, height: u16) -> Self {
        let tab_height = if height >= 12 { 3 } else { 1 };
        Self {
            width,
            height,
            menu_height: 1,
            tab_height,
            panel_height: 0,
            status_height: 1,
            cmdline_height: 0,
            sidebar_width: 0,
            gutter_width: 0,
            menu_bar_items: Vec::new(),
            tab_rects: Vec::new(),
            tab_scroll: 0,
        }
    }

    pub fn recompute(
        &mut self,
        menus: &[crate::widgets::menu::Menu],
        buffers: &[zee_core::buffer::Editor],
        active_buffer_idx: usize,
        show_line_numbers: bool,
        show_sidebar: bool,
        vi_mode: bool,
    ) {
        self.cmdline_height = if vi_mode { 1 } else { 0 };
        self.tab_height = if self.height >= 12 { 3 } else { 1 };

        // Recompute menu items
        self.menu_bar_items.clear();
        let mut current_x = 1;
        for menu in menus {
            let start = current_x;
            let label_w: u16 = menu.label.chars().map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0) as u16).sum();
            let end = current_x + label_w + 2;
            self.menu_bar_items.push((menu.label.clone(), start, end));
            current_x = end;
        }

        // Recompute tabs
        self.tab_rects.clear();
        let mut current_tab_x = 0;
        for (i, buffer) in buffers.iter().enumerate() {
            let name = buffer.path.as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "[No Name]".to_string());
            let modified = if buffer.is_modified() { "[+] " } else { "" };
            let ro = if buffer.read_only { "[RO] " } else { "" };
            let label = format!(" {}{}{} × ", ro, modified, name);
            let width: u16 = label.chars().map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0) as u16).sum();
            
            self.tab_rects.push((i, current_tab_x, current_tab_x + width + 1));
            current_tab_x += width + 1;
        }

        // Sidebar width
        if show_sidebar && self.width >= 45 {
            self.sidebar_width = 26.min(self.width.saturating_sub(25) / 2).max(16);
        } else {
            self.sidebar_width = 0;
        }

        // Gutter width
        if show_line_numbers {
            if let Some(active_buffer) = buffers.get(active_buffer_idx) {
                let line_count = active_buffer.line_count();
                self.gutter_width = (line_count.to_string().len() as u16).max(2) + 2;
            } else {
                self.gutter_width = 4;
            }
        } else {
            self.gutter_width = 0;
        }
    }

    pub fn sidebar_bounds(&self) -> (u16, u16, u16, u16) {
        if self.sidebar_width == 0 {
            return (0, 0, 0, 0);
        }
        let y = self.menu_height + self.tab_height + self.panel_height;
        let bottom_offset = self.status_height + self.cmdline_height;
        let h = self.height.saturating_sub(y).saturating_sub(bottom_offset);
        (0, y, self.sidebar_width, h)
    }

    pub fn editor_bounds(&self) -> (u16, u16, u16, u16) {
        let x = self.sidebar_width + self.gutter_width;
        let y = self.menu_height + self.tab_height + self.panel_height;
        let w = self.width.saturating_sub(self.sidebar_width + self.gutter_width);
        let bottom_offset = self.status_height + self.cmdline_height;
        let h = self.height.saturating_sub(y).saturating_sub(bottom_offset);
        (x, y, w, h)
    }

    pub fn gutter_bounds(&self) -> (u16, u16, u16, u16) {
        let x = self.sidebar_width;
        let y = self.menu_height + self.tab_height + self.panel_height;
        let w = self.gutter_width;
        let bottom_offset = self.status_height + self.cmdline_height;
        let h = self.height.saturating_sub(y).saturating_sub(bottom_offset);
        (x, y, w, h)
    }

    pub fn menu_bounds(&self) -> (u16, u16, u16, u16) {
        (0, 0, self.width, self.menu_height)
    }

    pub fn tab_bounds(&self) -> (u16, u16, u16, u16) {
        (0, self.menu_height, self.width, self.tab_height)
    }

    pub fn panel_bounds(&self) -> (u16, u16, u16, u16) {
        (0, self.menu_height + self.tab_height, self.width, self.panel_height)
    }

    pub fn status_bounds(&self) -> (u16, u16, u16, u16) {
        let bottom_offset = self.status_height + self.cmdline_height;
        (0, self.height.saturating_sub(bottom_offset), self.width, self.status_height)
    }

    pub fn cmdline_bounds(&self) -> (u16, u16, u16, u16) {
        (0, self.height.saturating_sub(self.cmdline_height), self.width, self.cmdline_height)
    }

    pub fn dialog_bounds(&self, dims: (u16, u16)) -> (u16, u16, u16, u16) {
        let (dw, dh) = dims;
        let x = self.width.saturating_sub(dw) / 2;
        let y = self.height.saturating_sub(dh) / 2;
        (x, y, dw, dh)
    }
}

