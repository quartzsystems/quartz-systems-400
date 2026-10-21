// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*SPLF`: a spooled file, an object in `QSPL` named `SPnnnnnnnn` after
//! its spool identifier. Its space holds the attributes (the `SPLA0100`
//! and `SPLA0200` set) at offset 0 and the SCS data after them, which
//! grows in place as the file is written.

use std::fmt::Write as _;

use ipc::{wire_enum, wire_struct};
use store::obj::ObjectId;
use store::obj::space::Space;
use store::txn::Txn;

use crate::cmd::Params;
use crate::rec::{Layout, Rec, field};
use crate::{Error, Result};

/// The attributes.
pub const LAYOUT: Layout = Layout {
    name: "SpooledFile",
    size: Some(512),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("Status", 10, 1),
        field("Schedule", 11, 1),
        field("Id", 12, 4),
        field("Number", 16, 4),
        field("JobNumber", 20, 4),
        field("Pages", 24, 4),
        field("Records", 28, 4),
        field("DataLen", 32, 8),
        field("Created", 40, 8),
        field("Expires", 48, 4),
        field("LastPage", 52, 4),
        field("Copies", 56, 2),
        field("OutPty", 58, 1),
        field("Save", 59, 1),
        field("PageLen", 60, 2),
        field("PageWidth", 62, 2),
        field("Lpi", 64, 1),
        field("Cpi", 65, 1),
        field("Ovrflw", 66, 2),
        field("Drawer", 68, 1),
        field("Duplex", 69, 1),
        field("Align", 70, 1),
        field("PrtQlty", 71, 1),
        field("CopiesLeft", 72, 2),
        field("HeldFrom", 74, 1),
        field("Reserved", 75, 1),
        field("Name", 76, 10),
        field("JobName", 86, 10),
        field("JobUser", 96, 10),
        field("UsrDta", 106, 10),
        field("FormType", 116, 10),
        field("OutqName", 126, 10),
        field("OutqLib", 136, 10),
        field("DevType", 146, 10),
        field("Font", 156, 10),
        field("PrtfName", 166, 10),
        field("PrtfLib", 176, 10),
        field("Writer", 186, 10),
        field("UsrDfnDta", 196, 255),
        field("Reserved2", 451, 61),
    ],
};

/// Where the data starts.
pub const DATA_OFF: u64 = 512;

const MAGIC: &[u8; 8] = b"QSWMSPLF";
const VERSION: u64 = 1;

/// A spooled file's status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// `RDY`: waiting for a writer.
    #[default]
    Rdy,
    /// `HLD`: held.
    Hld,
    /// `SAV`: printed and kept.
    Sav,
    /// `WTR`: being printed.
    Wtr,
    /// `OPN`: still being written.
    Opn,
    /// `CLO`: closed, held back by `SCHEDULE(*JOBEND)`.
    Clo,
    /// `PND`: waiting to be handed to a writer.
    Pnd,
    /// `DFR`: deferred.
    Dfr,
    /// `MSGW`: the writer waits on a message.
    Msgw,
}

wire_enum!(Status {
    0 => Rdy,
    1 => Hld,
    2 => Sav,
    3 => Wtr,
    4 => Opn,
    5 => Clo,
    6 => Pnd,
    7 => Dfr,
    8 => Msgw,
});

const STATUSES: [(Status, &str); 9] = [
    (Status::Rdy, "RDY"),
    (Status::Hld, "HLD"),
    (Status::Sav, "SAV"),
    (Status::Wtr, "WTR"),
    (Status::Opn, "OPN"),
    (Status::Clo, "CLO"),
    (Status::Pnd, "PND"),
    (Status::Dfr, "DFR"),
    (Status::Msgw, "MSGW"),
];

impl Status {
    /// The three or four letters.
    #[must_use]
    pub fn code(self) -> &'static str {
        STATUSES
            .iter()
            .find(|(s, _)| *s == self)
            .map_or("RDY", |(_, c)| c)
    }

    fn number(self) -> u64 {
        STATUSES.iter().position(|(s, _)| *s == self).unwrap_or(0) as u64
    }

    fn from_number(n: u64) -> Self {
        usize::try_from(n)
            .ok()
            .and_then(|i| STATUSES.get(i))
            .map_or(Self::Rdy, |(s, _)| *s)
    }
}

