// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The RPG commands' parameters as compile options, CRTSQLRPGI's
//! precompile options, and a compiled member made into a module and a
//! program in a store.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use cmd::analyze::{Env, MemSource, analyze};
use cmd::block::Parms;
use ile::CreateData;
use ile::store::Ctx;
use qsname::Name;
use rpgc::Command;
use store::obj::library::Library;
use store::obj::{CreateParams, NameContext, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

use super::object::{Made, Shape, make};
use super::opts::request;
use super::source::{Source, member_of};

const NOW: u64 = 1_792_713_600;

/// The parameters of a command string, defaults filled, as the
/// processing program reads them.
fn parms(text: &str) -> Parms {
    let mut src = MemSource::default();
    for name in ["CRTBNDRPG", "CRTRPGMOD", "CRTSQLRPGI"] {
        src.put(sys_cpps::install::object(name, 0).expect("definition").def);
    }
    let mut env = Env::new(&mut src);
    let r = analyze(&mut env, text).unwrap_or_else(|e| panic!("{text}: {e}"));
    Parms::from_bound(&r.def, r.parms.clone())
}

fn source(lines: &[&str]) -> Source {
    let recs: Vec<(u32, u32, String)> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            (
                u32::try_from(i + 1).unwrap() * 100,
                261_023,
                (*l).to_string(),
            )
        })
        .collect();
    Source {
        member: member_of("SRC", "QRPGLESRC", "HELLO", &recs),
        text: "Says hello".into(),
    }
}

#[test]
fn crtbndrpg_defaults_and_choices() {
    let r = request("CRTBNDRPG", &parms("CRTBNDRPG PGM(QGPL/HELLO)")).unwrap();
    assert_eq!(r.command, Command::CrtBndRpg);
    assert_eq!(r.shape, Shape::Program);
    assert_eq!((r.name.as_str(), r.lib.as_str()), ("HELLO", "QGPL"));
    assert_eq!(
        (r.srcfile.as_str(), r.mbr.as_deref()),
        ("QRPGLESRC", Some("HELLO"))
    );
    assert!(r.dftactgrp && r.print && r.replace && r.truncnbr);
    assert_eq!(
        (r.genlvl, r.dbgview.as_str(), r.actgrp.as_str()),
        (10, "*STMT", "QILE")
    );
    let o = r.options("HELLO", "QGPL", &source(&["**FREE"]));
    assert_eq!(o.text, "Says hello");
    assert_eq!(o.src, ("SRC".into(), "QRPGLESRC".into(), "HELLO".into()));

    let r = request(
        "CRTBNDRPG",
        &parms(
            "CRTBNDRPG PGM(QGPL/HELLO) SRCFILE(DEV/QSRC) SRCMBR(HI) GENLVL(20) TEXT('Hi') \
             DFTACTGRP(*NO) ACTGRP(*NEW) OPTION(*NOGEN *SRCSTMT) DBGVIEW(*SOURCE) \
             OUTPUT(*NONE) REPLACE(*NO) DEFINE(TESTING) BNDDIR(TOOLS/UTIL QC2LE) USRPRF(*OWNER)",
        ),
    )
    .unwrap();
    assert_eq!(
        (r.srcfile.as_str(), r.srclib.as_str(), r.mbr.as_deref()),
        ("QSRC", "DEV", Some("HI"))
    );
    assert!(!r.dftactgrp && !r.print && !r.replace && r.usrprf_owner);
    assert_eq!(
        (r.genlvl, r.actgrp.as_str(), r.dbgview.as_str()),
        (20, "*NEW", "*SOURCE")
    );
    assert!(r.option.contains(&"*NOGEN".to_string()));
    assert_eq!(r.define, ["TESTING"]);
    assert_eq!(r.bnddir, ["TOOLS/UTIL", "QC2LE"]);
    assert_eq!(r.options("HELLO", "QGPL", &source(&[])).text, "Hi");
}

