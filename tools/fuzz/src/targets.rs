// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every parser that takes bytes from outside the system, as a fuzz
//! target: `run` must return (any `Err` is fine) and never panic or hang.

use std::io::Cursor;
use std::sync::OnceLock;

/// One fuzz target.
pub struct Target {
    /// Its name on the command line and in `tools/fuzz/corpus/`.
    pub name: &'static str,
    /// What it covers.
    pub what: &'static str,
    /// The code under test.
    pub run: fn(&[u8]),
    /// Seed inputs for an empty corpus.
    pub seeds: &'static [&'static [u8]],
    /// Tokens mutations insert.
    pub dict: &'static [&'static [u8]],
    /// The longest input tried.
    pub max_len: usize,
}

fn text(data: &[u8]) -> String {
    String::from_utf8_lossy(data).into_owned()
}

fn dds(data: &[u8]) {
    let (src, _) = dds::Source::from_text(&text(data));
    let opts = dds::CompileOpts::default();
    let _ = dds::compile_pf(&src, &dds::NoRefs, &opts);
    let _ = dds::compile_lf(&src, &dds::NoRefs, &opts);
    let _ = dds::compile_prtf(&src, &dds::NoRefs, &opts);
}

/// The shipped command definitions, built once.
fn commands() -> &'static cmd::analyze::MemSource {
    static DEFS: OnceLock<cmd::analyze::MemSource> = OnceLock::new();
    DEFS.get_or_init(|| {
        let mut mem = cmd::analyze::MemSource::default();
        for (n, _) in sys_cpps::defs::DEFS {
            if let Ok(o) = sys_cpps::install::object(n, 0) {
                mem.put(o.def);
            }
        }
        mem
    })
}

/// The shipped definitions as a source the analyzer reads.
struct Shipped;

impl cmd::analyze::CmdSource for Shipped {
    fn lookup(
        &mut self,
        name: &str,
        lib: Option<&str>,
    ) -> cmd::error::Result<Option<(cmd::payload::CmdObject, String)>> {
        if lib.is_some_and(|l| l != "QSYS" && l != "*LIBL") {
            return Ok(None);
        }
        Ok(commands()
            .cmds
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, o)| (o.clone(), "QSYS".to_string())))
    }
}

fn cl(data: &[u8]) {
    let mut src = Shipped;
    let mut env = cmd::analyze::Env::new(&mut src);
    env.setting = cmd::analyze::Setting {
        interactive: false,
        program: true,
        exec: false,
        module: false,
        syntax_only: false,
    };
    let opts = cl::Options {
        name: "FUZZ".into(),
        ..cl::Options::default()
    };
    let _ = cl::compile(&text(data), &opts, &mut env, &mut cl::NoFiles);
}

fn rpg(data: &[u8]) {
    let member = rpgfront::Member::from_text("FUZZ", "QRPGLESRC", "FUZZ", &text(data));
    let opts = rpgfront::reader::Options {
        defines: Vec::new(),
        copynest: 32,
    };
    let _ = rpgfront::front(member, &mut rpgfront::NoIncludes, &opts);
}

fn tn5250(data: &[u8]) {
    let _ = tn5250::ds::parse::parse(data);
    let _ = tn5250::record::Record::decode(data);
    let mut p = tn5250::telnet::Parser::new();
    let _ = p.feed(data);
    let mut m = tn5250::screen::ScreenModel::new(false);
    m.field(5, 10, 20, 0x24, "", tn5250::ds::field::Ffw::default(), &[]);
    let _ = m.accept(data);
}

fn qap(data: &[u8]) {
    let mut r = qapd::frames::FrameReader::new();
    r.push(data);
    while let Ok(Some(env)) = r.next_frame() {
        let b = &env.body;
        let _ = ipc::decode::<qap_proto::ops::ifs::ListDirectory>(b);
        let _ = ipc::decode::<qap_proto::ops::ifs::PathRef>(b);
        let _ = ipc::decode::<qap_proto::ops::ifs::ReadFile>(b);
        let _ = ipc::decode::<qap_proto::ops::ifs::FromTo>(b);
        qap_v8(b);
    }
    let _ = ipc::decode::<qap_proto::Envelope>(data);
}