/// `SCHEDULE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Schedule {
    /// `*IMMED`: printable while still open.
    Immed,
    /// `*FILEEND`: printable once closed.
    #[default]
    FileEnd,
    /// `*JOBEND`: printable once the job ends.
    JobEnd,
}

wire_enum!(Schedule {
    0 => Immed,
    1 => FileEnd,
    2 => JobEnd,
});

impl Schedule {
    /// From `*IMMED`, `*FILEEND`, `*JOBEND`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "*IMMED" => Self::Immed,
            "*FILEEND" => Self::FileEnd,
            "*JOBEND" => Self::JobEnd,
            _ => return None,
        })
    }

    /// The special value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Immed => "*IMMED",
            Self::FileEnd => "*FILEEND",
            Self::JobEnd => "*JOBEND",
        }
    }
}

/// A spooled file's attributes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SplfAttrs {
    /// The spool identifier, unique on the system.
    pub id: u32,
    /// The file's number in its job.
    pub number: u32,
    /// The file name (the printer file's, or `SPLFNAME`).
    pub name: String,
    /// The job: name, user, number.
    pub job: (String, String, u32),
    /// `USRDTA`.
    pub usrdta: String,
    /// Status.
    pub status: Status,
    /// The status to go back to on release.
    pub held_from: Status,
    /// `SCHEDULE`.
    pub schedule: Schedule,
    /// Pages written.
    pub pages: u32,
    /// Records (lines) written.
    pub records: u32,
    /// Bytes of SCS data.
    pub data_len: u64,
    /// Created, microseconds since the epoch.
    pub created: u64,
    /// `EXPDATE` as `YYYYMMDD`; 0 for `*NONE`.
    pub expires: u32,
    /// The last page a writer printed.
    pub last_page: u32,
    /// `COPIES`.
    pub copies: u16,
    /// Copies still to print.
    pub copies_left: u16,
    /// `OUTPTY`.
    pub outpty: u8,
    /// `SAVE`.
    pub save: bool,
    /// `PAGESIZE` length, lines.
    pub page_len: u16,
    /// `PAGESIZE` width, positions.
    pub page_width: u16,
    /// `LPI`.
    pub lpi: u8,
    /// `CPI`.
    pub cpi: u8,
    /// `OVRFLW`.
    pub ovrflw: u16,
    /// `DRAWER`.
    pub drawer: u8,
    /// `DUPLEX`: 0 `*NO`, 1 `*YES`, 2 `*TUMBLE`.
    pub duplex: u8,
    /// `ALIGN`.
    pub align: bool,
    /// `PRTQLTY`: 0 `*STD`, 1 `*DRAFT`, 2 `*NLQ`, 3 `*DEVD`.
    pub prtqlty: u8,
    /// `FORMTYPE`.
    pub formtype: String,
    /// The output queue, `lib/name`.
    pub outq: String,
    /// `DEVTYPE`: `*SCS`.
    pub devtype: String,
    /// `FONT`.
    pub font: String,
    /// The printer file it came from, `lib/name`.
    pub prtf: String,
    /// The writer printing it, blank when none.
    pub writer: String,
    /// `USRDFNDTA`.
    pub usrdfndta: String,
}

wire_struct!(SplfAttrs {
    id,
    number,
    name,
    job,
    usrdta,
    status,
    held_from,
    schedule,
    pages,
    records,
    data_len,
    created,
    expires,
    last_page,
    copies,
    copies_left,
    outpty,
    save,
    page_len,
    page_width,
    lpi,
    cpi,
    ovrflw,
    drawer,
    duplex,
    align,
    prtqlty,
    formtype,
    outq,
    devtype,
    font,
    prtf,
    writer,
    usrdfndta
});

fn split_q(q: &str) -> (&str, &str) {
    q.split_once('/').unwrap_or(("", q))
}