#[test]
fn crtrpgmod_and_crtsqlrpgi_shapes() {
    let r = request("CRTRPGMOD", &parms("CRTRPGMOD MODULE(QGPL/M1)")).unwrap();
    assert_eq!(
        (r.shape, r.command, r.dftactgrp),
        (Shape::Module, Command::CrtRpgMod, false)
    );
    let r = request("CRTSQLRPGI", &parms("CRTSQLRPGI OBJ(QGPL/S1)")).unwrap();
    assert_eq!(
        (r.shape, r.command, r.dftactgrp, r.sql),
        (Shape::Program, Command::CrtBndRpg, false, true)
    );
    assert_eq!((r.dbgview.as_str(), r.print), ("*NONE", false));
    let r = request(
        "CRTSQLRPGI",
        &parms("CRTSQLRPGI OBJ(QGPL/S1) OBJTYPE(*MODULE)"),
    )
    .unwrap();
    assert_eq!(r.shape, Shape::Module);
    let r = request(
        "CRTSQLRPGI",
        &parms("CRTSQLRPGI OBJ(QGPL/S1) OBJTYPE(*SRVPGM)"),
    )
    .unwrap();
    assert_eq!(r.shape, Shape::SrvPgm);
}

#[test]
fn crtsqlrpgi_precompile_options() {
    let r = request("CRTSQLRPGI", &parms("CRTSQLRPGI OBJ(QGPL/S1)")).unwrap();
    let o = r.sqlopts.clone().expect("SQL options");
    assert_eq!((o.commit.as_str(), o.naming.as_str(), o.closqlcsr.as_str()), ("*CHG", "*SYS", "*ENDACTGRP"));
    let r = request(
        "CRTSQLRPGI",
        &parms("CRTSQLRPGI OBJ(QGPL/S1) COMMIT(*NONE) OPTION(*SQL) CLOSQLCSR(*ENDMOD) DATFMT(*ISO)"),
    )
    .unwrap();
    let o = r.sqlopts.clone().expect("SQL options");
    assert_eq!(
        (o.commit.as_str(), o.naming.as_str(), o.closqlcsr.as_str(), o.datfmt.as_str()),
        ("*NONE", "*SQL", "*ENDMOD", "*ISO")
    );
    assert!(r.options("S1", "QGPL", &source(&[])).sql.is_some());
    let r = request("CRTBNDRPG", &parms("CRTBNDRPG PGM(QGPL/HELLO)")).unwrap();
    assert!(r.sqlopts.is_none());
}

#[test]
fn embedded_sql_precompiles_and_fails_as_sql9001() {
    let ok = source(&[
        "**FREE",
        "dcl-s n int(10);",
        "exec sql values 6 * 7 into :n;",
        "*inlr = *on;",
    ]);
    let req = request("CRTSQLRPGI", &parms("CRTSQLRPGI OBJ(QGPL/HELLO) COMMIT(*NONE)")).unwrap();
    let opts = req.options("HELLO", "QGPL", &ok);
    let c = rpgc::compile(ok.member.clone(), &mut rpgfront::NoIncludes, &mut rpgc::NoExtFiles, &opts);
    assert!(c.created && !c.sql_failed, "{:?}", c.diags);
    assert!(c.qir.contains("_QRNX_SQLRUN"));
    let bad = source(&["**FREE", "exec sql values 1 into :nosuch;", "*inlr = *on;"]);
    let c = rpgc::compile(bad.member.clone(), &mut rpgfront::NoIncludes, &mut rpgc::NoExtFiles, &opts);
    assert!(!c.created && c.sql_failed);
    assert_eq!(c.diags.iter().map(|d| d.msgid).collect::<Vec<_>>(), vec!["SQL0312"]);
}

