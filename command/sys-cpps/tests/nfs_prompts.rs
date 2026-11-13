// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NFS, mounted file system and RPC commands against the primary
//! (V7R1M0): their definitions against `QCDRCMDD`'s XML in
//! `tools/conformance/oracle/nfs/cmdd71/`, and their prompts against
//! `tools/conformance/golden/nfs/` character for character on rows 1
//! to 23 (the message line, row 24, is IBM's text there and ours here:
//! only whether a message shows is compared).

use std::path::{Path, PathBuf};

use cmd::analyze::{Env, MemSource, analyze};
use cmd::def::{CmdDef, Parm};
use cmd::prompt::{PAGE_DOWN, Prompter};

fn source() -> MemSource {
    let mut s = MemSource::default();
    for (name, _) in sys_cpps::defs::DEFS {
        let o = sys_cpps::install::object(name, 0).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        s.put(o.def);
    }
    s
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance")
}

fn golden(name: &str) -> Vec<String> {
    let p = root().join("golden/nfs").join(format!("{name}.txt"));
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect()
}

/// The display of `text` after `keys`, as 24 rows.
fn ours(text: &str, keys: &[u8]) -> Vec<String> {
    let mut src = source();
    let r = {
        let mut env = Env::new(&mut src);
        env.setting.interactive = true;
        analyze(&mut env, text).unwrap_or_else(|e| panic!("{text}: {e}"))
    };
    let mut p = Prompter::new(&r);
    for k in keys {
        let fields: Vec<String> = p.screen().fields.iter().map(|f| f.value.clone()).collect();
        let mut env = Env::new(&mut src);
        env.setting.interactive = true;
        let _ = p.key(*k, &fields, &mut env);
    }
    p.screen()
        .text()
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect()
}

