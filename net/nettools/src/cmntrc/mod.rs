// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The communications trace: a trace per configuration object and type
//! (`STRCMNTRC CFGOBJ CFGTYPE`), holding the frames a line's kernel link
//! sent and received, up to `MAXSTG` bytes, wrapping or stopping when
//! full (`TRCFULL`), each cut to `USRDTA` bytes.
//!
//! - [`Trace`], [`Header`], [`Record`]: the trace and its byte form
//!   ([`Trace::encode`], [`Trace::decode`]), laid out in
//!   `docs/TCPIP.md` ("Communications trace").
//! - [`registry`]: the traces of the system and the commands' rules.
//! - [`frame`]: Ethernet, ARP, IPv4, IPv6, TCP, UDP and ICMP headers.
//! - [`report`]: the `PRTCMNTRC` listing.

pub mod frame;
pub mod registry;
pub mod report;

use std::collections::VecDeque;

/// `CFGTYPE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CfgType {
    /// `*LIN`: a line description.
    Line,
    /// `*NWI`: a network interface description.
    Nwi,
    /// `*NWS`: a network server description.
    Nws,
}

impl CfgType {
    /// The special value.
    pub fn name(self) -> &'static str {
        match self {
            CfgType::Line => "*LIN",
            CfgType::Nwi => "*NWI",
            CfgType::Nws => "*NWS",
        }
    }

    /// From the special value.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "*LIN" => Some(CfgType::Line),
            "*NWI" => Some(CfgType::Nwi),
            "*NWS" => Some(CfgType::Nws),
            _ => None,
        }
    }

    /// The number the report shows (`1` line, `2` network interface,
    /// `3` network server).
    pub fn number(self) -> u8 {
        match self {
            CfgType::Line => 1,
            CfgType::Nwi => 2,
            CfgType::Nws => 3,
        }
    }

    fn from_number(n: u8) -> Option<Self> {
        match n {
            1 => Some(CfgType::Line),
            2 => Some(CfgType::Nwi),
            3 => Some(CfgType::Nws),
            _ => None,
        }
    }
}

/// `DTADIR`: which frames are kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// `*BOTH`.
    Both,
    /// `*SND`.
    Sent,
    /// `*RCV`.
    Received,
}

impl Direction {
    /// From the special value.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "*BOTH" => Some(Direction::Both),
            "*SND" => Some(Direction::Sent),
            "*RCV" => Some(Direction::Received),
            _ => None,
        }
    }

    /// Whether a frame going this way (`sent`) is kept.
    pub fn keeps(self, sent: bool) -> bool {
        match self {
            Direction::Both => true,
            Direction::Sent => sent,
            Direction::Received => !sent,
        }
    }

    fn number(self) -> u8 {
        match self {
            Direction::Sent => 1,
            Direction::Received => 2,
            Direction::Both => 3,
        }
    }

    fn from_number(n: u8) -> Self {
        match n {
            1 => Direction::Sent,
            2 => Direction::Received,
            _ => Direction::Both,
        }
    }
}

/// Where a trace is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Collecting.
    Active,
    /// Ended by `ENDCMNTRC`; its data kept until `DLTCMNTRC`.
    Inactive,
    /// The buffer filled with `TRCFULL(*STOPTRC)`.
    Stopped,
    /// The link could not be opened; nothing collected.
    Failed,
}

impl Status {
    /// The word `CHKCMNTRC` shows.
    pub fn name(self) -> &'static str {
        match self {
            Status::Active => "Active",
            Status::Inactive => "Inactive",
            Status::Stopped => "Stopped",
            Status::Failed => "Failed",
        }
    }

    fn number(self) -> u8 {
        match self {
            Status::Active => 1,
            Status::Inactive => 2,
            Status::Stopped => 3,
            Status::Failed => 4,
        }
    }

    fn from_number(n: u8) -> Self {
        match n {
            1 => Status::Active,
            3 => Status::Stopped,
            4 => Status::Failed,
            _ => Status::Inactive,
        }
    }
}

/// What `STRCMNTRC` asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// `CFGOBJ`.
    pub cfgobj: String,
    /// `CFGTYPE`.
    pub cfgtype: CfgType,
    /// The kernel link traced (`eth0`).
    pub ifname: String,
    /// The line's protocol as the report names it (`ETHERNET`).
    pub protocol: String,
    /// `MAXSTG` in bytes.
    pub maxstg: u64,
    /// `TRCFULL(*WRAP)`: drop the oldest frames when full.
    pub wrap: bool,
    /// `USRDTA`: bytes kept of each frame; `None` is `*CALC` (all).
    pub usrdta: Option<u32>,
    /// `DTADIR`.
    pub dir: Direction,
    /// `TEXT`.
    pub text: String,
    /// When it started, microseconds since the epoch.
    pub started: u64,
    /// When it ended, when it has.
    pub ended: Option<u64>,
    /// Its status.
    pub status: Status,
}

