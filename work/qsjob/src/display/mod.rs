// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job's display when it owns a 5250 device: the socket to the
//! session holding the device, the screen model, and panels turned
//! into data streams. Every interactive job has one, the console's too.
//! Display files (`dds`) and UIM (`uim`) write through the model.

pub mod dds;
pub mod msgline;
pub mod surface;
pub mod uim;

/// The framebuffer console's device, held by the console job
/// `QCONSOLE` with or without TCP/IP.
pub const CONSOLE_DEVICE: &str = "QSCONSOLE";

use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::mpsc::Sender;

use ipc::chan::Chan;
use tn5250::ds::field::Ffw;
use tn5250::ds::inbound::Inbound;
use tn5250::ds::wdsf::Model;
use tn5250::ds::{aid, attr};
use tn5250::record::flags;
use tn5250::screen::{Read, ScreenModel};
use wm::ctl::{Panel, ToJob};
use wm::dev::{self, FromDevice, ToDevice, key};

/// The 5250 attribute of a panel's text attribute code.
fn text_attr(code: u8) -> u8 {
    match code {
        1 => attr::WHITE,
        2 => attr::BLUE,
        3 => attr::RED,
        _ => attr::GREEN,
    }
}

/// Render a panel into a model: the screen is cleared (panels are
/// 24x80), texts and fields placed, the message on the last row, the
/// cursor set.
pub fn render(p: &Panel, m: &mut ScreenModel) {
    m.resize(false);
    m.clear();
    let rows = m.rows();
    for (row, col, a, text) in &p.texts {
        if !p.message.is_empty() && usize::from(*row) == rows {
            continue;
        }
        m.put(usize::from(*row), usize::from(*col), text, text_attr(*a));
    }
    for (row, col, len, hidden, value) in &p.fields {
        let reverse = p.reverse.contains(&(*row, *col));
        let a = if *hidden {
            attr::NONDISPLAY
        } else if reverse {
            attr::GREEN_UL_RI
        } else {
            attr::GREEN_UL
        };
        m.field(
            usize::from(*row),
            usize::from(*col),
            usize::from(*len),
            a,
            value,
            Ffw::default(),
            &[],
        );
    }
    if !p.message.is_empty() {
        m.set_error_row(rows);
        m.error_line(&p.message, attr::WHITE);
        if p.error {
            m.alarm();
        }
    }
    if let Some((r, c)) = p.cursor {
        m.set_cursor(usize::from(r), usize::from(c));
    }
}

/// The key number of an inbound record: the AID, or the flags.
pub fn key_of(flags_: u16, a: u8) -> Option<u8> {
    if flags_ & flags::SRQ != 0 {
        return Some(key::SYSREQ);
    }
    if flags_ & flags::ATN != 0 {
        return Some(key::ATTN);
    }
    if flags_ & flags::TRQ != 0 {
        return Some(key::TESTREQ);
    }
    match a {
        aid::ROLL_DOWN => Some(key::PAGE_UP),
        aid::ROLL_UP => Some(key::PAGE_DOWN),
        aid::HELP => Some(key::HELP),
        aid::PRINT => Some(key::PRINT),
        aid::CLEAR => Some(key::CLEAR),
        aid::RECORD_BACKSPACE => Some(key::RECORD_BACKSPACE),
        other => ScreenModel::key_of(other),
    }
}

/// A display's screen put aside.
#[derive(Clone, Debug)]
pub struct Snapshot {
    model: ScreenModel,
    last: Option<Panel>,
}

impl Snapshot {
    /// A message on the last row, when no field is there.
    pub fn message(&mut self, text: &str) {
        let rows = self.model.rows();
        if self.model.fields().iter().any(|f| f.row == rows) {
            return;
        }
        self.model.set_error_row(rows);
        self.model.error_line(text, attr::WHITE);
        self.last = None;
    }
}

/// The key of a record that carries one without fields (System
/// Request, Attention, Test Request).
#[must_use]
pub fn flag_key(flags_: u16) -> Option<u8> {
    if flags_ & (flags::SRQ | flags::ATN | flags::TRQ) != 0 {
        return key_of(flags_, 0);
    }
    None
}

/// The job's end of its device.
pub struct Display {
    chan: Chan,
    /// The screen the client shows.
    pub model: ScreenModel,
    last: Option<Panel>,
    /// The device's name.
    pub device: String,
    /// The device shows 27x132; what is shown picks the mode.
    pub wide: bool,
}

