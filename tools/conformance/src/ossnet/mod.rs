// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The open-source projects that need sockets and the IFS, run in
//! memory: HTTPAPI (`oss/src/httpapi`, BSD-2-Clause, fetched, never
//! tracked) built as `INSTALL` builds it, then its example programs
//! called against the harness host's HTTP server ([`server`]) with an
//! IFS in memory. The examples name hosts on the internet; their
//! source is changed in memory only, the host part of each URL
//! pointing at the server. Each run is a text of what the program did:
//! how it ended, the requests the server read, the files it left and
//! its job log's message identifiers.

pub mod server;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ifs::client::IfsClient as _;
use rpg_oracle::Case;
use rpg_oracle::outcome::Run;
use server::{Answer, Request, Server};
use sysapi::ifs::memory::MemIfs;

/// Where the fetched projects are: `QS400_OSS_DIR` or `oss/src`.
#[must_use]
pub fn oss_root() -> PathBuf {
    std::env::var("QS400_OSS_DIR").map_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("oss/src"), PathBuf::from)
}

/// HTTPAPI's members, by name, as `INSTALL` would copy them to
/// `QRPGLESRC`, and the configuration it is built with.
pub struct Httpapi {
    /// Members by name, upper case.
    pub members: BTreeMap<String, Vec<String>>,
}

/// The `/define`s of `CONFIG_H` the build turns off: no `GSKit` binding
/// for ILE exists yet, and NTLM needs the MD4 and RC4 modules that do
/// not compile.
pub const UNDEFINED: &[&str] = &["HAVE_SSLAPI", "NTLM_SUPPORT"];

/// The modules of the service program, in `INSTALL`'s order, without SSL
/// (`COMMSSLR4`), NTLM or the expat parser (`XMLSTUBR4` stands in).
pub const MODULES: &[&str] = &[
    "HTTPAPIR4",
    "COMPATR4",
    "COMMTCPR4",
    "HTTPUTILR4",
    "ENCODERR4",
    "DECODERR4",
    "CCSIDR4",
    "HEADERR4",
    "XMLSTUBR4",
];

impl Httpapi {
    /// The project under `root`, or `None` when it was not fetched.
    #[must_use]
    pub fn load(root: &Path) -> Option<Self> {
        let dir = root.join("httpapi/src/rpglesrc");
        let mut members = BTreeMap::new();
        for e in std::fs::read_dir(&dir).ok()?.flatten() {
            let p = e.path();
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
            if !matches!(ext.as_str(), "rpgle" | "rpgleinc" | "sqlrpgle") {
                continue;
            }
            let name = p.file_stem()?.to_string_lossy().to_ascii_uppercase();
            let text = String::from_utf8_lossy(&std::fs::read(&p).ok()?).replace("\r\n", "\n");
            members.insert(name, text.lines().map(str::to_string).collect());
        }
        // The member the makefile writes.
        members.insert(
            "VERSION".into(),
            vec!["     H COPYRIGHT('HTTPAPI as built by the QS/400 harness')".into()],
        );
        if let Some(cfg) = members.get_mut("CONFIG_H") {
            for l in cfg.iter_mut() {
                for d in UNDEFINED {
                    let pat = format!("/define {d}");
                    if l.to_ascii_lowercase().contains(&pat.to_ascii_lowercase()) {
                        *l = l.replacen("/define", "/undefine", 1).replacen("/DEFINE", "/undefine", 1);
                    }
                }
            }
        }
        Some(Self { members })
    }

    /// The commands `INSTALL` runs to build the service program and the
    /// `HTTPAPI` binding directory the examples name.
    #[must_use]
    pub fn setup() -> Vec<String> {
        let mut s = vec!["CRTBNDDIR BNDDIR(QGPL/HTTPAPIR4)".to_string()];
        for m in MODULES {
            s.push(format!("CRTRPGMOD MODULE(QGPL/{m}) SRCFILE(QGPL/QRPGLESRC) DBGVIEW(*LIST)"));
            if *m != "HTTPAPIR4" {
                s.push(format!("ADDBNDDIRE BNDDIR(QGPL/HTTPAPIR4) OBJ((QGPL/{m} *MODULE))"));
            }
        }
        s.push("CRTSRVPGM SRVPGM(QGPL/HTTPAPIR4) MODULE(QGPL/HTTPAPIR4) EXPORT(*SRCFILE) BNDDIR(QGPL/HTTPAPIR4 QC2LE) ACTGRP(*CALLER)".into());
        s.push("CRTBNDDIR BNDDIR(QGPL/HTTPAPI)".into());
        s.push("ADDBNDDIRE BNDDIR(QGPL/HTTPAPI) OBJ((QGPL/HTTPAPIR4 *SRVPGM))".into());
        s
    }

