// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The outbound data stream, parsed into commands and orders. The
//! terminal applies the result; the trace prints it.

use super::{ESC, cmd, order};
use crate::Error;

/// An order of Write To Display (and of Write Error Code).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Order {
    /// Set Buffer Address, 1-based row and column.
    Sba(u8, u8),
    /// Insert Cursor.
    Ic(u8, u8),
    /// Move Cursor.
    Mc(u8, u8),
    /// Repeat the byte up to and including the address.
    Ra(u8, u8, u8),
    /// Erase up to and including the address; the attribute types.
    Ea(u8, u8, Vec<u8>),
    /// Transparent data.
    Td(Vec<u8>),
    /// Write Extended Attribute: type and value.
    Wea(u8, u8),
    /// Start of Header: the bytes after the length.
    Soh(Vec<u8>),
    /// Start of Field.
    Sf {
        /// The Field Format Word.
        ffw: u16,
        /// The Field Control Words.
        fcws: Vec<u16>,
        /// The attribute.
        attr: u8,
        /// The length.
        len: u16,
    },
    /// Characters and attributes written at the buffer address.
    Data(Vec<u8>),
}

/// A command of the outbound stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Clear Unit.
    ClearUnit,
    /// Clear Unit Alternate with its parameter.
    ClearUnitAlt(u8),
    /// Clear Format Table.
    ClearFormatTable,
    /// Write To Display.
    Wtd {
        /// Control character 1.
        cc1: u8,
        /// Control character 2.
        cc2: u8,
        /// The orders.
        orders: Vec<Order>,
    },
    /// Write Error Code.
    WriteErrorCode(Vec<Order>),
    /// Write Error Code To Window: the two parameter bytes, the orders.
    WriteErrorCodeWindow(u8, u8, Vec<Order>),
    /// Read Input Fields.
    ReadInputFields(u8, u8),
    /// Read MDT Fields.
    ReadMdtFields(u8, u8),
    /// Read MDT Fields Alternate.
    ReadMdtFieldsAlt(u8, u8),
    /// Read Screen (Immediate); the variant byte.
    ReadScreen(u8),
    /// Read Immediate.
    ReadImmediate,
    /// Read Modified Immediate Alternate.
    ReadImmediateAlt,
    /// Save Screen.
    SaveScreen,
    /// Save Partial Screen with its parameters.
    SavePartialScreen(Vec<u8>),
    /// Restore Screen: the commands that follow do the work.
    RestoreScreen,
    /// Restore Partial Screen.
    RestorePartialScreen,
    /// Roll.
    Roll {
        /// Up (toward row 1) or down.
        up: bool,
        /// Lines.
        lines: u8,
        /// First row of the rolled area, 1-based.
        top: u8,
        /// Last row of the rolled area, 1-based.
        bottom: u8,
    },
    /// Write Structured Field: the whole field, length bytes included.
    Wsf(Vec<u8>),
    /// Copy To Printer.
    CopyToPrinter(u8),
}

struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn byte(&mut self) -> Result<u8, Error> {
        let b = *self
            .data
            .get(self.at)
            .ok_or(Error::Truncated { at: self.at })?;
        self.at += 1;
        Ok(b)
    }

    fn bytes(&mut self, n: usize) -> Result<Vec<u8>, Error> {
        if self.at + n > self.data.len() {
            return Err(Error::Truncated {
                at: self.data.len(),
            });
        }
        let out = self.data[self.at..self.at + n].to_vec();
        self.at += n;
        Ok(out)
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.at).copied()
    }

    fn done(&self) -> bool {
        self.at >= self.data.len()
    }
}

/// Parse the data of one record.
pub fn parse(data: &[u8]) -> Result<Vec<Command>, Error> {
    let mut c = Cursor { data, at: 0 };
    let mut out = Vec::new();
    while !c.done() {
        let b = c.byte()?;
        if b != ESC {
            // Data outside a command: the stream is continuing a WTD
            // that an earlier record started, or is malformed. Keep it
            // as a WTD without control characters.
            c.at -= 1;
            let orders = orders(&mut c)?;
            out.push(Command::Wtd {
                cc1: 0,
                cc2: 0,
                orders,
            });
            continue;
        }
        let command = c.byte()?;
        out.push(match command {
            cmd::CLEAR_UNIT => Command::ClearUnit,
            cmd::CLEAR_UNIT_ALT => Command::ClearUnitAlt(c.byte()?),
            cmd::CLEAR_FORMAT_TABLE => Command::ClearFormatTable,
            cmd::WRITE_TO_DISPLAY => {
                let cc1 = c.byte()?;
                let cc2 = c.byte()?;
                Command::Wtd {
                    cc1,
                    cc2,
                    orders: orders(&mut c)?,
                }
            }
            cmd::WRITE_ERROR_CODE => Command::WriteErrorCode(orders(&mut c)?),
            cmd::WRITE_ERROR_CODE_WINDOW => {
                let a = c.byte()?;
                let b = c.byte()?;
                Command::WriteErrorCodeWindow(a, b, orders(&mut c)?)
            }
            cmd::READ_INPUT_FIELDS => {
                let a = c.byte()?;
                Command::ReadInputFields(a, c.byte()?)
            }
            cmd::READ_MDT_FIELDS => {
                let a = c.byte()?;
                Command::ReadMdtFields(a, c.byte()?)
            }
            cmd::READ_MDT_FIELDS_ALT => {
                let a = c.byte()?;
                Command::ReadMdtFieldsAlt(a, c.byte()?)
            }
            cmd::READ_SCREEN
            | cmd::READ_SCREEN_EXT
            | cmd::READ_SCREEN_PRINT
            | cmd::READ_SCREEN_PRINT_EXT => Command::ReadScreen(command),
            cmd::READ_IMMEDIATE => Command::ReadImmediate,
            cmd::READ_IMMEDIATE_ALT => Command::ReadImmediateAlt,
            cmd::SAVE_SCREEN => Command::SaveScreen,
            cmd::SAVE_PARTIAL_SCREEN => Command::SavePartialScreen(c.bytes(3)?),
            cmd::RESTORE_SCREEN => Command::RestoreScreen,
            cmd::RESTORE_PARTIAL_SCREEN => Command::RestorePartialScreen,
            cmd::ROLL => {
                let d = c.byte()?;
                let top = c.byte()?;
                let bottom = c.byte()?;
                Command::Roll {
                    up: d & 0x80 != 0,
                    lines: d & 0x1F,
                    top,
                    bottom,
                }
            }
            cmd::WRITE_STRUCTURED_FIELD => {
                let hi = c.byte()?;
                let lo = c.byte()?;
                let len = usize::from(u16::from_be_bytes([hi, lo]));
                let mut sf = vec![hi, lo];
                sf.extend(c.bytes(len.saturating_sub(2))?);
                Command::Wsf(sf)
            }
            cmd::COPY_TO_PRINTER => Command::CopyToPrinter(c.byte()?),
            other => return Err(Error::Command(other)),
        });
    }
    Ok(out)
}

