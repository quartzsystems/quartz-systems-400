// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every compiler's diagnostics are its listing's messages, nothing
//! added and nothing dropped: for each fixture of the kits (and HLINV's
//! source when it is there), the set of (member, line, identifier) the
//! compile reported through its sink equals the set read back out of
//! the listing it printed (`conformance::compdiag`).

use std::collections::BTreeSet;
use std::path::Path;

use compdiag::no_help;
use conformance::compdiag::{
    Key, Members, cl_listing, cmd_report, dds_listing, keys, rpg_listing, sql_listing,
};
use conformance::hlinv;
use conformance::rpg::{Kit, compile, files, member};
use dds::MapRefs;
use rpg_oracle::case::{Case, DbStep, DdsKind};
use rpgfront::source::Member;

/// Report up to a few differences, and fail when there are any.
fn verdict(what: &str, checked: usize, bad: &[String]) {
    eprintln!("{what}: {checked} compiles checked, {} differ", bad.len());
    for b in bad.iter().take(40) {
        eprintln!("  {b}");
    }
    assert!(bad.is_empty(), "{what}: {} compiles differ", bad.len());
    assert!(checked > 0, "{what}: nothing compiled");
}

fn differ(name: &str, ours: &BTreeSet<Key>, listing: &BTreeSet<Key>) -> Option<String> {
    (ours != listing).then(|| {
        let extra: Vec<_> = ours.difference(listing).collect();
        let missing: Vec<_> = listing.difference(ours).collect();
        format!("{name}: reported only {extra:?}; listed only {missing:?}")
    })
}

fn members_of(main: &Member, with: &[(String, Vec<String>)]) -> Members {
    let mut m = Members::new();
    m.insert(
        main.name.to_ascii_uppercase(),
        main.records.iter().map(|r| r.seq.clone()).collect(),
    );
    for (n, lines) in with {
        let x = Member::from_text("QGPL", "QRPGLESRC", n, &lines.join("\n"));
        m.insert(
            n.to_ascii_uppercase(),
            x.records.iter().map(|r| r.seq.clone()).collect(),
        );
    }
    m
}

/// One RPG compile against its listing.
fn rpg_case(case: &Case, db: Option<&files::KitDb>, in_copy: &mut usize) -> Option<String> {
    let c = compile(case, db);
    let ours = keys(&rpgc::report::diagnostics(&c, None, &mut no_help));
    let main = case.name.to_ascii_uppercase();
    *in_copy += ours
        .iter()
        .filter(|k| !k.0.is_empty() && k.0 != main)
        .count();
    let members = members_of(&member(case), &case.with);
    let listed = rpg_listing(&c.listing, &case.name, &members);
    differ(&case.name, &ours, &listed)
}

#[test]
fn rpg_diagnostics_are_the_listings() {
    let kit = Kit::locate();
    let mut checked = 0;
    let mut in_copy = 0;
    let mut bad = Vec::new();
    for (_, case) in kit.cases().expect("the corpus") {
        let verb = case.cmd.trim_start().to_ascii_uppercase();
        if case.rpg3 || verb.starts_with("CRTPGM") {
            continue;
        }
        let db = if case.db.is_empty() {
            None
        } else {
            match files::KitDb::build(&case.db) {
                Ok(d) => Some(d),
                Err(_) => continue,
            }
        };
        checked += 1;
        bad.extend(rpg_case(&case, db.as_ref(), &mut in_copy));
    }
    // HLINV, unmodified, when its source is there.
    let found = hlinv::members(&["rpgle", "sqlrpgle"]);
    if !found.is_empty() {
        let db = hlinv::db();
        for p in found {
            let (name, lines) = hlinv::read(&p);
            let mut case = Case::parse(&name, &lines.join("\n"));
            case.call.clear();
            checked += 1;
            bad.extend(rpg_case(&case, db.as_ref(), &mut in_copy));
        }
    }
    eprintln!("ILE RPG: {in_copy} diagnostics in copied members");
    assert!(in_copy > 0, "no fixture reports into a copied member");
    verdict("ILE RPG", checked, &bad);
}

