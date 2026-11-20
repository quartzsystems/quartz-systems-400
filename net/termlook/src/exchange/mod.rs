// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The exchange between the terminal page's script (`assets/term.js`)
//! and its server, over a WebSocket of JSON text messages. The server
//! sends:
//!
//! - `screen`: the whole screen: `v` a number that grows, `rows`,
//!   `cols`, `cursor` `[row, col]` 1-based, `shape` when the cursor is
//!   not an underline (`block`, `hidden`), `state` (`disconnected`
//!   greys the screen), `system`, `tls`, `kb` the keyboard's text,
//!   `ins`, `mw`, and `cells`: `[row, text, styles]` for every row
//!   (0-based), a character a column each;
//! - `delta`: the same, `cells` only the rows that changed;
//! - `status`: the same less `rows`, `cols` and `cells`: the cursor and
//!   the indicators;
//! - `closed`: `reason`, the session is over;
//! - `message`: `id` and `text`, what the last key's action said.
//!
//! A style is one character, U+0100 plus the bits of [`style_char`].
//! A field that does not display is sent as blanks. The page sends
//! `{"t":"key","k":NAME}` ([`keys`]), `{"t":"type","s":TEXT}` and,
//! with the 5250 extras, `{"t":"cursor","r":ROW,"c":COL}` ([`Input`]).

pub mod json;
pub mod keys;

use qsipart::console::Key;

use crate::look::render::{Color, Keyboard, Style, Width};
use crate::screen::{Screen, Shape};

/// A style as the exchange carries it: U+0100 plus bits 0 to 2 the
/// colour (green, white, red, blue, turquoise, pink, yellow), 3 high
/// intensity, 4 reverse, 5 underline, 7 column separators, 8
/// nondisplay, 9 the second half of a double-width character, 10 the
/// first. The bits are the console's own (`qsipart::console::attr`).
pub fn style_char(s: Style) -> char {
    let color: u32 = match s.color {
        Color::Green => 0,
        Color::White => 1,
        Color::Red => 2,
        Color::Blue => 3,
        Color::Turquoise => 4,
        Color::Pink => 5,
        Color::Yellow => 6,
    };
    let bits = [
        (s.high, 1 << 3),
        (s.reverse, 1 << 4),
        (s.underline, 1 << 5),
        (s.column_sep, 1 << 7),
        (s.nondisplay, 1 << 8),
        (s.width == Width::Continuation, 1 << 9),
        (s.width == Width::Double, 1 << 10),
    ]
    .iter()
    .filter(|(on, _)| *on)
    .fold(color, |b, (_, bit)| b | bit);
    char::from_u32(0x100 + bits).unwrap_or('\u{100}')
}

/// The style a style character carries.
pub fn style_of(c: char) -> Option<Style> {
    let bits = u32::from(c).checked_sub(0x100).filter(|b| *b < 0x800)?;
    let on = |bit: u32| bits & bit != 0;
    Some(Style {
        color: match bits & 7 {
            1 => Color::White,
            2 => Color::Red,
            3 => Color::Blue,
            4 => Color::Turquoise,
            5 => Color::Pink,
            6 => Color::Yellow,
            _ => Color::Green,
        },
        high: on(1 << 3),
        reverse: on(1 << 4),
        underline: on(1 << 5),
        column_sep: on(1 << 7),
        nondisplay: on(1 << 8),
        width: if on(1 << 9) {
            Width::Continuation
        } else if on(1 << 10) {
            Width::Double
        } else {
            Width::Single
        },
    })
}

/// Row `r` as `[r,text,styles]`.
pub fn row(s: &Screen, r: usize) -> String {
    let cells = &s.cells[r * s.cols..(r + 1) * s.cols];
    let text: String = cells
        .iter()
        .map(|p| if p.style.nondisplay { ' ' } else { p.ch })
        .collect();
    let styles: String = cells.iter().map(|p| style_char(p.style)).collect();
    format!("[{r},{},{}]", json::string(&text), json::string(&styles))
}

/// The keyboard's text on the status row.
pub fn keyboard_text(k: Option<Keyboard>) -> String {
    match k {
        Some(Keyboard::System) => "X SYSTEM".into(),
        Some(Keyboard::HostError) => "X II".into(),
        Some(Keyboard::OperatorError(c)) => format!("X II {c:04}"),
        Some(Keyboard::Open) | None => String::new(),
    }
}

/// The fields every message about a screen has after its kind's own.
fn status_fields(s: &Screen, state: &str) -> String {
    let o = &s.status;
    let shape = match s.shape {
        Shape::Underline => String::new(),
        Shape::Block => ",\"shape\":\"block\"".into(),
        Shape::Hidden => ",\"shape\":\"hidden\"".into(),
    };
    format!(
        "\"cursor\":[{},{}]{shape},\"state\":{},\"system\":{},\"tls\":{},\"kb\":{},\"ins\":{},\"mw\":{}",
        o.cursor.0,
        o.cursor.1,
        json::string(state),
        json::string(&o.system),
        o.tls,
        json::string(&keyboard_text(o.keyboard)),
        o.insert,
        o.message_waiting,
    )
}

