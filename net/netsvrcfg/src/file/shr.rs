// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QAZLSSHR`: the shares. A share is its name record followed by its
//! attributes, until the next name record. The tags are
//! `docs/NETSERVER.md` 4.4's table.

use std::collections::BTreeMap;

use super::FileError;
use super::tlv::{self, Record, flag, int, put, put_flag, put_int, text};
use crate::model::share::{FileShare, Kind, Permission, PrintShare, Qualified, Share, SpoolType};

/// The file's magic.
pub const MAGIC: &[u8; 8] = b"QSZLSSHR";

/// The share tags.
pub mod tag {
    /// A share's name: starts the share.
    pub const NAME: u16 = 0x0100;
    /// `FILE` or `PRINT`.
    pub const TYPE: u16 = 0x0101;
    /// Text.
    pub const TEXT: u16 = 0x0102;
    /// Encryption required.
    pub const ENCRYPTION: u16 = 0x0103;
    /// Authorization list.
    pub const AUTL: u16 = 0x0104;
    /// File: the path.
    pub const PATH: u16 = 0x0110;
    /// File: the permission code.
    pub const PERMISSION: u16 = 0x0111;
    /// File: maximum users.
    pub const MAX_USERS: u16 = 0x0112;
    /// File: text CCSID.
    pub const TEXT_CCSID: u16 = 0x0113;
    /// File: text conversion.
    pub const TEXT_CONVERSION: u16 = 0x0114;
    /// File: one extension (one record each, in order).
    pub const EXTENSION: u16 = 0x0115;
    /// Print: output queue, `LIB/NAME`.
    pub const OUTQ: u16 = 0x0120;
    /// Print: printer driver.
    pub const DRIVER: u16 = 0x0121;
    /// Print: spooled file type code.
    pub const SPOOL_TYPE: u16 = 0x0122;
    /// Print: printer file, `LIB/NAME` or empty.
    pub const PRINTER_FILE: u16 = 0x0123;
    /// Print: publish.
    pub const PUBLISH: u16 = 0x0124;
}

fn qual(q: &Qualified) -> String {
    if q.is_empty() {
        String::new()
    } else {
        format!("{}/{}", q.library, q.name)
    }
}

fn unqual(t: u16, s: &str) -> Result<Qualified, FileError> {
    if s.is_empty() {
        return Ok(Qualified::default());
    }
    let (lib, name) = s.split_once('/').ok_or(FileError::Value(t))?;
    Ok(Qualified::new(name, lib))
}

fn share_records(s: &Share, out: &mut Vec<Record>) {
    out.push(put(tag::NAME, &s.name));
    out.push(put(
        tag::TYPE,
        match s.kind {
            Kind::File(_) => "FILE",
            Kind::Print(_) => "PRINT",
        },
    ));
    out.push(put(tag::TEXT, &s.text));
    out.push(put_flag(tag::ENCRYPTION, s.encryption_required));
    out.push(put(tag::AUTL, &s.autl));
    match &s.kind {
        Kind::File(f) => {
            out.push(put(tag::PATH, &f.path));
            out.push(put_int(tag::PERMISSION, f.permission.code()));
            out.push(put_int(tag::MAX_USERS, f.max_users));
            out.push(put_int(tag::TEXT_CCSID, f.text_ccsid));
            out.push(put_flag(tag::TEXT_CONVERSION, f.text_conversion));
            out.extend(f.extensions.iter().map(|e| put(tag::EXTENSION, e)));
        }
        Kind::Print(p) => {
            out.push(put(tag::OUTQ, &qual(&p.outq)));
            out.push(put(tag::DRIVER, &p.driver));
            out.push(put_int(tag::SPOOL_TYPE, p.spool_type.code()));
            out.push(put(tag::PRINTER_FILE, &qual(&p.printer_file)));
            out.push(put_flag(tag::PUBLISH, p.publish));
        }
    }
    out.extend(s.unknown.iter().cloned());
}

/// The file's bytes.
pub fn write(c: &crate::model::config::Config) -> Result<Vec<u8>, FileError> {
    let mut r: Vec<Record> = c.unknown_shares.clone();
    for s in c.shares.values() {
        share_records(s, &mut r);
    }
    tlv::write(MAGIC, &r)
}

/// A share being read: the common records, then its kind's.
struct Building {
    share: Share,
    file: FileShare,
    print: PrintShare,
    is_print: bool,
}