/// Every capture: its name, the command typed before F4 and the keys.
const CASES: &[(&str, &str, &[u8])] = &[
    ("prompt-strnfssvr", "?STRNFSSVR", &[]),
    ("prompt-strnfssvr-f10", "?STRNFSSVR", &[10]),
    ("prompt-strnfssvr-f10-2", "?STRNFSSVR", &[10, PAGE_DOWN]),
    ("prompt-strnfssvr-serversvr", "?STRNFSSVR SERVER(*SVR)", &[]),
    (
        "prompt-strnfssvr-serversvr-f10",
        "?STRNFSSVR SERVER(*SVR)",
        &[10],
    ),
    (
        "prompt-strnfssvr-serversvr-f10-2",
        "?STRNFSSVR SERVER(*SVR)",
        &[10, PAGE_DOWN],
    ),
    ("prompt-strnfssvr-serverrpc", "?STRNFSSVR SERVER(*RPC)", &[]),
    (
        "prompt-strnfssvr-serverrpc-f10",
        "?STRNFSSVR SERVER(*RPC)",
        &[10],
    ),
    (
        "prompt-strnfssvr-serverrpc-f10-2",
        "?STRNFSSVR SERVER(*RPC)",
        &[10, PAGE_DOWN],
    ),
    ("prompt-endnfssvr", "?ENDNFSSVR", &[]),
    ("prompt-endnfssvr-f10", "?ENDNFSSVR", &[10]),
    ("prompt-endnfssvr-f10-2", "?ENDNFSSVR", &[10, PAGE_DOWN]),
    ("prompt-exportfs", "?EXPORTFS", &[]),
    ("prompt-exportfs-f10", "?EXPORTFS", &[10]),
    ("prompt-exportfs-f10-2", "?EXPORTFS", &[10, PAGE_DOWN]),
    ("prompt-chgnfsexp", "?CHGNFSEXP", &[]),
    ("prompt-chgnfsexp-f10", "?CHGNFSEXP", &[10]),
    ("prompt-chgnfsexp-f10-2", "?CHGNFSEXP", &[10, PAGE_DOWN]),
    ("prompt-mount", "?MOUNT", &[]),
    ("prompt-mount-f10", "?MOUNT", &[10]),
    ("prompt-mount-f10-2", "?MOUNT", &[10, PAGE_DOWN]),
    ("prompt-mount-typenfs", "?MOUNT TYPE(*NFS)", &[]),
    ("prompt-mount-typenfs-f10", "?MOUNT TYPE(*NFS)", &[10]),
    (
        "prompt-mount-typenfs-f10-2",
        "?MOUNT TYPE(*NFS)",
        &[10, PAGE_DOWN],
    ),
    ("prompt-addmfs", "?ADDMFS", &[]),
    ("prompt-addmfs-f10", "?ADDMFS", &[10]),
    ("prompt-addmfs-f10-2", "?ADDMFS", &[10, PAGE_DOWN]),
    ("prompt-unmount", "?UNMOUNT", &[]),
    ("prompt-unmount-f10", "?UNMOUNT", &[10]),
    ("prompt-unmount-f10-2", "?UNMOUNT", &[10, PAGE_DOWN]),
    ("prompt-unmount-typenfs", "?UNMOUNT TYPE(*NFS)", &[]),
    ("prompt-unmount-typenfs-f10", "?UNMOUNT TYPE(*NFS)", &[10]),
    (
        "prompt-unmount-typenfs-f10-2",
        "?UNMOUNT TYPE(*NFS)",
        &[10, PAGE_DOWN],
    ),
    ("prompt-rmvmfs", "?RMVMFS", &[]),
    ("prompt-rmvmfs-f10", "?RMVMFS", &[10]),
    ("prompt-rmvmfs-f10-2", "?RMVMFS", &[10, PAGE_DOWN]),
    ("prompt-dspmfsinf", "?DSPMFSINF", &[]),
    ("prompt-dspmfsinf-f10", "?DSPMFSINF", &[10]),
    ("prompt-dspmfsinf-f10-2", "?DSPMFSINF", &[10, PAGE_DOWN]),
    ("prompt-statfs", "?STATFS", &[]),
    ("prompt-statfs-f10", "?STATFS", &[10]),
    ("prompt-statfs-f10-2", "?STATFS", &[10, PAGE_DOWN]),
    ("prompt-rlsifslck", "?RLSIFSLCK", &[]),
    ("prompt-rlsifslck-f10", "?RLSIFSLCK", &[10]),
    ("prompt-rlsifslck-f10-2", "?RLSIFSLCK", &[10, PAGE_DOWN]),
    ("prompt-rpcbind", "?RPCBIND", &[]),
    ("prompt-rpcbind-f10", "?RPCBIND", &[10]),
    ("prompt-rpcbind-f10-2", "?RPCBIND", &[10, PAGE_DOWN]),
    ("prompt-endrpcbind", "?ENDRPCBIND", &[]),
    ("prompt-endrpcbind-f10", "?ENDRPCBIND", &[10]),
    ("prompt-endrpcbind-f10-2", "?ENDRPCBIND", &[10, PAGE_DOWN]),
];

#[test]
fn prompts_match_the_primary() {
    let mut bad = Vec::new();
    for (name, text, keys) in CASES {
        let want = golden(name);
        let got = ours(text, keys);
        let mut diffs = Vec::new();
        for r in 0..23 {
            let w = want.get(r).map_or("", String::as_str);
            let g = got.get(r).map_or("", String::as_str);
            if w != g {
                diffs.push(format!(
                    "  row {:>2} want {w:?}\n         got  {g:?}",
                    r + 1
                ));
            }
        }
        let msg = |v: &[String]| v.get(23).is_some_and(|l| !l.trim().is_empty());
        if msg(&want) != msg(&got) {
            diffs.push(format!(
                "  row 24 want {:?}\n         got  {:?}",
                want.get(23),
                got.get(23)
            ));
        }
        if !diffs.is_empty() {
            bad.push(format!("{name} ({text} {keys:?}):\n{}", diffs.join("\n")));
        }
    }
    assert!(
        bad.is_empty(),
        "{} of {} differ:\n{}",
        bad.len(),
        CASES.len(),
        bad.join("\n\n")
    );
}

