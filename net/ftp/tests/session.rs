// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sessions in memory: sign-on and the exit points, the attributes a
//! session starts with, `RCMD`, members in each type, RFC 3659.

use ftp::memhost::MemHost;
use ftp::net::mem::MemNet;
use ftp::session::{Config, Flow, Session};
use ftp::net::Net;
use ftp::{Host, Reply};
use regfac::{ExitAnswer, ExitRequest};
use servers::attrs::ftp::FtpAttrs;

fn world() -> MemHost {
    let mut h = MemHost::new(&[("BOB", "SECRET"), ("ANON", "NOPE")]);
    h.add_library("BOBLIB");
    h.add_file("BOBLIB", "QCLSRC", 80, true, 37, "BOB");
    h.add_member("BOBLIB", "QCLSRC", "HELLO", &["PGM", "ENDPGM"]);
    if let Some((_, p)) = h.users.get_mut("BOB") {
        p.curlib = "BOBLIB".into();
    }
    h
}

fn attrs(pairs: &[(&str, &str)]) -> FtpAttrs {
    let mut a = FtpAttrs::default();
    for (k, v) in pairs {
        a.set(k, v).unwrap();
    }
    a
}

struct T {
    s: Session<MemHost>,
    net: MemNet,
}

impl T {
    fn new(host: MemHost, a: FtpAttrs) -> Self {
        let mut s = Session::new(host, Config::new(a, "QS400T"));
        let mut net = MemNet::default();
        assert_eq!(s.greet(&mut net), Flow::Continue);
        assert_eq!(net.take_replies()[0].code, 220);
        Self { s, net }
    }

    fn cmd(&mut self, line: &str) -> Vec<Reply> {
        self.s.handle(line, &mut self.net);
        self.net.take_replies()
    }

    fn code(&mut self, line: &str) -> u16 {
        self.cmd(line).last().map_or(0, |r| r.code)
    }

    fn text(&mut self, line: &str) -> String {
        self.cmd(line).last().map(|r| r.lines.join("\n")).unwrap_or_default()
    }

    fn login(&mut self) {
        assert_eq!(self.code("USER bob"), 331);
        assert_eq!(self.code("PASS SECRET"), 230);
    }

    fn get(&mut self, line: &str) -> Vec<u8> {
        self.net.passive(false).unwrap();
        let r = self.cmd(line);
        // Transfers end in 226, listings in 250 (IBM's).
        assert!(matches!(r.last().map(|r| r.code), Some(226 | 250)), "{line}: {r:?}");
        self.net.take_sent().pop().unwrap_or_default()
    }

    fn put(&mut self, line: &str, data: &[u8]) -> Vec<Reply> {
        self.net.uploads.push_back(data.to_vec());
        self.cmd(line)
    }
}

#[test]
fn sign_on_rules() {
    let mut t = T::new(world(), FtpAttrs::default());
    assert_eq!(t.code("PWD"), 530);
    assert_eq!(t.code("PASS x"), 503);
    assert_eq!(t.code("USER bob"), 331);
    assert_eq!(t.code("PASS wrong"), 530);
    t.login();
    assert_eq!(t.code("USER ANON"), 501, "REIN first");
    assert_eq!(t.code("REIN"), 220);
    assert_eq!(t.code("PWD"), 530);
    assert_eq!(t.code("QUIT"), 221);
}

#[test]
fn the_default_attributes_start_in_the_current_library() {
    // NAMEFMT(*LIB) CURDIR(*CURLIB) LISTFMT(*DFT).
    let mut t = T::new(world(), FtpAttrs::default());
    t.login();
    assert_eq!(t.text("PWD"), "\"BOBLIB\" is current library.");
    let list = String::from_utf8(t.get("LIST")).unwrap();
    assert!(list.contains("*FILE      QCLSRC"), "{list}");
    assert!(list.contains("*MEM       QCLSRC.HELLO"), "{list}");
    let text = String::from_utf8(t.get("RETR QCLSRC.HELLO")).unwrap();
    assert_eq!(text, "PGM\r\nENDPGM\r\n");
}

