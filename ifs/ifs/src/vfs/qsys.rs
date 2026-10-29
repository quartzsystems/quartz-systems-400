// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/QSYS.LIB`: the object store seen as directories and files with
//! IBM's naming (`/QSYS.LIB/LIB.LIB/FILE.FILE/MBR.MBR`, `.PGM`,
//! `.USRSPC` ...). The store side is the [`QsysView`] trait, which the
//! storage service implements over the store; [`MemQsys`] is a fake for
//! tests. Members read and write as byte streams, as IBM i 7.5 does
//! (checked on pub400, `docs/NET_OPEN_QUESTIONS.md`): a source member
//! is its data records (without sequence number and date, trailing
//! blanks kept) back to back, in text mode each ended by the CCSID's
//! line feed; a data member its fixed-length records back to back;
//! `*USRSPC` and `*SAVF` are their bytes. A text-mode write is split
//! into records at line ends, a binary one at the record length.

use std::collections::BTreeMap;
use std::sync::Mutex;

use sec::auth::bits::Auth;
use sec::auth::principal::Principal;

use crate::backend::Nanos;
use crate::errno::{Errno, Result};

/// Object types with their `/QSYS.LIB` extension: the type without the
/// asterisk, as IBM names them.
pub const TYPES: &[&str] = &[
    "*ALRTBL", "*AUTL", "*BLKSF", "*BNDDIR", "*CFGL", "*CHTFMT", "*CLD", "*CLS", "*CMD", "*CNNL",
    "*COSD", "*CRG", "*CRQD", "*CSI", "*CSPMAP", "*CSPTBL", "*CTLD", "*DEVD", "*DTAARA", "*DTADCT",
    "*DTAQ", "*EDTD", "*EXITRG", "*FCT", "*FILE", "*FNTRSC", "*FNTTBL", "*FORMDF", "*FTR", "*GSS",
    "*IGCDCT", "*IGCSRT", "*IGCTBL", "*IMGCLG", "*IPXD", "*JOBD", "*JOBQ", "*JOBSCD", "*JRN",
    "*JRNRCV", "*LIB", "*LIND", "*LOCALE", "*MEDDFN", "*MENU", "*MGTCOL", "*MODD", "*MODULE",
    "*MSGF", "*MSGQ", "*M36", "*M36CFG", "*NODGRP", "*NODL", "*NTBD", "*NWID", "*NWSCFG", "*NWSD",
    "*OUTQ", "*OVL", "*PAGDFN", "*PAGSEG", "*PDFMAP", "*PDG", "*PGM", "*PNLGRP", "*PRDAVL",
    "*PRDDFN", "*PRDLOD", "*PSFCFG", "*QMFORM", "*QMQRY", "*QRYDFN", "*RCT", "*S36", "*SAVF",
    "*SBSD", "*SCHIDX", "*SPADCT", "*SQLPKG", "*SQLUDT", "*SQLXSR", "*SRVPGM", "*SSND", "*SVRSTG",
    "*TBL", "*TIMZON", "*USRIDX", "*USRPRF", "*USRQ", "*USRSPC", "*VLDL", "*WSCST",
];

/// The type of a `.EXT` (`FILE` gives `*FILE`; `MBR` gives `*MBR`).
#[must_use]
pub fn type_of_ext(ext: &str) -> Option<String> {
    let t = format!("*{}", ext.to_ascii_uppercase());
    (t == "*MBR" || TYPES.contains(&t.as_str())).then_some(t)
}

/// The extension of a type (`*FILE` gives `FILE`).
#[must_use]
pub fn ext_of_type(t: &str) -> &str {
    t.trim_start_matches('*')
}

/// A name as `/QSYS.LIB` shows it: quoted when it is not a simple name.
#[must_use]
pub fn show_name(name: &str) -> String {
    let simple = !name.is_empty()
        && name.chars().all(|c| {
            c.is_ascii_uppercase() || c.is_ascii_digit() || matches!(c, '$' | '#' | '@' | '_')
        })
        && !name.starts_with(|c: char| c.is_ascii_digit() || c == '_');
    if simple {
        name.to_string()
    } else {
        format!("\"{name}\"")
    }
}

