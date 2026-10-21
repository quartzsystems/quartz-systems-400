// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system reply list: entries in sequence-number order, each a
//! message identifier (generic when it ends in `0000` or `00`, or
//! `*ANY`), comparison data at a start position, a reply and whether to
//! dump the job. A job with `INQMSGRPY(*SYSRPYL)` takes the reply of
//! the first entry an inquiry matches. The list is kept in
//! `QSYS/QRPYLIST`, a user space.

use std::fmt::Write as _;

use ipc::wire_struct;

use crate::rec::{Layout, Rec, field, text_bytes, text_of};
use crate::{Error, Result};

/// One entry's fixed part.
pub const LAYOUT: Layout = Layout {
    name: "ReplyListEntry",
    size: Some(64),
    fields: &[
        field("SeqNbr", 0, 2),
        field("Dump", 2, 1),
        field("Reserved", 3, 1),
        field("MsgId", 4, 7),
        field("Pad", 11, 1),
        field("Start", 12, 2),
        field("CmpLen", 14, 2),
        field("Reply", 16, 32),
        field("Reserved2", 48, 16),
    ],
};

/// The name of the space that holds the list.
pub const SPACE: &str = "QRPYLIST";

/// An entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RpyEntry {
    /// `SEQNBR` 1-9999.
    pub seq: u16,
    /// `MSGID`: an identifier, a generic one, or `*ANY`.
    pub msgid: String,
    /// `CMPDTA`: text the message data must contain at `start`;
    /// empty for `*NONE`.
    pub cmpdta: String,
    /// The comparison's start position, 1-based.
    pub start: u16,
    /// `RPY`: a reply, `*DFT`, or `*RQD`.
    pub reply: String,
    /// `DUMP(*YES)`.
    pub dump: bool,
}

wire_struct!(RpyEntry {
    seq,
    msgid,
    cmpdta,
    start,
    reply,
    dump
});

impl RpyEntry {
    /// Whether an identifier matches: exact, generic (`CPA0000` covers
    /// `CPA`, `CPA2400` covers `CPA24`), or `*ANY`.
    #[must_use]
    pub fn matches_id(&self, msgid: &str) -> bool {
        if self.msgid == "*ANY" || self.msgid == msgid {
            return true;
        }
        let stem = self.msgid.trim_end_matches('0');
        self.msgid.len() == 7 && self.msgid.ends_with("00") && msgid.starts_with(stem)
    }

    /// Whether message data matches the comparison data.
    #[must_use]
    pub fn matches_data(&self, data: &str) -> bool {
        if self.cmpdta.is_empty() {
            return true;
        }
        let from = usize::from(self.start.max(1) - 1);
        data.get(from..)
            .is_some_and(|d| d.starts_with(&self.cmpdta))
    }
}

/// The list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RpyList {
    /// Entries, by sequence number.
    pub entries: Vec<RpyEntry>,
}

impl RpyList {
    /// What IBM ships, with Quartz comparison data: dumps and a cancel
    /// for the language runtimes' function checks, ignore for the
    /// printer-alignment inquiry.
    #[must_use]
    pub fn shipped() -> Self {
        let e = |seq, msgid: &str, reply: &str, dump| RpyEntry {
            seq,
            msgid: msgid.into(),
            cmpdta: String::new(),
            start: 1,
            reply: reply.into(),
            dump,
        };
        Self {
            entries: vec![
                e(10, "CPA0700", "D", true),
                e(20, "RPG0000", "D", true),
                e(30, "CBE0000", "D", true),
                e(40, "PLI0000", "D", true),
                e(50, "CPA5305", "I", false),
            ],
        }
    }

    /// The entry an inquiry matches, by sequence number.
    #[must_use]
    pub fn find(&self, msgid: &str, data: &str) -> Option<&RpyEntry> {
        self.entries
            .iter()
            .find(|e| e.matches_id(msgid) && e.matches_data(data))
    }

    /// `ADDRPYLE`: `CPF2555` when the sequence number is taken.
    pub fn add(&mut self, e: RpyEntry) -> Result<()> {
        if !(1..=9999).contains(&e.seq) {
            return Err(Error::value("CPD0085", "SEQNBR is 1 through 9999"));
        }
        if self.entries.iter().any(|x| x.seq == e.seq) {
            return Err(Error::value(
                "CPF2555",
                format!("sequence number {} is already in the reply list", e.seq),
            ));
        }
        self.entries.push(e);
        self.entries.sort_by_key(|x| x.seq);
        Ok(())
    }

    /// `RMVRPYLE`: `CPF2556` when there is no such entry.
    pub fn remove(&mut self, seq: u16) -> Result<()> {
        let before = self.entries.len();
        self.entries.retain(|x| x.seq != seq);
        if before == self.entries.len() {
            return Err(Error::value(
                "CPF2556",
                format!("sequence number {seq} is not in the reply list"),
            ));
        }
        Ok(())
    }