#[test]
fn the_home_directory_and_paths() {
    let mut t = T::new(world(), attrs(&[("NAMEFMT", "*PATH"), ("CURDIR", "*HOMEDIR"), ("LISTFMT", "*UNIX")]));
    t.login();
    assert_eq!(t.text("PWD"), "\"/home/BOB\" is current directory.");
    assert_eq!(t.code("SITE NAMEFMT 0"), 550, "the home is not a library");
    assert_eq!(t.put("STOR a.txt", b"one\r\ntwo\r\n").last().unwrap().code, 226);
    assert_eq!(t.get("RETR a.txt"), b"one\r\ntwo\r\n");
    assert_eq!(t.text("SIZE a.txt"), "10");
    assert_eq!(t.text("MDTM a.txt").len(), 14);
    let facts = t.text("MLST a.txt");
    assert!(facts.contains("type=file;size=10;"), "{facts}");
    // REST: the next RETR starts there.
    assert_eq!(t.code("REST 5"), 350);
    assert_eq!(t.get("RETR a.txt"), b"two\r\n");
    // STOU picks a new name.
    let r = t.put("STOU a.txt", b"x");
    assert!(r[0].lines[0].contains("a.txt1"), "{r:?}");
    // A member by path, in EBCDIC.
    assert_eq!(t.code("TYPE E"), 200);
    let e = t.get("RETR /QSYS.LIB/BOBLIB.LIB/QCLSRC.FILE/HELLO.MBR");
    assert_eq!(e, [0xD7, 0xC7, 0xD4, 0x15, 0xC5, 0xD5, 0xC4, 0xD7, 0xC7, 0xD4, 0x15]);
}

#[test]
fn members_files_made_and_records() {
    let mut t = T::new(world(), FtpAttrs::default());
    t.login();
    // A file STOR makes: records as long as the longest line.
    assert_eq!(t.put("STOR NEWF.M1", b"abc\r\nabcdef\r\n").last().unwrap().code, 226);
    assert!(t.s.host.commands.iter().any(|c| c.starts_with("CRTPF FILE(BOBLIB/NEWF) RCDLEN(6) MBR(M1)")));
    assert_eq!(t.s.host.member_text("BOBLIB", "NEWF", "M1").unwrap(), ["abc", "abcdef"]);
    // A second member in it; a line too long is cut and said so.
    let r = t.put("STOR NEWF.M2", b"1234567890\r\n");
    assert_eq!(r.last().unwrap().code, 426);
    assert_eq!(t.s.host.member_text("BOBLIB", "NEWF", "M2").unwrap(), ["123456"]);
    // APPE adds records.
    assert_eq!(t.put("APPE NEWF.M1", b"z\r\n").last().unwrap().code, 226);
    assert_eq!(t.s.host.member_text("BOBLIB", "NEWF", "M1").unwrap(), ["abc", "abcdef", "z"]);
    // Binary: records of 512 bytes, padded with nulls.
    assert_eq!(t.code("TYPE I"), 200);
    assert_eq!(t.put("STOR BINF", &[1, 2, 3]).last().unwrap().code, 226);
    let back = t.get("RETR BINF");
    assert_eq!(back.len(), 512);
    assert_eq!(&back[..4], &[1, 2, 3, 0]);
    // Rename a member, then the file; delete the file by name.
    assert_eq!(t.code("RNFR NEWF.M2"), 350);
    assert_eq!(t.code("RNTO NEWF.M3"), 250);
    assert_eq!(t.code("RNFR NEWF"), 350);
    assert_eq!(t.code("RNTO NEWG"), 250);
    assert!(t.s.host.member_text("BOBLIB", "NEWG", "M3").is_some());
    assert_eq!(t.code("DELE NEWG"), 250);
    assert!(t.s.host.member_text("BOBLIB", "NEWG", "M3").is_none());
    // A member not in its file.
    assert_eq!(t.code("RETR QCLSRC.NONE"), 426);
    assert_eq!(t.code("RETR NOFILE.X"), 550);
}