impl Building {
    fn new(name: String) -> Self {
        Self {
            share: Share {
                name,
                text: String::new(),
                encryption_required: false,
                autl: String::new(),
                kind: Kind::File(FileShare {
                    path: String::new(),
                    permission: Permission::ReadWrite,
                    max_users: -1,
                    text_ccsid: 0,
                    text_conversion: false,
                    extensions: Vec::new(),
                }),
                unknown: Vec::new(),
            },
            file: FileShare {
                path: String::new(),
                permission: Permission::ReadWrite,
                max_users: -1,
                text_ccsid: 0,
                text_conversion: false,
                extensions: Vec::new(),
            },
            print: PrintShare {
                outq: Qualified::default(),
                driver: String::new(),
                spool_type: SpoolType::Auto,
                printer_file: Qualified::default(),
                publish: false,
            },
            is_print: false,
        }
    }

    fn done(mut self) -> Share {
        self.share.kind = if self.is_print {
            Kind::Print(self.print)
        } else {
            Kind::File(self.file)
        };
        self.share
    }

    fn apply(&mut self, t: u16, v: Vec<u8>) -> Result<(), FileError> {
        match t {
            tag::TYPE => {
                self.is_print = match text(t, &v)?.as_str() {
                    "FILE" => false,
                    "PRINT" => true,
                    _ => return Err(FileError::Value(t)),
                };
            }
            tag::TEXT => self.share.text = text(t, &v)?,
            tag::ENCRYPTION => self.share.encryption_required = flag(t, &v)?,
            tag::AUTL => self.share.autl = text(t, &v)?,
            tag::PATH => self.file.path = text(t, &v)?,
            tag::PERMISSION => {
                self.file.permission =
                    Permission::from_code(int(t, &v)?).ok_or(FileError::Value(t))?;
            }
            tag::MAX_USERS => self.file.max_users = int(t, &v)?,
            tag::TEXT_CCSID => self.file.text_ccsid = int(t, &v)?,
            tag::TEXT_CONVERSION => self.file.text_conversion = flag(t, &v)?,
            tag::EXTENSION => self.file.extensions.push(text(t, &v)?),
            tag::OUTQ => self.print.outq = unqual(t, &text(t, &v)?)?,
            tag::DRIVER => self.print.driver = text(t, &v)?,
            tag::SPOOL_TYPE => {
                self.print.spool_type =
                    SpoolType::from_code(int(t, &v)?).ok_or(FileError::Value(t))?;
            }
            tag::PRINTER_FILE => self.print.printer_file = unqual(t, &text(t, &v)?)?,
            tag::PUBLISH => self.print.publish = flag(t, &v)?,
            _ => self.share.unknown.push((t, v)),
        }
        Ok(())
    }
}

/// The shares and the records before the first share.
pub fn read(b: &[u8]) -> Result<(BTreeMap<String, Share>, Vec<Record>), FileError> {
    let mut shares = BTreeMap::new();
    let mut before = Vec::new();
    let mut at: Option<Building> = None;
    for (t, v) in tlv::read(MAGIC, b)? {
        if t == tag::NAME {
            if let Some(s) = at.take() {
                let s = s.done();
                shares.insert(s.name.clone(), s);
            }
            at = Some(Building::new(text(t, &v)?));
            continue;
        }
        match at.as_mut() {
            Some(s) => s.apply(t, v)?,
            None if (tag::NAME..=0x01FF).contains(&t) => return Err(FileError::Orphan(t)),
            None => before.push((t, v)),
        }
    }
    if let Some(s) = at {
        let s = s.done();
        shares.insert(s.name.clone(), s);
    }
    Ok((shares, before))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config::Config;

    #[test]
    fn file_and_print_shares_round_trip() {
        let mut c = Config::shipped("QS400");
        let mut f = Share::file("DOCS", "/home/docs", "Documents");
        if let Kind::File(fs) = &mut f.kind {
            fs.permission = Permission::ReadOnly;
            fs.max_users = 10;
            fs.text_conversion = true;
            fs.extensions = vec!["TXT".into(), "MBR".into()];
        }
        f.encryption_required = true;
        f.autl = "DOCAUTL".into();
        f.unknown.push((0x01F0, b"x".to_vec()));
        let mut p = Share::print("LASER", Qualified::new("PRT01", "QUSRSYS"), "Laser");
        if let Kind::Print(ps) = &mut p.kind {
            ps.driver = "HP LaserJet 4".into();
            ps.spool_type = SpoolType::UserAscii;
            ps.printer_file = Qualified::new("QPNPSPRTF", "QSYS");
        }
        c.shares.insert(f.name.clone(), f);
        c.shares.insert(p.name.clone(), p);
        c.unknown_shares.push((0x0700, b"y".to_vec()));
        let b = write(&c).expect("writes");
        let (s, u) = read(&b).expect("reads");
        assert_eq!(s, c.shares);
        assert_eq!(u, c.unknown_shares);
    }

    #[test]
    fn a_share_record_before_any_share_is_an_error() {
        let b = tlv::write(MAGIC, &[put(tag::TEXT, "x")]).expect("writes");
        assert_eq!(read(&b), Err(FileError::Orphan(tag::TEXT)));
    }
}
