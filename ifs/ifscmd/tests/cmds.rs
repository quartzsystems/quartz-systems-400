// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS commands over an IFS in memory, as the service runs them for
//! a job: the work done, IBM's completion and escape messages with
//! their data, the text forms of the displays.

#![allow(clippy::many_single_char_names)] // c, s, r, a, d, m: client, system, reply, arguments

use std::sync::Arc;

use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::{IfsClient, LocalIfs, read_all, write_all};
use ifs::ops::audit::MemAudit;
use ifs::ops::flags::{O_CCSID, O_CREAT, O_RDONLY, O_WRONLY};
use ifs::ops::{Attr, AttrId, Config, Ifs, OpenArgs, Owner};
use ifs::vfs::qsys::{MemQsys, MemberData};
use ifscmd::cmds::{Subtree, admin, attr, auth, dirs, links, udfs};
use ifscmd::{Msg, Reply, display, text};
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::profile::spcaut::SpecialAuth;

fn principal(user: &str) -> Principal {
    let mut p = Principal {
        user: user.into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    if user == "QSECOFR" {
        p.spcaut = SpecialAuth::ALLOBJ.0 | SpecialAuth::IOSYSCFG.0 | SpecialAuth::AUDIT.0;
    }
    if user == "ANN" {
        p.groups = vec!["DEV".into()];
    }
    p
}

/// One IFS, callers on it by name.
struct Sys {
    ifs: Arc<Ifs<MemBackend>>,
    env: Arc<MemEnv>,
}

impl Sys {
    fn new() -> Self {
        let q = MemQsys::new(Auth::USE);
        q.add_lib("QGPL");
        q.add_obj("QGPL", "QCLSRC", "*FILE", "PF-SRC", "QSYS");
        q.add_member(
            "QGPL",
            "QCLSRC",
            "HELLO",
            MemberData {
                records: vec![ccsid::conv::from_str(37, &format!("{:<20}", "PGM")).unwrap()],
                record_len: 20,
                source: true,
                ccsid: 37,
            },
        );
        let env = MemEnv::default()
            .user("QSYS", 0)
            .user("QSECOFR", 1)
            .user("BOB", 100)
            .user("ANN", 101)
            .user("QDFTOWN", 3)
            .group("DEV", 200, 500);
        let ifs = Ifs::new(MemBackend::new(), Box::new(q), Config::default());
        ifs.ensure_layout(&env).unwrap();
        Self {
            ifs: Arc::new(ifs),
            env: Arc::new(env),
        }
    }

    fn as_user(&self, user: &str, owner: u64) -> LocalIfs<MemBackend> {
        LocalIfs {
            ifs: Arc::clone(&self.ifs),
            env: Arc::clone(&self.env) as Arc<dyn ifs::auth::AuthEnv>,
            audit: Arc::new(MemAudit::default()),
            principal: principal(user),
            owner: Owner(owner),
            job_ccsid: 37,
        }
    }
}

fn put(c: &dyn IfsClient, path: &str, data: &[u8], ccsid: u32) {
    let fd = c
        .open(path, OpenArgs::new(O_WRONLY | O_CREAT | O_CCSID).mode(0o644).conv(ccsid))
        .unwrap();
    write_all(c, fd, data).unwrap();
    c.close(fd).unwrap();
}

fn get(c: &dyn IfsClient, path: &str) -> Vec<u8> {
    let fd = c.open(path, OpenArgs::new(O_RDONLY)).unwrap();
    let d = read_all(c, fd).unwrap();
    c.close(fd).unwrap();
    d
}

/// The completion messages' identifiers.
fn completions(r: &Reply) -> Vec<String> {
    r.messages
        .iter()
        .filter(|(k, _)| *k == ifscmd::msg::COMPLETION)
        .map(|(_, m)| m.id.clone())
        .collect()
}

fn done(r: &Reply, id: &str) {
    assert!(r.escape.is_none(), "{id}: escape {:?}", r.escape);
    assert!(completions(r).contains(&id.to_string()), "{id} not in {r:?}");
}

fn refused(r: &Reply, id: &str) {
    assert_eq!(r.escape.as_ref().map(|m| m.id.as_str()), Some(id), "{r:?}");
}

/// The first `*CHAR *VARY 4` value of a message, as text.
fn first_path(m: &Msg) -> String {
    let n = u32::from_be_bytes(m.data[..4].try_into().unwrap()) as usize;
    ccsid::from_ebcdic(&m.data[4..4 + n])
}

#[test]
fn directories_and_the_current_directory() {
    let s = Sys::new();
    let c = s.as_user("QSECOFR", 1);
    let r = c.with(|c| dirs::md(c, &dirs::Md::new("/home/bob")));
    done(&r, "CPCA083");
    assert_eq!(first_path(&r.messages[0].1), "/home/bob");
    let r = c.with(|c| dirs::cd(c, "/home/bob"));
    done(&r, "CPCA08B");
    assert_eq!(c.with(|c| c.getcwd()).unwrap(), "/home/bob");
    // Relative to the current directory, and the directory stays per owner.
    done(&c.with(|c| dirs::md(c, &dirs::Md::new("sub"))), "CPCA083");
    assert!(c.with(|c| c.stat("/home/bob/sub", true)).is_ok());
    let other = s.as_user("QSECOFR", 2);
    assert_eq!(other.with(|c| c.getcwd()).unwrap(), "/");
    // Twice is CPFA0A0, naming the directory.
    let r = c.with(|c| dirs::md(c, &dirs::Md::new("sub")));
    refused(&r, "CPFA0A0");
    assert_eq!(first_path(r.escape.as_ref().unwrap()), "sub");
    // A public data authority given explicitly.
    let mut md = dirs::Md::new("/home/bob/priv");
    md.dtaaut = "*EXCLUDE".into();
    md.objaut = vec!["*NONE".into()];
    done(&c.with(|c| dirs::md(c, &md)), "CPCA083");
    let a = c.with(|c| c.dspaut("/home/bob/priv")).unwrap();
    assert_eq!(a.entries[0].who, "*PUBLIC");
    assert_eq!(a.entries[0].data, "*EXCLUDE");
    // Not empty, then the subtree.
    put(&c, "/home/bob/sub/f", b"x", 819);
    refused(&c.with(|c| dirs::rd(c, "/home/bob/sub", Subtree::None, false)), "CPFA0AC");
    refused(&c.with(|c| dirs::rd(c, "sub", Subtree::All, true)), "CPDA0B8");
    done(&c.with(|c| dirs::rd(c, "/home/bob/sub", Subtree::None, true)), "CPCA085");
    done(&c.with(|c| dirs::md(c, &dirs::Md::new("/home/bob/t/"))), "CPCA083");
    put(&c, "/home/bob/t/g", b"y", 819);
    done(&c.with(|c| dirs::rd(c, "/home/bob/t", Subtree::All, false)), "CPCA085");
    refused(&c.with(|c| dirs::cd(c, "/nowhere")), "CPFA0A9");
}

#[test]
fn links_rename_move_copy_and_patterns() {
    let s = Sys::new();
    let c = s.as_user("QSECOFR", 1);
    done(&c.with(|c| dirs::md(c, &dirs::Md::new("/w"))), "CPCA083");
    for f in ["a.txt", "b.txt", "c.log"] {
        put(&c, &format!("/w/{f}"), f.as_bytes(), 819);
    }
    // A pattern removes each match.
    let r = c.with(|c| links::del(c, "/w/*.TXT"));
    assert!(r.escape.is_none(), "{r:?}");
    assert_eq!(completions(&r), ["CPCA089", "CPCA089"]);
    refused(&c.with(|c| links::del(c, "/w/*.txt")), "CPFA093");
    refused(&c.with(|c| links::del(c, "/w/nothing")), "CPFA0A9");
    // Rename: a name, not a path.
    refused(&c.with(|c| links::ren(c, "/w/c.log", "/x")), "CPFA0B4");
    refused(&c.with(|c| links::ren(c, "/w/c.log", "a/b")), "CPFA0B5");
    done(&c.with(|c| links::ren(c, "/w/c.log", "d.log")), "CPCA084");
    // Copy with conversion to CCSID 37, then move.
    let a = links::Cpy {
        obj: "/w/d.log".into(),
        todir: "/tmp".into(),
        to_ccsid: Some(37),
        ..links::Cpy::default()
    };
    done(&c.with(|c| links::cpy(c, &a)), "CPCA087");
    assert_eq!(get(&c, "/tmp/d.log"), ccsid::conv::from_str(37, "c.log").unwrap());
    refused(&c.with(|c| links::cpy(c, &a)), "CPFA097");
    let a = links::Cpy {
        replace: true,
        ..a
    };
    done(&c.with(|c| links::cpy(c, &a)), "CPCA087");
    let m = links::Mov {
        obj: "/w/d.log".into(),
        todir: "/tmp".into(),
        toobj: Some("/tmp/e.log".into()),
        to_ccsid: None,
    };
    refused(&c.with(|c| links::mov(c, &m)), "CPFA0B3");
    let m = links::Mov {
        todir: String::new(),
        ..m
    };
    done(&c.with(|c| links::mov(c, &m)), "CPCA086");
    assert_eq!(get(&c, "/tmp/e.log"), b"c.log");
    // A directory needs SUBTREE.
    let d = links::Cpy {
        obj: "/w".into(),
        toobj: Some("/w2".into()),
        todir: String::new(),
        ..links::Cpy::default()
    };
    refused(&c.with(|c| links::cpy(c, &d)), "CPFA097");
    let d = links::Cpy {
        subtree: links::CpySubtree::All,
        ..d
    };
    done(&c.with(|c| links::cpy(c, &d)), "CPCA087");
    // Links, symbolic and hard.
    done(&c.with(|c| links::addlnk(c, "/tmp/e.log", "/tmp/sym", false)), "CPCA088");
    assert_eq!(c.with(|c| c.readlink("/tmp/sym")).unwrap(), "/tmp/e.log");
    done(&c.with(|c| links::addlnk(c, "/tmp/e.log", "/tmp/hard", true)), "CPCA088");
    assert_eq!(c.with(|c| c.stat("/tmp/e.log", true)).unwrap().nlink, 2);
}

#[test]
fn authority_owner_and_group() {
    let s = Sys::new();
    let sec = s.as_user("QSECOFR", 1);
    done(&sec.with(|c| dirs::md(c, &dirs::Md::new("/p"))), "CPCA083");
    put(&sec, "/p/f", b"data", 819);
    let ch = auth::Chgaut {
        obj: "/p/f".into(),
        users: vec!["*PUBLIC".into()],
        dtaaut: "*R".into(),
        objaut: vec!["*NONE".into()],
        ..auth::Chgaut::default()
    };
    let r = sec.with(|c| auth::chgaut(c, &ch));
    assert!(r.escape.is_none() && r.text.contains("1 object"), "{r:?}");
    let ch = auth::Chgaut {
        users: vec!["BOB".into()],
        dtaaut: "*RWX".into(),
        objaut: vec!["*OBJMGT".into()],
        ..ch
    };
    assert!(!sec.with(|c| auth::chgaut(c, &ch)).failed());
    let rows = sec.with(|c| c.dspaut("/p/f")).unwrap();
    let bob = rows.entries.iter().find(|e| e.who == "BOB").unwrap();
    assert_eq!(bob.data, "*RWX");
    // BOB may now write.
    let bob_c = s.as_user("BOB", 3);
    assert!(bob_c.with(|c| c.open("/p/f", OpenArgs::new(O_WRONLY))).is_ok());
    // *NONE and *NONE removes the private authority.
    let ch = auth::Chgaut {
        dtaaut: "*NONE".into(),
        objaut: vec!["*NONE".into()],
        ..ch
    };
    assert!(!sec.with(|c| auth::chgaut(c, &ch)).failed());
    let rows = sec.with(|c| c.dspaut("/p/f")).unwrap();
    assert!(rows.entries.iter().all(|e| e.who != "BOB"));
    // Patterns are not user names; *INDIR and *SAME stand alone.
    let bad = auth::Chgaut {
        users: vec!["B*".into()],
        dtaaut: "*R".into(),
        ..ch.clone()
    };
    refused(&sec.with(|c| auth::chgaut(c, &bad)), "CPFA091");
    let bad = auth::Chgaut {
        users: vec!["BOB".into()],
        objaut: vec!["*SAME".into(), "*OBJREF".into()],
        ..ch
    };
    refused(&sec.with(|c| auth::chgaut(c, &bad)), "CPFA0C3");
    // A new owner, then a group with the data authority given.
    let r = sec.with(|c| auth::chgown(c, "/p/f", "BOB", true, Subtree::None, false));
    assert!(!r.failed(), "{r:?}");
    assert_eq!(sec.with(|c| c.stat("/p/f", true)).unwrap().owner, "BOB");
    let g = auth::Chgpgp {
        obj: "/p/f".into(),
        newpgp: "DEV".into(),
        dtaaut: "*RX".into(),
        objaut: vec!["*NONE".into()],
        rvkoldaut: true,
        ..auth::Chgpgp::default()
    };
    assert!(!sec.with(|c| auth::chgpgp(c, &g)).failed());
    let rows = sec.with(|c| c.dspaut("/p/f")).unwrap();
    assert_eq!(rows.group.as_deref(), Some("DEV"));
    assert!(rows.entries.iter().any(|e| e.who == "DEV" && e.data == "*RX"));
    // The subtree: every object, counted.
    put(&sec, "/p/g", b"x", 819);
    let r = sec.with(|c| auth::chgown(c, "/p", "ANN", false, Subtree::All, false));
    assert!(r.text.contains("3 objects"), "{r:?}");
    // Not the owner: refused with the object named.
    let r = bob_c.with(|c| auth::chgown(c, "/p/g", "BOB", true, Subtree::None, false));
    refused(&r, "CPFA09C");
}

#[test]
fn attributes_checkout_and_journaling() {
    let s = Sys::new();
    let c = s.as_user("QSECOFR", 1);
    put(&c, "/tmp/f", b"abc", 819);
    let r = c.with(|c| attr::chgatr(c, "/tmp/f", attr::atr::READONLY, attr::val::YES, "", Subtree::None, false));
    assert!(!r.failed(), "{r:?}");
    let e = c.with(|c| c.open("/tmp/f", OpenArgs::new(O_WRONLY))).unwrap_err();
    refused(&Reply::errno(e, "/tmp/f"), "CPFA0B1");
    let r = c.with(|c| attr::chgatr(c, "/tmp/f", attr::atr::CCSID, 1208, "", Subtree::None, false));
    assert!(!r.failed(), "{r:?}");
    assert_eq!(c.with(|c| c.stat("/tmp/f", true)).unwrap().ccsid, 1208);
    let r = c.with(|c| attr::chgatr(c, "/tmp/f", attr::atr::TEXT, attr::val::NONE, "a probe", Subtree::None, false));
    assert!(!r.failed(), "{r:?}");
    let t = c
        .with(|c| c.get_attrs("/tmp/f", &[AttrId::Text], false))
        .unwrap();
    assert_eq!(t, vec![Attr::Text("a probe".into())]);
    refused(
        &c.with(|c| attr::chgatr(c, "/tmp/f", attr::atr::TEXT, attr::val::YES, "x", Subtree::None, false)),
        "CPDA0C2",
    );
    refused(
        &c.with(|c| attr::chgatr(c, "/tmp/f", attr::atr::HIDDEN, attr::val::NO, "x", Subtree::None, false)),
        "CPDA0C1",
    );
    refused(
        &c.with(|c| attr::chgatr(c, "/tmp/f", attr::atr::HIDDEN, attr::val::DYNAMIC, "", Subtree::None, false)),
        "CPDA03D",
    );
    refused(
        &c.with(|c| attr::chgatr(c, "/tmp/f", attr::atr::RUNEXIT, attr::val::YES, "", Subtree::None, false)),
        "CPFA0ED",
    );
    // Check-out and check-in.
    put(&c, "/tmp/g", b"g", 819);
    done(&c.with(|c| attr::checkout(c, "/tmp/g", Subtree::None, true)), "CPCA08F");
    let bob = s.as_user("BOB", 2);
    refused(&bob.with(|c| attr::checkout(c, "/tmp/g", Subtree::None, false)), "CPFA09C");
    done(&c.with(|c| attr::checkout(c, "/tmp/g", Subtree::None, false)), "CPCA08A");
    // Journaling, stored.
    let j = attr::StrJrn {
        objs: vec!["/tmp/g".into()],
        jrn: "QGPL/JRN".into(),
        inherit: false,
        both: true,
        ..attr::StrJrn::default()
    };
    let r = c.with(|c| attr::strjrn(c, &j));
    assert!(!r.failed() && r.text.contains("started for 1 object"), "{r:?}");
    let info = c
        .with(|c| c.get_attrs("/tmp/g", &[AttrId::JournalInformation], false))
        .unwrap();
    match &info[0] {
        Attr::JournalInformation(Some(j)) => {
            assert_eq!(j.journal, "QGPL/JRN");
            assert_eq!(j.images, 1);
        }
        other => panic!("{other:?}"),
    }
    assert!(c.with(|c| attr::strjrn(c, &j)).failed());
    let r = c.with(|c| attr::endjrn(c, &["/tmp/g".into()], Subtree::None));
    assert!(!r.failed() && r.text.contains("ended"), "{r:?}");
}

#[test]
fn user_defined_file_systems() {
    let s = Sys::new();
    let c = s.as_user("QSECOFR", 1);
    let u = "/dev/QASP01/mine.udfs";
    done(&c.with(|c| udfs::crtudfs(c, u, false, "*SYSVAL", "probe")), "CPCA1C0");
    done(&c.with(|c| dirs::md(c, &dirs::Md::new("/mnt"))), "CPCA083");
    refused(&c.with(|c| udfs::mount(c, "*NFS", "host:/x", "/mnt", "rw")), "CPFA0AD");
    done(&c.with(|c| udfs::mount(c, "*UDFS", u, "/mnt", "rw")), "CPCA1B0");
    let r = c.with(|c| udfs::dspudfs(c, u));
    assert!(r.text.contains("Mounted over") && r.text.contains("/mnt"), "{}", r.text);
    assert!(r.text.contains("*MONO") && r.text.contains("probe"), "{}", r.text);
    refused(&c.with(|c| udfs::dltudfs(c, u)), "CPFA09E");
    // BOB lacks *IOSYSCFG.
    let bob = s.as_user("BOB", 2);
    assert!(bob.with(|c| udfs::unmount(c, "*UDFS", u, "")).failed());
    done(&c.with(|c| udfs::unmount(c, "*UDFS", "", "/mnt")), "CPCA1B1");
    done(&c.with(|c| udfs::dltudfs(c, u)), "CPCA1C1");
}

#[test]
fn text_forms() {
    let s = Sys::new();
    let c = s.as_user("QSECOFR", 1);
    done(&c.with(|c| dirs::md(c, &dirs::Md::new("/home/q"))), "CPCA083");
    put(&c, "/home/q/qsnet.txt", b"Quartz IFS probe\r\nline two\r\n\tline three after a tab\r\n", 819);
    put(&c, "/home/q/a-very-long-name-for-a-file.txt", b"x", 819);
    let r = c.with(|c| text::links_text(c, "/home/q/*", "*ALL"));
    let lines: Vec<&str> = r.text.lines().collect();
    assert_eq!(lines[0].trim_end(), " Directory  . . . . :   /home/q");
    assert!(lines[2].starts_with(" a-very-long-name-f >   STMF"), "{}", lines[2]);
    assert!(lines[3].starts_with(" qsnet.txt              STMF"), "{}", lines[3]);
    let r = c.with(|c| text::links_text(c, "/", "*ALL"));
    assert!(r.text.contains(" /                      DIR"), "{}", r.text);
    let r = c.with(|c| text::links_text(c, "/home/q/*", "*DIR"));
    refused(&r, "CPFA0B2");
    let r = c.with(|c| text::file_text(c, "/home/q/qsnet.txt"));
    assert_eq!(r.text, "Quartz IFS probe\nline two\n line three after a tab\n");
    refused(&c.with(|c| text::file_text(c, "/home/q")), "CPFA0DA");
    let r = c.with(|c| text::authority_text(c, "/home/q/qsnet.txt"));
    assert!(r.text.contains(" Owner  . . . . . . . . . . . . :   QSECOFR"), "{}", r.text);
    assert!(r.text.lines().any(|l| l.starts_with(" QSECOFR     *RW "))
        && r.text.contains('X'), "{}", r.text);
    let r = c.with(|c| text::attributes_text(c, "/home/q/qsnet.txt", true));
    assert!(r.text.contains("Coded character set ID"), "{}", r.text);
    assert!(r.text.contains(":   819"), "{}", r.text);
    let v = c
        .with(|c| display::attributes(c, "/home/q/qsnet.txt", false))
        .unwrap();
    assert!(v.contains(&("AUDIT", "*NOTAVL".into())));
    assert!(v.contains(&("SIZE", "53".into())));
}

#[test]
fn reclaim_and_save() {
    let s = Sys::new();
    let c = s.as_user("QSECOFR", 1);
    let r = c.with(|c| admin::rcllnk(c, true));
    assert!(r.escape.is_none() && r.text.contains("object links checked"), "{r:?}");
    let r = c.with(|c| admin::sav(c, &["/home".into()], true));
    refused(&r, "CPF3700");
    assert!(r.text.contains("Phase 11"), "{}", r.text);
    // Members show through /QSYS.LIB as the commands see them.
    let l = c
        .with(|c| display::links(c, "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/*", "*ALL"))
        .unwrap();
    assert_eq!(l.links.len(), 1);
    assert_eq!(l.links[0].name, "HELLO.MBR");
    assert_eq!(l.links[0].objtype, "MBR");
}
