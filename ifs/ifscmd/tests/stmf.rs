// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CPYTOSTMF` and `CPYFRMSTMF` over an IFS in memory: source and data
//! members to stream files in EBCDIC, ISO 8859-1, UTF-8 and Windows
//! 1252 and back, every line end, tabs, the options for an existing
//! target, and the refusals with IBM's messages.

use std::sync::Arc;

use ifs::Errno;
use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::LocalIfs;
use ifs::ops::audit::MemAudit;
use ifs::ops::{Config, Ifs, Owner};
use ifs::vfs::qsys::{MemQsys, MemberData, QsysEntry, QsysNode, QsysView};
use ifscmd::Reply;
use ifscmd::stmf::{Common, Cvt, EndLin, FrmStmf, Opt, StmfCcsid, ToStmf, cpyfrmstmf, cpytostmf};
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;

/// One `MemQsys` both the IFS and the commands see.
struct Shared(Arc<MemQsys>);

impl QsysView for Shared {
    fn libraries(&self) -> ifs::Result<Vec<QsysEntry>> {
        self.0.libraries()
    }
    fn objects(&self, lib: &str) -> ifs::Result<Vec<QsysEntry>> {
        self.0.objects(lib)
    }
    fn object(&self, lib: &str, name: &str, objtype: &str) -> ifs::Result<QsysEntry> {
        self.0.object(lib, name, objtype)
    }
    fn members(&self, lib: &str, file: &str) -> ifs::Result<Vec<QsysEntry>> {
        self.0.members(lib, file)
    }
    fn read_member(&self, lib: &str, file: &str, mbr: &str) -> ifs::Result<MemberData> {
        self.0.read_member(lib, file, mbr)
    }
    fn write_member(&self, lib: &str, file: &str, mbr: &str, r: Vec<Vec<u8>>) -> ifs::Result<()> {
        self.0.write_member(lib, file, mbr, r)
    }
    fn read_stream(&self, lib: &str, name: &str, t: &str) -> ifs::Result<Vec<u8>> {
        self.0.read_stream(lib, name, t)
    }
    fn write_stream(&self, lib: &str, name: &str, t: &str, d: &[u8]) -> ifs::Result<()> {
        self.0.write_stream(lib, name, t, d)
    }
    fn authority(&self, lib: &str, name: &str, t: &str, who: &Principal) -> ifs::Result<Auth> {
        self.0.authority(lib, name, t, who)
    }
    fn create_member(&self, lib: &str, file: &str, mbr: &str, who: &Principal) -> ifs::Result<()> {
        self.0.create_member(lib, file, mbr, who)
    }
    fn delete(&self, node: &QsysNode, who: &Principal) -> ifs::Result<()> {
        self.0.delete(node, who)
    }
}

struct Sys {
    q: Arc<MemQsys>,
    c: LocalIfs<MemBackend>,
}

fn e37(s: &str, len: usize) -> Vec<u8> {
    ccsid::conv::from_str(37, &format!("{s:<len$}")).unwrap()
}

fn sys() -> Sys {
    let q = Arc::new(MemQsys::new(Auth::CHANGE));
    q.add_lib("QGPL");
    q.add_obj("QGPL", "QCLSRC", "*FILE", "PF-SRC", "QSYS");
    q.add_member(
        "QGPL",
        "QCLSRC",
        "HELLO",
        MemberData {
            records: vec![e37("PGM", 20), e37("  SNDPGMMSG 'Hi'", 20), e37("ENDPGM", 20)],
            record_len: 20,
            source: true,
            ccsid: 37,
        },
    );
    q.add_obj("QGPL", "DATA", "*FILE", "PF-DTA", "QSYS");
    q.add_member(
        "QGPL",
        "DATA",
        "DATA",
        MemberData {
            records: vec![e37("A1", 6), e37("B2", 6)],
            record_len: 6,
            source: false,
            ccsid: 37,
        },
    );
    let env = MemEnv::default().user("QSYS", 0).user("BOB", 100).user("QDFTOWN", 3);
    let ifs = Ifs::new(MemBackend::new(), Box::new(Shared(Arc::clone(&q))), Config::default());
    ifs.ensure_layout(&env).unwrap();
    let principal = Principal {
        user: "BOB".into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        spcaut: sec::profile::spcaut::SpecialAuth::ALLOBJ.0,
        ..Principal::default()
    };
    let c = LocalIfs {
        ifs: Arc::new(ifs),
        env: Arc::new(env),
        audit: Arc::new(MemAudit::default()),
        principal,
        owner: Owner(1),
        job_ccsid: 37,
    };
    Sys { q, c }
}

const SRC: &str = "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR";