/// QAP version 8's request bodies (`Develop`, `Jobs`, `Sql` 12 and 13).
fn qap_v8(b: &[u8]) {
    use qap_proto::ops::{develop as d, jobs as j, sql as s};
    let _ = ipc::decode::<d::SetLibraryList>(b);
    let _ = ipc::decode::<d::ListLibraries>(b);
    let _ = ipc::decode::<d::ListObjects>(b);
    let _ = ipc::decode::<d::DescribeObject>(b);
    let _ = ipc::decode::<d::ListMembers>(b);
    let _ = ipc::decode::<d::ReadMember>(b);
    let _ = ipc::decode::<d::WriteMember>(b);
    let _ = ipc::decode::<d::MemberRef>(b);
    let _ = ipc::decode::<d::CreateMember>(b);
    let _ = ipc::decode::<d::CopyMember>(b);
    let _ = ipc::decode::<d::RenameMember>(b);
    let _ = ipc::decode::<d::ChangeMember>(b);
    let _ = ipc::decode::<d::CompileReply>(b);
    let _ = ipc::decode::<j::ListJobs>(b);
    let _ = ipc::decode::<j::JobLog>(b);
    let _ = ipc::decode::<j::JobRef>(b);
    let _ = ipc::decode::<j::EndJob>(b);
    let _ = ipc::decode::<s::ListRoutines>(b);
    let _ = ipc::decode::<s::ListIndexes>(b);
}

fn hostsvr(data: &[u8]) {
    let _ = hostsvr::Frame::decode(data);
    let _ = hostsvr::codec::decode(hostsvr::Dir::Request, data);
    let _ = hostsvr::codec::decode(hostsvr::Dir::Reply, data);
}

fn save(data: &[u8]) {
    if let Ok(mut r) = savfmt::Reader::new(Cursor::new(data)) {
        for _ in 0..64 {
            match r.next_object() {
                Ok(Some(_)) => {}
                _ => break,
            }
        }
    }
    let _ = savfmt::read_index(&mut Cursor::new(data));
}

fn ptf(data: &[u8]) {
    let _ = ptf::package::open(data, &[[7u8; 32]]);
    let _ = ptf::PtfHeader::decode(data);
    let _ = ptf::group::verify_signed(&text(data), &[[7u8; 32]]);
}

fn rsp(data: &[u8]) {
    let _ = instmedia::Response::parse(&text(data));
}

fn iso(data: &[u8]) {
    let img = optical::MemImage::new(data.to_vec());
    if let Ok(iso) = optical::IsoImage::open(img) {
        let root = iso.root().clone();
        if let Ok(list) = iso.list(&root) {
            for e in list.iter().take(16) {
                let _ = iso.list(e);
            }
        }
    }
}

fn journal(data: &[u8]) {
    // A receiver's entries one after another, as replay reads them.
    let mut rest = data;
    for _ in 0..64 {
        let Ok(len) = jrn::Entry::announced_len(rest) else {
            break;
        };
        if len == 0 || len > rest.len() {
            break;
        }
        if let Ok(e) = jrn::Entry::decode(&rest[..len]) {
            let _ = jrn::RecordImage::decode(&e.data);
        }
        rest = &rest[len..];
    }
    let _ = jrn::RecordImage::decode(data);
}

fn drda(data: &[u8]) {
    if let Ok(chain) = ddm::wire::dss::decode_chain(data) {
        for d in &chain {
            if let ddm::wire::dss::Payload::Raw(b) = &d.payload {
                let _ = ddm::wire::object::decode_all(b);
            }
        }
    }
    let _ = ddm::wire::object::decode_all(data);
}

fn smtp(data: &[u8]) {
    let _ = mail::smtp::replies(data);
    let _ = mail::Address::parse(&text(data));
}

fn ftp(data: &[u8]) {
    let mut host = ftp::memhost::MemHost::new(&[("BOB", "SECRET")]);
    host.add_library("BOBLIB");
    let mut s = ftp::Session::new(
        host,
        ftp::Config::new(servers::attrs::ftp::FtpAttrs::default(), "QS400T"),
    );
    let mut net = ftp::net::mem::MemNet::default();
    let _ = s.greet(&mut net);
    for line in text(data).lines().take(64) {
        if s.handle(line, &mut net) != ftp::Flow::Continue {
            break;
        }
        let _ = net.take_replies();
    }
}

fn ssh(data: &[u8]) {
    let _ = sshd::transport::read_ident(&mut Cursor::new(data));
    let mut o = sshd::transport::packet::Opener::default();
    let mut c = Cursor::new(data);
    for _ in 0..16 {
        match o.read(&mut c) {
            Ok(p) => {
                let _ = sshd::transport::algo::KexInit::decode(&p);
            }
            Err(_) => break,
        }
    }
    let _ = sshd::transport::algo::KexInit::decode(data);
}

