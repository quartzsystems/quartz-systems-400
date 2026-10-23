// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The screen a job's displays draw on: its 5250 device when it has
//! one, else a model of its own shown to `slic` as a panel (the
//! console job; `MIGRATE(phase8)`: the console becomes a TN5250 client
//! and the panel path goes). Display files and UIM panels both write
//! through it and wait for the operator through it.

use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use tn5250::ds::{aid, attr};
use tn5250::screen::{Read, ScreenModel};
use tn5250::term::Terminal;
use wm::ctl::{Panel, ToJob};
use wm::dev::key;

use crate::console::host::Host;

/// Where a job's displays go.
pub struct Surface {
    host: Host,
    /// The console job's own screen.
    console: ScreenModel,
}

/// A panel with the model's cells: text runs with their attributes,
/// the fields, the cursor.
#[must_use]
pub fn panel_of(m: &ScreenModel) -> Panel {
    let mut panel = Panel::default();
    let cols = m.cols();
    let cells = m.cells();
    let at = |n: usize| u8::try_from(n).unwrap_or(1);
    for row in 0..m.rows() {
        let line = &cells[row * cols..(row + 1) * cols];
        let mut col = 0;
        let mut color = 0u8;
        while col < cols {
            let cell = line[col];
            if attr::is_attr(cell) {
                color = match cell {
                    attr::WHITE | attr::WHITE_UL => 1,
                    attr::BLUE => 2,
                    attr::RED => 3,
                    _ => 0,
                };
                col += 1;
                continue;
            }
            if let Some(f) = m.fields().iter().find(|f| f.row == row + 1 && f.col == col + 1) {
                panel.fields.push((
                    at(row + 1),
                    at(col + 1),
                    u16::try_from(f.len).unwrap_or(1),
                    attr::is_nondisplay(f.attr),
                    f.text(),
                ));
                col += f.len;
                continue;
            }
            let start = col;
            let mut text = String::new();
            while col < cols
                && !attr::is_attr(line[col])
                && !m.fields().iter().any(|f| f.row == row + 1 && f.col == col + 1)
            {
                text.push(if line[col] == 0 { ' ' } else { ccsid::char_of(line[col]) });
                col += 1;
            }
            let trimmed = text.trim_end();
            if !trimmed.is_empty() {
                panel.text(at(row + 1), at(start + 1), color, trimmed);
            }
        }
    }
    panel.cursor = m.cursor().map(|(r, c)| (at(r), at(c)));
    panel
}

/// The attention identifier of a console key number.
fn aid_of(k: u8) -> u8 {
    match k {
        0 => aid::ENTER,
        1..=24 => Terminal::function_aid(k),
        key::PAGE_UP => aid::ROLL_DOWN,
        key::PAGE_DOWN => aid::ROLL_UP,
        key::HELP => aid::HELP,
        key::PRINT => aid::PRINT,
        key::CLEAR => aid::CLEAR,
        _ => aid::ENTER,
    }
}

impl Surface {
    /// Over the job's surroundings.
    #[must_use]
    pub fn new(host: Host) -> Self {
        Self {
            host,
            console: ScreenModel::new(false),
        }
    }

    /// The job's surroundings.
    #[must_use]
    pub fn host(&self) -> &Host {
        &self.host
    }

    /// Whether the screen is 132 columns wide.
    pub fn wide(&mut self) -> bool {
        self.with_model(|m| m.cols() == 132)
    }

    /// Run `f` on the screen the job draws on.
    pub fn with_model<T>(&mut self, f: impl FnOnce(&mut ScreenModel) -> T) -> T {
        if let Some(d) = self.host.display.borrow_mut().as_mut() {
            return f(&mut d.model);
        }
        f(&mut self.console)
    }

    /// Send the screen: to the device, or as a panel to `slic`.
    pub fn flush(&mut self, read: Read) {
        if let Some(d) = self.host.display.borrow_mut().as_mut() {
            d.forget_panel();
            d.flush(read);
            return;
        }
        let p = panel_of(&self.console);
        self.host.show(p);
    }

    /// Wait for the display: the attention identifier and the cursor,
    /// the model updated with what was typed. System Request,
    /// Attention and Print are the system's (`crate::sysdsp::keys`).
    /// `None` when the job is to end, the device is gone or the request
    /// was ended.
    pub fn wait(&mut self) -> Option<(u8, (u8, u8))> {
        // A job waiting at its display gives its activity level to
        // another; it takes one again before it goes on.
        stg::remote::display_wait(&self.host.link, true);
        let got = self.wait_input();
        if got.is_some() {
            stg::remote::display_wait(&self.host.link, false);
        }
        got
    }

    /// The wait itself: keys, resets, the job's end.
    fn wait_input(&mut self) -> Option<(u8, (u8, u8))> {
        use crate::sysdsp::keys;
        loop {
            if keys::unwinding() {
                return None;
            }
            match self.host.next(Duration::from_secs(1)) {
                Ok(ToJob::Record { flags, data, .. }) => {
                    let got = self
                        .host
                        .display
                        .borrow_mut()
                        .as_mut()
                        .and_then(|d| d.accept_raw(flags, &data));
                    match got {
                        Some((k, _)) if keys::is_system(k) => {
                            let host = self.host.clone();
                            if keys::take(&host, k) == keys::After::Unwind {
                                return None;
                            }
                        }
                        Some((_, Some(inb))) => return Some((inb.aid, (inb.row, inb.col))),
                        Some((_, None)) => self.flush(Read::MdtFields),
                        None => {}
                    }
                }
                Ok(ToJob::Input { key: k, fields }) => {
                    for (i, v) in fields.iter().enumerate() {
                        self.console.set_value(i, v);
                    }
                    let cursor = self.console.cursor().map_or((1, 1), |(r, c)| {
                        (u8::try_from(r).unwrap_or(1), u8::try_from(c).unwrap_or(1))
                    });
                    return Some((aid_of(k), cursor));
                }
                Ok(ToJob::DeviceReset { device }) => {
                    if let Some(d) = self.host.display.borrow_mut().as_mut() {
                        d.reset(device);
                    }
                }
                Ok(ToJob::DeviceGone | ToJob::End { .. }) | Err(RecvTimeoutError::Disconnected) => {
                    return None;
                }
                Err(RecvTimeoutError::Timeout) => {
                    if self.host.display.borrow().is_some() {
                        let host = self.host.clone();
                        crate::sysdsp::msgs::poll(&host);
                    }
                }
                Ok(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_from_model() {
        let mut m = ScreenModel::new(false);
        m.put(1, 36, "Sign On", attr::WHITE);
        m.put(6, 17, "User  . . :", attr::GREEN);
        m.field(6, 53, 10, attr::GREEN_UL, "", tn5250::ds::field::Ffw::default(), &[]);
        m.field(7, 53, 10, attr::NONDISPLAY, "", tn5250::ds::field::Ffw::default(), &[]);
        let p = panel_of(&m);
        assert!(p.texts.contains(&(1, 36, 1, "Sign On".into())));
        assert!(p.texts.contains(&(6, 17, 0, "User  . . :".into())));
        assert_eq!(p.fields.len(), 2);
        assert_eq!(p.fields[0], (6, 53, 10, false, String::new()));
        assert!(p.fields[1].3);
    }
}