/// One frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// When, microseconds since the epoch.
    pub time: u64,
    /// Sent by this system, else received.
    pub sent: bool,
    /// The frame's length on the wire.
    pub len: u32,
    /// The bytes kept (`USRDTA`).
    pub data: Vec<u8>,
}

impl Record {
    /// The storage a record takes against `MAXSTG`.
    pub fn size(&self) -> u64 {
        16 + self.data.len() as u64
    }
}

/// A trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trace {
    /// What was asked and where it is.
    pub header: Header,
    /// The frames, oldest first.
    pub records: VecDeque<Record>,
    /// Record numbers given so far (the first kept may not be 1 after
    /// a wrap).
    pub numbered: u64,
    /// Storage the records take.
    pub used: u64,
    /// Bytes seen on the link, kept or not.
    pub collected: u64,
}

/// A trace's bytes are not one.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("communications trace data is not valid: {0}")]
pub struct BadTrace(pub &'static str);

/// The byte form's identifier.
pub const MAGIC: &[u8; 8] = b"QSCMNTRC";
/// The byte form's version.
pub const VERSION: u16 = 1;

impl Trace {
    /// A new, active trace.
    pub fn new(header: Header) -> Self {
        Self {
            header,
            records: VecDeque::new(),
            numbered: 0,
            used: 0,
            collected: 0,
        }
    }

    /// Add a frame seen on the link at `time`. Returns `false` once the
    /// trace no longer collects (stopped when full, or ended).
    pub fn push(&mut self, time: u64, sent: bool, frame: &[u8]) -> bool {
        if self.header.status != Status::Active {
            return false;
        }
        self.collected += frame.len() as u64;
        if !self.header.dir.keeps(sent) {
            return true;
        }
        let keep = self
            .header
            .usrdta
            .map_or(frame.len(), |n| frame.len().min(n as usize));
        let r = Record {
            time,
            sent,
            len: u32::try_from(frame.len()).unwrap_or(u32::MAX),
            data: frame[..keep].to_vec(),
        };
        let need = r.size();
        while self.used + need > self.header.maxstg {
            if !self.header.wrap || self.records.is_empty() {
                self.header.status = Status::Stopped;
                self.header.ended = Some(time);
                return false;
            }
            if let Some(old) = self.records.pop_front() {
                self.used -= old.size();
            }
        }
        self.used += need;
        self.numbered += 1;
        self.records.push_back(r);
        true
    }

    /// The number of the first record kept.
    pub fn first_number(&self) -> u64 {
        self.numbered - self.records.len() as u64 + 1
    }

    /// The byte form: magic, version, the header, then each record.
    pub fn encode(&self) -> Vec<u8> {
        let h = &self.header;
        let mut b = Vec::new();
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&VERSION.to_be_bytes());
        put_str(&mut b, &h.cfgobj);
        b.push(h.cfgtype.number());
        put_str(&mut b, &h.ifname);
        put_str(&mut b, &h.protocol);
        b.extend_from_slice(&h.maxstg.to_be_bytes());
        b.push(u8::from(h.wrap));
        b.extend_from_slice(&h.usrdta.unwrap_or(0).to_be_bytes());
        b.push(h.dir.number());
        put_str(&mut b, &h.text);
        b.extend_from_slice(&h.started.to_be_bytes());
        b.extend_from_slice(&h.ended.unwrap_or(0).to_be_bytes());
        b.push(h.status.number());
        b.extend_from_slice(&self.numbered.to_be_bytes());
        b.extend_from_slice(&self.collected.to_be_bytes());
        b.extend_from_slice(&u32::try_from(self.records.len()).unwrap_or(u32::MAX).to_be_bytes());
        for r in &self.records {
            b.extend_from_slice(&r.time.to_be_bytes());
            b.push(u8::from(r.sent));
            b.extend_from_slice(&r.len.to_be_bytes());
            b.extend_from_slice(&u32::try_from(r.data.len()).unwrap_or(u32::MAX).to_be_bytes());
            b.extend_from_slice(&r.data);
        }
        b
    }

    /// From the byte form.
    pub fn decode(b: &[u8]) -> Result<Self, BadTrace> {
        let mut c = Cursor { b, at: 0 };
        if c.take(8)? != MAGIC {
            return Err(BadTrace("not a trace"));
        }
        if c.u16()? != VERSION {
            return Err(BadTrace("unknown version"));
        }
        let cfgobj = c.str()?;
        let cfgtype = CfgType::from_number(c.u8()?).ok_or(BadTrace("configuration type"))?;
        let ifname = c.str()?;
        let protocol = c.str()?;
        let maxstg = c.u64()?;
        let wrap = c.u8()? != 0;
        let usrdta = Some(c.u32()?).filter(|n| *n != 0);
        let dir = Direction::from_number(c.u8()?);
        let text = c.str()?;
        let started = c.u64()?;
        let ended = Some(c.u64()?).filter(|n| *n != 0);
        let status = Status::from_number(c.u8()?);
        let numbered = c.u64()?;
        let collected = c.u64()?;
        let n = c.u32()?;
        let mut records = VecDeque::new();
        let mut used = 0;
        for _ in 0..n {
            let time = c.u64()?;
            let sent = c.u8()? != 0;
            let len = c.u32()?;
            let dl = c.u32()? as usize;
            let data = c.take(dl)?.to_vec();
            let r = Record { time, sent, len, data };
            used += r.size();
            records.push_back(r);
        }
        if c.at != b.len() {
            return Err(BadTrace("bytes after the last record"));
        }
        if (records.len() as u64) > numbered {
            return Err(BadTrace("record count"));
        }
        Ok(Self {
            header: Header {
                cfgobj,
                cfgtype,
                ifname,
                protocol,
                maxstg,
                wrap,
                usrdta,
                dir,
                text,
                started,
                ended,
                status,
            },
            records,
            numbered,
            used,
            collected,
        })
    }
}

