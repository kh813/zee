use crossterm::{
    cursor,
    style::{self, Color, ContentStyle},
    QueueableCommand,
};
use std::io::{self, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub underline: bool,
    pub width: u8,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Reset,
            bg: Color::Reset,
            bold: false,
            underline: false,
            width: 1,
        }
    }
}

pub struct Renderer {
    width: u16,
    height: u16,
    prev_buffer: Vec<Cell>,
    curr_buffer: Vec<Cell>,
}

impl Renderer {
    pub fn new(width: u16, height: u16) -> Self {
        let size = (width as usize) * (height as usize);
        Self {
            width,
            height,
            prev_buffer: vec![Cell::default(); size],
            curr_buffer: vec![Cell::default(); size],
        }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        let size = (width as usize) * (height as usize);
        self.prev_buffer = vec![Cell::default(); size];
        self.curr_buffer = vec![Cell::default(); size];
    }

    pub fn set_cell(&mut self, x: u16, y: u16, cell: Cell) {
        if x < self.width && y < self.height {
            let idx = (y as usize) * (self.width as usize) + (x as usize);

            // If the cell to the left was double-width, writing to x invalidates it
            if x > 0 {
                let left_idx = idx - 1;
                if self.curr_buffer[left_idx].width > 1 {
                    self.curr_buffer[left_idx].ch = ' ';
                    self.curr_buffer[left_idx].width = 1;
                }
            }

            // If the cell we are overwriting was double-width, invalidate its continuation cell
            if self.curr_buffer[idx].width > 1 && (x + 1) < self.width {
                self.curr_buffer[idx + 1] = Cell {
                    ch: ' ',
                    width: 1,
                    fg: self.curr_buffer[idx].fg,
                    bg: self.curr_buffer[idx].bg,
                    bold: false,
                    underline: false,
                };
            }

            // If new cell is double-width but at the very right edge of screen, downgrade to space
            if cell.width > 1 && x + 1 >= self.width {
                self.curr_buffer[idx] = Cell {
                    ch: ' ',
                    width: 1,
                    fg: cell.fg,
                    bg: cell.bg,
                    bold: cell.bold,
                    underline: cell.underline,
                };
                return;
            }

            self.curr_buffer[idx] = cell;

            // If double-width, mark the next cell as a zero-width continuation cell
            if cell.width > 1 && (x + 1) < self.width {
                // If the next cell was itself double-width, clear its continuation cell
                if self.curr_buffer[idx + 1].width > 1 && (x + 2) < self.width {
                    self.curr_buffer[idx + 2] = Cell {
                        ch: ' ',
                        width: 1,
                        fg: self.curr_buffer[idx + 1].fg,
                        bg: self.curr_buffer[idx + 1].bg,
                        bold: false,
                        underline: false,
                    };
                }
                self.curr_buffer[idx + 1] = Cell {
                    ch: ' ',
                    fg: cell.fg,
                    bg: cell.bg,
                    bold: cell.bold,
                    underline: cell.underline,
                    width: 0,
                };
            }
        }
    }

    pub fn clear(&mut self) {
        for cell in self.curr_buffer.iter_mut() {
            *cell = Cell::default();
        }
    }

    #[allow(dead_code)]
    pub fn get_cell(&self, x: u16, y: u16) -> Cell {
        if x < self.width && y < self.height {
            let idx = (y as usize) * (self.width as usize) + (x as usize);
            self.curr_buffer[idx]
        } else {
            Cell::default()
        }
    }

    pub fn present<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        let mut last_style = ContentStyle::default();
        let mut term_cursor: Option<(u16, u16)> = None;

        for y in 0..self.height {
            let mut x = 0;
            while x < self.width {
                let idx = (y as usize) * (self.width as usize) + (x as usize);
                let curr = self.curr_buffer[idx];
                let prev = self.prev_buffer[idx];

                if curr.width == 0 {
                    x += 1;
                    continue;
                }

                let step = (curr.width as u16).max(1);

                if curr != prev {
                    if term_cursor != Some((x, y)) {
                        writer.queue(cursor::MoveTo(x, y))?;
                    }

                    let mut style = ContentStyle {
                        foreground_color: Some(curr.fg),
                        background_color: Some(curr.bg),
                        ..Default::default()
                    };
                    
                    if curr.bold {
                        style.attributes.set(style::Attribute::Bold);
                    }
                    if curr.underline {
                        style.attributes.set(style::Attribute::Underlined);
                    }

                    if style != last_style {
                        if last_style.attributes != style.attributes {
                            writer.queue(style::SetAttribute(style::Attribute::Reset))?;
                        }
                        writer.queue(style::SetStyle(style))?;
                        last_style = style;
                    }

                    writer.queue(style::Print(curr.ch))?;
                    term_cursor = Some((x + step, y));
                }
                x += step;
            }
        }

        self.prev_buffer.copy_from_slice(&self.curr_buffer);
        writer.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_wide_character_cells() {
        let mut r = Renderer::new(10, 2);
        // Set '日' (width 2) at (0, 0)
        r.set_cell(0, 0, Cell { ch: '日', width: 2, ..Default::default() });
        assert_eq!(r.curr_buffer[0].ch, '日');
        assert_eq!(r.curr_buffer[0].width, 2);
        assert_eq!(r.curr_buffer[1].width, 0);

        // Set '本' (width 2) at (2, 0)
        r.set_cell(2, 0, Cell { ch: '本', width: 2, ..Default::default() });
        assert_eq!(r.curr_buffer[2].ch, '本');
        assert_eq!(r.curr_buffer[2].width, 2);
        assert_eq!(r.curr_buffer[3].width, 0);

        // Overwrite cell at (1, 0) with 'a' (width 1): should invalidate '日' at (0, 0)
        r.set_cell(1, 0, Cell { ch: 'a', width: 1, ..Default::default() });
        assert_eq!(r.curr_buffer[0].ch, ' ');
        assert_eq!(r.curr_buffer[0].width, 1);
        assert_eq!(r.curr_buffer[1].ch, 'a');
        assert_eq!(r.curr_buffer[1].width, 1);

        let mut output = Vec::new();
        r.present(&mut output).unwrap();
        assert!(!output.is_empty());
    }
}