fn to(s: &Sys, stmf: &str, f: impl FnOnce(&mut ToStmf)) -> Reply {
    let mut a = ToStmf {
        frommbr: SRC.into(),
        tostmf: stmf.into(),
        ..ToStmf::default()
    };
    f(&mut a);
    s.c.with(|c| cpytostmf(c, &Shared(Arc::clone(&s.q)), &s.c.principal, &a))
}

fn from(s: &Sys, stmf: &str, mbr: &str, f: impl FnOnce(&mut FrmStmf)) -> Reply {
    let mut a = FrmStmf {
        fromstmf: stmf.into(),
        tombr: mbr.into(),
        common: Common {
            endlinfmt: EndLin::All,
            ..Common::default()
        },
        tabexpn: true,
        ..FrmStmf::default()
    };
    f(&mut a);
    s.c.with(|c| cpyfrmstmf(c, &Shared(Arc::clone(&s.q)), &s.c.principal, &a))
}

fn bytes(s: &Sys, path: &str) -> (Vec<u8>, u16) {
    s.c.with(|c| ifscmd::stmf::read_stream(c, path)).unwrap()
}

fn put(s: &Sys, path: &str, data: &[u8], ccsid: u32) {
    use ifs::ops::OpenArgs;
    use ifs::ops::flags::{O_CCSID, O_CREAT, O_TRUNC, O_WRONLY};
    s.c.with(|c| {
        let fd = c
            .open(path, OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC | O_CCSID).mode(0o644).conv(ccsid))
            .unwrap();
        ifs::client::write_all(c, fd, data).unwrap();
        c.close(fd).unwrap();
    });
}

fn ok(r: &Reply, id: &str) {
    assert!(r.escape.is_none(), "{r:?}");
    assert_eq!(r.messages.last().map(|(_, m)| m.id.as_str()), Some(id), "{r:?}");
}

fn refused(r: &Reply, escape: &str, diag: &str) {
    assert_eq!(r.escape.as_ref().map(|m| m.id.as_str()), Some(escape), "{r:?}");
    if !diag.is_empty() {
        assert_eq!(r.messages.first().map(|(_, m)| m.id.as_str()), Some(diag), "{r:?}");
    }
}

fn member(s: &Sys, file: &str, mbr: &str) -> Vec<String> {
    s.q.read_member("QGPL", file, mbr)
        .unwrap()
        .records
        .iter()
        .map(|r| ccsid::conv::to_string(37, r).unwrap())
        .collect()
}

#[test]
fn source_member_to_each_ccsid() {
    let s = sys();
    // *STMF on a new file: the member's CCSID, no conversion.
    ok(&to(&s, "/tmp/a37.txt", |_| {}), "CPCA082");
    let (b, tag) = bytes(&s, "/tmp/a37.txt");
    assert_eq!(tag, 37);
    assert_eq!(b, ccsid::conv::from_str(37, "PGM\r\n  SNDPGMMSG 'Hi'\r\nENDPGM\r\n").unwrap());
    for (i, (choice, want)) in [
        (StmfCcsid::Ccsid(819), 819),
        (StmfCcsid::Ccsid(1208), 1208),
        (StmfCcsid::PcAscii, 1252),
        (StmfCcsid::StdAscii, 819),
    ]
    .into_iter()
    .enumerate()
    {
        let path = format!("/tmp/a{i}-{want}.txt");
        let r = to(&s, &path, |a| {
            a.common.stmfccsid = choice;
            a.common.endlinfmt = EndLin::Lf;
        });
        ok(&r, "CPCA082");
        let (b, tag) = bytes(&s, &path);
        assert_eq!(tag, want);
        assert_eq!(b, b"PGM\n  SNDPGMMSG 'Hi'\nENDPGM\n", "{want}");
    }
}