/// One `/QSYS.LIB` component split into name and type: `"a.b".FILE`
/// gives (`a.b`, `*FILE`); unquoted names are uppercased. Names are at
/// most ten characters.
pub fn parse_component(c: &str) -> Result<(String, String)> {
    let (name, ext) = if let Some(rest) = c.strip_prefix('"') {
        let close = rest.find('"').ok_or(Errno::ENOENT)?;
        let name = rest[..close].to_string();
        let ext = rest[close + 1..].strip_prefix('.').ok_or(Errno::ENOENT)?;
        (name, ext)
    } else {
        let dot = c.rfind('.').ok_or(Errno::ENOENT)?;
        (c[..dot].to_ascii_uppercase(), &c[dot + 1..])
    };
    if name.is_empty() || name.chars().count() > 10 {
        return Err(Errno::ENOENT);
    }
    let t = type_of_ext(ext).ok_or(Errno::ENOENT)?;
    Ok((name, t))
}

/// A node of `/QSYS.LIB`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QsysNode {
    /// `/QSYS.LIB` itself.
    Root,
    /// A library.
    Lib(String),
    /// An object (`lib`, `name`, `*TYPE`).
    Obj(String, String, String),
    /// A member (`lib`, `file`, `member`).
    Mbr(String, String, String),
}

/// The library, object, type and member of a `/QSYS.LIB` path
/// (`Qp0lCvtPathToQSYSObjName`), from the components after `QSYS.LIB`.
pub fn node_of(comps: &[String]) -> Result<QsysNode> {
    let mut lib: Option<String> = None;
    let mut obj: Option<(String, String)> = None;
    let mut mbr: Option<String> = None;
    for c in comps {
        let (name, t) = parse_component(c)?;
        match (t.as_str(), &lib, &obj) {
            ("*LIB", None, None) if name != "QSYS" => lib = Some(name),
            ("*MBR", _, Some((_, ft))) if ft == "*FILE" && mbr.is_none() => mbr = Some(name),
            (t, _, None) if t != "*LIB" && t != "*MBR" => obj = Some((name, t.to_string())),
            _ => return Err(Errno::ENOENT),
        }
    }
    let lib_name = lib.clone().unwrap_or_else(|| "QSYS".into());
    Ok(match (lib, obj, mbr) {
        (None, None, None) => QsysNode::Root,
        (Some(l), None, None) => QsysNode::Lib(l),
        (_, Some((o, t)), None) => QsysNode::Obj(lib_name, o, t),
        (_, Some((f, _)), Some(m)) => QsysNode::Mbr(lib_name, f, m),
        _ => return Err(Errno::ENOENT),
    })
}

/// The path of an object (`Qp0lCvtQSYSObjNameToPath`'s direction).
#[must_use]
pub fn path_of(node: &QsysNode) -> String {
    let lib = |l: &str| {
        if l == "QSYS" {
            "/QSYS.LIB".to_string()
        } else {
            format!("/QSYS.LIB/{}.LIB", show_name(l))
        }
    };
    match node {
        QsysNode::Root => "/QSYS.LIB".into(),
        QsysNode::Lib(l) => lib(l),
        QsysNode::Obj(l, o, t) => format!("{}/{}.{}", lib(l), show_name(o), ext_of_type(t)),
        QsysNode::Mbr(l, f, m) => {
            format!("{}/{}.FILE/{}.MBR", lib(l), show_name(f), show_name(m))
        }
    }
}

/// An object or member as the view lists it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QsysEntry {
    /// Name.
    pub name: String,
    /// `*LIB`, `*FILE`, `*MBR`, `*PGM` ...
    pub objtype: String,
    /// Attribute (`PF-SRC`, `PF-DTA`, `LF`, `SAVF`, `CLLE` ...).
    pub attr: String,
    /// Size in bytes (for a member: its stream's length).
    pub size: u64,
    /// Owner.
    pub owner: String,
    /// Created.
    pub created: Nanos,
    /// Last changed.
    pub changed: Nanos,
    /// Data CCSID (65535 for binary objects).
    pub ccsid: u16,
    /// Text.
    pub text: String,
}