impl Display {
    /// Connect to the session's socket for `device` and read its
    /// records onto the job's inbox.
    pub fn connect(device: &str, inbox: &Sender<ToJob>, model: Model) -> Result<Self, String> {
        let path = dev::socket(device);
        let mut last_err = String::new();
        let mut stream = None;
        for _ in 0..50 {
            match UnixStream::connect(Path::new(&path)) {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(e) => {
                    last_err = e.to_string();
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
        let stream = stream.ok_or_else(|| format!("device {device}: {last_err}"))?;
        let chan = Chan::new(stream);
        let mut reader = chan.try_clone().map_err(|e| e.to_string())?;
        let tx = inbox.clone();
        std::thread::spawn(move || {
            loop {
                let Ok(m) = reader.recv::<FromDevice>() else {
                    let _ = tx.send(ToJob::DeviceGone);
                    return;
                };
                let to = match m {
                    FromDevice::Record {
                        flags,
                        opcode,
                        data,
                    } => ToJob::Record {
                        flags,
                        opcode,
                        data,
                    },
                    FromDevice::Reset { device } => ToJob::DeviceReset { device },
                    FromDevice::Gone => ToJob::DeviceGone,
                    // A display gets no printer records.
                    FromDevice::Printed { .. } => continue,
                };
                if tx.send(to).is_err() {
                    return;
                }
            }
        });
        Ok(Self {
            chan,
            model: ScreenModel::new(false),
            last: None,
            device: device.to_string(),
            wide: model.wide(),
        })
    }

    /// Show a panel and open the keyboard.
    pub fn show(&mut self, p: &Panel) {
        render(p, &mut self.model);
        self.last = Some(p.clone());
        self.flush(Read::MdtFields);
    }

    /// Send the model as it stands.
    pub fn flush(&mut self, read: Read) {
        let out = self.model.render(read);
        let _ = self.chan.send(&ToDevice::Write {
            opcode: out.opcode,
            data: out.data,
        });
    }

    /// The display was reset: send the last panel whole again, or the
    /// model as it stands when display files drew it.
    pub fn reset(&mut self, device: String) {
        if !device.is_empty() {
            self.device = device;
        }
        self.model.invalidate();
        if let Some(p) = self.last.clone() {
            self.show(&p);
        } else {
            let read = if self.model.is_reading() {
                Read::MdtFields
            } else {
                Read::None
            };
            self.flush(read);
        }
    }

    /// Display files took the screen: no panel to repeat.
    pub fn forget_panel(&mut self) {
        self.last = None;
    }

    /// The screen as it stands, to put back after a system display.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            model: self.model.clone(),
            last: self.last.clone(),
        }
    }

    /// Put a snapshot back and send it whole, the keyboard open again
    /// when a read was outstanding.
    pub fn restore(&mut self, s: Snapshot) {
        let reading = s.model.is_reading();
        self.model = s.model;
        self.last = s.last;
        self.model.invalidate();
        self.flush(if reading { Read::MdtFields } else { Read::None });
    }

    /// Put a snapshot back without sending it: something is drawn over
    /// it before the next flush.
    pub fn restore_quiet(&mut self, s: Snapshot) {
        self.model = s.model;
        self.last = None;
        self.model.invalidate();
    }


    /// A record from the display: the key, and the inbound record when
    /// the key carried one (System Request and Attention do not).
    pub fn accept_raw(&mut self, flags_: u16, data: &[u8]) -> Option<(u8, Option<Inbound>)> {
        if flags_ & (flags::SRQ | flags::ATN | flags::TRQ) != 0 {
            return key_of(flags_, 0).map(|k| (k, None));
        }
        let inb = self.model.accept(data)?;
        let k = key_of(flags_, inb.aid)?;
        Some((k, Some(inb)))
    }

    /// A record from the display: the key and the fields, when it was
    /// an attention key.
    pub fn accept(&mut self, flags_: u16, data: &[u8]) -> Option<(u8, Vec<String>)> {
        if flags_ & (flags::SRQ | flags::ATN | flags::TRQ) != 0 {
            return key_of(flags_, 0).map(|k| (k, self.model.values()));
        }
        let inb = self.model.accept(data)?;
        let k = key_of(flags_, inb.aid)?;
        Some((k, self.model.values()))
    }
}

impl Drop for Display {
    /// The job is done with its device.
    fn drop(&mut self) {
        let _ = self.chan.send(&ToDevice::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tn5250::ds::wdsf::Capabilities;
    use tn5250::record::Record;
    use tn5250::term::{Key, Terminal};

    #[test]
    fn panel_reaches_a_terminal() {
        let p = wm::panels::sign_on("QS400", "QINTER", "QPADEV0001", false, "");
        let mut m = ScreenModel::new(false);
        render(&p, &mut m);
        let out = m.render(Read::MdtFields);
        let mut t = Terminal::new(Capabilities {
            model: Model::M3179,
            gui: false,
            color: true,
        });
        t.apply(&Record::new(out.opcode, out.data)).unwrap();
        let rows = t.text_rows();
        assert_eq!(&rows[0][35..42], "Sign On");
        assert_eq!(&rows[3][69..79], "QPADEV0001");
        assert_eq!(t.fields().len(), 5);
        assert_eq!(t.cursor(), (6, 53));
        t.type_str("QSECOFR");
        t.key(Key::F(3));
        let rec = t.take_outbox().remove(0);
        let inb = m.accept(&rec.data).unwrap();
        assert_eq!(key_of(rec.flags, inb.aid), Some(3));
        assert_eq!(m.values()[0], "QSECOFR");
    }

    /// A panel is 24x80 on any device: Clear Unit, not the alternate.
    #[test]
    fn panel_is_24x80_on_a_wide_screen() {
        let p = wm::panels::sign_on("QS400", "QINTER", "QPADEV0001", false, "");
        let mut m = ScreenModel::new(true);
        render(&p, &mut m);
        assert_eq!((m.rows(), m.cols()), (24, 80));
        let out = m.render(Read::MdtFields);
        assert_eq!(&out.data[..2], &[0x04, tn5250::ds::cmd::CLEAR_UNIT]);
    }

    #[test]
    fn message_replaces_the_last_row() {
        let p = wm::panels::main_menu("QS400", "", "Command not found.");
        let mut m = ScreenModel::new(false);
        render(&p, &mut m);
        let rows = m.text_rows();
        assert!(rows[23].contains("Command not found."));
        assert!(!rows[23].contains("COPYRIGHT"));
    }

    /// A panel as a terminal shows it.
    fn shown(p: &Panel) -> Terminal {
        let mut m = ScreenModel::new(false);
        render(p, &mut m);
        let out = m.render(Read::MdtFields);
        let mut t = Terminal::new(Capabilities {
            model: Model::M3179,
            gui: false,
            color: true,
        });
        t.apply(&Record::new(out.opcode, out.data)).unwrap();
        t
    }

    /// Change Password's attributes against IBM's capture: each label
    /// row from its attribute byte to the field's, and every cell of every
    /// field, which a 128-character field shares with the two rows
    /// below. No field may reach a label.
    #[test]
    fn change_password_attributes_match_ibm() {
        for (source, long) in [("oracle71", false), ("pub400", true)] {
            let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/conformance/golden/verified")
                .join(source)
                .join("signon/change-password.attr");
            let want: Vec<Vec<u8>> = std::fs::read_to_string(&path)
                .unwrap()
                .lines()
                .map(|l| l.trim_end_matches('\r').as_bytes().to_vec())
                .collect();
            let p = wm::panels::change_password("USERNAME", 0, long, "");
            let t = shown(&p);
            let got: Vec<Vec<u8>> = t.attr_rows().into_iter().map(String::into_bytes).collect();
            let mut cells = Vec::new();
            for f in t.fields() {
                assert_eq!(f.len, if long { 128 } else { 10 });
                let row = f.start / 80;
                cells.extend((2..46).map(|c| row * 80 + c));
                cells.extend(f.start..f.start + f.len);
            }
            for i in cells {
                let (r, c) = (i / 80, i % 80);
                assert_eq!(
                    got[r][c], want[r][c],
                    "{source}: row {} column {}",
                    r + 1,
                    c + 1
                );
            }
            let text = t.text_rows();
            for row in if long { [8, 11, 14] } else { [8, 10, 12] } {
                assert!(text[row][3..].starts_with("Current password") || text[row][3..].starts_with("New password"));
            }
        }
    }
}
