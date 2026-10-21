// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The binder in memory: resolution order, duplicates, unresolved
//! imports, service program exports and signatures, payload round trips.

use ile::binder::{Resolved, SrvPgmInfo};
use ile::{
    ActGrpAttr, BindInput, BindOptions, BndDir, BndDirEntry, BndEntryKind, CreateData, ExportKind,
    ExportSpec, ImportTarget, ModuleObject, PgmKind, ProgramObject, Severity, Signature, bind,
    parse_binder_source,
};

fn module(text: &str) -> ModuleObject {
    ModuleObject::compile(
        qir::parse(text).unwrap_or_else(|e| panic!("{e}")),
        CreateData {
            compiler: "QIRC".into(),
            ..CreateData::default()
        },
        1_767_225_600,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

const MAIN: &str = "
module MAIN
extern proc HELPER(byval $n: i32) -> i32
extern proc UTIL(byval $n: i32) -> i32
import $counter: i32
proc MAIN() entry {
L0:
  %a = callb HELPER(1)
  %b = callb UTIL(%a)
  store $counter, %b
  ret
}
";

const HELPER: &str = "
module HELPER
static $counter: i32 inz(0) export
proc HELPER(byval $n: i32) -> i32 export {
L0:
  %n = load $n
  %r = add %n, 1
  ret %r
}
";

const UTIL: &str = "
module UTIL
proc UTIL(byval $n: i32) -> i32 export {
L0:
  %n = load $n
  %r = mul %n, 2
  ret %r
}
proc HELPER(byval $n: i32) -> i32 export {
L0:
  ret 0
}
";

fn no_lookup(_: &BndDirEntry) -> Option<Resolved> {
    None
}

#[test]
fn listed_modules_resolve_in_order_and_duplicates_warn() {
    let (p, diags) = bind(BindInput {
        modules: vec![
            (module(MAIN), "QGPL".into()),
            (module(HELPER), "QGPL".into()),
            (module(UTIL), "QGPL".into()),
        ],
        bnddirs: Vec::new(),
        lookup: &no_lookup,
        options: BindOptions::new("MAIN", PgmKind::Pgm),
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(p.entry, Some((0, 0)));
    assert_eq!(p.modules.len(), 3);
    assert_eq!(p.modules[1].frame_offset, 0);
    assert_eq!(p.modules[2].frame_offset, 16);
    assert_eq!(p.static_size, 16);
    let helper = p.imports.iter().find(|i| i.name == "HELPER").unwrap();
    assert_eq!(
        helper.target,
        ImportTarget::Proc { module: 1, proc: 0 },
        "the first module exporting wins"
    );
    let util = p.imports.iter().find(|i| i.name == "UTIL").unwrap();
    assert_eq!(util.target, ImportTarget::Proc { module: 2, proc: 0 });
    let counter = p.imports.iter().find(|i| i.name == "counter").unwrap();
    assert_eq!(counter.kind, ExportKind::Data);
    assert_eq!(counter.target, ImportTarget::Data { offset: 0 });
    assert!(
        diags
            .iter()
            .any(|d| d.id == "CPD5D04" && d.severity == Severity::Warning),
        "{diags:?}"
    );
    let bytes = p.encode();
    let again = ProgramObject::decode(&bytes).unwrap();
    assert_eq!(again.modules, p.modules);
    assert_eq!(again.imports, p.imports);
    assert_eq!(again.entry, p.entry);
    assert_eq!(again.attr, p.attr);
}

#[test]
fn unresolved_imports_fail_unless_allowed() {
    let err = bind(BindInput {
        modules: vec![(module(MAIN), "QGPL".into())],
        bnddirs: Vec::new(),
        lookup: &no_lookup,
        options: BindOptions::new("MAIN", PgmKind::Pgm),
    })
    .unwrap_err();
    assert!(err.to_string().contains("CPD5D02"), "{err}");
    let mut opts = BindOptions::new("MAIN", PgmKind::Pgm);
    opts.allow_unresolved = true;
    let (p, diags) = bind(BindInput {
        modules: vec![(module(MAIN), "QGPL".into())],
        bnddirs: Vec::new(),
        lookup: &no_lookup,
        options: opts,
    })
    .unwrap();
    assert!(
        p.imports
            .iter()
            .all(|i| i.target == ImportTarget::Unresolved)
    );
    assert!(diags.iter().any(|d| d.id == "CPD5D02"));
}

#[test]
fn binding_directories_bind_modules_and_service_programs() {
    let dir = BndDir {
        entries: vec![
            BndDirEntry {
                name: "MATH".into(),
                lib: "QGPL".into(),
                kind: BndEntryKind::SrvPgm,
                defer: false,
            },
            BndDirEntry {
                name: "HELPER".into(),
                lib: "*LIBL".into(),
                kind: BndEntryKind::Module,
                defer: false,
            },
        ],
    };
    let sig = Signature::of_exports(&["UTIL".into()]);
    let lookup = move |e: &BndDirEntry| -> Option<Resolved> {
        match e.name.as_str() {
            "MATH" => Some(Resolved::SrvPgm(SrvPgmInfo {
                name: "MATH".into(),
                lib: "QGPL".into(),
                exports: vec![("UTIL".into(), ExportKind::Proc)],
                signature: sig,
            })),
            "HELPER" => Some(Resolved::Module(module(HELPER), "QGPL".into())),
            _ => None,
        }
    };
    let (p, _) = bind(BindInput {
        modules: vec![(module(MAIN), "QGPL".into())],
        bnddirs: vec![dir],
        lookup: &lookup,
        options: BindOptions::new("MAIN", PgmKind::Pgm),
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        p.modules.len(),
        2,
        "HELPER was copied in from the directory"
    );
    assert_eq!(p.srvpgms.len(), 1);
    assert_eq!(p.srvpgms[0].signature, sig);
    let util = p.imports.iter().find(|i| i.name == "UTIL").unwrap();
    assert_eq!(
        util.target,
        ImportTarget::SrvPgm {
            srvpgm: 0,
            export: "UTIL".into()
        }
    );
    let helper = p.imports.iter().find(|i| i.name == "HELPER").unwrap();
    assert_eq!(helper.target, ImportTarget::Proc { module: 1, proc: 0 });
    let text = ile::dsp::dsppgm(&p);
    assert!(text.contains("UTIL in *SRVPGM MATH"), "{text}");
}

#[test]
fn service_program_exports_and_signatures() {
    let mut opts = BindOptions::new("UTILS", PgmKind::SrvPgm);
    opts.attr.actgrp = ActGrpAttr::Named("QILE".into());
    let (all, _) = bind(BindInput {
        modules: vec![
            (module(UTIL), "QGPL".into()),
            (module(HELPER), "QGPL".into()),
        ],
        bnddirs: Vec::new(),
        lookup: &no_lookup,
        options: opts.clone(),
    })
    .unwrap();
    let names: Vec<&str> = all.exports.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["UTIL", "HELPER", "counter"]);
    assert_eq!(all.signatures.len(), 1);
    assert_eq!(
        all.signatures[0],
        Signature::of_exports(&["UTIL".into(), "HELPER".into(), "counter".into()])
    );

    let src = "STRPGMEXP PGMLVL(*CURRENT) SIGNATURE('UTILS V2')\n EXPORT SYMBOL('UTIL')\n EXPORT SYMBOL('counter')\nENDPGMEXP\nSTRPGMEXP PGMLVL(*PRV)\n EXPORT SYMBOL('UTIL')\nENDPGMEXP\n";
    opts.export = ExportSpec::Source(parse_binder_source(src).unwrap(), src.into());
    let (p, _) = bind(BindInput {
        modules: vec![
            (module(UTIL), "QGPL".into()),
            (module(HELPER), "QGPL".into()),
        ],
        bnddirs: Vec::new(),
        lookup: &no_lookup,
        options: opts.clone(),
    })
    .unwrap();
    let names: Vec<&str> = p.exports.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["UTIL", "counter"]);
    assert_eq!(
        p.exports[1].offset, 0,
        "counter lives in HELPER's frame, which follows UTIL's empty one"
    );
    assert_eq!(p.signatures.len(), 2);
    assert_eq!(
        p.signatures[0],
        Signature::from_source("UTILS V2", false).unwrap()
    );
    assert_eq!(p.signatures[1], Signature::of_exports(&["UTIL".into()]));
    assert!(p.accepts(p.signatures[1]));
    assert!(!p.accepts(Signature::of_exports(&["X".into()])));
    let again = ProgramObject::decode(&p.encode()).unwrap();
    assert_eq!(again.exports, p.exports);
    assert_eq!(again.signatures, p.signatures);
    assert_eq!(again.binder_source.as_deref(), Some(src));
    let text = ile::dsp::dspsrvpgm(&p);
    assert!(
        text.contains("*CURRENT") && text.contains("UTILS V2") || text.contains("Signatures"),
        "{text}"
    );

    opts.export = ExportSpec::Source(
        parse_binder_source("STRPGMEXP\n EXPORT SYMBOL('NOPE')\nENDPGMEXP").unwrap(),
        String::new(),
    );
    let err = bind(BindInput {
        modules: vec![(module(UTIL), "QGPL".into())],
        bnddirs: Vec::new(),
        lookup: &no_lookup,
        options: opts,
    })
    .unwrap_err();
    assert!(err.to_string().contains("CPD5D03"), "{err}");
}

#[test]
fn module_payload_round_trips() {
    let mut m = module(HELPER);
    m.source = Some(vec![
        "     H DFTACTGRP(*NO)".into(),
        "     D x s 10a".into(),
    ]);
    let again = ModuleObject::decode(&m.encode()).unwrap();
    assert_eq!(again, m);
    let text = ile::dsp::dspmod(&m);
    assert!(
        text.contains("HELPER") && text.contains("*EXPORT"),
        "{text}"
    );
}