/// A member's data.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemberData {
    /// Records: for a source member the data part only.
    pub records: Vec<Vec<u8>>,
    /// Record length (the data part for a source member).
    pub record_len: usize,
    /// A source member.
    pub source: bool,
    /// The data CCSID.
    pub ccsid: u16,
}

/// The store as `/QSYS.LIB` sees it.
pub trait QsysView: Send + Sync {
    /// The libraries in `QSYS`.
    fn libraries(&self) -> Result<Vec<QsysEntry>>;
    /// The objects of a library (`QSYS` for those directly in `/QSYS.LIB`).
    fn objects(&self, lib: &str) -> Result<Vec<QsysEntry>>;
    /// One object.
    fn object(&self, lib: &str, name: &str, objtype: &str) -> Result<QsysEntry>;
    /// The members of a file.
    fn members(&self, lib: &str, file: &str) -> Result<Vec<QsysEntry>>;
    /// A member's records.
    fn read_member(&self, lib: &str, file: &str, mbr: &str) -> Result<MemberData>;
    /// Replace a member's records.
    fn write_member(&self, lib: &str, file: &str, mbr: &str, records: Vec<Vec<u8>>) -> Result<()>;
    /// A `*USRSPC` or `*SAVF` as bytes.
    fn read_stream(&self, lib: &str, name: &str, objtype: &str) -> Result<Vec<u8>>;
    /// Replace a `*USRSPC` or `*SAVF`'s bytes.
    fn write_stream(&self, lib: &str, name: &str, objtype: &str, data: &[u8]) -> Result<()>;
    /// The user's authority to an object (the Phase 6 search); for a
    /// member, the file's.
    fn authority(&self, lib: &str, name: &str, objtype: &str, who: &Principal) -> Result<Auth>;
    /// Make a library (`mkdir /QSYS.LIB/X.LIB`).
    fn create_library(&self, _name: &str, _who: &Principal) -> Result<()> {
        Err(Errno::ENOTSUP)
    }
    /// Add a member (`open(O_CREAT)` of a `.MBR`).
    fn create_member(&self, _lib: &str, _file: &str, _mbr: &str, _who: &Principal) -> Result<()> {
        Err(Errno::ENOTSUP)
    }
    /// Delete an object or member (`unlink`, `rmdir` of a library).
    fn delete(&self, _node: &QsysNode, _who: &Principal) -> Result<()> {
        Err(Errno::ENOTSUP)
    }
}

/// The line end of a CCSID: EBCDIC `0x25`, else `0x0A`.
fn line_end(ccsid: u16) -> &'static [u8] {
    match ccsid::conv::Codec::of(ccsid) {
        Ok(Some(c)) if c.is_ebcdic() => &[0x25],
        Ok(Some(ccsid::conv::Codec {
            form: ccsid::conv::Form::Utf16,
            ..
        })) => &[0, 0x0A],
        _ => &[0x0A],
    }
}

/// A member as a byte stream: the records back to back, in text mode
/// (`O_TEXTDATA`) each followed by the CCSID's line feed.
#[must_use]
pub fn member_to_stream(m: &MemberData, text: bool) -> Vec<u8> {
    if !m.source || !text {
        return m.records.concat();
    }
    let mut out = Vec::new();
    for r in &m.records {
        out.extend_from_slice(r);
        out.extend_from_slice(line_end(m.ccsid));
    }
    out
}

/// The size `stat` reports for a member: IBM counts two line-end
/// bytes per source record.
#[must_use]
pub fn member_size(m: &MemberData) -> u64 {
    let n = m.records.len() as u64;
    let data: u64 = m.records.iter().map(|r| r.len() as u64).sum();
    if m.source { data + 2 * n } else { data }
}