/// A `screen` message, or a `delta` of the rows that changed since
/// `prev` when it has the same size. `version` grows with each change;
/// `state` is the session's (`disconnected` greys the page's screen).
pub fn screen(now: &Screen, prev: Option<&Screen>, version: u64, state: &str) -> String {
    let same = prev.is_some_and(|p| p.rows == now.rows && p.cols == now.cols);
    let w = now.cols;
    let rows: Vec<String> = (0..now.rows)
        .filter(|r| {
            !same
                || prev
                    .is_some_and(|p| p.cells[r * w..(r + 1) * w] != now.cells[r * w..(r + 1) * w])
        })
        .map(|r| row(now, r))
        .collect();
    format!(
        "{{\"t\":\"{}\",\"v\":{version},\"rows\":{},\"cols\":{},{},\"cells\":[{}]}}",
        if same { "delta" } else { "screen" },
        now.rows,
        now.cols,
        status_fields(now, state),
        rows.join(",")
    )
}

/// A `status` message: the cursor and the indicators alone.
pub fn status(now: &Screen, version: u64, state: &str) -> String {
    format!(
        "{{\"t\":\"status\",\"v\":{version},{}}}",
        status_fields(now, state)
    )
}

/// A `closed` message.
pub fn closed(reason: &str) -> String {
    format!("{{\"t\":\"closed\",\"reason\":{}}}", json::string(reason))
}

/// A `message` message.
pub fn message(id: &str, text: &str) -> String {
    format!(
        "{{\"t\":\"message\",\"id\":{},\"text\":{}}}",
        json::string(id),
        json::string(text)
    )
}

/// What the page sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    /// A key by its name (`Enter`, `F:3`, ...; [`keys`]).
    Key(String),
    /// Characters typed at the cursor.
    Text(String),
    /// The cursor moved to a 1-based row and column (5250 extras).
    Cursor(usize, usize),
}

impl Input {
    /// A message from the page; `None` for one it does not know.
    pub fn parse(msg: &str) -> Option<Input> {
        let field = |name: &str| json::field(msg, name);
        match field("t")?.as_str() {
            "key" => Some(Input::Key(field("k")?)),
            "type" => Some(Input::Text(field("s")?)),
            "cursor" => Some(Input::Cursor(
                field("r")?.parse().ok()?,
                field("c")?.parse().ok()?,
            )),
            _ => None,
        }
    }

    /// The console keys it is: a key the console has, or each printable
    /// character typed; nothing for a cursor move or a 5250-only key.
    pub fn console_keys(&self) -> Vec<Key> {
        match self {
            Input::Key(k) => keys::console_key(k).into_iter().collect(),
            Input::Text(s) => s
                .chars()
                .filter(|c| !c.is_control())
                .map(Key::Char)
                .collect(),
            Input::Cursor(..) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::look::render::{self, Look, Oia, Paint};

    #[test]
    fn styles_round_trip() {
        for a in 0x20..0x40u8 {
            let s = render::style(a);
            assert_eq!(style_of(style_char(s)), Some(s), "{a:#x}");
        }
        for bits in 0..0x800u16 {
            if bits & (1 << 6) != 0 {
                continue;
            }
            let s = crate::screen::grid::style(bits);
            assert_eq!(style_of(style_char(s)), Some(s), "{bits:#x}");
        }
        assert_eq!(style_of('A'), None);
    }

    fn screen_of(text: &str) -> Screen {
        let cells = text
            .chars()
            .map(|ch| Paint::text(ch, Color::Green))
            .collect::<Vec<_>>();
        Screen {
            rows: 2,
            cols: text.len() / 2,
            cells,
            shape: Shape::Underline,
            status: Oia {
                system: "SYS".into(),
                cursor: (1, 1),
                ..Oia::default()
            },
            look: Look::default(),
        }
    }

    #[test]
    fn whole_then_the_rows_that_changed() {
        let a = screen_of("abcd");
        let mut b = a.clone();
        b.cells[3].ch = 'X';
        b.cells[2].style.nondisplay = true;
        let first = screen(&a, None, 1, "connected");
        assert!(first.starts_with("{\"t\":\"screen\",\"v\":1,\"rows\":2,\"cols\":2,\"cursor\":[1,1],\"state\":\"connected\",\"system\":\"SYS\""));
        assert!(first.contains("[0,\"ab\",\"\u{100}\u{100}\"],[1,\"cd\""));
        let next = screen(&b, Some(&a), 2, "connected");
        assert!(
            next.starts_with("{\"t\":\"delta\"")
                && next.contains("\"cells\":[[1,\" X\",\"\u{200}\u{100}\"]]"),
            "{next}"
        );
        assert!(status(&b, 3, "connected").starts_with("{\"t\":\"status\",\"v\":3,\"cursor\""));
        assert_eq!(closed("ended"), "{\"t\":\"closed\",\"reason\":\"ended\"}");
    }

    #[test]
    fn input_and_console_keys() {
        assert_eq!(
            Input::parse("{\"t\":\"key\",\"k\":\"F:3\"}"),
            Some(Input::Key("F:3".into()))
        );
        assert_eq!(
            Input::parse("{\"t\":\"cursor\",\"r\":5,\"c\":10}"),
            Some(Input::Cursor(5, 10))
        );
        assert_eq!(Input::parse("{\"t\":\"x\"}"), None);
        assert_eq!(Input::Key("F:3".into()).console_keys(), [Key::Function(3)]);
        assert_eq!(Input::Key("Help".into()).console_keys(), []);
        assert_eq!(
            Input::Text("a\nb".into()).console_keys(),
            [Key::Char('a'), Key::Char('b')]
        );
    }
}