#[test]
fn ctlspec_and_bad_values() {
    let r = request("CRTBNDRPG", &parms("CRTBNDRPG SRCMBR(HELLO)")).unwrap();
    assert_eq!(
        (r.name.as_str(), r.mbr.as_deref()),
        ("*CTLSPEC", Some("HELLO"))
    );
    assert_eq!(r.object_name(&source(&[])), "HELLO");
    let r = request("CRTBNDRPG", &parms("CRTBNDRPG")).unwrap();
    assert_eq!(r.mbr, None);
    let err = request("CRTBNDRPG", &parms("CRTBNDRPG PGM(A) TGTRLS(V3R1M0)")).unwrap_err();
    assert!(err.starts_with("CPF0006"), "{err}");
    assert!(request("CRTBNDRPG", &parms("CRTBNDRPG PGM(A) TGTRLS(V7R5M0)")).is_ok());
}

fn open_new() -> (Store, ObjectId, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 9));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
    let (qsys, lib) = {
        let mut t = store.begin().unwrap();
        let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
        let lib = ops::create(
            &mut t,
            &CreateParams {
                library: qsys,
                name: Name::parse("QGPL").unwrap(),
                type_code: types::LIB,
                attribute: String::new(),
                text: "General purpose".into(),
                owner: "QSECOFR".into(),
                system: "QS400".into(),
                now: NOW,
                by_system: false,
            },
        )
        .unwrap();
        t.commit().unwrap();
        (qsys, lib)
    };
    (store, qsys, lib)
}

#[test]
fn a_compiled_member_becomes_a_module_and_a_program() {
    let src = source(&[
        "**FREE",
        "ctl-opt dftactgrp(*no);",
        "dcl-s n int(10) inz(1);",
        "n += 1;",
        "n = square(n);",
        "*inlr = *on;",
        "dcl-proc square;",
        "  dcl-pi *n int(10);",
        "    n int(10) value;",
        "  end-pi;",
        "  return n * n;",
        "end-proc;",
    ]);
    let req = request("CRTBNDRPG", &parms("CRTBNDRPG PGM(QGPL/HELLO) SRCFILE(QGPL/QRPGLESRC)")).unwrap();
    let opts = req.options("HELLO", "QGPL", &src);
    let compiled = rpgc::compile(
        src.member.clone(),
        &mut rpgfront::NoIncludes,
        &mut rpgc::NoExtFiles,
        &opts,
    );
    assert!(compiled.created, "{:?}", compiled.diags);
    assert!(
        compiled.listing.iter().any(|l| l.starts_with('1')),
        "the listing starts pages with FCFC 1"
    );
    let (mut store, qsys, lib) = open_new();
    let ctx = Ctx {
        qsys,
        names: NameContext {
            curlib: Some(lib),
            libl: vec![qsys, lib],
        },
        now: NOW,
        owner: "QSECOFR".into(),
        system: "QS400".into(),
    };
    let mut made = Made {
        shape: Shape::Program,
        name: "HELLO".into(),
        text: "Says hello".into(),
        replace: true,
        usrprf_owner: false,
        // The command's BNDDIR(*NONE) default reaches the bind.
        bnddir: compiled.ctl.bnddir.clone(),
        source: Some(vec!["**FREE".into()]),
    };
    let mut txn = store.begin().unwrap();
    make(
        &mut txn,
        &ctx,
        lib,
        &made,
        &compiled.qir,
        CreateData::default(),
        NOW,
    )
    .unwrap();
    let qgpl = Library::load(&mut txn, lib).unwrap();
    let hello = Name::parse("HELLO").unwrap();
    let pgm = qgpl
        .lookup(&mut txn, types::PGM, hello)
        .unwrap()
        .expect("the program");
    let program = ile::store::load_program(&mut txn, pgm).unwrap();
    assert_eq!(program.modules.len(), 1);
    assert!(
        qgpl.lookup(&mut txn, types::MODULE, hello)
            .unwrap()
            .is_none(),
        "the bind's module is gone"
    );
    // REPLACE(*NO) keeps what is there.
    made.replace = false;
    let err = make(
        &mut txn,
        &ctx,
        lib,
        &made,
        &compiled.qir,
        CreateData::default(),
        NOW,
    )
    .unwrap_err();
    assert!(err.starts_with("CPF5D0D"), "{err}");
    made.shape = Shape::Module;
    make(
        &mut txn,
        &ctx,
        lib,
        &made,
        &compiled.qir,
        CreateData::default(),
        NOW,
    )
    .unwrap();
    assert!(
        qgpl.lookup(&mut txn, types::MODULE, hello)
            .unwrap()
            .is_some()
    );
}