fn join_q(lib: &str, name: &str) -> String {
    if name.is_empty() {
        String::new()
    } else {
        format!("{lib}/{name}")
    }
}

const QUALITIES: [&str; 4] = ["*STD", "*DRAFT", "*NLQ", "*DEVD"];

impl SplfAttrs {
    /// The object name in `QSPL`.
    #[must_use]
    pub fn object_name(id: u32) -> String {
        format!("SP{id:08}")
    }

    /// `number/user/name` of the job.
    #[must_use]
    pub fn job_text(&self) -> String {
        format!("{:06}/{}/{}", self.job.2, self.job.1, self.job.0)
    }

    /// The attribute record.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_uint("Status", self.status.number());
        r.set_uint("HeldFrom", self.held_from.number());
        r.set_uint(
            "Schedule",
            match self.schedule {
                Schedule::Immed => 0,
                Schedule::FileEnd => 1,
                Schedule::JobEnd => 2,
            },
        );
        r.set_uint("Id", u64::from(self.id));
        r.set_uint("Number", u64::from(self.number));
        r.set_uint("JobNumber", u64::from(self.job.2));
        r.set_uint("Pages", u64::from(self.pages));
        r.set_uint("Records", u64::from(self.records));
        r.set_uint("DataLen", self.data_len);
        r.set_uint("Created", self.created);
        r.set_uint("Expires", u64::from(self.expires));
        r.set_uint("LastPage", u64::from(self.last_page));
        r.set_uint("Copies", u64::from(self.copies));
        r.set_uint("CopiesLeft", u64::from(self.copies_left));
        r.set_uint("OutPty", u64::from(self.outpty));
        r.set_flag("Save", self.save);
        r.set_uint("PageLen", u64::from(self.page_len));
        r.set_uint("PageWidth", u64::from(self.page_width));
        r.set_uint("Lpi", u64::from(self.lpi));
        r.set_uint("Cpi", u64::from(self.cpi));
        r.set_uint("Ovrflw", u64::from(self.ovrflw));
        r.set_uint("Drawer", u64::from(self.drawer));
        r.set_uint("Duplex", u64::from(self.duplex));
        r.set_flag("Align", self.align);
        r.set_uint("PrtQlty", u64::from(self.prtqlty));
        r.set_text("Name", &self.name);
        r.set_text("JobName", &self.job.0);
        r.set_text("JobUser", &self.job.1);
        r.set_text("UsrDta", &self.usrdta);
        r.set_text("FormType", &self.formtype);
        let (ol, on) = split_q(&self.outq);
        r.set_text("OutqName", on);
        r.set_text("OutqLib", ol);
        r.set_text("DevType", &self.devtype);
        r.set_text("Font", &self.font);
        let (pl, pn) = split_q(&self.prtf);
        r.set_text("PrtfName", pn);
        r.set_text("PrtfLib", pl);
        r.set_text("Writer", &self.writer);
        r.set_text("UsrDfnDta", &self.usrdfndta);
        r.buf
    }

    /// Read an attribute record.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        if r.bytes("Magic") != MAGIC || r.uint("Version") != VERSION {
            return Err(Error::Corrupt("not a spooled file".into()));
        }
        let small = |f: &str| u8::try_from(r.uint(f)).unwrap_or(0);
        let half = |f: &str| u16::try_from(r.uint(f)).unwrap_or(0);
        let word = |f: &str| u32::try_from(r.uint(f)).unwrap_or(0);
        Ok(Self {
            id: word("Id"),
            number: word("Number"),
            name: r.text("Name"),
            job: (r.text("JobName"), r.text("JobUser"), word("JobNumber")),
            usrdta: r.text("UsrDta"),
            status: Status::from_number(r.uint("Status")),
            held_from: Status::from_number(r.uint("HeldFrom")),
            schedule: match r.uint("Schedule") {
                0 => Schedule::Immed,
                2 => Schedule::JobEnd,
                _ => Schedule::FileEnd,
            },
            pages: word("Pages"),
            records: word("Records"),
            data_len: r.uint("DataLen"),
            created: r.uint("Created"),
            expires: word("Expires"),
            last_page: word("LastPage"),
            copies: half("Copies"),
            copies_left: half("CopiesLeft"),
            outpty: small("OutPty"),
            save: r.flag("Save"),
            page_len: half("PageLen"),
            page_width: half("PageWidth"),
            lpi: small("Lpi"),
            cpi: small("Cpi"),
            ovrflw: half("Ovrflw"),
            drawer: small("Drawer"),
            duplex: small("Duplex"),
            align: r.flag("Align"),
            prtqlty: small("PrtQlty"),
            formtype: r.text("FormType"),
            outq: join_q(&r.text("OutqLib"), &r.text("OutqName")),
            devtype: r.text("DevType"),
            font: r.text("Font"),
            prtf: join_q(&r.text("PrtfLib"), &r.text("PrtfName")),
            writer: r.text("Writer"),
            usrdfndta: r.text("UsrDfnDta"),
        })
    }

    /// `CHGSPLFA` keywords. `OUTQ` is left to the caller, which moves
    /// the file between queues.
    pub fn apply(&mut self, p: &Params) -> Result<()> {
        let bad = |k: &str, v: &str| Error::value("CPD0084", format!("{k}({v}) is not valid"));
        for (k, raw) in &p.kw {
            let v = raw.trim();
            let up = v.to_ascii_uppercase();
            let num = |max: u32| -> Result<u32> {
                up.parse()
                    .ok()
                    .filter(|n| (1..=max).contains(n))
                    .ok_or_else(|| bad(k, v))
            };
            match k.as_str() {
                "FILE" | "JOB" | "SPLNBR" | "OUTQ" => {}
                "COPIES" => self.copies = u16::try_from(num(255)?).unwrap_or(1),
                "OUTPTY" => self.outpty = u8::try_from(num(9)?).unwrap_or(5),
                "FORMTYPE" => self.formtype = up,
                "USRDTA" => self.usrdta = v.trim_matches('\'').to_string(),
                "SAVE" => self.save = up == "*YES",
                "DRAWER" => self.drawer = u8::try_from(num(255)?).unwrap_or(1),
                "DUPLEX" => {
                    self.duplex = match up.as_str() {
                        "*NO" => 0,
                        "*YES" => 1,
                        "*TUMBLE" => 2,
                        _ => return Err(bad(k, v)),
                    };
                }
                "PRTQLTY" => {
                    let i = QUALITIES
                        .iter()
                        .position(|q| *q == up)
                        .ok_or_else(|| bad(k, v))?;
                    self.prtqlty = u8::try_from(i).unwrap_or(0);
                }
                "EXPDATE" => {
                    self.expires = if up == "*NONE" {
                        0
                    } else {
                        up.parse()
                            .ok()
                            .filter(|_| up.len() == 8)
                            .ok_or_else(|| bad(k, v))?
                    };
                }
                "USRDFNDTA" => self.usrdfndta = v.trim_matches('\'').to_string(),
                _ => {
                    return Err(Error::value(
                        "CPD0043",
                        format!("keyword {k} not valid for this command"),
                    ));
                }
            }
        }
        Ok(())
    }

    /// `WRKSPLFA` as text.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut out = String::new();
        let mut row = |label: &str, value: &str| {
            let dots = ". ".repeat(14);
            let pad = format!("{label} {dots}");
            let _ = writeln!(out, "{} : {value}", &pad[..28]);
        };
        row("File", &self.name);
        row("Job", &self.job.0);
        row("User", &self.job.1);
        row("Number", &format!("{:06}", self.job.2));
        row("File number", &self.number.to_string());
        row("Status", self.status.code());
        row("Output queue", &self.outq);
        row("Output priority", &self.outpty.to_string());
        row("Total pages", &self.pages.to_string());
        row("Records", &self.records.to_string());
        row("Copies", &self.copies.to_string());
        row("Form type", &self.formtype);
        row("User data", &self.usrdta);
        row("Device type", &self.devtype);
        row(
            "Page size",
            &format!("{} by {}", self.page_len, self.page_width),
        );
        row("Lines per inch", &self.lpi.to_string());
        row("Characters per inch", &self.cpi.to_string());
        row("Overflow line", &self.ovrflw.to_string());
        row("Schedule", self.schedule.name());
        row("Save", if self.save { "*YES" } else { "*NO" });
        row("Printer file", &self.prtf);
        row(
            "Print quality",
            QUALITIES
                .get(usize::from(self.prtqlty))
                .copied()
                .unwrap_or("*STD"),
        );
        row(
            "Expiration date",
            &if self.expires == 0 {
                "*NONE".into()
            } else {
                self.expires.to_string()
            },
        );
        out
    }
}