#[test]
fn cl_diagnostics_are_the_listings() {
    let mut cases = conformance::cl::Kit::locate().cases().expect("the corpus");
    for p in hlinv::members(&["clle", "clp"]) {
        let (name, lines) = hlinv::read(&p);
        cases.push(conformance::cl::Case::parse(&name, &lines.join("\n")));
    }
    let mut bad = Vec::new();
    let mut reported = 0;
    for case in &cases {
        let c = conformance::cl::compile(case);
        let at = compdiag::Where::Member(compdiag::MemberName {
            library: "QGPL".into(),
            file: "QCLSRC".into(),
            member: case.name.clone(),
        });
        let ours = keys(&cl::diag::diagnostics(&c.diags, &at, &mut no_help));
        reported += ours.len();
        let listed = cl_listing(&c.listing, &case.name);
        bad.extend(differ(&case.name, &ours, &listed));
    }
    assert!(reported > 0, "no CL fixture reports a diagnostic");
    verdict("CL", cases.len(), &bad);
}

/// DDS sources with errors of their own, beside the kits' clean ones:
/// a keyword no file takes, a field without a length, a format missing.
const DDS_ERRORS: &[(&str, DdsKind, &str)] = &[
    (
        "BADPF",
        DdsKind::Pf,
        "     A          R BADREC\n     A            FLD1           5A         NOSUCHKWD\n     A            FLD2            A\n",
    ),
    (
        "NOFMT",
        DdsKind::Pf,
        "     A            FLD1           5A\n",
    ),
    (
        "BADLF",
        DdsKind::Lf,
        "     A          R CUSREC                    PFILE(NOSUCHPF)\n     A          K CUSNO\n",
    ),
    (
        "BADDSPF",
        DdsKind::Dspf,
        "     A                                      DSPSIZ(24 80 *DS3)\n     A          R SCR\n     A            FLD1          10A  B 99 99\n     A                                  1  2'X' NOSUCHKWD\n",
    ),
    (
        "BADPRTF",
        DdsKind::Prtf,
        "     A          R LINE\n     A            FLD1          10Y  2\n     A                                     1'X' NOSUCHKWD\n",
    ),
];

#[test]
fn dds_diagnostics_are_the_listings() {
    let mut sources: Vec<(String, DdsKind, Vec<String>)> = Vec::new();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("db/dds");
    let mut kit: Vec<_> = std::fs::read_dir(&dir)
        .expect("db/dds")
        .flatten()
        .map(|e| e.path())
        .collect();
    kit.sort();
    for kind in [DdsKind::Pf, DdsKind::Lf] {
        for p in &kit {
            if p.extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case(kind.ext()))
            {
                let (name, lines) = hlinv::read(p);
                sources.push((name, kind, lines));
            }
        }
    }
    for (_, case) in Kit::locate().cases().expect("the corpus") {
        for st in &case.db {
            if let DbStep::Dds { name, kind, source } = st {
                sources.push((name.clone(), *kind, source.clone()));
            }
        }
    }
    for kind in [DdsKind::Pf, DdsKind::Lf, DdsKind::Dspf, DdsKind::Prtf] {
        for p in hlinv::members(&[kind.ext()]) {
            let (name, lines) = hlinv::read(&p);
            sources.push((name, kind, lines));
        }
    }
    for (name, kind, text) in DDS_ERRORS {
        sources.push((
            (*name).into(),
            *kind,
            text.lines().map(str::to_string).collect(),
        ));
    }
    // Physical files first, so the files over them find their formats.
    sources.sort_by_key(|s| match s.1 {
        DdsKind::Pf => 0,
        _ => 1,
    });
    let mut refs = MapRefs::default();
    let mut bad = Vec::new();
    let mut reported = 0;
    for (name, kind, lines) in &sources {
        let (src, _) = dds::Source::from_text(&lines.join("\n"));
        let opts = dds::CompileOpts {
            file: Some(name.clone()),
            ..dds::CompileOpts::default()
        };
        let (diags, listing) = match kind {
            DdsKind::Pf => {
                let c = dds::compile_pf(&src, &refs, &opts);
                if let Some(d) = &c.definition {
                    refs.insert_pf("QGPL", name, d);
                }
                (c.diagnostics, c.listing)
            }
            DdsKind::Lf => {
                let c = dds::compile_lf(&src, &refs, &opts);
                (c.diagnostics, c.listing)
            }
            DdsKind::Dspf => {
                let c = dspf::compile_dspf(&src, &refs, &opts);
                (c.diagnostics, c.listing)
            }
            DdsKind::Prtf => {
                let c = dds::compile_prtf(&src, &refs, &opts);
                (c.diagnostics, c.listing)
            }
        };
        let at = compdiag::Where::Member(compdiag::MemberName {
            library: "QGPL".into(),
            file: "QDDSSRC".into(),
            member: name.clone(),
        });
        let ours = keys(&dds::diag::diagnostics(&diags, &at, &mut no_help));
        reported += ours.len();
        bad.extend(differ(name, &ours, &dds_listing(&listing, name)));
    }
    eprintln!("DDS: {reported} diagnostics");
    assert!(reported > 0, "no DDS fixture reports a diagnostic");
    verdict("DDS", sources.len(), &bad);
}