/// One description, whatever the name.
struct OneFile(rpgc::files::ExtFile);

impl rpgc::ExtFiles for OneFile {
    fn describe(&mut self, _name: &str, _lib: &str) -> Option<rpgc::files::ExtFile> {
        Some(self.0.clone())
    }
}

#[test]
fn a_dds_printer_file_describes_its_formats() {
    let dds_src = [
        "     A          R HDR                       SKIPB(1)",
        "     A                                     2'Report'",
        "     A          R DTL                       SPACEA(1)",
        "     A            DNUM           5S 0      2",
        "     A            DNAME         20A       10",
        "     A  50                                32'*'",
    ]
    .join("\n");
    let (src, _) = dds::Source::from_text(&dds_src);
    let opts = dds::CompileOpts {
        file: Some("RPT".into()),
        ..dds::CompileOpts::default()
    };
    let def = dds::compile_prtf(&src, &dds::NoRefs, &opts)
        .definition
        .expect("printer DDS");
    let (mut store, qsys, lib) = open_new();
    let mut txn = store.begin().unwrap();
    let prtf = wm::obj::prtf::Prtf {
        formats: dds::prtf::codec::encode(&def),
        ..wm::obj::prtf::Prtf::default()
    };
    let stamp = wm::obj::Stamp {
        owner: "QSECOFR".into(),
        system: "QS400".into(),
        now: NOW,
    };
    let rpt = Name::parse("RPT").unwrap();
    wm::obj::create_attr(&mut txn, lib, (rpt, types::FILE, "PRTF"), "", &stamp, &prtf.encode())
        .unwrap();
    // A program-described one has no formats.
    let plain = Name::parse("PLAIN").unwrap();
    let encoded = wm::obj::prtf::Prtf::default().encode();
    wm::obj::create_attr(&mut txn, lib, (plain, types::FILE, "PRTF"), "", &stamp, &encoded)
        .unwrap();
    let names = NameContext {
        curlib: Some(lib),
        libl: vec![qsys, lib],
    };
    let none = super::source::device_in(&mut txn, qsys, &names, "*LIBL", "PLAIN").unwrap();
    assert!(none.is_none());
    let ext = super::source::device_in(&mut txn, qsys, &names, "*LIBL", "RPT")
        .unwrap()
        .expect("a printer file");
    assert_eq!((ext.dev, ext.lib.as_str()), (rpgc::files::Dev::Printer, "QGPL"));
    let names: Vec<&str> = ext.formats.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["HDR", "DTL"]);
    // DTL's buffer: indicator 50, then DNUM and DNAME.
    let dtl = &ext.formats[1];
    let fields: Vec<(&str, u32)> = dtl.fields.iter().map(|f| (f.name.as_str(), f.offset)).collect();
    assert_eq!(fields, [("DNUM", 1), ("DNAME", 6)]);
    assert_eq!(dtl.len, 26);
    assert_eq!(dtl.dsp.as_ref().unwrap().option_inds, [50]);
    txn.abort();

    // CRTBNDRPG compiles a program against it.
    let rpg = source(&[
        "**FREE",
        "dcl-f RPT printer;",
        "write HDR;",
        "DNUM = 7;",
        "DNAME = 'Seven';",
        "*in50 = *on;",
        "write DTL;",
        "*inlr = *on;",
    ]);
    let req = request("CRTBNDRPG", &parms("CRTBNDRPG PGM(QGPL/HELLO)")).unwrap();
    let opts = req.options("HELLO", "QGPL", &rpg);
    let c = rpgc::compile(rpg.member.clone(), &mut rpgfront::NoIncludes, &mut OneFile(ext), &opts);
    assert!(c.created, "{:?}", c.diags);
    assert!(c.qir.contains("prtio") && c.qir.contains("fmt(DTL)"), "{}", c.qir);
}