/// Write a new spooled file's attributes into its object.
pub fn init(txn: &mut Txn<'_>, id: ObjectId, attrs: &SplfAttrs) -> Result<()> {
    let space = Space::ensure(txn, id, DATA_OFF + 4096, 0)?;
    space.write(txn, 0, &attrs.encode())?;
    Ok(())
}

/// A spooled file's attributes.
pub fn read_attrs(txn: &mut Txn<'_>, id: ObjectId) -> Result<SplfAttrs> {
    let (space, _) = Space::load(txn, id)?;
    SplfAttrs::decode(&space.read(txn, 0, 512)?)
}

/// Replace a spooled file's attributes.
pub fn write_attrs(txn: &mut Txn<'_>, id: ObjectId, attrs: &SplfAttrs) -> Result<()> {
    let (space, _) = Space::load(txn, id)?;
    space.write(txn, 0, &attrs.encode())?;
    Ok(())
}

/// Append SCS data; the attributes' length, pages and records follow.
pub fn append(
    txn: &mut Txn<'_>,
    id: ObjectId,
    attrs: &mut SplfAttrs,
    data: &[u8],
    pages: u32,
    records: u32,
) -> Result<()> {
    let (space, _) = Space::load(txn, id)?;
    let end = DATA_OFF + attrs.data_len + data.len() as u64;
    // Grow by doubling so a file written a line at a time stays cheap.
    let space = if end > space.bytes() {
        Space::ensure(txn, id, end.max(space.bytes() * 2), 0)?
    } else {
        space
    };
    space.write(txn, DATA_OFF + attrs.data_len, data)?;
    attrs.data_len += data.len() as u64;
    attrs.pages = pages;
    attrs.records = records;
    space.write(txn, 0, &attrs.encode())?;
    Ok(())
}