/// Command definition sources with errors, beside the shipped ones.
const CMD_ERRORS: &[(&str, &str)] = &[
    ("NOCMD", " PARM KWD(A) TYPE(*CHAR) LEN(10)\n"),
    (
        "BADVAL",
        " CMD PROMPT('Bad values')\n PARM KWD(A) TYPE(*NOSUCH)\n PARM KWD(B) TYPE(*CHAR) LEN(10) +\n      NOSUCHKWD(1)\n",
    ),
    (
        "DUPKWD",
        " CMD PROMPT('Twice')\n PARM KWD(A) TYPE(*CHAR)\n PARM KWD(A) TYPE(*DEC) RTNVAL(*YES) DFT(1)\n",
    ),
    (
        "ORPHAN",
        " CMD PROMPT('Orphan')\n PARM KWD(A) TYPE(Q1)\nQ2: QUAL TYPE(*NAME)\n",
    ),
];

#[test]
fn cmd_diagnostics_are_the_report() {
    let mut sources: Vec<(String, String)> = sys_cpps::defs::DEFS
        .iter()
        .map(|(n, s)| ((*n).to_string(), (*s).to_string()))
        .collect();
    sources.extend(
        CMD_ERRORS
            .iter()
            .map(|(n, s)| ((*n).to_string(), (*s).to_string())),
    );
    let mut bad = Vec::new();
    let mut reported = 0;
    for (name, text) in &sources {
        let Err(diags) = cmd::source::compile(name, text) else {
            continue;
        };
        let at = compdiag::Where::Member(compdiag::MemberName {
            library: "QGPL".into(),
            file: "QCMDSRC".into(),
            member: name.clone(),
        });
        let ours = keys(&cmd::source::diagnostics(&diags, &at, &mut no_help));
        reported += ours.len();
        let printed = cmd_report(&cmd::source::report_lines(&diags), name);
        bad.extend(differ(name, &ours, &printed));
    }
    assert!(
        reported >= CMD_ERRORS.len(),
        "the error fixtures report {reported}"
    );
    verdict("CRTCMD", sources.len(), &bad);
}

/// The words of a listing here: the identifier and its values.
fn words(id: &str, v: &[&str]) -> String {
    if v.is_empty() {
        id.to_string()
    } else {
        format!("{id} {}", v.join(" "))
    }
}