/// A byte stream back to records of `m`'s shape. Written in text mode
/// (`O_TEXTDATA`), a source member's stream is lines: a carriage return
/// before a line end is dropped, each line padded or cut to the record
/// length. Otherwise the stream is records back to back, the last one
/// padded, as IBM i stores a binary write (`docs/NET_OPEN_QUESTIONS.md`).
#[must_use]
pub fn stream_to_records(m: &MemberData, data: &[u8], text: bool) -> Vec<Vec<u8>> {
    let len = m.record_len.max(1);
    let ebcdic = matches!(ccsid::conv::Codec::of(m.ccsid), Ok(Some(c)) if c.is_ebcdic());
    let pad = match (ebcdic, m.source) {
        (true, _) => 0x40,
        (false, true) => 0x20,
        (false, false) => 0,
    };
    if !m.source || !text {
        return data
            .chunks(len)
            .map(|c| {
                let mut r = c.to_vec();
                r.resize(len, pad);
                r
            })
            .collect();
    }
    let (lf, cr) = if ebcdic {
        (0x25u8, 0x0Du8)
    } else {
        (0x0A, 0x0D)
    };
    let mut out = Vec::new();
    let mut lines: Vec<&[u8]> = data.split(|&b| b == lf).collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    for mut l in lines {
        if l.last() == Some(&cr) {
            l = &l[..l.len() - 1];
        }
        let mut r = l[..l.len().min(len)].to_vec();
        r.resize(len, pad);
        out.push(r);
    }
    out
}

/// Libraries by name, objects by name and type.
type Libs = BTreeMap<String, BTreeMap<(String, String), MemObj>>;

#[derive(Debug, Clone, Default)]
struct MemObj {
    entry: QsysEntry,
    members: BTreeMap<String, MemberData>,
    stream: Vec<u8>,
}

/// An in-memory `/QSYS.LIB` for tests: libraries, objects, members,
/// and a fixed authority for everyone.
#[derive(Debug, Default)]
pub struct MemQsys {
    libs: Mutex<Libs>,
    /// What [`QsysView::authority`] answers for users other than the
    /// owner.
    pub public: Auth,
}