/// `/COPY` of stream files through an IFS client, as the service's
/// includes do with the job's.
struct StmfIncludes<'a> {
    client: &'a dyn ifs::client::IfsClient,
    incdir: Vec<String>,
}

impl rpgfront::Includes for StmfIncludes<'_> {
    fn member(&mut self, lib: &str, file: &str, name: &str) -> Option<rpgfront::Member> {
        if lib != rpgfront::reader::STMF {
            return None;
        }
        let c = self.client;
        let mut read = |p: &str| crate::cpp::stmf::read_text(c, p).ok();
        crate::cpp::stmf::find(&mut read, name, &self.incdir, file)
    }
}

fn put(c: &dyn ifs::client::IfsClient, path: &str, text: &str) {
    crate::cpp::stmf::write_text(c, path, text).unwrap();
}

#[test]
fn srcstmf_incdir_and_infostmf_are_parameters() {
    let r = request(
        "CRTBNDRPG",
        &parms("CRTBNDRPG SRCSTMF('/home/QSTEST/x.rpgle') INCDIR('/inc' 'more')"),
    )
    .unwrap();
    assert_eq!(r.srcstmf.as_deref(), Some("/home/QSTEST/x.rpgle"));
    assert_eq!(r.incdir, ["/inc", "more"]);
    let src = Source {
        member: crate::cpp::stmf::member_of("/home/QSTEST/x.rpgle", "**FREE"),
        text: String::new(),
    };
    assert_eq!(r.object_name(&src), "X", "the stream file names the program");
    let e = request(
        "CRTBNDRPG",
        &parms("CRTBNDRPG PGM(QGPL/X) SRCFILE(QGPL/QRPGLESRC) SRCSTMF('/a.rpgle')"),
    );
    assert!(e.unwrap_err().starts_with("CPD0074"));
}

