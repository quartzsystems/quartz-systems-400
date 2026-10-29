// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*OUTQ`: spooled files waiting to print, in queue order, and the
//! queue's attributes. The files themselves are `*SPLF` objects in
//! `QSPL`; the queue holds their spool identifiers. A remote queue
//! (`RMTSYS`) is printed by a remote writer (`STRRMTWTR`) over LPR; its
//! attributes beyond the system and printer queue are the version-2
//! section [`super::outqrmt`].

use std::fmt::Write as _;

use ipc::{wire_enum, wire_struct};

use super::outqrmt::RemoteQ;
use crate::cmd::Params;
use crate::rec::{Layout, Rec, field};
use crate::{Error, Result};

/// The fixed part.
pub const LAYOUT: Layout = Layout {
    name: "OutputQueue",
    size: Some(256),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("Held", 10, 1),
        field("Seq", 11, 1),
        field("Count", 12, 4),
        field("MaxPages", 16, 4),
        field("AutoStrWtr", 20, 2),
        field("DspDta", 22, 1),
        field("OprCtl", 23, 1),
        field("AutChk", 24, 10),
        field("Writer", 34, 10),
        field("CnnType", 44, 1),
        field("Reserved", 45, 3),
        field("Text", 48, 50),
        field("Reserved2", 98, 30),
        field("RmtSys", 128, 64),
        field("RmtPrtQ", 192, 64),
    ],
};

const MAGIC: &[u8; 8] = b"QSWMOUTQ";
const VERSION: u64 = 2;

/// `SEQ`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Seq {
    /// `*FIFO`: in arrival order.
    #[default]
    Fifo,
    /// `*JOBNBR`: by the creating job's number, then arrival.
    JobNbr,
}

wire_enum!(Seq {
    0 => Fifo,
    1 => JobNbr,
});

/// An output queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutQ {
    /// `HLDOUTQ`.
    pub held: bool,
    /// `SEQ`.
    pub seq: Seq,
    /// `MAXPAGES`; 0 is `*NONE`.
    pub maxpages: u32,
    /// `AUTOSTRWTR`; 0 is `*NONE`.
    pub autostrwtr: u16,
    /// `DSPDTA`: `*NO`, `*YES`, `*OWNER`.
    pub dspdta: String,
    /// `OPRCTL`.
    pub oprctl: bool,
    /// `AUTCHK`: `*OWNER`, `*DTAAUT`.
    pub autchk: String,
    /// The writer started to it, blank for none.
    pub writer: String,
    /// `CNNTYPE`: 0 `*IP`, 1 `*SNA`, 2 `*USRDFN`.
    pub cnntype: u8,
    /// `TEXT`.
    pub text: String,
    /// `RMTSYS`.
    pub rmtsys: String,
    /// `RMTPRTQ`.
    pub rmtprtq: String,
    /// The remote section.
    pub remote: RemoteQ,
    /// The spooled files on it, by spool identifier, in queue order.
    pub files: Vec<u32>,
}

wire_struct!(OutQ {
    held,
    seq,
    maxpages,
    autostrwtr,
    dspdta,
    oprctl,
    autchk,
    writer,
    cnntype,
    text,
    rmtsys,
    rmtprtq,
    remote,
    files
});

impl Default for OutQ {
    fn default() -> Self {
        Self {
            held: false,
            seq: Seq::Fifo,
            maxpages: 0,
            autostrwtr: 0,
            dspdta: "*NO".into(),
            oprctl: true,
            autchk: "*OWNER".into(),
            writer: String::new(),
            cnntype: 0,
            text: String::new(),
            rmtsys: String::new(),
            rmtprtq: String::new(),
            remote: RemoteQ::default(),
            files: Vec::new(),
        }
    }
}

fn dspdta_code(s: &str) -> u64 {
    match s {
        "*YES" => 1,
        "*OWNER" => 2,
        _ => 0,
    }
}