#[test]
fn rcmd_runs_in_the_job() {
    let mut t = T::new(world(), FtpAttrs::default());
    t.login();
    assert_eq!(t.code("RCMD CRTSRCPF FILE(BOBLIB/QRPGSRC) RCDLEN(112)"), 250);
    let r = t.cmd("RCMD CHKOBJ OBJ(BOBLIB/NOPE) OBJTYPE(*FILE)");
    assert_eq!(r[0].code, 550);
    assert_eq!(r[0].lines.len(), 2);
    assert!(r[0].lines[1].contains("NOPE"));
    assert_eq!(t.code("RCMD"), 501);
    assert_eq!(t.code("RCMD CHGCURLIB CURLIB(QGPL)"), 250);
    assert_eq!(t.text("PWD"), "\"QGPL\" is current library.");
}

/// The answer an exit program gives: `parms` changed by `f`.
fn answer(req: &ExitRequest, f: impl FnOnce(&mut Vec<Vec<u8>>)) -> ExitAnswer {
    let mut outputs = req.parms();
    f(&mut outputs);
    let allow = req.allowed(&outputs);
    ExitAnswer {
        allow,
        outputs,
        called: vec![regfac::Called {
            point: String::new(),
            format: req.format().to_string(),
            pgm: "EXITPGM".into(),
            lib: "QGPL".into(),
            rc: String::new(),
            allowed: allow,
            failed: None,
        }],
        error: None,
    }
}

#[test]
fn anonymous_ftp_through_the_logon_exit() {
    let mut h = world();
    h.exits = Some(Box::new(|point, req| {
        if point != "QIBM_QTMF_SVR_LOGON" || req.format() != "TCPL0100" {
            return None;
        }
        let ExitRequest::Logon { logon, .. } = req else { return None };
        let anonymous = logon.user_id.eq_ignore_ascii_case("anonymous");
        Some(answer(req, |p| {
            // Return code 3: sign on as the profile returned, no
            // password; 0 refuses.
            p[7] = if anonymous { 3i32 } else { 0 }.to_be_bytes().to_vec();
            p[8] = ccsid::to_ebcdic("ANON      ").unwrap();
        }))
    }));
    let mut t = T::new(h, FtpAttrs::default());
    assert_eq!(t.code("USER anonymous"), 331);
    assert_eq!(t.code("PASS guest@example.com"), 230);
    assert_eq!(t.s.user(), Some("ANON"));
    assert_eq!(t.code("REIN"), 220);
    assert_eq!(t.code("USER bob"), 331);
    assert_eq!(t.code("PASS SECRET"), 530, "the exit refuses everyone else");
}

#[test]
fn the_request_exit_refuses() {
    let mut h = world();
    h.exits = Some(Box::new(|point, req| {
        let ExitRequest::Validate(v) = req else { return None };
        if point != "QIBM_QTMF_SERVER_REQ" {
            return None;
        }
        let allow = v.op != regfac::formats::tcp::op::SEND_FILE;
        Some(answer(req, |p| p[7] = i32::from(allow).to_be_bytes().to_vec()))
    }));
    let mut t = T::new(h, FtpAttrs::default());
    t.login();
    assert_eq!(t.code("RETR QCLSRC.HELLO"), 550);
    assert_eq!(t.code("CWD QGPL"), 250);
    // A session the exit refuses outright.
    let mut h = world();
    h.exits = Some(Box::new(|_, req| Some(answer(req, |p| p[7] = 0i32.to_be_bytes().to_vec()))));
    let mut s = Session::new(h, Config::new(FtpAttrs::default(), "QS400T"));
    let mut net = MemNet::default();
    assert_eq!(s.greet(&mut net), Flow::Close);
    assert_eq!(net.take_replies()[0].code, 421);
}

#[test]
fn listing_in_ebcdic_ends_lines_with_nl() {
    let mut t = T::new(world(), FtpAttrs::default());
    t.login();
    assert_eq!(t.code("TYPE E"), 200);
    let l = t.get("NLST QCLSRC.*");
    assert_eq!(l, ccsid::to_ebcdic("QCLSRC.HELLO\u{85}").unwrap());
    assert_eq!(t.s.host.job_ccsid(), 37);
}