impl MemQsys {
    /// An empty view with `QSYS`.
    #[must_use]
    pub fn new(public: Auth) -> Self {
        let m = Self {
            libs: Mutex::default(),
            public,
        };
        m.lock().insert("QSYS".into(), BTreeMap::new());
        m
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Libs> {
        self.libs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Add a library.
    pub fn add_lib(&self, name: &str) {
        self.lock().entry(name.into()).or_default();
    }

    /// Add an object.
    pub fn add_obj(&self, lib: &str, name: &str, objtype: &str, attr: &str, owner: &str) {
        let mut l = self.lock();
        let objs = l.entry(lib.into()).or_default();
        objs.insert(
            (name.into(), objtype.into()),
            MemObj {
                entry: QsysEntry {
                    name: name.into(),
                    objtype: objtype.into(),
                    attr: attr.into(),
                    owner: owner.into(),
                    ccsid: 37,
                    ..QsysEntry::default()
                },
                ..MemObj::default()
            },
        );
    }

    /// Add a member to a file.
    pub fn add_member(&self, lib: &str, file: &str, mbr: &str, data: MemberData) {
        let mut l = self.lock();
        if let Some(o) = l
            .get_mut(lib)
            .and_then(|objs| objs.get_mut(&(file.into(), "*FILE".into())))
        {
            o.members.insert(mbr.into(), data);
        }
    }
}

impl QsysView for MemQsys {
    fn libraries(&self) -> Result<Vec<QsysEntry>> {
        Ok(self
            .lock()
            .keys()
            .filter(|l| *l != "QSYS")
            .map(|l| QsysEntry {
                name: l.clone(),
                objtype: "*LIB".into(),
                owner: "QSYS".into(),
                ccsid: 37,
                ..QsysEntry::default()
            })
            .collect())
    }

    fn objects(&self, lib: &str) -> Result<Vec<QsysEntry>> {
        let l = self.lock();
        let objs = l.get(lib).ok_or(Errno::ENOENT)?;
        Ok(objs.values().map(|o| o.entry.clone()).collect())
    }

    fn object(&self, lib: &str, name: &str, objtype: &str) -> Result<QsysEntry> {
        if objtype == "*LIB" {
            return self
                .libraries()?
                .into_iter()
                .find(|e| e.name == name)
                .ok_or(Errno::ENOENT);
        }
        let l = self.lock();
        let o = l
            .get(lib)
            .and_then(|objs| objs.get(&(name.into(), objtype.into())))
            .ok_or(Errno::ENOENT)?;
        let mut e = o.entry.clone();
        if !o.stream.is_empty() {
            e.size = o.stream.len() as u64;
        }
        Ok(e)
    }

    fn members(&self, lib: &str, file: &str) -> Result<Vec<QsysEntry>> {
        let l = self.lock();
        let o = l
            .get(lib)
            .and_then(|objs| objs.get(&(file.into(), "*FILE".into())))
            .ok_or(Errno::ENOENT)?;
        Ok(o.members
            .iter()
            .map(|(n, d)| QsysEntry {
                name: n.clone(),
                objtype: "*MBR".into(),
                size: member_size(d),
                owner: o.entry.owner.clone(),
                ccsid: d.ccsid,
                ..QsysEntry::default()
            })
            .collect())
    }

    fn read_member(&self, lib: &str, file: &str, mbr: &str) -> Result<MemberData> {
        let l = self.lock();
        l.get(lib)
            .and_then(|objs| objs.get(&(file.into(), "*FILE".into())))
            .and_then(|o| o.members.get(mbr))
            .cloned()
            .ok_or(Errno::ENOENT)
    }

    fn write_member(&self, lib: &str, file: &str, mbr: &str, records: Vec<Vec<u8>>) -> Result<()> {
        let mut l = self.lock();
        let m = l
            .get_mut(lib)
            .and_then(|objs| objs.get_mut(&(file.into(), "*FILE".into())))
            .and_then(|o| o.members.get_mut(mbr))
            .ok_or(Errno::ENOENT)?;
        m.records = records;
        Ok(())
    }

    fn read_stream(&self, lib: &str, name: &str, objtype: &str) -> Result<Vec<u8>> {
        let l = self.lock();
        l.get(lib)
            .and_then(|objs| objs.get(&(name.into(), objtype.into())))
            .map(|o| o.stream.clone())
            .ok_or(Errno::ENOENT)
    }

    fn write_stream(&self, lib: &str, name: &str, objtype: &str, data: &[u8]) -> Result<()> {
        let mut l = self.lock();
        let o = l
            .get_mut(lib)
            .and_then(|objs| objs.get_mut(&(name.into(), objtype.into())))
            .ok_or(Errno::ENOENT)?;
        o.stream = data.to_vec();
        Ok(())
    }

    fn authority(&self, lib: &str, name: &str, objtype: &str, who: &Principal) -> Result<Auth> {
        let owner = if objtype == "*LIB" {
            "QSYS".to_string()
        } else {
            self.object(lib, name, objtype)?.owner
        };
        Ok(if owner == who.user {
            Auth::ALL
        } else {
            self.public
        })
    }

    fn create_library(&self, name: &str, _who: &Principal) -> Result<()> {
        let mut l = self.lock();
        if l.contains_key(name) {
            return Err(Errno::EEXIST);
        }
        l.insert(name.into(), BTreeMap::new());
        Ok(())
    }

    fn create_member(&self, lib: &str, file: &str, mbr: &str, _who: &Principal) -> Result<()> {
        let mut l = self.lock();
        let o = l
            .get_mut(lib)
            .and_then(|objs| objs.get_mut(&(file.into(), "*FILE".into())))
            .ok_or(Errno::ENOENT)?;
        let shape = o.members.values().next().cloned().unwrap_or(MemberData {
            records: Vec::new(),
            record_len: 80,
            source: true,
            ccsid: 37,
        });
        o.members.insert(
            mbr.into(),
            MemberData {
                records: Vec::new(),
                ..shape
            },
        );
        Ok(())
    }

    fn delete(&self, node: &QsysNode, _who: &Principal) -> Result<()> {
        let mut l = self.lock();
        match node {
            QsysNode::Mbr(lib, f, m) => l
                .get_mut(lib)
                .and_then(|objs| objs.get_mut(&(f.clone(), "*FILE".into())))
                .and_then(|o| o.members.remove(m))
                .map(|_| ())
                .ok_or(Errno::ENOENT),
            QsysNode::Obj(lib, o, t) => l
                .get_mut(lib)
                .and_then(|objs| objs.remove(&(o.clone(), t.clone())))
                .map(|_| ())
                .ok_or(Errno::ENOENT),
            QsysNode::Lib(lib) => {
                if l.get(lib).is_some_and(|o| !o.is_empty()) {
                    return Err(Errno::ENOTEMPTY);
                }
                l.remove(lib).map(|_| ()).ok_or(Errno::ENOENT)
            }
            QsysNode::Root => Err(Errno::EPERM),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn naming() {
        assert_eq!(
            parse_component("qgpl.lib").unwrap(),
            ("QGPL".into(), "*LIB".into())
        );
        assert_eq!(
            parse_component("\"a.b\".FILE").unwrap(),
            ("a.b".into(), "*FILE".into())
        );
        assert_eq!(parse_component("X.BOGUS"), Err(Errno::ENOENT));
        assert_eq!(parse_component("ELEVENCHARS.PGM"), Err(Errno::ENOENT));
        assert_eq!(node_of(&[]).unwrap(), QsysNode::Root);
        assert_eq!(
            node_of(&c(&["QGPL.LIB", "QCLSRC.FILE", "X.MBR"])).unwrap(),
            QsysNode::Mbr("QGPL".into(), "QCLSRC".into(), "X".into())
        );
        assert_eq!(
            node_of(&c(&["QSYSOPR.MSGQ"])).unwrap(),
            QsysNode::Obj("QSYS".into(), "QSYSOPR".into(), "*MSGQ".into())
        );
        assert_eq!(node_of(&c(&["A.LIB", "B.LIB"])), Err(Errno::ENOENT));
        assert_eq!(
            node_of(&c(&["A.LIB", "P.PGM", "X.MBR"])),
            Err(Errno::ENOENT)
        );
        for t in TYPES {
            assert_eq!(type_of_ext(ext_of_type(t)).as_deref(), Some(*t));
        }
        assert_eq!(
            path_of(&QsysNode::Mbr("QGPL".into(), "a.b".into(), "M".into())),
            "/QSYS.LIB/QGPL.LIB/\"a.b\".FILE/M.MBR"
        );
        assert_eq!(
            path_of(&QsysNode::Obj(
                "QSYS".into(),
                "QSYSOPR".into(),
                "*MSGQ".into()
            )),
            "/QSYS.LIB/QSYSOPR.MSGQ"
        );
    }

    #[test]
    fn member_streams() {
        let src = MemberData {
            records: vec![
                ccsid::to_ebcdic("PGM       ").unwrap(),
                ccsid::to_ebcdic("ENDPGM    ").unwrap(),
            ],
            record_len: 10,
            source: true,
            ccsid: 37,
        };
        let s = member_to_stream(&src, true);
        assert_eq!(ccsid::from_ebcdic(&s), "PGM       \nENDPGM    \n");
        assert_eq!(stream_to_records(&src, &s, true), src.records);
        let binary = member_to_stream(&src, false);
        assert_eq!(binary, src.records.concat());
        assert_eq!(stream_to_records(&src, &binary, false), src.records);
        assert_eq!(member_size(&src), 24);
        let dta = MemberData {
            records: vec![vec![1, 2], vec![3, 4]],
            record_len: 2,
            source: false,
            ccsid: 65535,
        };
        assert_eq!(member_to_stream(&dta, true), vec![1, 2, 3, 4]);
        assert_eq!(
            stream_to_records(&dta, &[1, 2, 3], false),
            vec![vec![1, 2], vec![3, 0]]
        );
        let ascii = MemberData {
            record_len: 4,
            source: true,
            ccsid: 819,
            ..MemberData::default()
        };
        assert_eq!(
            stream_to_records(&ascii, b"ab\r\nlonger\n", true),
            vec![b"ab  ".to_vec(), b"long".to_vec()]
        );
        // A binary write keeps its line ends: records back to back.
        assert_eq!(
            stream_to_records(&ascii, b"ab\r\nlonger", false),
            vec![b"ab\r\n".to_vec(), b"long".to_vec(), b"er  ".to_vec()]
        );
    }
}