fn tls_client() -> &'static std::sync::Arc<tls::rustls::ClientConfig> {
    static CFG: OnceLock<std::sync::Arc<tls::rustls::ClientConfig>> = OnceLock::new();
    CFG.get_or_init(|| {
        tls::config::client_config(
            &tls::policy::TlsPolicy::default(),
            tls::rustls::RootCertStore::empty(),
            None,
            tls::config::ServerAuthMode::Passthru,
        )
        .expect("client configuration")
    })
}

fn tls_records(data: &[u8]) {
    let name = tls::config::server_name("fuzz.example").expect("name");
    let Ok(mut e) = tls::engine::Engine::client(tls_client().clone(), name) else {
        return;
    };
    let mut out = Vec::new();
    let _ = e.take_output(&mut out);
    for chunk in data.chunks(512) {
        if e.feed(chunk).is_err() || e.process().is_err() {
            break;
        }
        let mut buf = [0u8; 1024];
        let _ = e.read_plain(&mut buf);
        let _ = e.take_output(&mut out);
    }
}

fn ebcdic(data: &[u8]) {
    let _ = ccsid::from_ebcdic(data);
    let _ = ccsid::to_ebcdic(&text(data));
    let ids = ccsid::conv::supported();
    if data.len() >= 2 && !ids.is_empty() {
        let from = ids[usize::from(data[0]) % ids.len()];
        let to = ids[usize::from(data[1]) % ids.len()];
        let _ = ccsid::conv::convert(from, to, &data[2..]);
    }
}