    /// An example as a case: its source with the hosts replaced, the
    /// build as setup, called with `parms`.
    #[must_use]
    pub fn case(&self, name: &str, port: u16, parms: &str) -> Option<Case> {
        let here = format!("http://127.0.0.1:{port}");
        let lines = self
            .members
            .get(name)?
            .iter()
            .map(|l| {
                HOSTS.iter().fold(l.clone(), |l, h| {
                    let path = h.splitn(4, '/').nth(3).map_or(String::new(), |p| format!("/{p}"));
                    l.replace(h, &format!("{here}{path}"))
                })
            })
            .collect();
        let with = self.members.iter().filter(|(n, _)| *n != name).map(|(n, l)| (n.clone(), l.clone())).collect();
        Some(Case {
            name: name.to_string(),
            lines,
            cmd: format!("CRTBNDRPG PGM(QGPL/{name}) SRCFILE(QGPL/QRPGLESRC) DBGVIEW(*LIST)"),
            call: if parms.is_empty() { format!("CALL {name}") } else { format!("CALL {name} PARM({parms})") },
            rcdlen: 112,
            setup: Self::setup(),
            with,
            ..Case::default()
        })
    }
}

/// The URL prefixes the examples call, as written; each becomes the
/// server's address with the rest kept.
const HOSTS: &[&str] = &["http://www.scottklement.com", "http://www.restfulwebservices.net/wcf"];

/// One example program and what it is called with.
pub struct Example {
    /// Member name.
    pub name: &'static str,
    /// `PARM` of the call.
    pub parms: &'static str,
    /// IFS files to report after the call.
    pub files: &'static [&'static str],
}

/// The examples run: each needs only HTTP, the IFS and its own
/// parameters (no display, no proxy, no password prompts).
pub const EXAMPLES: &[Example] = &[
    Example {
        name: "EXAMPLE1",
        parms: "",
        files: &["/tmp/Scott's HTTPAPI presentation handout.pdf"],
    },
    Example {
        name: "EXAMPLE15",
        parms: "",
        files: &["/tmp/testpost.html"],
    },
    Example {
        name: "EXAMPLE16",
        parms: "'USD' 'JPY' 12.00",
        files: &[],
    },
];

/// The handout `EXAMPLE1` fetches: 5000 bytes of a fixed pattern.
#[must_use]
pub fn handout() -> Vec<u8> {
    (0..5000u32).map(|i| u8::try_from((i * 7 + i / 251) % 256).unwrap_or(0)).collect()
}

/// The server's answers to the examples' requests.
#[must_use]
pub fn answer(r: &Request, port: u16) -> Answer {
    let path = r.path.split('?').next().unwrap_or("");
    match (r.method.as_str(), path) {
        ("GET", p) if p.starts_with("/presentations/") => Answer::ok("application/pdf", handout()),
        ("POST", "/cgi-bin/email_comment.cgi") => Answer {
            status: 302,
            reason: "Found",
            headers: vec![("Location".into(), format!("http://127.0.0.1:{port}/comment/thanks.html"))],
            body: Vec::new(),
        },
        ("GET", "/comment/thanks.html") => Answer::ok("text/html", b"<html><body>Thank you</body></html>\n".to_vec()),
        ("POST", "/wcf/CurrencyService.svc") => Answer::ok("text/xml; charset=utf-8", SOAP_RATE.as_bytes().to_vec()),
        _ => Answer {
            status: 404,
            reason: "Not Found",
            headers: vec![("Content-Type".into(), "text/plain".into())],
            body: b"no such page\n".to_vec(),
        },
    }
}

/// The page `EXAMPLE15`'s redirect leads to.
const THANKS: &[u8] = b"<html><body>Thank you</body></html>\n";