#[test]
fn every_line_end() {
    let s = sys();
    for (e, end) in [
        (EndLin::Crlf, "\r\n"),
        (EndLin::Lf, "\n"),
        (EndLin::Cr, "\r"),
        (EndLin::Lfcr, "\n\r"),
    ] {
        let path = format!("/tmp/e{}.txt", end.len() * 10 + end.as_bytes()[0] as usize);
        ok(&to(&s, &path, |a| {
            a.common.stmfccsid = StmfCcsid::Ccsid(819);
            a.common.endlinfmt = e;
        }), "CPCA082");
        let want = format!("PGM{end}  SNDPGMMSG 'Hi'{end}ENDPGM{end}");
        assert_eq!(bytes(&s, &path).0, want.as_bytes());
        // And back: each line end read, *ALL and the exact one.
        for read in [EndLin::All, e] {
            let r = from(&s, &path, "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/BACK.MBR", |a| {
                a.mbropt = Opt::Replace;
                a.common.endlinfmt = read;
            });
            ok(&r, "CPCA081");
            assert_eq!(member(&s, "QCLSRC", "BACK"), member(&s, "QCLSRC", "HELLO"), "{e:?} {read:?}");
        }
    }
    // *FIXED: the records whole, no line ends.
    ok(&to(&s, "/tmp/fixed.txt", |a| {
        a.common.stmfccsid = StmfCcsid::Ccsid(819);
        a.common.endlinfmt = EndLin::Fixed;
    }), "CPCA082");
    assert_eq!(bytes(&s, "/tmp/fixed.txt").0.len(), 60);
    let r = from(&s, "/tmp/fixed.txt", "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/FIX.MBR", |a| {
        a.common.endlinfmt = EndLin::Fixed;
        a.tabexpn = false;
    });
    ok(&r, "CPCA081");
    assert_eq!(member(&s, "QCLSRC", "FIX"), member(&s, "QCLSRC", "HELLO"));
}

#[test]
fn data_member_round_trip_1252() {
    let s = sys();
    ok(&to(&s, "/tmp/d.csv", |a| {
        a.frommbr = "/qsys.lib/qgpl.lib/data.file/data.mbr".into();
        a.common.stmfccsid = StmfCcsid::Ccsid(1252);
    }), "CPCA082");
    // A data member keeps its blanks.
    assert_eq!(bytes(&s, "/tmp/d.csv").0, b"A1    \r\nB2    \r\n");
    let r = from(&s, "/tmp/d.csv", "/QSYS.LIB/QGPL.LIB/DATA.FILE/COPY.MBR", |_| {});
    ok(&r, "CPCA081");
    assert_eq!(member(&s, "DATA", "COPY"), ["A1    ", "B2    "]);
}

#[test]
fn from_stream_tabs_utf8_and_cuts() {
    let s = sys();
    put(&s, "/tmp/t.txt", "a\tb\nÉté\n".as_bytes(), 1208);
    let r = from(&s, "/tmp/t.txt", "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/TABS.MBR", |_| {});
    ok(&r, "CPCA081");
    assert_eq!(member(&s, "QCLSRC", "TABS"), [format!("{:<20}", "a       b"), format!("{:<20}", "Été")]);
    let r = from(&s, "/tmp/t.txt", "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/RAW.MBR", |a| a.tabexpn = false);
    ok(&r, "CPCA081");
    assert_eq!(member(&s, "QCLSRC", "RAW")[0], format!("{:<20}", "a\tb"));
    // A line longer than the record is cut, and the escape says so.
    put(&s, "/tmp/long.txt", b"0123456789012345678901234\nshort\n", 819);
    let r = from(&s, "/tmp/long.txt", "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/LONG.MBR", |_| {});
    refused(&r, "CPFA096", "");
    assert_eq!(member(&s, "QCLSRC", "LONG")[0], "01234567890123456789");
}

#[test]
fn existing_targets() {
    let s = sys();
    let opt = |o| move |a: &mut ToStmf| {
        a.stmfopt = o;
        a.common.stmfccsid = StmfCcsid::Ccsid(819);
        a.common.endlinfmt = EndLin::Lf;
    };
    ok(&to(&s, "/tmp/x.txt", opt(Opt::None)), "CPCA082");
    refused(&to(&s, "/tmp/x.txt", opt(Opt::None)), "CPFA097", "CPDA090");
    ok(&to(&s, "/tmp/x.txt", opt(Opt::Add)), "CPCA082");
    assert_eq!(bytes(&s, "/tmp/x.txt").0.len(), 2 * 28);
    ok(&to(&s, "/tmp/x.txt", opt(Opt::Replace)), "CPCA082");
    assert_eq!(bytes(&s, "/tmp/x.txt").0.len(), 28);
    // *ADD in another CCSID is refused; *REPLACE retags.
    let r = to(&s, "/tmp/x.txt", |a| {
        a.stmfopt = Opt::Add;
        a.common.stmfccsid = StmfCcsid::Ccsid(1208);
    });
    refused(&r, "CPFA097", "CPDA08A");
    ok(&to(&s, "/tmp/x.txt", |a| {
        a.stmfopt = Opt::Replace;
        a.common.stmfccsid = StmfCcsid::Ccsid(1208);
    }), "CPCA082");
    assert_eq!(bytes(&s, "/tmp/x.txt").1, 1208);
    // Members: *NONE refuses an existing one, *ADD appends.
    let back = "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR";
    refused(&from(&s, "/tmp/x.txt", back, |_| {}), "CPFA095", "CPDA089");
    ok(&from(&s, "/tmp/x.txt", back, |a| a.mbropt = Opt::Add), "CPCA081");
    assert_eq!(member(&s, "QCLSRC", "HELLO").len(), 6);
    ok(&from(&s, "/tmp/x.txt", back, |a| a.mbropt = Opt::Replace), "CPCA081");
    assert_eq!(member(&s, "QCLSRC", "HELLO").len(), 3);
}