/// One `Parm` element of the XML as attribute pairs.
fn attrs(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    while let Some(eq) = rest.find("=\"") {
        let key = rest[..eq]
            .rsplit([' ', '<'])
            .next()
            .unwrap_or("")
            .to_string();
        let after = &rest[eq + 2..];
        let end = after.find('"').unwrap_or(after.len());
        out.push((key, after[..end].replace("&apos;", "'")));
        rest = &after[end..];
    }
    out
}

fn get<'a>(a: &'a [(String, String)], k: &str) -> Option<&'a str> {
    a.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
}

fn def(name: &str) -> CmdDef {
    let src = sys_cpps::defs::source(name).unwrap_or_else(|| panic!("{name} has no definition"));
    cmd::source::compile(name, src).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

/// The scalar a parameter (or element) is, as the XML names its type.
fn xml_type(p: &Parm) -> String {
    use cmd::def::ParmType as T;
    match &p.ty {
        T::Int4 => "INT4".into(),
        T::Char => "CHAR".into(),
        T::PName => "PNAME".into(),
        T::Elem { .. } => "ELEM".into(),
        other => format!("{other:?}"),
    }
}

const COMMANDS: &[&str] = &[
    "STRNFSSVR",
    "ENDNFSSVR",
    "EXPORTFS",
    "CHGNFSEXP",
    "MOUNT",
    "ADDMFS",
    "UNMOUNT",
    "RMVMFS",
    "DSPMFSINF",
    "STATFS",
    "RLSIFSLCK",
    "RPCBIND",
    "ENDRPCBIND",
];

#[test]
#[allow(clippy::too_many_lines)] // one check per attribute
fn definitions_match_the_primary() {
    let mut bad = Vec::new();
    for name in COMMANDS {
        let xml = std::fs::read_to_string(root().join(format!("oracle/nfs/cmdd71/{name}.xml")))
            .expect("the XML");
        let d = def(name);
        let cmd_tag = xml.split("<Parm ").next().unwrap_or("");
        let ca = attrs(cmd_tag);
        if get(&ca, "Prompt") != Some(d.prompt.as_str()) {
            bad.push(format!("{name}: prompt {:?}", d.prompt));
        }
        if get(&ca, "MaxPos").and_then(|v| v.parse::<u8>().ok()) != d.maxpos {
            bad.push(format!("{name}: MAXPOS {:?}", d.maxpos));
        }
        if get(&ca, "HlpID") != d.hlpid.as_deref() {
            bad.push(format!("{name}: HLPID {:?}", d.hlpid));
        }
        let parms: Vec<&str> = xml.split("<Parm ").skip(1).collect();
        if parms.len() != d.parms.len() {
            bad.push(format!(
                "{name}: {} parameters, {} captured",
                d.parms.len(),
                parms.len()
            ));
            continue;
        }
        for (x, p) in parms.iter().zip(&d.parms) {
            let head = x.split('>').next().unwrap_or("");
            let a = attrs(head);
            let kwd = get(&a, "Kwd").unwrap_or("");
            let mut check = |what: &str, ok: bool| {
                if !ok {
                    bad.push(format!("{name} {kwd}: {what}"));
                }
            };
            check("keyword", p.kwd == kwd);
            check("type", get(&a, "Type") == Some(xml_type(p).as_str()));
            check("prompt", get(&a, "Prompt") == Some(p.prompt_text()));
            check(
                "min",
                get(&a, "Min").and_then(|v| v.parse::<u16>().ok()) == Some(p.min),
            );
            check(
                "max",
                get(&a, "Max").and_then(|v| v.parse::<u16>().ok()) == Some(p.max),
            );
            check(
                "key parameter",
                (get(&a, "KeyParm") == Some("YES")) == p.keyparm,
            );
            check("default", get(&a, "Dft") == p.dft.as_deref());
            if let Some(len) = get(&a, "Len") {
                check("length", p.length().len == len.parse::<u32>().unwrap_or(0));
            }
            let pmtrqs = matches!(p.pmtctl, Some(cmd::def::PmtCtlRef::PmtRqs));
            let label = matches!(p.pmtctl, Some(cmd::def::PmtCtlRef::Label { .. }));
            match get(&a, "PmtCtl") {
                Some("PMTRQS") => check("PMTCTL(*PMTRQS)", pmtrqs),
                Some("PMTCTL") => check("PMTCTL(label)", label),
                _ => check("no PMTCTL", p.pmtctl.is_none()),
            }
            // Special values and their mapped values, in order.
            let spc = x
                .split("<SpcVal>")
                .nth(1)
                .and_then(|s| s.split("</SpcVal>").next())
                .map(|s| {
                    s.split("<Value ")
                        .skip(1)
                        .map(|v| {
                            let va = attrs(v);
                            (
                                get(&va, "Val").unwrap_or("").to_string(),
                                get(&va, "MapTo").unwrap_or("").to_string(),
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let elem_first = x.find("<Elem ").unwrap_or(x.len());
            let spc_here = x.find("<SpcVal>").is_some_and(|i| i < elem_first);
            if spc_here {
                let ours: Vec<(String, String)> = p
                    .spcval
                    .iter()
                    .map(|s| {
                        (
                            s.from.clone(),
                            s.to.clone().unwrap_or_else(|| s.from.clone()),
                        )
                    })
                    .collect();
                check(
                    &format!("special values {ours:?}, captured {spc:?}"),
                    ours == spc,
                );
            }
            if let (Some(lo), Some(hi)) = (get(&a, "RangeMinVal"), get(&a, "RangeMaxVal")) {
                check(
                    "range",
                    p.range.as_ref() == Some(&(lo.to_string(), hi.to_string())),
                );
            }
            // Elements: types, defaults, ranges, prompts.
            let elems: Vec<&str> = x.split("<Elem ").skip(1).collect();
            check("elements", elems.len() == p.items.len() || elems.is_empty());
            for (ex, e) in elems.iter().zip(&p.items) {
                let ea = attrs(ex.split('>').next().unwrap_or(""));
                let ek = format!("element {}", get(&ea, "Prompt").unwrap_or(""));
                check(
                    &format!("{ek} prompt"),
                    get(&ea, "Prompt") == Some(e.prompt_text()),
                );
                check(
                    &format!("{ek} type"),
                    get(&ea, "Type") == Some(xml_type(e).as_str()),
                );
                check(
                    &format!("{ek} default"),
                    get(&ea, "Dft") == e.dft.as_deref(),
                );
                if let (Some(lo), Some(hi)) = (get(&ea, "RangeMinVal"), get(&ea, "RangeMaxVal")) {
                    check(
                        &format!("{ek} range"),
                        e.range.as_ref() == Some(&(lo.to_string(), hi.to_string())),
                    );
                }
            }
        }
        // Dependencies: their messages in order.
        let deps: Vec<String> = xml
            .split("<Dep ")
            .skip(1)
            .filter_map(|d| get(&attrs(d), "MsgID").map(str::to_string))
            .collect();
        let ours: Vec<String> = d.deps.iter().filter_map(|x| x.msgid.clone()).collect();
        if deps != ours {
            bad.push(format!("{name}: dependencies {ours:?}, captured {deps:?}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Every dependency's message is in QCPFMSG.
#[test]
fn dependency_messages_exist() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/msgf/msgs/qcpfmsg.toml"),
    )
    .expect("qcpfmsg.toml");
    for name in COMMANDS {
        for d in &def(name).deps {
            let id = d.msgid.as_deref().unwrap_or("CPD0061");
            assert!(src.contains(&format!("id = \"{id}\"")), "{name}: {id}");
        }
    }
}