const SOAP_RATE: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"utf-8\"?>",
    "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>",
    "<GetConversionRateResponse xmlns=\"http://www.restfulwebservices.net/ServiceContracts/2008/01\">",
    "<GetConversionRateResult xmlns:a=\"http://www.restfulwebservices.net/DataContracts/2008/01\">",
    "<a:FromCurrency>USD</a:FromCurrency><a:Rate>151.25</a:Rate><a:ToCurrency>JPY</a:ToCurrency>",
    "</GetConversionRateResult></GetConversionRateResponse></s:Body></s:Envelope>"
);

/// A file's bytes from the IFS, `None` when it is not there.
fn file_of(sys: &MemIfs, path: &str) -> Option<Vec<u8>> {
    let c = sys.client("QSECOFR", 9, 37);
    let fd = c.open(path, ifs::ops::OpenArgs::new(ifs::ops::flags::O_RDONLY)).ok()?;
    let mut out = Vec::new();
    while let Ok(b) = c.read(fd, 65536) {
        if b.is_empty() {
            break;
        }
        out.extend_from_slice(&b);
    }
    let _ = c.close(fd);
    Some(out)
}

/// Compile every module of the service program and the examples, and
/// bind the service program: one line per member, then the binding.
#[must_use]
pub fn build_report(h: &Httpapi) -> String {
    let inc = || rpgfront::MemIncludes {
        members: h.members.iter().map(|(n, l)| rpgfront::Member::from_text("QGPL", "QRPGLESRC", n, &l.join("\n"))).collect(),
    };
    let mut objs = crate::rpg::build::Objects::default();
    let mut out = String::new();
    let compile = |mbr: &str, module: &str, cmd: &str| -> Result<String, String> {
        let lines = h.members.get(mbr).ok_or_else(|| format!("{mbr}: no such member"))?;
        let case = Case {
            name: module.to_string(),
            lines: lines.clone(),
            cmd: cmd.to_string(),
            rcdlen: 112,
            ..Case::default()
        };
        let c = rpgc::compile(crate::rpg::member(&case), &mut inc(), &mut rpgc::NoExtFiles, &crate::rpg::options(&case));
        if let Some(d) = std::env::var_os("QS400_OSS_LISTINGS") {
            let _ = std::fs::write(Path::new(&d).join(format!("{module}.lst")), c.listing.join("
"));
            let _ = std::fs::write(Path::new(&d).join(format!("{module}.qir")), &c.ir);
        }
        if c.created {
            Ok(c.ir)
        } else {
            let ids: Vec<String> = c
                .diags
                .iter()
                .filter(|d| d.severity > 10)
                .map(|d| if d.msgid == "RNF0001" { format!("RNF0001 ({})", d.args.join(" ")) } else { d.msgid.to_string() })
                .collect();
            Err(format!("{mbr} not created: {}", ids.join(" ")))
        }
    };
    for cmd in Httpapi::setup() {
        let verb = cmd.split_whitespace().next().unwrap_or("").to_string();
        if !matches!(verb.as_str(), "CRTRPGMOD" | "CRTSRVPGM") {
            let _ = objs.setup(&cmd, &compile);
            continue;
        }
        let what = crate::rpg::build::kwd(&cmd, if verb == "CRTSRVPGM" { "SRVPGM" } else { "MODULE" });
        // A module the binding names that was not made.
        let missing: Vec<String> = crate::rpg::build::names_of(&crate::rpg::build::kwd(&cmd, "MODULE"))
            .into_iter()
            .filter(|m| verb == "CRTSRVPGM" && !objs.modules.contains_key(m))
            .collect();
        if !missing.is_empty() {
            let _ = writeln!(out, "{verb} {what}: not created, no module {}", missing.join(" "));
            continue;
        }
        match objs.setup(&cmd, &compile) {
            Ok(()) => {
                let _ = writeln!(out, "{verb} {what}: created");
            }
            Err(e) => {
                let _ = writeln!(out, "{verb} {what}: {e}");
            }
        }
    }
    for ex in EXAMPLES {
        let Some(lines) = h.members.get(ex.name) else { continue };
        let case = Case {
            name: ex.name.to_string(),
            lines: lines.clone(),
            cmd: format!("CRTBNDRPG PGM(QGPL/{})", ex.name),
            rcdlen: 112,
            ..Case::default()
        };
        let c = rpgc::compile(crate::rpg::member(&case), &mut inc(), &mut rpgc::NoExtFiles, &crate::rpg::options(&case));
        let _ = writeln!(out, "CRTBNDRPG {}: {}", ex.name, if c.created { "created" } else { "not created" });
    }
    out
}

/// A short, stable account of some bytes.
fn digest(b: &[u8]) -> String {
    let sum = b.iter().fold(0u32, |s, &x| s.rotate_left(5) ^ u32::from(x));
    format!("{} bytes, sum {sum:08X}", b.len())
}

/// Hexadecimal of some bytes.
fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, c| {
        let _ = write!(s, "{c:02X}");
        s
    })
}