#[test]
fn refusals() {
    let s = sys();
    refused(&to(&s, "/tmp/r.txt", |a| a.common.cvtdta = Cvt::Tbl), "CPFA097", "CPDA086");
    refused(&to(&s, "/tmp/r.txt", |a| a.common.tbl = Some("/x".into())), "CPFA097", "CPDA098");
    refused(
        &to(&s, "/tmp/r.txt", |a| {
            a.common.stmfccsid = StmfCcsid::Ccsid(819);
            a.common.stmfcodpag = Some(StmfCcsid::Ccsid(850));
        }),
        "CPFA097",
        "CPDA0B9",
    );
    refused(&to(&s, "/tmp/r.txt", |a| a.common.dbfccsid = Some(500)), "CPFA097", "CPDA08B");
    refused(&to(&s, "/tmp/r.txt", |a| a.frommbr = "/home/x.mbr".into()), "CPFA097", "CPDA082");
    let r = to(&s, "/tmp/r.txt", |a| a.frommbr = "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/NONE.MBR".into());
    refused(&r, "CPFA097", "CPFA0A9");
    refused(&to(&s, "/tmp", |a| a.stmfopt = Opt::Replace), "CPFA097", "CPFA0DA");
    let r = from(&s, "/tmp/none.txt", "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/N.MBR", |_| {});
    refused(&r, "CPFA095", "CPFA0A9");
    let r = from(&s, "/tmp/none.txt", "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/N.MBR", |a| {
        a.common.endlinfmt = EndLin::Fixed;
    });
    refused(&r, "CPFA08A", "");
    // The messages' data: the two paths, then the count.
    let r = to(&s, "/tmp/m.txt", |_| {});
    let m = &r.messages[0].1;
    let n = u32::from_be_bytes(m.data[..4].try_into().unwrap()) as usize;
    assert_eq!(n, SRC.len());
    assert_eq!(&m.data[m.data.len() - 4..], &3i32.to_be_bytes());
    let _ = Errno::ENOENT;
}

#[test]
fn import_files_through_the_ifs() {
    use ifscmd::stmf::StmfAut;
    use ifscmd::stmf::impf::{Write, ccsid_word, read_text, write_text};
    let s = sys();
    let w = |opt, ccsid| Write {
        opt,
        ccsid,
        aut: StmfAut::Dft,
    };
    let text = "\"Zoë\",1\r\n\"Ann\",2\r\n".as_bytes();
    // A new file: *STMF is UTF-8.
    let tag = s.c.with(|c| write_text(c, "/tmp/i.csv", text, w(Opt::None, StmfCcsid::Stmf))).unwrap();
    assert_eq!(tag, 1208);
    assert_eq!(bytes(&s, "/tmp/i.csv"), (text.to_vec(), 1208));
    let m = s.c.with(|c| write_text(c, "/tmp/i.csv", text, w(Opt::None, StmfCcsid::Stmf))).unwrap_err();
    assert_eq!(m.id, "CPDA090");
    let m = s.c.with(|c| write_text(c, "/tmp/i.csv", text, w(Opt::Add, StmfCcsid::Ccsid(819)))).unwrap_err();
    assert_eq!(m.id, "CPDA08A");
    // *ADD in the file's CCSID, *REPLACE into ISO 8859-1.
    s.c.with(|c| write_text(c, "/tmp/i.csv", text, w(Opt::Add, StmfCcsid::Stmf))).unwrap();
    assert_eq!(bytes(&s, "/tmp/i.csv").0.len(), 2 * text.len());
    s.c.with(|c| write_text(c, "/tmp/i.csv", text, w(Opt::Replace, ccsid_word("819").unwrap()))).unwrap();
    let (b, tag) = bytes(&s, "/tmp/i.csv");
    assert_eq!(tag, 819);
    assert_eq!(b.len(), text.len() - 1);
    assert_eq!(b[3], 0xEB);
    // Read back as UTF-8, from the tag or as STMFCCSID says.
    assert_eq!(s.c.with(|c| read_text(c, "/tmp/i.csv", StmfCcsid::Stmf)).unwrap(), text);
    assert_eq!(ccsid_word("*PCASCII"), Some(StmfCcsid::PcAscii));
    let m = s.c.with(|c| read_text(c, "/tmp/none.csv", StmfCcsid::Stmf)).unwrap_err();
    assert_eq!(m.id, "CPFA0A9");
}
