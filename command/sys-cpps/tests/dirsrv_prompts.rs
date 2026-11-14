// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The directory server's commands against the primary (V7R1M0):
//! `CHGDIRSVRA`'s definition against `QCDRCMDD`'s XML in
//! `tools/conformance/oracle/dirsrv/cmdd71/`, `STRTCPSVR` and
//! `ENDTCPSVR`'s `INSTANCE` and its dependency, and the prompts against
//! `tools/conformance/golden/dirsrv/` character for character on rows 1
//! to 23 (row 24, the message line, is IBM's text there and ours here:
//! only whether a message shows is compared).

use std::path::{Path, PathBuf};

use cmd::analyze::{Env, MemSource, analyze};
use cmd::def::{CmdDef, Parm, PmtCtlRef};
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
    let p = root().join("golden/dirsrv").join(format!("{name}.txt"));
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
    ("prompt-chgdirsvra", "?CHGDIRSVRA", &[]),
    ("prompt-chgdirsvra-f10", "?CHGDIRSVRA", &[10]),
    ("prompt-chgdirsvra-f10-2", "?CHGDIRSVRA", &[10, PAGE_DOWN]),
    ("prompt-strtcpsvr-dirsrv", "?STRTCPSVR SERVER(*DIRSRV)", &[]),
    (
        "prompt-strtcpsvr-dirsrv-f10",
        "?STRTCPSVR SERVER(*DIRSRV)",
        &[10],
    ),
    (
        "prompt-strtcpsvr-dirsrv-f10-2",
        "?STRTCPSVR SERVER(*DIRSRV)",
        &[10, PAGE_DOWN],
    ),
    ("prompt-endtcpsvr-dirsrv", "?ENDTCPSVR SERVER(*DIRSRV)", &[]),
    (
        "prompt-endtcpsvr-dirsrv-f10",
        "?ENDTCPSVR SERVER(*DIRSRV)",
        &[10],
    ),
    (
        "prompt-endtcpsvr-dirsrv-f10-2",
        "?ENDTCPSVR SERVER(*DIRSRV)",
        &[10, PAGE_DOWN],
    ),
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

/// An XML element's attributes as pairs.
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

fn xml(name: &str) -> String {
    std::fs::read_to_string(root().join(format!("oracle/dirsrv/cmdd71/{name}.xml")))
        .expect("the XML")
}

/// The scalar a parameter (or element) is, as the XML names its type.
fn xml_type(p: &Parm) -> String {
    use cmd::def::ParmType as T;
    match &p.ty {
        T::Char => "CHAR".into(),
        T::Name => "NAME".into(),
        T::Elem { .. } => "ELEM".into(),
        other => format!("{other:?}"),
    }
}

/// The special (or single) values of an XML block, in order, with
/// their mapped values.
fn values(x: &str, tag: &str) -> Vec<(String, String)> {
    x.split(&format!("<{tag}>"))
        .nth(1)
        .and_then(|s| s.split(&format!("</{tag}>")).next())
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
                .collect()
        })
        .unwrap_or_default()
}

fn ours_values(v: &[cmd::def::Special]) -> Vec<(String, String)> {
    v.iter()
        .map(|s| {
            (
                s.from.clone(),
                s.to.clone().unwrap_or_else(|| s.from.clone()),
            )
        })
        .collect()
}