/// A spooled file's SCS data.
pub fn read_data(txn: &mut Txn<'_>, id: ObjectId, attrs: &SplfAttrs) -> Result<Vec<u8>> {
    let (space, _) = Space::load(txn, id)?;
    let n = usize::try_from(attrs.data_len).map_err(|_| Error::Corrupt("data length".into()))?;
    Ok(space.read(txn, DATA_OFF, n)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let a = SplfAttrs {
            id: 7,
            number: 2,
            name: "QSYSPRT".into(),
            job: ("WMJOB".into(), "QSECOFR".into(), 42),
            status: Status::Hld,
            held_from: Status::Rdy,
            schedule: Schedule::JobEnd,
            pages: 3,
            records: 100,
            data_len: 4096,
            copies: 1,
            outpty: 5,
            page_len: 66,
            page_width: 132,
            lpi: 6,
            cpi: 10,
            ovrflw: 60,
            prtqlty: 1,
            outq: "QGPL/QPRINT".into(),
            devtype: "*SCS".into(),
            prtf: "QSYS/QSYSPRT".into(),
            ..SplfAttrs::default()
        };
        assert_eq!(SplfAttrs::decode(&a.encode()).unwrap(), a);
        assert_eq!(SplfAttrs::object_name(7), "SP00000007");
        assert_eq!(a.job_text(), "000042/QSECOFR/WMJOB");
        let mut b = a.clone();
        b.apply(&Params::parse("x COPIES(3) PRTQLTY(*NLQ) SAVE(*YES)"))
            .unwrap();
        assert_eq!((b.copies, b.prtqlty, b.save), (3, 2, true));
        assert!(b.describe().contains("*NLQ"));
    }
}