#[test]
fn an_rpg_program_from_a_stream_file_with_copy_and_incdir() {
    use ifs::client::IfsClient as _;
    let sys = sysapi::ifs::memory::MemIfs::new();
    let c = sys.client("QSTEST", 5, 37);
    c.chdir("/home/QSTEST").unwrap();
    c.mkdir("/home/QSTEST/inc", 0o755).unwrap();
    c.mkdir("/home/QSTEST/src", 0o755).unwrap();
    // The same name in INCDIR and in the source's directory: INCDIR first.
    put(&c, "/home/QSTEST/inc/protos.rpgleinc", "**FREE
dcl-s fromInc int(10) inz(1);");
    put(&c, "/home/QSTEST/src/protos.rpgleinc", "**FREE
dcl-s fromSrc int(10) inz(2);");
    // Only in the source's directory.
    put(&c, "/home/QSTEST/src/more.rpgleinc", "**FREE
dcl-s more int(10) inz(3);");
    // In the current directory: before INCDIR.
    put(&c, "/home/QSTEST/cwd.rpgleinc", "**FREE
dcl-s fromCwd int(10) inz(4);");
    put(&c, "/home/QSTEST/inc/cwd.rpgleinc", "**FREE
dcl-s wrong int(10);");
    put(
        &c,
        "/home/QSTEST/src/x.rpgle",
        "**FREE\n/copy protos.rpgleinc\n/include 'more.rpgleinc'\n/copy cwd.rpgleinc\n\
         dsply %char(fromInc + more + fromCwd);\n*inlr = *on;\n",
    );
    let r = request(
        "CRTBNDRPG",
        &parms("CRTBNDRPG SRCSTMF('/home/QSTEST/src/x.rpgle') INCDIR('/home/QSTEST/inc')"),
    )
    .unwrap();
    let path = r.srcstmf.clone().unwrap();
    let text = crate::cpp::stmf::read_text(&c, &path).unwrap();
    let src = Source {
        member: crate::cpp::stmf::member_of(&path, &text),
        text: String::new(),
    };
    let opts = r.options(&r.object_name(&src), "QGPL", &src);
    let mut inc = StmfIncludes {
        client: &c,
        incdir: r.incdir.clone(),
    };
    let compiled = rpgc::compile(src.member.clone(), &mut inc, &mut rpgc::NoExtFiles, &opts);
    assert!(compiled.created, "{:?}", compiled.diags);
    let names: Vec<&str> = compiled.read.members.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "/home/QSTEST/src/x.rpgle",
            "/home/QSTEST/inc/protos.rpgleinc",
            "/home/QSTEST/src/more.rpgleinc",
            "cwd.rpgleinc",
        ]
    );
    // A name no directory has is RNF0264-style: not found.
    put(&c, "/home/QSTEST/src/y.rpgle", "**FREE\n/copy nowhere.rpgleinc\n*inlr = *on;\n");
    let text = crate::cpp::stmf::read_text(&c, "/home/QSTEST/src/y.rpgle").unwrap();
    let y = crate::cpp::stmf::member_of("/home/QSTEST/src/y.rpgle", &text);
    let compiled = rpgc::compile(y, &mut inc, &mut rpgc::NoExtFiles, &opts);
    assert!(!compiled.created);
    // A member-form name in a stream file is still a member.
    put(&c, "/home/QSTEST/src/z.rpgle", "**FREE\n/copy QRPGLESRC,PROTOS\n*inlr = *on;\n");
    let text = crate::cpp::stmf::read_text(&c, "/home/QSTEST/src/z.rpgle").unwrap();
    let z = crate::cpp::stmf::member_of("/home/QSTEST/src/z.rpgle", &text);
    let mut mem = rpgfront::MemIncludes {
        members: vec![rpgfront::Member::from_text("QSTEST", "QRPGLESRC", "PROTOS", "**FREE
dcl-s m int(10);")],
    };
    let compiled = rpgc::compile(z, &mut mem, &mut rpgc::NoExtFiles, &opts);
    assert!(compiled.created, "{:?}", compiled.diags);
    assert_eq!(compiled.read.members[1].name, "PROTOS");
}

#[test]
fn a_cl_program_from_a_stream_file() {
    use ifs::client::IfsClient as _;
    let sys = sysapi::ifs::memory::MemIfs::new();
    let c = sys.client("QSTEST", 6, 37);
    put(&c, "/home/QSTEST/hello.clle", "PGM\r\n  DCL VAR(&N) TYPE(*DEC) LEN(3 0) VALUE(1)\r\nENDPGM\r\n");
    let source = crate::cpp::stmf::read_text(&c, "/home/QSTEST/hello.clle").unwrap();
    let mut mem = cmd::analyze::MemSource::default();
    for n in ["PGM", "DCL", "ENDPGM"] {
        if let Ok(o) = sys_cpps::install::object(n, 0) {
            mem.put(o.def);
        }
    }
    let mut env = Env::new(&mut mem);
    env.setting.program = true;
    let opts = cl::Options {
        name: "HELLO".into(),
        ..cl::Options::default()
    };
    let compiled = cl::compile(&source.replace("\r\n", "\n"), &opts, &mut env, &mut cl::NoFiles);
    assert!(compiled.created, "{:?}", compiled.diags);
    let st = c.stat("/home/QSTEST/hello.clle", true).unwrap();
    assert_eq!(st.ccsid, 1208, "write_text tags 1208");
}