fn put_str(b: &mut Vec<u8>, s: &str) {
    b.extend_from_slice(&u16::try_from(s.len()).unwrap_or(u16::MAX).to_be_bytes());
    b.extend_from_slice(&s.as_bytes()[..s.len().min(usize::from(u16::MAX))]);
}

struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], BadTrace> {
        let s = self.b.get(self.at..self.at + n).ok_or(BadTrace("truncated"))?;
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, BadTrace> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, BadTrace> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().map_err(|_| BadTrace("truncated"))?))
    }
    fn u32(&mut self) -> Result<u32, BadTrace> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().map_err(|_| BadTrace("truncated"))?))
    }
    fn u64(&mut self) -> Result<u64, BadTrace> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().map_err(|_| BadTrace("truncated"))?))
    }
    fn str(&mut self) -> Result<String, BadTrace> {
        let n = usize::from(self.u16()?);
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| BadTrace("text"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn header() -> Header {
        Header {
            cfgobj: "ETHLINE".into(),
            cfgtype: CfgType::Line,
            ifname: "eth0".into(),
            protocol: "ETHERNET".into(),
            maxstg: 1000,
            wrap: true,
            usrdta: None,
            dir: Direction::Both,
            text: "test".into(),
            started: 1_700_000_000_000_000,
            ended: None,
            status: Status::Active,
        }
    }

    #[test]
    fn wraps_when_full() {
        let mut t = Trace::new(header());
        for i in 0..20u64 {
            assert!(t.push(i, i % 2 == 0, &[0u8; 84]));
        }
        // 100 bytes a record against 1000: the last ten stay.
        assert_eq!(t.records.len(), 10);
        assert_eq!(t.numbered, 20);
        assert_eq!(t.first_number(), 11);
        assert_eq!(t.records[0].time, 10);
        assert_eq!(t.used, 1000);
        assert_eq!(t.collected, 20 * 84);
    }

    #[test]
    fn stops_when_full_and_asked_to() {
        let mut t = Trace::new(Header { wrap: false, ..header() });
        let mut n = 0;
        while t.push(n, true, &[0u8; 84]) {
            n += 1;
        }
        assert_eq!(n, 10);
        assert_eq!(t.header.status, Status::Stopped);
        assert_eq!(t.header.ended, Some(10));
        assert!(!t.push(99, true, &[1]));
    }

    #[test]
    fn direction_and_user_data() {
        let mut t = Trace::new(Header { dir: Direction::Received, usrdta: Some(4), ..header() });
        t.push(1, true, &[1, 2, 3, 4, 5, 6]);
        t.push(2, false, &[1, 2, 3, 4, 5, 6]);
        assert_eq!(t.records.len(), 1);
        assert_eq!(t.records[0].data, [1, 2, 3, 4]);
        assert_eq!(t.records[0].len, 6);
    }

    #[test]
    fn byte_form_round_trips() {
        let mut t = Trace::new(Header { usrdta: Some(64), ..header() });
        t.push(5, true, &[0xaa; 70]);
        t.push(6, false, &[0xbb; 20]);
        t.header.status = Status::Inactive;
        t.header.ended = Some(7);
        let b = t.encode();
        assert_eq!(&b[..8], MAGIC);
        assert_eq!(Trace::decode(&b), Ok(t.clone()));
        assert_eq!(Trace::decode(&b[..b.len() - 1]), Err(BadTrace("truncated")));
        let mut more = b.clone();
        more.push(0);
        assert!(Trace::decode(&more).is_err());
        assert_eq!(Trace::decode(b"QSCMNTRX\0\x01"), Err(BadTrace("not a trace")));
    }
}