#[test]
fn runsqlstm_diagnostics_are_the_listings() {
    let cases = conformance::sql::case::all(&conformance::sql::root()).expect("the SQL kit");
    let (mut store, mut ctx) = sql_store();
    let opts = sql::RunOpts {
        command: true,
        ..sql::RunOpts::default()
    };
    let mut bad = Vec::new();
    let mut reported = 0;
    let mut checked = 0;
    for c in &cases {
        let mut text = String::new();
        for s in &c.statements {
            text.push_str(s.replace("{lib}", "QSTEST").trim_end_matches(';'));
            text.push_str(";\n");
        }
        let Ok(report) = sql::runsqlstm(&mut store, &mut ctx, &text, &opts) else {
            continue;
        };
        let name = c.name.to_ascii_uppercase();
        let l = sql::listing::Listing {
            product: "QS00SS1",
            release: "V1R1M0",
            name: &name,
            source: vec![
                ("TXT0943", "QGPL/QSQLSRC".into()),
                ("TXT0944", name.clone()),
            ],
            value: &|_| String::new(),
            stamp: ("26-01-01".into(), "00:00:00".into()),
            changed: None,
            records: text.lines().map(str::to_string).collect(),
        };
        let listing = sql::listing::listing(&l, &report, &text, &words);
        let at = compdiag::Where::Member(compdiag::MemberName {
            library: "QGPL".into(),
            file: "QSQLSRC".into(),
            member: name.clone(),
        });
        let ours = keys(&sql::listing::diagnostics(
            &report,
            &text,
            &at,
            &words,
            &mut no_help,
        ));
        reported += ours.len();
        checked += 1;
        bad.extend(differ(
            &name,
            &ours,
            &sql_listing(&listing, "TXT0965", &name),
        ));
    }
    assert!(reported > 0, "no SQL fixture reports a message");
    verdict("RUNSQLSTM", checked, &bad);
}

/// A store with `QSTEST`, its context: `*SYS` naming, `QSTEST` current.
fn sql_store() -> (store::Store, sql::SqlContext) {
    use blockdev::{BlockDev, MemBlockDev};
    use store::obj::{CreateParams, JobHandle, ops, types};
    const NOW: u64 = 1_767_225_600;
    let dev = std::sync::Arc::new(MemBlockDev::with_seed(65536, 7));
    let params = store::FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 512,
        now: NOW,
    };
    let mut store = store::Store::format(
        vec![Box::new(dev) as Box<dyn BlockDev>],
        &params,
        store::StoreConfig::default(),
    )
    .expect("a store");
    let mut txn = store.begin().expect("a transaction");
    let qsys = ops::bootstrap(&mut txn, NOW, "QS400").expect("QSYS");
    let lib = ops::create(
        &mut txn,
        &CreateParams {
            library: qsys,
            name: qsname::Name::parse("QSTEST").expect("valid"),
            type_code: types::LIB,
            attribute: String::new(),
            text: String::new(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: false,
        },
    )
    .expect("QSTEST");
    txn.commit().expect("committed");
    let mut ctx = sql::SqlContext::new(
        qsys,
        JobHandle(1),
        store::obj::ops::NameContext {
            curlib: Some(lib),
            libl: vec![lib],
        },
    );
    ctx.now = Some(NOW);
    ctx.current_schema = Some(qsname::Name::parse("QSTEST").expect("valid"));
    (store, ctx)
}

/// What the binder printed: `ID text` per diagnostic, or the one error
/// (`ID: text`, or `binder source line N: text`).
fn binder_printed(lines: &[String], member: &str) -> BTreeSet<Key> {
    lines
        .iter()
        .map(|l| {
            if let Some(rest) = l.strip_prefix("binder source line ") {
                let n = rest
                    .split(':')
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                return (member.to_string(), n, String::new());
            }
            let id = l.split([' ', ':']).next().unwrap_or("");
            let ok = id.len() == 7 && id.starts_with("CP");
            (
                String::new(),
                0,
                if ok { id.to_string() } else { String::new() },
            )
        })
        .collect()
}