impl OutQ {
    /// A queue with this text and the defaults.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// The payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_flag("Held", self.held);
        r.set_uint("Seq", u64::from(self.seq == Seq::JobNbr));
        r.set_uint("Count", self.files.len() as u64);
        r.set_uint("MaxPages", u64::from(self.maxpages));
        r.set_uint("AutoStrWtr", u64::from(self.autostrwtr));
        r.set_uint("DspDta", dspdta_code(&self.dspdta));
        r.set_flag("OprCtl", self.oprctl);
        r.set_text("AutChk", &self.autchk);
        r.set_text("Writer", &self.writer);
        r.set_uint("CnnType", u64::from(self.cnntype));
        r.set_text("Text", &self.text);
        r.set_text("RmtSys", &self.rmtsys);
        r.set_text("RmtPrtQ", &self.rmtprtq);
        let mut out = r.buf;
        out.extend_from_slice(&self.remote.encode());
        for f in &self.files {
            out.extend_from_slice(&f.to_le_bytes());
        }
        out
    }

    /// Read a payload back.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        let version = r.uint("Version");
        if r.bytes("Magic") != MAGIC || !matches!(version, 1 | VERSION) {
            return Err(Error::Corrupt("not an output queue".into()));
        }
        let n = usize::try_from(r.uint("Count")).unwrap_or(usize::MAX);
        let mut tail = r.tail();
        let mut remote = RemoteQ::default();
        if version == VERSION {
            let size = super::outqrmt::LAYOUT.size.unwrap_or(0);
            let sec = tail
                .get(..size)
                .ok_or_else(|| Error::Corrupt("output queue remote section truncated".into()))?;
            remote = RemoteQ::decode(sec)?;
            tail = &tail[size..];
        }
        if tail.len() < n.saturating_mul(4) {
            return Err(Error::Corrupt("output queue entries truncated".into()));
        }
        Ok(Self {
            held: r.flag("Held"),
            seq: if r.uint("Seq") == 1 {
                Seq::JobNbr
            } else {
                Seq::Fifo
            },
            maxpages: u32::try_from(r.uint("MaxPages")).unwrap_or(0),
            autostrwtr: u16::try_from(r.uint("AutoStrWtr")).unwrap_or(0),
            dspdta: match r.uint("DspDta") {
                1 => "*YES",
                2 => "*OWNER",
                _ => "*NO",
            }
            .into(),
            oprctl: r.flag("OprCtl"),
            autchk: r.text("AutChk"),
            writer: r.text("Writer"),
            cnntype: u8::try_from(r.uint("CnnType")).unwrap_or(0),
            text: r.text("Text"),
            rmtsys: r.text("RmtSys"),
            rmtprtq: r.text("RmtPrtQ"),
            remote,
            files: tail[..n * 4]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect(),
        })
    }

    /// `CRTOUTQ` and `CHGOUTQ` keywords over this queue.
    pub fn apply(&mut self, p: &Params) -> Result<()> {
        let bad = |k: &str, v: &str| Error::value("CPD0084", format!("{k}({v}) is not valid"));
        for (k, raw) in &p.kw {
            let v = raw.trim();
            let up = v.to_ascii_uppercase();
            if up == "*SAME" {
                continue;
            }
            match k.as_str() {
                "SEQ" => {
                    self.seq = match up.as_str() {
                        "*FIFO" => Seq::Fifo,
                        "*JOBNBR" => Seq::JobNbr,
                        _ => return Err(bad(k, v)),
                    };
                }
                "MAXPAGES" => {
                    self.maxpages = if up == "*NONE" {
                        0
                    } else {
                        up.split_whitespace()
                            .next()
                            .and_then(|n| n.parse().ok())
                            .ok_or_else(|| bad(k, v))?
                    };
                }
                "AUTOSTRWTR" => {
                    self.autostrwtr = if up == "*NONE" {
                        0
                    } else {
                        up.parse()
                            .ok()
                            .filter(|n| (1..=10).contains(n))
                            .ok_or_else(|| bad(k, v))?
                    };
                }
                "DSPDTA" => {
                    if !matches!(up.as_str(), "*NO" | "*YES" | "*OWNER") {
                        return Err(bad(k, v));
                    }
                    self.dspdta = up;
                }
                "OPRCTL" => self.oprctl = up != "*NO",
                "AUTCHK" => {
                    if !matches!(up.as_str(), "*OWNER" | "*DTAAUT") {
                        return Err(bad(k, v));
                    }
                    self.autchk = up;
                }
                "TEXT" => self.text = v.trim_matches('\'').replace("''", "'"),
                "RMTSYS" => {
                    let s = v.trim_matches('\'').trim().to_string();
                    if s.len() > 64 {
                        return Err(bad(k, v));
                    }
                    self.rmtsys = if up == "*NONE" {
                        String::new()
                    } else if s.starts_with('*') {
                        s.to_ascii_uppercase()
                    } else {
                        s
                    };
                }
                "RMTPRTQ" => {
                    let s = v.trim_matches('\'').to_string();
                    if s.len() > 64 {
                        return Err(bad(k, v));
                    }
                    self.rmtprtq = s;
                }
                "CNNTYPE" => {
                    self.cnntype = match up.as_str() {
                        "*IP" => 0,
                        "*SNA" => 1,
                        "*USRDFN" => 2,
                        _ => return Err(bad(k, v)),
                    };
                }
                // Accepted, not kept.
                "OUTQ" | "JOBSEP" | "DTAQ" | "AUT" | "USRDFNOBJ" | "USRDFNDTA" | "SPLFASP" | "ASPDEV"
                | "IMGCFG" => {}
                k if self.remote.apply_one(k, v)? => {}
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

    /// The attribute lines of `WRKOUTQD`.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut out = String::new();
        let yn = |b: bool| if b { "*YES" } else { "*NO" };
        let _ = writeln!(
            out,
            "Status . . . . . . . : {}",
            if self.held { "HLD" } else { "RLS" }
        );
        let _ = writeln!(
            out,
            "Order on queue . . . : {}",
            if self.seq == Seq::Fifo {
                "*FIFO"
            } else {
                "*JOBNBR"
            }
        );
        let _ = writeln!(
            out,
            "Maximum pages  . . . : {}",
            if self.maxpages == 0 {
                "*NONE".into()
            } else {
                self.maxpages.to_string()
            }
        );
        let _ = writeln!(out, "Display any file . . : {}", self.dspdta);
        let _ = writeln!(out, "Operator controlled  : {}", yn(self.oprctl));
        let _ = writeln!(out, "Authority to check . : {}", self.autchk);
        let _ = writeln!(
            out,
            "Writer . . . . . . . : {}",
            if self.writer.is_empty() {
                "*NONE"
            } else {
                &self.writer
            }
        );
        let _ = writeln!(out, "Text . . . . . . . . : {}", self.text);
        if self.is_remote() {
            let _ = writeln!(out, "Remote system  . . . : {}", self.rmtsys);
            let _ = writeln!(
                out,
                "Remote printer queue : {}",
                if self.rmtprtq.is_empty() { "*USER" } else { &self.rmtprtq }
            );
            let _ = writeln!(
                out,
                "Connection type  . . : {}",
                ["*IP", "*SNA", "*USRDFN"].get(usize::from(self.cnntype)).unwrap_or(&"*IP")
            );
            out.push_str(&self.remote.describe());
        }
        out
    }

    /// Whether a remote writer prints it (`RMTSYS` is not `*NONE`).
    #[must_use]
    pub fn is_remote(&self) -> bool {
        !self.rmtsys.is_empty()
    }

    /// The host the remote writer connects to: `INTNETADR` for
    /// `RMTSYS(*INTNETADR)`, else the remote system name.
    #[must_use]
    pub fn remote_host(&self) -> String {
        if self.rmtsys == "*INTNETADR" {
            self.remote.intnetadr.clone()
        } else {
            self.rmtsys.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_apply() {
        let mut q = OutQ::new("Test queue");
        q.apply(&Params::parse(
            "x OUTQ(Q) SEQ(*JOBNBR) MAXPAGES(50) DSPDTA(*YES) TEXT('It''s')",
        ))
        .unwrap();
        q.files = vec![3, 1, 2];
        let back = OutQ::decode(&q.encode()).unwrap();
        assert_eq!(back, q);
        assert_eq!(back.text, "It's");
        assert!(q.apply(&Params::parse("x SEQ(*LIFO)")).is_err());
        assert!(q.describe().contains("*JOBNBR"));
        q.apply(&Params::parse(
            "x RMTSYS(*INTNETADR) INTNETADR('10.0.0.9') RMTPRTQ('lp0') CNNTYPE(*IP) DESTTYPE(*OTHER) MFRTYPMDL(*HP4) DESTOPT(XAIX) JOBSEP(1)",
        ))
        .unwrap();
        assert!(q.is_remote());
        assert_eq!(q.remote_host(), "10.0.0.9");
        let back = OutQ::decode(&q.encode()).unwrap();
        assert_eq!(back, q);
        assert!(back.describe().contains("Remote printer queue : lp0"));
    }
}
