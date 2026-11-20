// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A partition console's grid (`qsipart::console::Grid`, kept from its
//! frames and updates) drawn cell for cell as the framebuffer console
//! draws it: each attribute bit as `cells_line` reads it, the cursor's
//! shape as sent, and below the grid the status row from the
//! indicators.

use qsipart::console::{self as wire, Grid, attr, indicator};

use super::{Screen, Shape};
use crate::look::render::{Color, Keyboard, Look, Oia, Paint, Scheme, Style, Width};

/// Rows and columns drawn before the first frame.
pub const WAITING: (usize, usize) = (24, 80);

/// How a console cell's attribute bits draw. Blink is not drawn (the
/// framebuffer does not); a colour number past yellow draws green.
pub fn style(a: u16) -> Style {
    let color = match a & attr::COLOR {
        attr::WHITE => Color::White,
        attr::RED => Color::Red,
        attr::BLUE => Color::Blue,
        attr::TURQUOISE => Color::Turquoise,
        attr::PINK => Color::Pink,
        attr::YELLOW => Color::Yellow,
        _ => Color::Green,
    };
    let on = |bit: u16| a & bit != 0;
    Style {
        color,
        reverse: on(attr::REVERSE),
        underline: on(attr::UNDERLINE),
        column_sep: on(attr::COLUMN_SEP),
        nondisplay: on(attr::NONDISPLAY),
        high: on(attr::HIGH),
        width: if on(attr::CONTINUATION) {
            Width::Continuation
        } else if on(attr::DOUBLE_WIDTH) {
            Width::Double
        } else {
            Width::Single
        },
    }
}

/// A console cell to draw.
pub fn paint(c: wire::Cell) -> Paint {
    Paint {
        ch: c.ch,
        style: style(c.attr),
        cursor: false,
        selected: false,
    }
}

/// A console grid, the session's state and the look it is drawn with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    /// The grid (the last one, when disconnected).
    pub grid: Grid,
    /// The name the status row shows.
    pub system: String,
    /// Whether the console is attached.
    pub connected: bool,
    /// Whether the connection is over TLS.
    pub tls: bool,
    /// The colour map.
    pub scheme: Scheme,
}

impl View {
    /// The status row: the keyboard `X SYSTEM` while input is
    /// inhibited or before the first frame, `INS` in insert mode.
    pub fn oia(&self) -> Oia {
        let s = &self.grid.status;
        let on = |bit: u16| s.indicators & bit != 0;
        let keyboard = if on(indicator::INHIBITED) || !self.grid.ready() {
            Keyboard::System
        } else {
            Keyboard::Open
        };
        Oia {
            system: self.system.clone(),
            state: if self.connected {
                String::new()
            } else {
                "Not connected".into()
            },
            keyboard: self.connected.then_some(keyboard),
            insert: on(indicator::INSERT) && self.connected,
            message_waiting: false,
            tls: self.tls && self.connected,
            cursor: (usize::from(s.row) + 1, usize::from(s.col) + 1),
        }
    }

    /// How the cells draw.
    pub fn look(&self) -> Look {
        Look {
            scheme: self.scheme,
            greyed: !self.connected,
        }
    }

    /// The screen to draw: a blank 24 by 80 until the first frame.
    pub fn shown(&self) -> Screen {
        let g = &self.grid;
        let (rows, cols, cells) = if g.ready() {
            (
                usize::from(g.rows),
                usize::from(g.cols),
                g.cells.iter().map(|c| paint(*c)).collect(),
            )
        } else {
            let blank = paint(wire::Cell::default());
            (WAITING.0, WAITING.1, vec![blank; WAITING.0 * WAITING.1])
        };
        Screen {
            rows,
            cols,
            cells,
            shape: match g.status.shape {
                wire::Shape::Block => Shape::Block,
                wire::Shape::Hidden => Shape::Hidden,
                _ => Shape::Underline,
            },
            status: self.oia(),
            look: self.look(),
        }
    }

    /// The picture at `scale`.
    pub fn png(&self, scale: usize) -> Vec<u8> {
        self.shown().png(scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_map_as_the_framebuffer_reads_them() {
        let s = style(attr::YELLOW | attr::HIGH | attr::UNDERLINE | attr::BLINK);
        assert_eq!((s.color, s.high, s.underline), (Color::Yellow, true, true));
        assert!(!s.reverse && !s.nondisplay && s.width == Width::Single);
        assert_eq!(style(attr::DOUBLE_WIDTH).width, Width::Double);
        assert_eq!(
            style(attr::CONTINUATION | attr::DOUBLE_WIDTH).width,
            Width::Continuation
        );
        assert_eq!(style(7).color, Color::Green);
    }

    #[test]
    fn waiting_before_the_first_frame() {
        let v = View {
            grid: Grid::default(),
            system: "SYS".into(),
            connected: true,
            tls: false,
            scheme: Scheme::Console,
        };
        let s = v.shown();
        assert_eq!((s.rows, s.cols), WAITING);
        assert_eq!(s.status.keyboard, Some(Keyboard::System));
    }
}