#[test]
fn binder_diagnostics_are_the_report() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("qir/programs");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .expect("qir/programs")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "qir"))
        .collect();
    paths.sort();
    // A module that exports a procedure and has no entry: twice it
    // exports the procedure twice (CPD5D04); alone, as a program, it has
    // no entry procedure (CPD5D06).
    let lib = "module LIBX\nproc TWICE(byval $v: i64) -> i64 export {\nL0:\n  %v = load $v\n  %r = add %v, %v\n  ret %r\n}\n";
    let mut modules = vec![(
        "LIBX".to_string(),
        conformance::rpg::build::module_object(lib, "LIBX").expect("LIBX"),
    )];
    for p in &paths {
        let ir = std::fs::read_to_string(p).expect("IR");
        let name = p
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_ascii_uppercase();
        if let Ok(m) = conformance::rpg::build::module_object(&ir, &name) {
            modules.push((name, m));
        }
    }
    // Each module alone, then each beside the next (two entries, the
    // same exports twice), as programs and as service programs.
    let mut binds: Vec<(String, Vec<ile::ModuleObject>, ile::PgmKind)> = Vec::new();
    for (i, (n, m)) in modules.iter().enumerate() {
        for kind in [ile::PgmKind::Pgm, ile::PgmKind::SrvPgm] {
            binds.push((n.clone(), vec![m.clone()], kind));
            if let Some((n2, m2)) = modules.get(i + 1) {
                binds.push((format!("{n}+{n2}"), vec![m.clone(), m2.clone()], kind));
            }
            binds.push((format!("{n}x2"), vec![m.clone(), m.clone()], kind));
        }
    }
    let lookup = |_: &ile::BndDirEntry| -> Option<ile::binder::Resolved> { None };
    let mut bad = Vec::new();
    let mut ids = BTreeSet::new();
    for (i, (name, mods, kind)) in binds.iter().enumerate() {
        // Most with OPTION(*UNRSLVREF): the runtime's imports resolve at
        // run time here.
        let mut options = ile::BindOptions::new("P", *kind);
        options.allow_unresolved = i % 5 != 0;
        let r = ile::bind(ile::BindInput {
            modules: mods
                .iter()
                .cloned()
                .map(|m| (m, "QGPL".to_string()))
                .collect(),
            bnddirs: Vec::new(),
            lookup: &lookup,
            options,
        });
        let (outcome, printed) = match &r {
            Ok((_, d)) => (Ok(d.as_slice()), ile::binder::report::report_lines(d)),
            Err(e) => (Err(e), vec![e.to_string()]),
        };
        let ours = keys(&ile::binder::report::diagnostics(
            outcome,
            None,
            &mut no_help,
        ));
        ids.extend(ours.iter().map(|k| k.2.clone()));
        bad.extend(differ(name, &ours, &binder_printed(&printed, "")));
    }
    // Binder source that does not parse points into its member.
    let at = compdiag::Where::Member(compdiag::MemberName {
        library: "QGPL".into(),
        file: "QSRVSRC".into(),
        member: "BADSRC".into(),
    });
    for src in [
        "STRPGMEXP PGMLVL(*CURRENT)\n  NOSUCH SYMBOL(X)\nENDPGMEXP\n",
        "  EXPORT SYMBOL(X)\n",
    ] {
        let e = ile::parse_binder_source(src).expect_err("refused");
        let ours = keys(&ile::binder::report::diagnostics(
            Err(&e),
            Some(&at),
            &mut no_help,
        ));
        bad.extend(differ(
            "BADSRC",
            &ours,
            &binder_printed(&[e.to_string()], "BADSRC"),
        ));
    }
    eprintln!("binder: identifiers reported {ids:?}");
    for id in ["CPD5D02", "CPD5D04", "CPD5D06"] {
        assert!(ids.contains(id), "{id} not among {ids:?}");
    }
    verdict("CRTPGM/CRTSRVPGM", binds.len(), &bad);
}
