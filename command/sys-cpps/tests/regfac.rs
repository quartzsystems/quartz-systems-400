// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The registration facility's commands against the oracle:
//! `WRKREGINF` keyword for keyword with pub400's `QCDRCMDD` definition
//! (`tools/conformance/oracle/net/pub400/command-definitions.txt`).
//! pub400 would not give `ADDEXITPGM` and `RMVEXITPGM` (`CPF9802`), so
//! theirs are checked for IBM's keywords and order.

use std::path::Path;

use cmd::def::ParmType;
use sys_cpps::{Site, defs, inventory};

fn captured(name: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/oracle/net/pub400/command-definitions.txt");
    let text = std::fs::read_to_string(&path).expect("the capture");
    let head = format!("CMD QSYS/{name}");
    let mut out = Vec::new();
    let mut lines = text.lines();
    for l in lines.by_ref() {
        if l == head || l.starts_with(&format!("{head} ")) {
            out.push(l.to_string());
            break;
        }
    }
    for l in lines {
        if l.trim().is_empty() {
            break;
        }
        out.push(l.trim().to_string());
    }
    out
}

/// `Key=value` from a capture line; values run to the next `Key=`.
fn attr(line: &str, key: &str) -> Option<String> {
    let at = line.find(&format!(" {key}="))?;
    let rest = &line[at + key.len() + 2..];
    let end = rest
        .match_indices(' ')
        .find(|(i, _)| {
            let next = &rest[i + 1..];
            next.split_once('=')
                .is_some_and(|(k, _)| k.chars().all(|c| c.is_ascii_alphanumeric()) && !k.is_empty())
                || next.starts_with("spc:")
                || next.starts_with("val:")
                || next.starts_with("sng:")
        })
        .map_or(rest.len(), |(i, _)| i);
    Some(rest[..end].to_string())
}

fn type_name(t: &ParmType) -> &'static str {
    match t {
        ParmType::Char => "CHAR",
        ParmType::Name => "NAME",
        ParmType::Generic => "GENERIC",
        ParmType::Int4 => "INT4",
        _ => "OTHER",
    }
}

fn def(name: &str) -> cmd::def::CmdDef {
    cmd::source::compile(name, defs::source(name).expect("shipped")).expect("compiles")
}

#[test]
fn wrkreginf_matches_the_oracle() {
    let cap = captured("WRKREGINF");
    assert!(cap.len() == 4, "{cap:?}");
    let d = def("WRKREGINF");
    assert_eq!(attr(&cap[0], "MaxPos").as_deref(), Some("2"));
    assert_eq!(d.maxpos, Some(2));
    assert_eq!(attr(&cap[0], "HlpPnlGrp").as_deref(), Some("QHUSCMD"));
    assert_eq!(
        d.hlppnlgrp.as_ref().map(|q| q.name.as_str()),
        Some("QHUSCMD")
    );
    assert_eq!(d.parms.len(), 3);
    for (line, p) in cap[1..].iter().zip(&d.parms) {
        let kwd = line.split_whitespace().nth(1).unwrap();
        assert_eq!(kwd, p.kwd);
        assert_eq!(
            attr(line, "Type").as_deref(),
            Some(type_name(&p.ty)),
            "{kwd}"
        );
        assert_eq!(
            attr(line, "Len").and_then(|l| l.parse::<u32>().ok()),
            p.len.as_ref().map(|l| l.len),
            "{kwd} length"
        );
        assert_eq!(attr(line, "Dft"), p.dft.clone(), "{kwd} default");
        assert_eq!(
            attr(line, "Rstd").as_deref() == Some("YES"),
            p.rstd,
            "{kwd} RSTD"
        );
        assert_eq!(
            attr(line, "Expr").as_deref() == Some("YES"),
            p.expr,
            "{kwd} EXPR"
        );
        let spc: Vec<String> = line
            .split_whitespace()
            .filter_map(|w| w.strip_prefix("spc:"))
            .map(str::to_string)
            .collect();
        let ours: Vec<String> = p
            .spcval
            .iter()
            .map(|s| {
                format!(
                    "{}={}",
                    s.from,
                    s.to.clone().unwrap_or_else(|| s.from.clone())
                )
            })
            .collect();
        assert_eq!(spc, ours, "{kwd} special values");
    }
}

#[test]
fn add_and_remove_have_ibms_keywords() {
    assert!(
        captured("ADDEXITPGM")[0].contains("CPF9802"),
        "pub400 refuses the definition"
    );
    let kwds = |n: &str| {
        def(n)
            .parms
            .iter()
            .map(|p| p.kwd.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        kwds("ADDEXITPGM"),
        [
            "EXITPNT",
            "FORMAT",
            "PGMNBR",
            "PGM",
            "THDSAFE",
            "MLTTHDACN",
            "TEXT",
            "REPLACE",
            "CRTEXITPNT",
            "PGMDTA"
        ]
    );
    assert_eq!(kwds("RMVEXITPGM"), ["EXITPNT", "FORMAT", "PGMNBR"]);
    let n = def("ADDEXITPGM");
    let pgmnbr = &n.parms[2];
    assert_eq!(
        pgmnbr
            .spcval
            .iter()
            .map(|s| s.from.as_str())
            .collect::<Vec<_>>(),
        ["*LOW", "*HIGH"]
    );
}

#[test]
fn the_commands_have_their_sites() {
    for name in ["ADDEXITPGM", "RMVEXITPGM", "WRKREGINF"] {
        let e = sys_cpps::entry(name).unwrap();
        assert!(
            inventory::DIRECT.contains(&name),
            "{name} runs in the service"
        );
        let want = if name == "WRKREGINF" {
            Site::Job
        } else {
            Site::Service
        };
        assert_eq!(e.site, want, "{name}");
    }
    assert!(inventory::in_service_too("WRKREGINF"));
}