/// One parameter against its `<Parm>` element: keyword, type, length,
/// default, prompt, single and special values, `PMTCTL`, elements.
fn check_parm(cmd: &str, x: &str, p: &Parm, bad: &mut Vec<String>) {
    let head = x.split('>').next().unwrap_or("");
    let a = attrs(head);
    let kwd = get(&a, "Kwd").unwrap_or("");
    let mut check = |what: &str, ok: bool| {
        if !ok {
            bad.push(format!("{cmd} {kwd}: {what}"));
        }
    };
    check("keyword", p.kwd == kwd);
    check("type", get(&a, "Type") == Some(xml_type(p).as_str()));
    check("prompt", get(&a, "Prompt") == Some(p.prompt_text()));
    check(
        "max",
        get(&a, "Max").and_then(|v| v.parse::<u16>().ok()) == Some(p.max),
    );
    check("default", get(&a, "Dft") == p.dft.as_deref());
    if let Some(len) = get(&a, "Len") {
        check("length", p.length().len == len.parse::<u32>().unwrap_or(0));
    }
    let label = matches!(p.pmtctl, Some(PmtCtlRef::Label { .. }));
    check(
        "PMTCTL",
        (get(&a, "PmtCtl") == Some("PMTCTL")) == label,
    );
    let elem_first = x.find("<Elem ").unwrap_or(x.len());
    if x.find("<SngVal>").is_some_and(|i| i < elem_first) {
        let ours = ours_values(&p.sngval);
        let want = values(x, "SngVal");
        check(&format!("single values {ours:?}, captured {want:?}"), ours == want);
    }
    if x.find("<SpcVal>").is_some_and(|i| i < elem_first) {
        let ours = ours_values(&p.spcval);
        let want = values(&x[..elem_first], "SpcVal");
        check(&format!("special values {ours:?}, captured {want:?}"), ours == want);
    }
    let elems: Vec<&str> = x.split("<Elem ").skip(1).collect();
    check("elements", elems.len() == p.items.len());
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
        if let Some(len) = get(&ea, "Len") {
            check(
                &format!("{ek} length"),
                e.length().len == len.parse::<u32>().unwrap_or(0),
            );
        }
        let ours = ours_values(&e.spcval);
        let want = values(ex, "SpcVal");
        check(&format!("{ek} special values {ours:?}, captured {want:?}"), ours == want);
    }
}

/// `CHGDIRSVRA` is the primary's, attribute for attribute.
#[test]
fn chgdirsvra_matches_the_primary() {
    let x = xml("CHGDIRSVRA");
    let d = def("CHGDIRSVRA");
    let mut bad = Vec::new();
    let ca = attrs(x.split("<Parm ").next().unwrap_or(""));
    if get(&ca, "Prompt") != Some(d.prompt.as_str()) {
        bad.push(format!("prompt {:?}", d.prompt));
    }
    if get(&ca, "MaxPos").and_then(|v| v.parse::<u8>().ok()) != d.maxpos {
        bad.push(format!("MAXPOS {:?}", d.maxpos));
    }
    if get(&ca, "HlpID") != d.hlpid.as_deref() {
        bad.push(format!("HLPID {:?}", d.hlpid));
    }
    let parms: Vec<&str> = x.split("<Parm ").skip(1).collect();
    assert_eq!(parms.len(), d.parms.len(), "parameters");
    for (px, p) in parms.iter().zip(&d.parms) {
        check_parm("CHGDIRSVRA", px, p, &mut bad);
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// `INSTANCE` of `STRTCPSVR` and `ENDTCPSVR` is the primary's, with
/// `TCP1617` among the dependencies.
#[test]
fn instance_matches_the_primary() {
    let mut bad = Vec::new();
    for name in ["STRTCPSVR", "ENDTCPSVR"] {
        let x = xml(name);
        let d = def(name);
        let px = x
            .split("<Parm ")
            .find(|p| p.starts_with("Kwd=\"INSTANCE\""))
            .expect("INSTANCE in the XML");
        let px = px.split("</Parm>").next().unwrap_or(px);
        let p = d.parms.iter().find(|p| p.kwd == "INSTANCE").expect("INSTANCE");
        check_parm(name, px, p, &mut bad);
        if !d.deps.iter().any(|x| x.msgid.as_deref() == Some("TCP1617")) {
            bad.push(format!("{name}: no TCP1617 dependency"));
        }
        if d.msgf.name != "QTCPMSG" {
            bad.push(format!("{name}: MSGF {:?}", d.msgf));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// `TCP1617` is in `QTCPMSG`.
#[test]
fn the_dependency_message_exists() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/msgf/msgs/qtcpmsg.toml"),
    )
    .expect("qtcpmsg.toml");
    assert!(src.contains("id = \"TCP1617\""));
}