/// Every target.
pub const TARGETS: &[Target] = &[
    Target {
        name: "dds",
        what: "DDS source: physical, logical and printer files",
        run: dds,
        seeds: &[b"     A          R REC\n     A            NAME          10A\n     A          K NAME\n"],
        dict: &[b"     A", b" R ", b" K ", b"PFILE(", b"JOIN(", b"COLHDG(", b"EDTCDE(", b"VALUES(", b" S ", b" O "],
        max_len: 4096,
    },
    Target {
        name: "cl",
        what: "CL source through the analyzer and the CL compiler",
        run: cl,
        seeds: &[b"PGM PARM(&A)\nDCL VAR(&A) TYPE(*CHAR) LEN(10)\nIF COND(&A *EQ 'X') THEN(DO)\nCHGVAR VAR(&A) VALUE('Y')\nENDDO\nENDPGM\n"],
        dict: &[b"PGM", b"DCL ", b"CHGVAR ", b"%SST(", b"%BIN(", b"MONMSG ", b"DOFOR ", b"SELECT", b"WHEN ", b"*CAT", b"+\n"],
        max_len: 4096,
    },
    Target {
        name: "rpg",
        what: "ILE RPG source through the reader and the parser",
        run: rpg,
        seeds: &[b"**free\ndcl-s x int(10);\nx = 1;\ndsply x;\n*inlr = *on;\n"],
        dict: &[b"**free", b"dcl-s ", b"dcl-ds ", b"dcl-proc ", b"/copy ", b"/if defined(", b"     C", b"     D", b"     F", b"     H"],
        max_len: 4096,
    },
    Target {
        name: "sql",
        what: "SQL statements, SQL PL bodies and SQL/JSON paths, parsed",
        run: crate::sql::parse,
        seeds: crate::sql::SEEDS,
        dict: crate::sql::DICT,
        max_len: 4096,
    },
    Target {
        name: "sqlrun",
        what: "SQL statements run against a store in memory",
        run: crate::sql::run,
        seeds: crate::sql::SEEDS,
        dict: crate::sql::DICT,
        max_len: 2048,
    },
    Target {
        name: "tn5250",
        what: "the 5250 data stream, records and telnet negotiation",
        run: tn5250,
        seeds: &[b"\x04\x11\x40\x52\x1D\xF0\x13\x01\x02"],
        dict: &[b"\x04", b"\x11", b"\x1D", b"\x13", b"\xFF\xFA", b"\xFF\xF0", b"\xFF\xEF"],
        max_len: 2048,
    },
    Target {
        name: "qap",
        what: "QAP frames and request bodies",
        run: qap,
        seeds: &[b"\x00\x00\x00\x10\x01\x00\x00\x00\x01\x00\x01\x00\x01\x00\x00\x00\x00\x00\x00\x00"],
        dict: &[b"\x00\x00\x00\x00", b"\xFF\xFF\xFF\xFF"],
        max_len: 4096,
    },
    Target {
        name: "hostsvr",
        what: "host server datastreams, both directions",
        run: hostsvr,
        seeds: &[b"\x00\x00\x00\x28\x00\x00\xE0\x09\x00\x00\x00\x00\x00\x00\x00\x01\x00\x08\x70\x03\x00\x00\x00\x0A\x11\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"],
        dict: &[b"\xE0\x04", b"\xE0\x08", b"\xE0\x09", b"\x38\x01", b"\x11\x01"],
        max_len: 4096,
    },
    Target {
        name: "save",
        what: "save streams: the media header, records, the index",
        run: save,
        seeds: &[b"QS400SAV\x00\x01"],
        dict: &[b"QS400SAV", b"\x00\x00\x00\x00"],
        max_len: 16384,
    },
    Target {
        name: "ptf",
        what: "PTF packages, headers and signed groups",
        run: ptf,
        seeds: &[b"QS400PTF\x00\x01"],
        dict: &[b"QS400PTF", b"SIGNATURE "],
        max_len: 8192,
    },
    Target {
        name: "rsp",
        what: "the install response file",
        run: rsp,
        seeds: &[b"QS400-INSTALL-RSP 1\nlanguage=2924\n"],
        dict: &[b"QS400-INSTALL-RSP 1\n", b"=", b"\n", b"[", b"]"],
        max_len: 4096,
    },
    Target {
        name: "iso",
        what: "ISO 9660 images with Joliet and Rock Ridge",
        run: iso,
        seeds: &[],
        dict: &[b"CD001", b"\x01CD001\x01", b"\x02CD001\x01", b"\xFFCD001\x01", b"RR", b"PX", b"NM"],
        max_len: 64 * 2048,
    },
    Target {
        name: "journal",
        what: "journal receiver entries and record images, as replay reads them",
        run: journal,
        seeds: &[],
        dict: &[b"\x00\x00\x00\x40", b"PT", b"UP", b"DL"],
        max_len: 8192,
    },
    Target {
        name: "drda",
        what: "DDM and DRDA data stream structures and objects",
        run: drda,
        seeds: &[b"\x00\x0A\xD0\x01\x00\x01\x00\x04\x10\x41"],
        dict: &[b"\xD0\x01", b"\xD0\x03", b"\xD0\x41", b"\x10\x41", b"\x14\x04", b"\x24\x12"],
        max_len: 8192,
    },
    Target {
        name: "smtp",
        what: "SMTP replies the mail client reads, and addresses",
        run: smtp,
        seeds: &[b"220 mail ESMTP\r\n250-mail\r\n250-STARTTLS\r\n250 AUTH PLAIN LOGIN\r\n"],
        dict: &[b"250-", b"250 ", b"\r\n", b"STARTTLS", b"AUTH ", b"<", b">", b"@"],
        max_len: 4096,
    },
    Target {
        name: "ftp",
        what: "FTP server sessions from command lines",
        run: ftp,
        seeds: &[b"USER BOB\nPASS SECRET\nSYST\nPWD\nCWD BOBLIB\nLIST\nQUIT\n"],
        dict: &[b"USER ", b"PASS ", b"CWD ", b"SITE ", b"RCMD ", b"NAMEFMT ", b"TYPE ", b"PORT ", b"EPRT ", b"MLSD", b"\n"],
        max_len: 4096,
    },
    Target {
        name: "ssh",
        what: "SSH identification lines, binary packets and KEXINIT",
        run: ssh,
        seeds: &[b"SSH-2.0-OpenSSH_9.6\r\n"],
        dict: &[b"SSH-2.0-", b"\r\n", b"\x14", b"curve25519-sha256", b"ssh-ed25519"],
        max_len: 8192,
    },
    Target {
        name: "tls",
        what: "TLS records a client reads from a server",
        run: tls_records,
        seeds: &[b"\x16\x03\x03\x00\x04\x02\x00\x00\x00"],
        dict: &[b"\x16\x03\x03", b"\x15\x03\x03", b"\x17\x03\x03", b"\x14\x03\x03\x00\x01\x01"],
        max_len: 16384,
    },
    Target {
        name: "ebcdic",
        what: "CCSID conversions between every supported pair",
        run: ebcdic,
        seeds: &[b"\x00\x01Hello \x0E\x41\x41\x0F"],
        dict: &[b"\x0E", b"\x0F", b"\xFF\xFE"],
        max_len: 4096,
    },
];

/// The target named `name`.
pub fn find(name: &str) -> Option<&'static Target> {
    TARGETS.iter().find(|t| t.name == name)
}