/// HTTPAPI's debug log, if the program wrote one, to standard error.
fn show_debug_log(sys: &MemIfs) {
    for f in ["/tmp/httpapi_debug.txt", "/tmp/httpapi_debug.log"] {
        if let Some(bytes) = file_of(sys, f) {
            // Lines in either code: the reading with more letters.
            let (ascii, ebcdic) = (String::from_utf8_lossy(&bytes).to_string(), ccsid::from_ebcdic(&bytes));
            let letters = |s: &str| s.chars().filter(char::is_ascii_alphabetic).count();
            let text = if letters(&ascii) >= letters(&ebcdic) { ascii } else { ebcdic };
            eprintln!("--- {f}\n{text}");
        }
    }
}

/// Build HTTPAPI and run one example against `srv`: the report.
///
/// # Errors
///
/// The case cannot be made or the harness fails (not the program).
pub fn run_example(api: &Httpapi, ex: &Example, srv: &Server) -> Result<String, String> {
    let case = api.case(ex.name, srv.port, ex.parms).ok_or_else(|| format!("{}: no such member", ex.name))?;
    let sys = Rc::new(MemIfs::new());
    let held = sys.clone();
    crate::rpg::set_job_setup(Some(Rc::new(move |job: &mut mi::Job| {
        sysapi::ifs::configure(job, Rc::new(held.client("QSTEST", 1, 37)), 37);
        sysapi::net::configure(
            job,
            sysapi::net::NetSetup {
                mark: None,
                ..sysapi::net::NetSetup::default()
            },
        );
    })));
    let _ = srv.take();
    let ran = crate::rpg::run(&case);
    crate::rpg::set_job_setup(None);
    let (outcome, _, notes) = ran?;
    let mut out = String::new();
    let _ = writeln!(out, "{}: created {}", ex.name, outcome.created);
    if !outcome.created {
        let _ = writeln!(out, "cause: {}", outcome.cause);
        for d in outcome.diags.iter().filter(|d| d.severity > 10) {
            let _ = writeln!(out, "diag: {} {}", d.msgid, d.severity);
        }
        for line in notes.lines().filter(|l| l.starts_with(';')).take(20) {
            let _ = writeln!(out, "{line}");
        }
        return Ok(out);
    }
    let how = match &outcome.run {
        Run::None => "not called".to_string(),
        Run::Ok => "ended normally".to_string(),
        Run::Escape(m) => format!("escape {m}"),
    };
    let _ = writeln!(out, "run: {how}");
    for req in srv.take() {
        let _ = writeln!(out, "request: {} {}", req.method, req.path);
        for name in ["content-type", "soapaction", "user-agent"] {
            if let Some(v) = req.header(name) {
                let _ = writeln!(out, "  {name}: {v}");
            }
        }
        if !req.body.is_empty() {
            let body = String::from_utf8_lossy(&req.body).replace('\r', "\\r").replace('\n', "\\n");
            let _ = writeln!(out, "  body: {body}");
        }
    }
    for f in ex.files {
        match file_of(&sys, f) {
            Some(bytes) => {
                let served = [handout(), THANKS.to_vec()].contains(&bytes);
                let first = hex(&bytes[..bytes.len().min(12)]);
                let _ = writeln!(out, "file {f}: {}, {first}{}", digest(&bytes), if served { ", the bytes served" } else { "" });
            }
            None => {
                let _ = writeln!(out, "file {f}: missing");
            }
        }
    }
    if std::env::var_os("QS400_OSS_DEBUG").is_some() {
        show_debug_log(&sys);
    }
    for line in crate::rpg::last_joblog() {
        let _ = writeln!(out, "joblog: {line}");
    }
    Ok(out)
}