/// Orders up to the next `ESC` or the end of the data.
fn orders(c: &mut Cursor<'_>) -> Result<Vec<Order>, Error> {
    let mut out = Vec::new();
    let mut data = Vec::new();
    let flush = |data: &mut Vec<u8>, out: &mut Vec<Order>| {
        if !data.is_empty() {
            out.push(Order::Data(std::mem::take(data)));
        }
    };
    while let Some(b) = c.peek() {
        if b == ESC {
            break;
        }
        match b {
            order::SBA | order::IC | order::MC => {
                c.at += 1;
                flush(&mut data, &mut out);
                let r = c.byte()?;
                let col = c.byte()?;
                out.push(match b {
                    order::SBA => Order::Sba(r, col),
                    order::IC => Order::Ic(r, col),
                    _ => Order::Mc(r, col),
                });
            }
            order::RA => {
                c.at += 1;
                flush(&mut data, &mut out);
                let r = c.byte()?;
                let col = c.byte()?;
                out.push(Order::Ra(r, col, c.byte()?));
            }
            order::EA => {
                c.at += 1;
                flush(&mut data, &mut out);
                let r = c.byte()?;
                let col = c.byte()?;
                let len = usize::from(c.byte()?);
                let types = c.bytes(len.saturating_sub(1))?;
                out.push(Order::Ea(r, col, types));
            }
            order::TD => {
                c.at += 1;
                flush(&mut data, &mut out);
                let hi = c.byte()?;
                let lo = c.byte()?;
                let len = usize::from(u16::from_be_bytes([hi, lo]));
                out.push(Order::Td(c.bytes(len)?));
            }
            order::WEA => {
                c.at += 1;
                flush(&mut data, &mut out);
                let t = c.byte()?;
                out.push(Order::Wea(t, c.byte()?));
            }
            order::SOH => {
                c.at += 1;
                flush(&mut data, &mut out);
                let len = usize::from(c.byte()?);
                out.push(Order::Soh(c.bytes(len)?));
            }
            order::SF => {
                c.at += 1;
                flush(&mut data, &mut out);
                let mut fcws = Vec::new();
                let mut ffw = 0u16;
                let attr;
                loop {
                    let hi = c.byte()?;
                    if super::field::Ffw::is_ffw(hi) {
                        ffw = u16::from_be_bytes([hi, c.byte()?]);
                    } else if super::field::is_fcw(hi) {
                        fcws.push(u16::from_be_bytes([hi, c.byte()?]));
                    } else {
                        attr = hi;
                        break;
                    }
                }
                let hi = c.byte()?;
                let lo = c.byte()?;
                out.push(Order::Sf {
                    ffw,
                    fcws,
                    attr,
                    len: u16::from_be_bytes([hi, lo]),
                });
            }
            _ => {
                c.at += 1;
                data.push(b);
            }
        }
    }
    flush(&mut data, &mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_sign_on_shaped_stream() {
        let data = [
            0x04, 0x40, // clear unit
            0x04, 0x11, 0x00, 0x20, // wtd
            0x01, 0x05, 0x00, 0x00, 0x00, 0x18, 0x00, // soh
            0x11, 0x01, 0x21, 0xE2, 0x89, // sba, "Si"
            0x1D, 0x40, 0x20, 0x24, 0x00, 0x0A, // sf, input, len 10
            0x13, 0x01, 0x30, // ic
            0x04, 0x52, 0x00, 0x00, // read mdt fields
        ];
        let cmds = parse(&data).unwrap();
        assert_eq!(cmds.len(), 3);
        match &cmds[1] {
            Command::Wtd { cc1, cc2, orders } => {
                assert_eq!((*cc1, *cc2), (0x00, 0x20));
                assert_eq!(orders.len(), 5);
                assert_eq!(orders[2], Order::Data(vec![0xE2, 0x89]));
                assert!(matches!(
                    orders[3],
                    Order::Sf {
                        ffw: 0x4020,
                        attr: 0x24,
                        len: 10,
                        ..
                    }
                ));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(cmds[2], Command::ReadMdtFields(0, 0));
    }
}