    /// The payload: a two-byte count, then each entry and its
    /// comparison data.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = u16::try_from(self.entries.len())
            .unwrap_or(u16::MAX)
            .to_le_bytes()
            .to_vec();
        for e in &self.entries {
            let mut r = Rec::new(&LAYOUT);
            r.set_uint("SeqNbr", u64::from(e.seq));
            r.set_flag("Dump", e.dump);
            r.set_text("MsgId", &e.msgid);
            r.set_uint("Start", u64::from(e.start));
            let cmp = text_bytes(&e.cmpdta, e.cmpdta.chars().count());
            r.set_uint("CmpLen", cmp.len() as u64);
            r.set_text("Reply", &e.reply);
            out.extend(r.buf);
            out.extend(cmp);
        }
        out
    }

    /// Read a payload back.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < 2 {
            return Err(Error::Corrupt("reply list truncated".into()));
        }
        let n = u16::from_le_bytes([b[0], b[1]]);
        let mut rest = &b[2..];
        let mut entries = Vec::new();
        for _ in 0..n {
            let r = Rec::read(&LAYOUT, rest)?;
            let k = usize::try_from(r.uint("CmpLen")).unwrap_or(usize::MAX);
            let cmp = r
                .tail()
                .get(..k)
                .ok_or_else(|| Error::Corrupt("reply list entry truncated".into()))?;
            entries.push(RpyEntry {
                seq: u16::try_from(r.uint("SeqNbr")).unwrap_or(0),
                msgid: r.text("MsgId"),
                cmpdta: text_of(cmp),
                start: u16::try_from(r.uint("Start")).unwrap_or(1),
                reply: r.text("Reply"),
                dump: r.flag("Dump"),
            });
            rest = &rest[LAYOUT.size.unwrap_or(0) + k..];
        }
        Ok(Self { entries })
    }

    /// `WRKRPYLE`'s list.
    #[must_use]
    pub fn display(&self) -> String {
        let mut out = String::from(
            "Sequence  Message                         Compare\n Number   ID       Reply     Dump      Value\n",
        );
        for e in &self.entries {
            let _ = writeln!(
                out,
                "  {:04}    {:<7}  {:<8}  {:<8}  {}",
                e.seq,
                e.msgid,
                e.reply,
                if e.dump { "*YES" } else { "*NO" },
                if e.cmpdta.is_empty() {
                    "*NONE".to_string()
                } else {
                    format!("'{}' at {}", e.cmpdta, e.start)
                }
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching() {
        let mut l = RpyList::shipped();
        assert_eq!(l.find("CPA0701", "").unwrap().seq, 10);
        assert_eq!(l.find("RPG1241", "").unwrap().reply, "D");
        assert!(l.find("CEE9901", "").is_none());
        l.add(RpyEntry {
            seq: 5,
            msgid: "CEE9901".into(),
            cmpdta: "FAILPGM".into(),
            start: 1,
            reply: "C".into(),
            dump: false,
        })
        .unwrap();
        assert!(l.find("CEE9901", "OTHERPGM  MCH1211").is_none());
        assert_eq!(l.find("CEE9901", "FAILPGM   MCH1211").unwrap().reply, "C");
        assert!(
            l.add(RpyEntry {
                seq: 5,
                ..RpyEntry::default()
            })
            .is_err()
        );
        assert_eq!(RpyList::decode(&l.encode()).unwrap(), l);
        l.remove(5).unwrap();
        assert!(l.remove(5).is_err());
        assert!(l.display().contains("CPA0700"));
    }
}

/// The list's space in `QSYS`, if it exists.
pub fn find(
    txn: &mut store::txn::Txn<'_>,
    qsys: store::obj::ObjectId,
) -> Result<Option<store::obj::ObjectId>> {
    let lib = store::obj::library::Library::load(txn, qsys)?;
    let n = qsname::Name::parse(SPACE).expect("valid");
    Ok(lib.lookup(txn, store::obj::types::USRSPC, n)?)
}

/// Read the list.
pub fn load(txn: &mut store::txn::Txn<'_>, qsys: store::obj::ObjectId) -> Result<RpyList> {
    let id = find(txn, qsys)?.ok_or_else(|| Error::NotFound {
        kind: "*USRSPC",
        lib: "QSYS".into(),
        name: SPACE.into(),
    })?;
    let attr = store::obj::usrspc::describe(txn, id)?;
    let head = store::obj::usrspc::read(txn, id, 0, 4)?;
    let n = u32::from_le_bytes(head[..4].try_into().expect("sized")) as usize;
    if 4 + n as u64 > attr.size {
        return Err(Error::Corrupt("reply list runs past its space".into()));
    }
    RpyList::decode(&store::obj::usrspc::read(txn, id, 4, n)?)
}

/// Write the list, creating its space when there is none.
pub fn save(
    txn: &mut store::txn::Txn<'_>,
    qsys: store::obj::ObjectId,
    list: &RpyList,
    stamp: &super::Stamp,
) -> Result<()> {
    let payload = list.encode();
    let mut bytes = u32::try_from(payload.len())
        .unwrap_or(u32::MAX)
        .to_le_bytes()
        .to_vec();
    bytes.extend(payload);
    let id = match find(txn, qsys)? {
        Some(id) => id,
        None => store::obj::usrspc::create(
            txn,
            &store::obj::CreateParams {
                library: qsys,
                name: qsname::Name::parse(SPACE).expect("valid"),
                type_code: store::obj::types::USRSPC,
                attribute: String::new(),
                text: "System reply list".into(),
                owner: stamp.owner.clone(),
                system: stamp.system.clone(),
                now: stamp.now,
                by_system: true,
            },
            bytes.len() as u64,
            true,
            0,
        )?,
    };
    if store::obj::usrspc::describe(txn, id)?.size < bytes.len() as u64 {
        store::obj::usrspc::change_size(txn, id, bytes.len() as u64)?;
    }
    store::obj::usrspc::write(txn, id, 0, &bytes)?;
    Ok(())
}
